//! Executable permanent trust, credential isolation, consent/revocation, and
//! zero-undeclared-egress integration gate (`DecapodLabs/decapod#1944`).

use decapod::core::trust_egress::{
    ConsentRevocationState, DECLARED_EGRESS_CHANNELS, PERMANENT_TRUST_INVARIANTS,
    SECRET_ENV_CANARIES, THIRD_PARTY_AUTH_PATHS, detect_fingerprinting_violation,
    scan_for_credential_leaks, sha256_hex, validate_egress_url,
};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tempfile::TempDir;

const INVENTORY_DOC: &str = include_str!("../docs/trust-and-egress-inventory.md");
const LIB_RS: &str = include_str!("../src/decapod/lib.rs");
const RPC_RS: &str = include_str!("../src/decapod/core/rpc.rs");
const TELEMETRY_RS: &str =
    include_str!("../src/decapod/core/observability_projections/telemetry.rs");

fn decapod_bin() -> &'static str {
    env!("CARGO_BIN_EXE_decapod")
}

fn init_git_repo(dir: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Trust Gate"][..],
        &["config", "user.email", "trust@decapod.invalid"][..],
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ][..],
    ] {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

struct NetworkTrap {
    _shim_dir: TempDir,
    shim_bin: PathBuf,
    shim_log: PathBuf,
    proxy_addr: String,
    proxy_hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl NetworkTrap {
    fn new() -> Self {
        let shim_dir = tempfile::tempdir().expect("shim dir");
        let shim_bin = shim_dir.path().join("bin");
        fs::create_dir_all(&shim_bin).expect("create shim bin");
        let shim_log = shim_dir.path().join("egress_shim.log");

        for tool in ["curl", "gh"] {
            let tool_path = shim_bin.join(tool);
            let script = if tool == "curl" {
                format!(
                    "#!/bin/sh\nprintf '%s:%s\\n' '{tool}' \"$*\" >> \"{}\"\nif [ \"${{1:-}}\" = \"--version\" ]; then echo \"curl 8.0.0\"; exit 0; fi\nexit 99\n",
                    shim_log.display()
                )
            } else {
                format!(
                    "#!/bin/sh\nprintf '%s:%s\\n' '{tool}' \"$*\" >> \"{}\"\nexit 99\n",
                    shim_log.display()
                )
            };
            fs::write(&tool_path, script).expect("write shim script");
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&tool_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&tool_path, perms).unwrap();
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind trap proxy");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().unwrap();
        let proxy_hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let hits_clone = Arc::clone(&proxy_hits);
        let stop_clone = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        hits_clone.fetch_add(1, Ordering::SeqCst);
                        let mut buf = [0u8; 64];
                        let _ = stream.read(&mut buf);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            _shim_dir: shim_dir,
            shim_bin,
            shim_log,
            proxy_addr: format!("http://{addr}"),
            proxy_hits,
            stop,
            handle: Some(handle),
        }
    }

    fn path_env(&self) -> String {
        let base = std::env::var("PATH").unwrap_or_default();
        format!("{}:{base}", self.shim_bin.display())
    }

    fn shim_invocations(&self) -> String {
        fs::read_to_string(&self.shim_log).unwrap_or_default()
    }

    fn proxy_hit_count(&self) -> usize {
        self.proxy_hits.load(Ordering::SeqCst)
    }
}

impl Drop for NetworkTrap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_isolated_cli(
    repo: &Path,
    home: &Path,
    xdg_config: &Path,
    xdg_data: &Path,
    trap: &NetworkTrap,
    extra_env: &[(&str, &str)],
    args: &[&str],
) -> Output {
    let mut cmd = Command::new(decapod_bin());
    cmd.args(args)
        .current_dir(repo)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg_config)
        .env("XDG_DATA_HOME", xdg_data)
        .env("PATH", trap.path_env())
        .env("HTTP_PROXY", &trap.proxy_addr)
        .env("HTTPS_PROXY", &trap.proxy_addr)
        .env("ALL_PROXY", &trap.proxy_addr)
        .env("http_proxy", &trap.proxy_addr)
        .env("https_proxy", &trap.proxy_addr)
        .env("all_proxy", &trap.proxy_addr)
        .env("NO_PROXY", "")
        .env("DECAPOD_AGENT_ID", "trust-gate-agent")
        .env_remove("DECAPOD_CHECK_LATEST")
        .env_remove("DECAPOD_CLOUD_FIRST_PARTY_ORIGIN")
        .env_remove("DECAPOD_CLOUD_TEST_ORIGIN");
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("execute decapod binary")
}

fn collect_tree_text(root: &Path, out: &mut String) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_tree_text(&path, out);
        } else if let Ok(bytes) = fs::read(&path) {
            out.push_str(&String::from_utf8_lossy(&bytes));
            out.push('\n');
        }
    }
}

#[test]
fn local_cli_workflows_and_restarts_make_zero_outbound_connections() {
    let repo = tempfile::tempdir().expect("repo");
    let home = tempfile::tempdir().expect("home");
    let xdg_config = tempfile::tempdir().expect("xdg config");
    let xdg_data = tempfile::tempdir().expect("xdg data");
    let trap = NetworkTrap::new();

    init_git_repo(repo.path());

    let init = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["init", "--force", "--proof"],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let docs = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["docs", "ingest"],
    );
    assert!(docs.status.success(), "docs ingest failed");

    let session = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["session", "acquire"],
    );
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let session_stdout = String::from_utf8_lossy(&session.stdout);
    let password = session_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .map(str::trim)
        .expect("session password printed")
        .to_string();
    let auth_env = [("DECAPOD_SESSION_PASSWORD", password.as_str())];

    let caps = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &auth_env,
        &["capabilities", "--format", "json"],
    );
    assert!(caps.status.success(), "capabilities failed");
    let caps_json: serde_json::Value =
        serde_json::from_slice(&caps.stdout).expect("capabilities JSON");
    assert_eq!(caps_json["version_check"], "unavailable");
    assert_eq!(caps_json["is_latest"], false);

    let add = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &auth_env,
        &["todo", "add", "Offline trust check", "--format", "json"],
    );
    assert!(
        add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let todo_json: serde_json::Value = serde_json::from_slice(&add.stdout).expect("todo JSON");
    let todo_id = todo_json["id"].as_str().expect("todo id").to_string();

    let claim = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &auth_env,
        &[
            "todo",
            "claim",
            "--id",
            &todo_id,
            "--agent",
            "trust-gate-agent",
        ],
    );
    assert!(
        claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim.stderr)
    );

    for args in [
        &["rpc", "--op", "agent.init"][..],
        &["rpc", "--op", "context.resolve"][..],
        &["rpc", "--op", "observe.telemetry"][..],
        &["analyze", "--read-only", "--format", "json"][..],
        // Simulated process restart reading persisted state
        &["session", "status"][..],
        &["todo", "list", "--format", "json"][..],
    ] {
        let out = run_isolated_cli(
            repo.path(),
            home.path(),
            xdg_config.path(),
            xdg_data.path(),
            &trap,
            &auth_env,
            args,
        );
        assert!(
            out.status.success(),
            "command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // validate --pre-build runs offline without network calls (exit 0 or 1 depending on governance.json claims, never network)
    let _pre_build = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &auth_env,
        &["validate", "--pre-build", "--format", "json"],
    );

    assert_eq!(
        trap.shim_invocations(),
        "",
        "local CLI workflows must never invoke curl or gh"
    );
    assert_eq!(
        trap.proxy_hit_count(),
        0,
        "local CLI workflows must make zero outbound HTTP/HTTPS connections"
    );
}

#[test]
fn planted_third_party_auth_stores_and_secret_env_canaries_never_leak_or_fingerprint() {
    let repo = tempfile::tempdir().expect("repo");
    let home = tempfile::tempdir().expect("home");
    let xdg_config = tempfile::tempdir().expect("xdg config");
    let xdg_data = tempfile::tempdir().expect("xdg data");
    let trap = NetworkTrap::new();

    init_git_repo(repo.path());

    // Plant synthetic third-party auth stores in isolated HOME
    let codex_canary = "sk-codex-canary-9f8e7d6c5b4a3210";
    let claude_canary = "sk-ant-canary-1a2b3c4d5e6f7890";
    let gh_canary = "gho_canaryTokenValue1234567890abcdef";

    let codex_dir = format!(".{}dex", "co");
    fs::create_dir_all(home.path().join(&codex_dir)).unwrap();
    fs::write(
        home.path().join(&codex_dir).join("auth.json"),
        format!(r#"{{"OPENAI_API_KEY":"{codex_canary}"}}"#),
    )
    .unwrap();

    fs::write(
        home.path().join(".claude.json"),
        format!(r#"{{"primaryApiKey":"{claude_canary}","user":"canary@example.invalid"}}"#),
    )
    .unwrap();

    fs::create_dir_all(home.path().join(".config/gh")).unwrap();
    fs::write(
        home.path().join(".config/gh/hosts.yml"),
        format!("github.com:\n    oauth_token: {gh_canary}\n    user: canary\n"),
    )
    .unwrap();

    let openai_env = "sk-openai-canary-deadbeef12345678";
    let anthropic_env = "sk-ant-api03-canary-cafebabe12345678";
    let stripe_env = "sk_live_canary_51abcdef12345678";
    let github_env = "ghp_canarySecretToken1234567890abcdef";

    let env_canaries = [
        ("OPENAI_API_KEY", openai_env),
        ("ANTHROPIC_API_KEY", anthropic_env),
        ("STRIPE_SECRET_KEY", stripe_env),
        ("GITHUB_TOKEN", github_env),
    ];

    let mut captured_output = String::new();
    for args in [
        &["init", "--force", "--proof"][..],
        &["session", "acquire"][..],
    ] {
        let out = run_isolated_cli(
            repo.path(),
            home.path(),
            xdg_config.path(),
            xdg_data.path(),
            &trap,
            &env_canaries,
            args,
        );
        assert!(out.status.success());
        captured_output.push_str(&String::from_utf8_lossy(&out.stdout));
        captured_output.push_str(&String::from_utf8_lossy(&out.stderr));
    }

    let password = captured_output
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .map(str::trim)
        .expect("session password")
        .to_string();

    let mut env_with_session = env_canaries.to_vec();
    env_with_session.push(("DECAPOD_SESSION_PASSWORD", password.as_str()));

    for args in [
        &["todo", "add", "canary isolation check"][..],
        &["rpc", "--op", "agent.init"][..],
        &["rpc", "--op", "observe.telemetry"][..],
        &["analyze", "--read-only", "--format", "json"][..],
        &["validate", "--pre-build", "--format", "json"][..],
    ] {
        let out = run_isolated_cli(
            repo.path(),
            home.path(),
            xdg_config.path(),
            xdg_data.path(),
            &trap,
            &env_with_session,
            args,
        );
        captured_output.push_str(&String::from_utf8_lossy(&out.stdout));
        captured_output.push_str(&String::from_utf8_lossy(&out.stderr));
    }

    let mut persisted_state = String::new();
    collect_tree_text(&repo.path().join(".decapod"), &mut persisted_state);
    collect_tree_text(xdg_config.path(), &mut persisted_state);
    collect_tree_text(xdg_data.path(), &mut persisted_state);

    let all_canaries = [
        codex_canary,
        claude_canary,
        gh_canary,
        openai_env,
        anthropic_env,
        stripe_env,
        github_env,
    ];
    scan_for_credential_leaks(&captured_output, &all_canaries)
        .expect("no secret canary or hash leaked to stdout/stderr");
    scan_for_credential_leaks(&persisted_state, &all_canaries)
        .expect("no secret canary or hash persisted under .decapod/ or XDG state");
    detect_fingerprinting_violation(&persisted_state, &all_canaries)
        .expect("no third-party credential fingerprinting");
}

#[test]
fn consent_persists_across_reloads_and_revocation_halts_egress_across_restarts() {
    let repo = tempfile::tempdir().expect("repo");
    let home = tempfile::tempdir().expect("home");
    let xdg_config = tempfile::tempdir().expect("xdg config");
    let xdg_data = tempfile::tempdir().expect("xdg data");
    let trap = NetworkTrap::new();

    init_git_repo(repo.path());

    let init = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["init", "--force", "--proof"],
    );
    assert!(init.status.success());

    // Default state: local, session_auto_update = false
    let state = ConsentRevocationState::load_from_project_root(repo.path()).expect("load config");
    assert!(!state.allows_cloud_egress());
    assert!(!state.allows_update_egress());

    // Opt-in to session_auto_update = true
    let cfg_path = repo.path().join(".decapod/config.toml");
    let raw = fs::read_to_string(&cfg_path).unwrap();
    let opted_in = raw.replace("[repo]", "[repo]\nsession_auto_update = true");
    fs::write(&cfg_path, opted_in).unwrap();

    // Reload preserves opt-in consent
    let reloaded =
        ConsentRevocationState::load_from_project_root(repo.path()).expect("reload config");
    assert!(reloaded.allows_update_egress());

    // Opted-in session acquire attempts version check via curl shim
    let _ = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["session", "acquire"],
    );
    assert!(
        trap.shim_invocations()
            .contains("crates.io/api/v1/crates/decapod"),
        "opted-in session_auto_update=true should invoke crates.io check; got: {}",
        trap.shim_invocations()
    );

    // Revoke consent: disable session_auto_update and clear shim log
    let revoked_cfg = fs::read_to_string(&cfg_path)
        .unwrap()
        .replace("session_auto_update = true", "session_auto_update = false");
    fs::write(&cfg_path, revoked_cfg).unwrap();
    let _ = fs::remove_file(&trap.shim_log);

    // Verify immediate cessation and persistence across restarts
    let post_revoke =
        ConsentRevocationState::load_from_project_root(repo.path()).expect("post-revoke config");
    assert!(!post_revoke.allows_update_egress());

    for _ in 0..2 {
        let out = run_isolated_cli(
            repo.path(),
            home.path(),
            xdg_config.path(),
            xdg_data.path(),
            &trap,
            &[],
            &["session", "acquire"],
        );
        assert!(out.status.success());
    }
    assert_eq!(
        trap.shim_invocations(),
        "",
        "revocation must halt outbound version checks immediately and across restarts"
    );
}

#[test]
fn negative_control_counter_tests_catch_undeclared_egress_non_https_and_leaks() {
    // 1. Undeclared host must fail
    let undeclared = validate_egress_url("https://undeclared.example.invalid/collect", &[]);
    assert!(
        undeclared.is_err(),
        "negative control: undeclared host must fail"
    );

    // 2. Non-HTTPS non-loopback must fail
    let non_https = validate_egress_url("http://api.decapodlabs.ai/v1/events", &[]);
    assert!(
        non_https.is_err(),
        "negative control: plaintext HTTP to non-loopback must fail"
    );

    // 3. Credential / fingerprint leak must fail
    let secret = "sk-openai-negative-control-123456";
    assert!(
        scan_for_credential_leaks(&format!("artifact body contains {secret}"), &[secret]).is_err(),
        "negative control: raw credential leak must fail"
    );
    let fp = sha256_hex(secret.as_bytes());
    assert!(
        detect_fingerprinting_violation(&format!("telemetry_id={}", &fp[..16]), &[secret]).is_err(),
        "negative control: hashed/truncated fingerprint must fail"
    );

    // 4. Runtime CurlTransport must fail closed BEFORE spawning curl on undeclared/non-HTTPS URLs
    use decapod::core::propodus::{CurlTransport, PropodusTransport};
    let undeclared_transport = CurlTransport::default().request(
        "GET",
        "https://undeclared.example.invalid/api/health",
        "",
        None,
    );
    assert!(
        undeclared_transport.is_err(),
        "negative control: CurlTransport must reject undeclared outbound host before curl"
    );
    let invalid_port = validate_egress_url("https://crates.io:notaport/api/v1/crates/decapod", &[]);
    assert!(
        invalid_port.is_err(),
        "negative control: invalid authority port must fail closed"
    );

    // 5. Network trap shim catches disallowed curl invocation when triggered
    let trap = NetworkTrap::new();
    let out = Command::new("curl")
        .arg("https://undeclared.example.invalid/ping")
        .env("PATH", trap.path_env())
        .output()
        .expect("invoke curl shim");
    assert_eq!(out.status.code(), Some(99));
    assert!(
        trap.shim_invocations()
            .contains("undeclared.example.invalid"),
        "negative control: shim must record disallowed egress"
    );
}

#[test]
fn static_inventory_completeness_matches_codebase_and_invariants() {
    for invariant in PERMANENT_TRUST_INVARIANTS {
        assert!(
            INVENTORY_DOC.contains(invariant),
            "docs/trust-and-egress-inventory.md missing invariant: {invariant}"
        );
    }

    for channel in DECLARED_EGRESS_CHANNELS {
        assert!(
            INVENTORY_DOC.contains(channel.id),
            "docs/trust-and-egress-inventory.md missing channel `{}`",
            channel.id
        );
        for host in channel.allowed_hosts {
            if *host == "localhost" || *host == "[::1]" {
                continue;
            }
            assert!(
                INVENTORY_DOC.contains(host),
                "docs/trust-and-egress-inventory.md missing host `{host}` for `{}`",
                channel.id
            );
        }
    }

    for path in THIRD_PARTY_AUTH_PATHS {
        let doc_needle = if path.ends_with("auth.json") {
            "auth.json"
        } else {
            *path
        };
        assert!(
            INVENTORY_DOC.contains(doc_needle),
            "docs/trust-and-egress-inventory.md missing third-party auth path `{path}`"
        );
    }
    for env_key in SECRET_ENV_CANARIES {
        assert!(
            INVENTORY_DOC.contains(env_key),
            "docs/trust-and-egress-inventory.md missing env canary `{env_key}`"
        );
    }

    assert!(
        INVENTORY_DOC.contains("ExecutionTelemetryProjection"),
        "inventory doc must explicitly cover ExecutionTelemetryProjection"
    );

    // Ensure session acquire gates check_and_update_version behind session_auto_update
    assert!(
        LIB_RS.contains("project_config.repo.session_auto_update\n        && let Ok(true) = check_and_update_version(true)")
            || LIB_RS.contains("project_config.repo.session_auto_update && let Ok(true) = check_and_update_version(true)"),
        "session acquire must gate check_and_update_version behind explicit session_auto_update"
    );

    // Ensure capabilities gates crates.io check behind explicit DECAPOD_CHECK_LATEST
    assert!(
        RPC_RS.contains("DECAPOD_CHECK_LATEST"),
        "generate_capabilities must gate crates.io lookup behind explicit DECAPOD_CHECK_LATEST"
    );

    // Ensure telemetry projection remains strictly read-only and local-only
    assert!(
        TELEMETRY_RS.contains("no default upload") && TELEMETRY_RS.contains("read_only: true"),
        "telemetry projection must preserve strictly read-only / no default upload invariant"
    );
}

fn collect_src_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(dir)
        .expect("read src dir")
        .map(|entry| entry.expect("dir entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_src_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn strip_cfg_test_items(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut skipping = false;
    let mut saw_brace = false;
    let mut depth: i32 = 0;
    for line in src.lines() {
        if !skipping {
            if line.trim() == "#[cfg(test)]" {
                skipping = true;
                saw_brace = false;
                depth = 0;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        } else {
            let opens = line.bytes().filter(|&b| b == b'{').count() as i32;
            let closes = line.bytes().filter(|&b| b == b'}').count() as i32;
            if opens > 0 {
                saw_brace = true;
            }
            depth += opens - closes;
            if (saw_brace && depth <= 0) || (!saw_brace && line.trim_end().ends_with(';')) {
                skipping = false;
            }
        }
    }
    out
}

#[test]
fn whole_src_outbound_network_and_telemetry_sinks_are_inventoried_and_fail_closed() {
    use decapod::core::telemetry::TelemetryRetentionPolicy;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_root = root.join("src/decapod");
    let mut rs_files = Vec::new();
    collect_src_rs_files(&src_root, &mut rs_files);
    assert!(!rs_files.is_empty());

    // Every production file under src/decapod/ with an outbound curl/gh/socket/HTTP sink
    // must be explicitly listed here and documented in docs/trust-and-egress-inventory.md.
    const REVIEWED_OUTBOUND_SOURCE_SINKS: &[(&str, &str)] = &[
        ("src/decapod/core/model_assets.rs", "model_assets.rs"),
        ("src/decapod/core/propodus.rs", "propodus.rs"),
        ("src/decapod/core/rpc.rs", "src/decapod/core/rpc.rs"),
        (
            "src/decapod/core/snapshot_provider/mod.rs",
            "snapshot_provider/mod.rs",
        ),
        (
            "src/decapod/core/snapshot_provider/public_buildkite.rs",
            "public_buildkite.rs",
        ),
        ("src/decapod/core/validate.rs", "validate.rs"),
        (
            "src/decapod/core/workspace_publication.rs",
            "workspace_publication.rs",
        ),
        ("src/decapod/lib.rs", "src/decapod/lib.rs"),
    ];

    // Schema URIs, documentation/badge URLs, and non-network registry metadata strings
    // that are never dialed as outbound egress endpoints.
    const NON_EGRESS_INFORMATIONAL_HOSTS: &[&str] = &[
        "decapod.dev",
        "decapodlabs.github.io",
        "example.invalid",
        "fixture.invalid",
        "img.shields.io",
        "index.crates.io",
        "json-schema.org",
    ];

    let sink_needles = [
        "Command::new(\"curl\")",
        "Command::new(\"/usr/bin/curl\")",
        "Command::new(&self.curl)",
        "Command::new(curl)",
        "Command::new(\"gh\")",
        "TcpStream::connect",
        "UdpSocket::",
        "reqwest::",
        "ureq::",
        "hyper::Client",
        "hyper_util::",
    ];

    let mut discovered_sinks = Vec::new();
    for path in &rs_files {
        let rel = path
            .strip_prefix(root)
            .expect("relative path")
            .to_string_lossy()
            .replace('\\', "/");
        if rel.ends_with("_tests.rs") || rel.ends_with("/tests.rs") || rel.contains("/tests/") {
            continue;
        }
        let raw = fs::read_to_string(path).expect("read src file");
        let prod = strip_cfg_test_items(&raw);

        let mut matched_sink = false;
        for line in prod.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if sink_needles.iter().any(|needle| line.contains(needle)) {
                matched_sink = true;
            }
            let mut rest = line;
            while let Some(pos) = rest.find("https://") {
                let after = &rest[pos + "https://".len()..];
                let authority: String = after
                    .chars()
                    .take_while(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '[' | ']' | ':')
                    })
                    .collect();
                let host = authority
                    .trim_start_matches('[')
                    .split(']')
                    .next()
                    .unwrap_or("")
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !host.is_empty() && !NON_EGRESS_INFORMATIONAL_HOSTS.contains(&host.as_str()) {
                    let probe_url = format!("https://{host}/");
                    assert!(
                        validate_egress_url(&probe_url, &[]).is_ok(),
                        "undeclared production https:// host `{host}` found in `{rel}`; add to DECLARED_EGRESS_CHANNELS & docs/trust-and-egress-inventory.md"
                    );
                }
                rest = after;
            }
        }
        if matched_sink {
            discovered_sinks.push(rel);
        }
    }

    for rel in &discovered_sinks {
        let entry = REVIEWED_OUTBOUND_SOURCE_SINKS
            .iter()
            .find(|(reviewed_rel, _)| reviewed_rel == &rel.as_str());
        assert!(
            entry.is_some(),
            "new uninventoried outbound sink `{rel}` found under src/decapod/; review and register in trust_egress_gate + docs/trust-and-egress-inventory.md"
        );
        let (_, doc_needle) = entry.unwrap();
        assert!(
            INVENTORY_DOC.contains(doc_needle),
            "docs/trust-and-egress-inventory.md missing sink entry `{doc_needle}` for `{rel}`"
        );
    }

    // model_assets.rs and telemetry default policy fail closed.
    let model_assets_rs = fs::read_to_string(root.join("src/decapod/core/model_assets.rs"))
        .expect("read model_assets.rs");
    assert!(
        model_assets_rs.contains("validate_egress_url(url, &[])"),
        "model_assets.rs must validate URLs via trust_egress::validate_egress_url"
    );
    assert!(
        !TelemetryRetentionPolicy::default().allow_identifier_observation,
        "TelemetryRetentionPolicy default must keep allow_identifier_observation = false"
    );
}

#[test]
fn cloud_to_local_revocation_and_check_latest_toggle_halt_outbound_egress_under_trap() {
    let repo = tempfile::tempdir().expect("repo");
    let home = tempfile::tempdir().expect("home");
    let xdg_config = tempfile::tempdir().expect("xdg config");
    let xdg_data = tempfile::tempdir().expect("xdg data");
    let trap = NetworkTrap::new();

    init_git_repo(repo.path());

    let init = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["init", "--force", "--proof"],
    );
    assert!(init.status.success());

    let session = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[],
        &["session", "acquire"],
    );
    assert!(session.status.success());
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .map(str::trim)
        .expect("session password")
        .to_string();

    // 1. DECAPOD_CHECK_LATEST=1 opts in `capabilities`, and clearing it halts immediately.
    let caps_off = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[("DECAPOD_SESSION_PASSWORD", password.as_str())],
        &["capabilities", "--format", "json"],
    );
    assert!(caps_off.status.success());
    assert_eq!(trap.shim_invocations(), "");

    let caps_on = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[
            ("DECAPOD_SESSION_PASSWORD", password.as_str()),
            ("DECAPOD_CHECK_LATEST", "1"),
        ],
        &["capabilities", "--format", "json"],
    );
    assert!(caps_on.status.success());
    assert!(
        trap.shim_invocations()
            .contains("crates.io/api/v1/crates/decapod"),
        "expected DECAPOD_CHECK_LATEST=1 to invoke curl shim; got: {}",
        trap.shim_invocations()
    );

    let _ = fs::remove_file(&trap.shim_log);
    let caps_revoked = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[("DECAPOD_SESSION_PASSWORD", password.as_str())],
        &["capabilities", "--format", "json"],
    );
    assert!(caps_revoked.status.success());
    assert_eq!(
        trap.shim_invocations(),
        "",
        "unsetting DECAPOD_CHECK_LATEST must immediately stop outbound crates.io checks"
    );

    // 2. Switching backend = "cloud" -> cloud logout + backend = "local" halts Cloud egress immediately.
    let cloud_listener = TcpListener::bind("127.0.0.1:0").expect("bind cloud trap");
    cloud_listener.set_nonblocking(true).expect("nonblocking");
    let cloud_addr = cloud_listener.local_addr().unwrap();
    let cloud_hits = Arc::new(AtomicUsize::new(0));
    let cloud_stop = Arc::new(AtomicBool::new(false));
    let hits_ref = Arc::clone(&cloud_hits);
    let stop_ref = Arc::clone(&cloud_stop);
    let cloud_thread = std::thread::spawn(move || {
        while !stop_ref.load(Ordering::Relaxed) {
            match cloud_listener.accept() {
                Ok((mut stream, _)) => {
                    hits_ref.fetch_add(1, Ordering::SeqCst);
                    let mut buf = [0u8; 256];
                    let _ = stream.read(&mut buf);
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    let cfg_path = repo.path().join(".decapod/config.toml");
    let base_cfg = fs::read_to_string(&cfg_path).expect("read config.toml");
    let cloud_cfg = base_cfg.replace("backend = \"local\"", "backend = \"cloud\"");
    assert_ne!(cloud_cfg, base_cfg, "config must toggle backend = cloud");
    fs::write(&cfg_path, cloud_cfg).expect("write cloud config");
    let state_cloud = ConsentRevocationState::load_from_project_root(repo.path()).expect("cloud");
    assert!(state_cloud.allows_cloud_egress());

    let origin_url = format!("http://{cloud_addr}");
    let _ = Command::new(decapod_bin())
        .args(["cloud", "status"])
        .current_dir(repo.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg_config.path())
        .env("XDG_DATA_HOME", xdg_data.path())
        .env("DECAPOD_AGENT_ID", "trust-gate-agent")
        .env("DECAPOD_SESSION_PASSWORD", password.as_str())
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin_url)
        .output()
        .expect("run decapod cloud status");
    assert!(
        cloud_hits.load(Ordering::SeqCst) > 0,
        "opted-in cloud status should reach loopback test origin before revocation"
    );

    // Revoke cloud consent: logout + switch backend back to local, then verify 0 trap hits.
    let logout = run_isolated_cli(
        repo.path(),
        home.path(),
        xdg_config.path(),
        xdg_data.path(),
        &trap,
        &[("DECAPOD_SESSION_PASSWORD", password.as_str())],
        &["cloud", "logout"],
    );
    assert!(
        logout.status.success(),
        "cloud logout failed: {}",
        String::from_utf8_lossy(&logout.stderr)
    );
    fs::write(&cfg_path, &base_cfg).expect("restore local backend");
    let state_local = ConsentRevocationState::load_from_project_root(repo.path()).expect("local");
    assert!(!state_local.allows_cloud_egress());

    cloud_hits.store(0, Ordering::SeqCst);
    for args in [
        &["session", "acquire"][..],
        &["capabilities", "--format", "json"][..],
        &["todo", "list", "--format", "json"][..],
        &["rpc", "--op", "observe.telemetry"][..],
        &["validate", "--pre-build", "--format", "json"][..],
    ] {
        let out = Command::new(decapod_bin())
            .args(args)
            .current_dir(repo.path())
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", xdg_config.path())
            .env("XDG_DATA_HOME", xdg_data.path())
            .env("DECAPOD_AGENT_ID", "trust-gate-agent")
            .env("DECAPOD_SESSION_PASSWORD", password.as_str())
            .env("DECAPOD_TEST_MODE", "1")
            .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin_url)
            .output()
            .expect("run post-revocation command");
        assert!(
            out.status.success(),
            "post-revocation {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert_eq!(
        cloud_hits.load(Ordering::SeqCst),
        0,
        "revoking cloud backend must halt all cloud origin egress across restarts"
    );
    cloud_stop.store(true, Ordering::Relaxed);
    let _ = cloud_thread.join();
}
