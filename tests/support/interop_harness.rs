//! Reusable black-box conformance runner for the Decapod governance
//! interoperability profile (issues #870/#871).
//!
//! Every scenario spawns the *real* `decapod` binary against an isolated
//! temporary git fixture (own XDG dirs, own store, no production
//! credentials, no network). The developer's real repository is never
//! touched. The runner records deterministic machine-readable case reports;
//! a report is diagnostic conformance evidence, never a publication receipt.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Bounded, deadlock-free child wait: stdout and stderr are drained on
/// separate threads (a chatty child must never block on a full pipe while the
/// parent waits), and the wait never exceeds the conformance bound.
fn wait_bounded(mut child: Child) -> Output {
    use std::io::Read;
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                let stdout = out_handle.join().unwrap_or_default();
                let stderr = err_handle.join().unwrap_or_default();
                return Output {
                    status,
                    stdout,
                    stderr,
                };
            }
            None => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("decapod rpc --stdin exceeded 120s bound");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Serialize scenario execution: fixtures use per-process sqlite stores, but
/// keep runs deterministic under `cargo test` parallelism anyway.
static RUN_LOCK: Mutex<()> = Mutex::new(());

pub fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles_dir).join("_main").join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

/// One machine-readable conformance case outcome.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CaseReport {
    pub id: String,
    pub interface: String,
    pub expected: String,
    pub observed: String,
    /// `pass` | `fail` | `unsupported`
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

pub struct Harness {
    pub bin: PathBuf,
    pub scenario: String,
    pub root: PathBuf,
    _temp: tempfile::TempDir,
    pub fixture_id: String,
    /// Session password acquired for agent `unknown` (fixture-local only).
    pub password: String,
    pub cases: Vec<CaseReport>,
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("git")
}

impl Harness {
    /// Create an isolated fixture repo, init Decapod, acquire a session, and
    /// complete the project prerequisites + awareness warm-up required before
    /// any mutating RPC op (same bootstrap contract as tests/agent_rpc_suite.rs).
    pub fn new(scenario: &str) -> Harness {
        let _guard = RUN_LOCK.lock().unwrap();
        let temp = tempfile::Builder::new()
            .prefix(&format!("decapod-interop-{scenario}-"))
            .tempdir()
            .expect("temp fixture");
        let root = temp.path().to_path_buf();
        let fixture_id = format!("interop-{scenario}-{}", ulid());

        let out = git(&root, &["init", "-b", "master"]);
        assert!(
            out.status.success(),
            "git init: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = git(&root, &["config", "user.name", "Interop Fixture"]);
        assert!(out.status.success());
        let out = git(&root, &["config", "user.email", "interop@fixture.invalid"]);
        assert!(out.status.success());

        let bin = resolve_decapod_bin();
        let mut h = Harness {
            bin: bin.clone(),
            scenario: scenario.to_string(),
            root: root.clone(),
            _temp: temp,
            fixture_id,
            password: String::new(),
            cases: Vec::new(),
        };

        let out = h.raw(
            &[
                "init",
                "--force",
                "--product-summary",
                "Interop conformance fixture repository with governed local state.",
            ],
            &[],
            false,
        );
        assert!(
            out.status.success(),
            "decapod init: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let out = git(&root, &["checkout", "-b", &format!("feat/{scenario}")]);
        assert!(out.status.success());
        let out = git(&root, &["add", "-A"]);
        assert!(out.status.success());
        let out = git(&root, &["commit", "-m", "interop fixture baseline"]);
        assert!(
            out.status.success(),
            "fixture commit: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // Session acquire (prints "Password: ...").
        let out = h.raw(&["session", "acquire"], &[], false);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let password = stdout
            .lines()
            .find_map(|l| l.strip_prefix("Password: ").map(|v| v.trim().to_string()))
            .expect("session password");
        h.password = password.clone();

        // Project prerequisites (marks validation scope completed for awareness).
        let out = h.raw(
            &["govern", "project-check"],
            &[
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
                ("DECAPOD_VALIDATE_TIMEOUT_SECONDS", "30"),
            ],
            true,
        );
        assert!(
            out.status.success(),
            "project-check: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        // Constitutional awareness warm-up required before mutating RPC ops.
        let res = h.rpc("constitution.get", json!({"section": "core/DECAPOD"}));
        assert_eq!(res["success"], true, "constitution.get warm-up");
        let res = h.rpc("agent.init", json!({}));
        assert_eq!(res["success"], true, "agent.init warm-up: {}", res);
        let res = h.rpc(
            "context.resolve",
            json!({"intent": "interop conformance fixture"}),
        );
        assert_eq!(res["success"], true, "context.resolve warm-up: {}", res);

        h
    }

    /// Run the real binary with fixture isolation under an arbitrary agent id
    /// and optional session password. Extra env pairs are applied last.
    pub fn run_raw(
        &self,
        agent_id: &str,
        password: Option<&str>,
        args: &[&str],
        extra_env: &[(&str, &str)],
    ) -> Output {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&self.root).args(args);
        cmd.env("XDG_CONFIG_HOME", self.root.join(".xdg-config"));
        cmd.env("XDG_DATA_HOME", self.root.join(".xdg-data"));
        cmd.env("DECAPOD_AGENT_ID", agent_id);
        cmd.env("DECAPOD_CLAIM_AUTORUN", "0");
        cmd.env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1");
        cmd.env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1");
        cmd.env_remove("DECAPOD_SESSION_PASSWORD");
        cmd.env_remove("GITHUB_ACTIONS");
        cmd.env_remove("BUILDKITE");
        if let Some(password) = password {
            cmd.env("DECAPOD_SESSION_PASSWORD", password);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn decapod");
        // Bounded wait: conformance must never hang.
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            match child.try_wait().expect("try_wait") {
                Some(status) => {
                    let mut stdout = Vec::new();
                    let mut stderr = Vec::new();
                    use std::io::Read;
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_end(&mut stdout);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_end(&mut stderr);
                    }
                    return Output {
                        status,
                        stdout,
                        stderr,
                    };
                }
                None => {
                    if std::time::Instant::now() > deadline {
                        let _ = child.kill();
                        panic!("decapod {:?} exceeded 120s bound", args);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    /// Run the real binary with fixture isolation. `with_session` attaches the
    /// acquired session password; extra env pairs are applied last.
    pub fn raw(&self, args: &[&str], extra_env: &[(&str, &str)], with_session: bool) -> Output {
        self.run_raw(
            "unknown",
            if with_session {
                Some(&self.password)
            } else {
                None
            },
            args,
            extra_env,
        )
    }

    /// Run the real binary as another agent with its own session password.
    pub fn raw_as(
        &self,
        agent_id: &str,
        password: Option<&str>,
        args: &[&str],
        extra_env: &[(&str, &str)],
    ) -> Output {
        self.run_raw(agent_id, password, args, extra_env)
    }

    /// Invoke an RPC op through the real `decapod rpc --stdin` boundary and
    /// return the parsed response envelope.
    pub fn rpc(&self, op: &str, params: Value) -> Value {
        let (res, out) = self.rpc_raw(json!({"op": op, "params": params}));
        assert!(
            out.status.success() || res.get("success").is_some(),
            "rpc {op} produced no envelope: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        res
    }

    /// Invoke `decapod rpc --stdin` with an arbitrary payload; returns
    /// (parsed envelope if any, raw Output).
    pub fn rpc_raw(&self, payload: Value) -> (Value, Output) {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&self.root)
            .args(["rpc", "--stdin"])
            .env("XDG_CONFIG_HOME", self.root.join(".xdg-config"))
            .env("XDG_DATA_HOME", self.root.join(".xdg-data"))
            .env("DECAPOD_AGENT_ID", "unknown")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1")
            .env("DECAPOD_SESSION_PASSWORD", &self.password)
            .env_remove("GITHUB_ACTIONS")
            .env_remove("BUILDKITE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn rpc");
        {
            let mut stdin = child.stdin.take().expect("stdin");
            use std::io::Write;
            stdin
                .write_all(serde_json::to_string(&payload).unwrap().as_bytes())
                .expect("write rpc payload");
        }
        let out = wait_bounded(child);
        let parsed = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (parsed, out)
    }

    /// Invoke `decapod rpc --stdin` as an arbitrary agent with an optional
    /// session password and an arbitrary payload.
    pub fn rpc_payload_raw(
        &self,
        agent_id: &str,
        password: Option<&str>,
        payload: Value,
    ) -> (Value, Output) {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(&self.root)
            .args(["rpc", "--stdin"])
            .env("XDG_CONFIG_HOME", self.root.join(".xdg-config"))
            .env("XDG_DATA_HOME", self.root.join(".xdg-data"))
            .env("DECAPOD_AGENT_ID", agent_id)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1")
            .env_remove("DECAPOD_SESSION_PASSWORD")
            .env_remove("GITHUB_ACTIONS")
            .env_remove("BUILDKITE");
        if let Some(password) = password {
            cmd.env("DECAPOD_SESSION_PASSWORD", password);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn rpc");
        {
            let mut stdin = child.stdin.take().expect("stdin");
            use std::io::Write;
            stdin
                .write_all(serde_json::to_string(&payload).unwrap().as_bytes())
                .expect("write rpc payload");
        }
        let out = wait_bounded(child);
        let parsed = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (parsed, out)
    }

    /// Target binary identity (version) via the public capabilities surface.
    pub fn target_version(&self) -> String {
        let out = self.raw(&["capabilities", "--format", "json"], &[], false);
        assert!(out.status.success());
        let v: Value = serde_json::from_slice(&out.stdout).expect("capabilities json");
        v["version"].as_str().unwrap_or("unknown").to_string()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &mut self,
        id: &str,
        interface: &str,
        expected: &str,
        observed: impl AsRef<str>,
        pass: bool,
        failure_reason: Option<String>,
        evidence: Vec<String>,
    ) {
        self.cases.push(CaseReport {
            id: id.to_string(),
            interface: interface.to_string(),
            expected: expected.to_string(),
            observed: observed.as_ref().to_string(),
            outcome: if pass {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            failure_reason,
            evidence,
        });
    }

    /// Machine-readable conformance report for this scenario.
    pub fn report(&self) -> Value {
        let pass = self.cases.iter().filter(|c| c.outcome == "pass").count();
        let fail = self.cases.iter().filter(|c| c.outcome == "fail").count();
        let unsupported = self
            .cases
            .iter()
            .filter(|c| c.outcome == "unsupported")
            .count();
        json!({
            "profile_id": "decapod.interop",
            "profile_version": "v1",
            "manifest_schema_version": "1.0.0",
            "target": {
                "binary": self.bin.display().to_string(),
                "version": self.target_version(),
            },
            "tested_interface": "decapod rpc --stdin (real process) + decapod CLI (real process)",
            "fixture_id": self.fixture_id,
            "scenario": self.scenario,
            "generated_by": format!("tests/interop_conformance.rs::{}", self.scenario),
            "note": "Diagnostic conformance evidence only; not a Decapod validation receipt and not publication authorization.",
            "summary": { "total": self.cases.len(), "pass": pass, "fail": fail, "unsupported": unsupported },
            "cases": self.cases,
        })
    }

    /// Write the report and fail the test if any required case failed.
    pub fn finish(self) {
        let report = self.report();
        let fail = report["summary"]["fail"].as_u64().unwrap_or(0);
        let dir = std::env::var("INTEROP_CONFORMANCE_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("target")
                    .join("interop-conformance")
            });
        // Under Bazel the manifest dir is a read-only runfiles tree; fall back
        // to a writable temp dir so the report is still emitted.
        let path = dir.join(format!("{}.json", self.scenario));
        let written = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()));
        let path = match written {
            Ok(()) => path,
            Err(_) => {
                let fallback = std::env::temp_dir().join("interop-conformance");
                std::fs::create_dir_all(&fallback).expect("fallback report dir");
                let p = fallback.join(format!("{}.json", self.scenario));
                std::fs::write(&p, serde_json::to_string_pretty(&report).unwrap())
                    .expect("write fallback report");
                p
            }
        };
        eprintln!("conformance report: {}", path.display());
        assert_eq!(
            fail,
            0,
            "conformance scenario {} has failing cases: {}",
            self.scenario,
            serde_json::to_string_pretty(&report).unwrap()
        );
    }
}

fn ulid() -> String {
    // Small unique suffix without pulling in extra deps.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{nanos:x}")
}
