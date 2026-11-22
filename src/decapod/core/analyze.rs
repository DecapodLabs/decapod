//! Lower-level zero-mutation read-only repository analysis mode (#1279).
//!
//! Separates epistemic/analysis operations (`OperationClass::AnalyzeReadOnly`)
//! from mutating governance/execution workflows (`OperationClass::GovernedChange`).
//!
//! Invariants:
//! - Zero filesystem mutations to the target repository or `.decapod/`.
//! - No session, todo, worktree, container, migration, or storage lock creation.
//! - Bounded traversal (file count, byte budget, wall-clock timeout).
//! - Symlinks are never followed; sensitive files are excluded without reading.
//! - Repository code and Git hooks are never executed.
//! - Prompt-safety scanning runs on repository instruction files.
//! - Observed facts, advisory draft inferences, and governance observations are
//!   strictly separated; missing governance or protected-main never block analysis.

use crate::cli::{AnalyzeCli, DecapodProjectConfig};
use crate::core::agent_eval::{self, EvalStatus};
use crate::core::bounded_process::BoundedCommand;
use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Versioned schema identifier for `decapod analyze` output.
pub const ANALYZE_SCHEMA_VERSION: &str = "decapod.analyze-readonly.v1";

const MAX_ALLOWED_FILES: usize = 50_000;
const MAX_ALLOWED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ALLOWED_TIMEOUT_MS: u64 = 60_000;
const MIN_ALLOWED_TIMEOUT_MS: u64 = 100;
const MAX_SINGLE_FILE_READ_BYTES: usize = 64 * 1024;

const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".turbo",
    ".cache",
];

const INSTRUCTION_FILES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    "CODEX.md",
    "README.md",
    ".decapod/OVERRIDE.md",
];

const EXPECTED_SPECS: &[&str] = &[
    "INTENT.md",
    "ARCHITECTURE.md",
    "INTERFACES.md",
    "SECURITY.md",
    "OPERATIONS.md",
    "VALIDATION.md",
    "SEMANTICS.md",
    "README.md",
];

/// First-class operation class distinguishing read-only epistemic analysis from
/// mutating governed implementation workflows (#1279).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationClass {
    /// Zero-mutation repository understanding and evidence gathering.
    AnalyzeReadOnly,
    /// Mutating implementation/governance workflow requiring full custody and proof.
    GovernedChange,
}

impl OperationClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnalyzeReadOnly => "analyze_read_only",
            Self::GovernedChange => "governed_change",
        }
    }

    /// Gates that do not apply to `AnalyzeReadOnly` (not bypassed or failed, simply N/A).
    pub fn non_applicable_gates(self) -> &'static [&'static str] {
        match self {
            Self::AnalyzeReadOnly => &[
                "protected_main",
                "worktree_isolation",
                "container_custody",
                "session_auth",
                "todo_claim",
                "trajectory",
                "proof_plan",
                "publication",
            ],
            Self::GovernedChange => &[],
        }
    }
}

/// Bounded analysis execution limits.
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeOptions {
    pub max_files: usize,
    pub max_bytes: u64,
    pub timeout_ms: u64,
    pub draft_spec: bool,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            max_bytes: 4 * 1024 * 1024,
            timeout_ms: 15_000,
            draft_spec: false,
        }
    }
}

impl AnalyzeOptions {
    fn validate(self) -> Result<Self, DecapodError> {
        if self.max_files == 0 || self.max_files > MAX_ALLOWED_FILES {
            return Err(DecapodError::ValidationError(format!(
                "ANALYZE_BOUNDS_INVALID: max_files must be in 1..={MAX_ALLOWED_FILES}"
            )));
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_ALLOWED_BYTES {
            return Err(DecapodError::ValidationError(format!(
                "ANALYZE_BOUNDS_INVALID: max_bytes must be in 1..={MAX_ALLOWED_BYTES}"
            )));
        }
        if self.timeout_ms < MIN_ALLOWED_TIMEOUT_MS || self.timeout_ms > MAX_ALLOWED_TIMEOUT_MS {
            return Err(DecapodError::ValidationError(format!(
                "ANALYZE_BOUNDS_INVALID: timeout_ms must be in {MIN_ALLOWED_TIMEOUT_MS}..={MAX_ALLOWED_TIMEOUT_MS}"
            )));
        }
        Ok(self)
    }
}

fn is_sensitive_filename(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    if base == ".env"
        || base.starts_with(".env.")
        || base == "id_rsa"
        || base == "id_ed25519"
        || base == "id_ecdsa"
        || base == ".npmrc"
        || base == ".netrc"
        || base == ".pypirc"
        || base == "credentials"
        || base == "credentials.json"
        || base.ends_with(".pem")
        || base.ends_with(".key")
        || base.ends_with(".p12")
        || base.ends_with(".pfx")
        || base.ends_with(".kdbx")
        || base.ends_with(".jks")
    {
        return true;
    }
    lower.contains("/.aws/")
        || lower.contains("/.ssh/")
        || lower.contains("secrets.yaml")
        || lower.contains("secrets.yml")
        || lower.contains("secrets.json")
        || lower.contains("secrets.toml")
}

fn should_skip_dir(rel: &str, name: &str) -> bool {
    if SKIPPED_DIRS.contains(&name) {
        return true;
    }
    rel == ".decapod/workspaces"
        || rel.starts_with(".decapod/workspaces/")
        || rel == ".decapod/data"
        || rel.starts_with(".decapod/data/")
}

fn language_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "ts" | "tsx" => Some("TypeScript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "py" => Some("Python"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "kt" | "kts" => Some("Kotlin"),
        "rb" => Some("Ruby"),
        "c" | "h" => Some("C"),
        "cc" | "cpp" | "cxx" | "hpp" => Some("C++"),
        "swift" => Some("Swift"),
        "sh" | "bash" | "zsh" => Some("Shell"),
        "sql" => Some("SQL"),
        "toml" => Some("TOML"),
        "yaml" | "yml" => Some("YAML"),
        "json" => Some("JSON"),
        "md" => Some("Markdown"),
        _ => None,
    }
}

fn is_manifest_name(base: &str) -> bool {
    matches!(
        base,
        "Cargo.toml"
            | "package.json"
            | "pyproject.toml"
            | "setup.py"
            | "requirements.txt"
            | "go.mod"
            | "Makefile"
            | "Dockerfile"
            | "BUILD.bazel"
            | "MODULE.bazel"
            | "WORKSPACE"
            | "flake.nix"
            | "Gemfile"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "CMakeLists.txt"
    )
}

fn is_entrypoint_path(rel: &str) -> bool {
    matches!(
        rel,
        "src/main.rs"
            | "src/cli.rs"
            | "main.go"
            | "main.py"
            | "cli.py"
            | "src/index.ts"
            | "src/index.js"
            | "src/cli.ts"
            | "src/cli.js"
    ) || (rel.starts_with("src/bin/") && rel.ends_with(".rs"))
        || (rel.starts_with("cmd/") && rel.ends_with(".go"))
        || rel.starts_with("bin/")
}

fn is_scannable_text(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    if is_manifest_name(base) {
        return true;
    }
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    matches!(
        ext,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "sh" | "toml" | "json" | "yaml" | "yml"
    )
}

fn read_bounded_utf8(path: &Path, remaining_budget: u64) -> Option<(String, u64)> {
    let cap = (remaining_budget as usize).min(MAX_SINGLE_FILE_READ_BYTES);
    if cap == 0 {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut buf = Vec::with_capacity(cap.min(8192));
    let n = file.by_ref().take(cap as u64).read_to_end(&mut buf).ok()? as u64;
    let text = String::from_utf8(buf).ok()?;
    Some((text, n))
}

#[derive(Default)]
struct SurfaceSignals {
    cli: BTreeSet<String>,
    ipc_process: BTreeSet<String>,
    network_server: BTreeSet<String>,
    storage: BTreeSet<String>,
    tests: BTreeSet<String>,
}

fn scan_surface_signals(rel: &str, text: &str, signals: &mut SurfaceSignals) {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    if rel.starts_with("tests/")
        || rel.contains("/tests/")
        || rel.ends_with("_test.rs")
        || rel.ends_with("_test.go")
        || rel.ends_with(".test.ts")
        || rel.ends_with(".spec.ts")
        || base.starts_with("test_")
    {
        signals.tests.insert(rel.to_string());
    }

    // CLI signals
    for needle in [
        "clap",
        "structopt",
        "argh",
        "argparse",
        "click",
        "typer",
        "cobra",
        "commander",
        "yargs",
        "std::env::args",
        "process.argv",
        "sys.argv",
    ] {
        if text.contains(needle) {
            signals.cli.insert(format!("{rel} ({needle})"));
            break;
        }
    }

    // Process / local IPC signals
    for needle in [
        "std::process::Command",
        "Command::new",
        "UnixStream",
        "UnixListener",
        "child_process",
        "subprocess",
        "os/exec",
        "io::stdin",
        "BufReader::new(io::stdin",
        "ipc",
    ] {
        if text.contains(needle) {
            signals.ipc_process.insert(format!("{rel} ({needle})"));
            break;
        }
    }

    // Network server / API gateway signals — ONLY recorded on concrete server markers!
    for needle in [
        "axum",
        "actix-web",
        "actix_web",
        "warp::serve",
        "rocket::build",
        "tonic::transport::Server",
        "http::ListenAndServe",
        "express()",
        "FastAPI(",
        "Flask(",
        "TcpListener::bind",
        "hyper::server",
    ] {
        if text.contains(needle) {
            signals.network_server.insert(format!("{rel} ({needle})"));
            break;
        }
    }

    // Local/remote storage signals
    for needle in [
        "rusqlite",
        "dactyl-db",
        "dactyl_db",
        "sqlite3",
        "sqlx",
        "diesel",
        "psycopg",
        "sqlalchemy",
        "prisma",
        "CREATE TABLE",
    ] {
        if text.contains(needle) {
            signals.storage.insert(format!("{rel} ({needle})"));
            break;
        }
    }
}

fn read_only_git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .bounded_output(Duration::from_secs(3))
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn observe_git(root: &Path) -> Value {
    let Some(inside) = read_only_git(root, &["rev-parse", "--is-inside-work-tree"]) else {
        return json!({
            "available": false,
            "is_git_repo": false,
            "reason": "git unavailable or target is not a git repository"
        });
    };
    if inside != "true" {
        return json!({
            "available": true,
            "is_git_repo": false
        });
    }
    let branch = read_only_git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let head_sha = read_only_git(root, &["rev-parse", "--verify", "HEAD"]);
    let is_protected = branch
        .as_deref()
        .is_some_and(|b| matches!(b, "main" | "master"));
    let dirty = read_only_git(root, &["status", "--porcelain"]).map(|s| !s.is_empty());
    json!({
        "available": true,
        "is_git_repo": true,
        "branch": branch,
        "head_sha": head_sha,
        "is_protected_branch": is_protected,
        "working_tree_dirty": dirty
    })
}

fn scan_instruction_prompt_safety(root: &Path) -> Value {
    let mut files_scanned = Vec::new();
    let mut blocked_files = Vec::new();
    let mut total_findings = 0usize;

    for rel in INSTRUCTION_FILES {
        let full = root.join(rel);
        let Ok(meta) = fs::symlink_metadata(&full) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Some((text, _)) = read_bounded_utf8(&full, MAX_SINGLE_FILE_READ_BYTES as u64) else {
            continue;
        };
        let eval = agent_eval::evaluate_prompt(&text);
        let finding_codes: Vec<String> = eval.findings.iter().map(|f| f.code.clone()).collect();
        total_findings += eval.findings.len();
        let status_str = match eval.status {
            EvalStatus::Allow => "allow",
            EvalStatus::Block => {
                blocked_files.push((*rel).to_string());
                "block"
            }
        };
        files_scanned.push(json!({
            "path": rel,
            "status": status_str,
            "finding_count": eval.findings.len(),
            "finding_codes": finding_codes,
        }));
    }

    json!({
        "checked": true,
        "overall_status": if blocked_files.is_empty() { "allow" } else { "block_findings_observed" },
        "files_scanned": files_scanned,
        "blocked_files": blocked_files,
        "total_findings": total_findings,
        "note": "Repository instructions are treated as untrusted data and never executed during read-only analysis."
    })
}

fn observe_governance(root: &Path, git_obs: &Value) -> Value {
    let decapod_dir = root.join(".decapod");
    let initialized = fs::symlink_metadata(&decapod_dir)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    let config_path = decapod_dir.join("config.toml");
    let config_present = fs::symlink_metadata(&config_path)
        .map(|m| m.is_file())
        .unwrap_or(false);
    let (config_valid, config_product_name) = if config_present {
        match read_bounded_utf8(&config_path, 512 * 1024) {
            Some((text, _)) => match DecapodProjectConfig::parse(&text) {
                Ok(cfg) => (Some(true), cfg.repo.product_name),
                Err(_) => (Some(false), None),
            },
            None => (Some(false), None),
        }
    } else {
        (None, None)
    };
    let governance_json_present = fs::symlink_metadata(decapod_dir.join("governance.json"))
        .map(|m| m.is_file())
        .unwrap_or(false);

    let specs_dir = decapod_dir.join("managed").join("specs");
    let mut living_specs_present = Vec::new();
    let mut living_specs_missing = Vec::new();
    for spec in EXPECTED_SPECS {
        if fs::symlink_metadata(specs_dir.join(spec))
            .map(|m| m.is_file())
            .unwrap_or(false)
        {
            living_specs_present.push((*spec).to_string());
        } else {
            living_specs_missing.push((*spec).to_string());
        }
    }

    let protected_branch_observed = git_obs.get("is_protected_branch").and_then(Value::as_bool);

    let mut observations = Vec::new();
    if !initialized {
        observations.push("Repository is not initialized with Decapod (.decapod/ absent).");
    }
    if config_present && config_valid == Some(false) {
        observations.push(".decapod/config.toml failed schema validation.");
    }
    if initialized && !governance_json_present {
        observations.push(".decapod/governance.json is absent.");
    }
    if protected_branch_observed == Some(true) {
        observations.push(
            "Checkout is on a protected branch (main/master); non-blocking for read-only analysis, requires isolated workspace before mutation.",
        );
    }

    json!({
        "blocks_analysis": false,
        "initialized": initialized,
        "config_present": config_present,
        "config_valid": config_valid,
        "configured_product_name": config_product_name,
        "governance_json_present": governance_json_present,
        "living_specs_present": living_specs_present,
        "living_specs_missing": living_specs_missing,
        "protected_branch_observed": protected_branch_observed,
        "non_applicable_gates": OperationClass::AnalyzeReadOnly.non_applicable_gates(),
        "observations": observations,
    })
}

fn infer_capabilities(
    manifests: &[String],
    entrypoints: &[String],
    signals: &SurfaceSignals,
) -> Vec<Value> {
    let mut caps = Vec::new();

    if !signals.cli.is_empty() || !entrypoints.is_empty() {
        let mut refs: Vec<String> = entrypoints.iter().take(6).cloned().collect();
        refs.extend(signals.cli.iter().take(6).cloned());
        caps.push(json!({
            "name": "local_cli_application",
            "status": "draft",
            "advisory": true,
            "confidence": if !signals.cli.is_empty() && !entrypoints.is_empty() { "high" } else { "medium" },
            "evidence_refs": refs,
            "rationale": "Observed CLI entrypoints or argument-parsing dependencies in repository source."
        }));
    }

    if !signals.ipc_process.is_empty() {
        caps.push(json!({
            "name": "local_process_or_ipc",
            "status": "draft",
            "advisory": true,
            "confidence": "medium",
            "evidence_refs": signals.ipc_process.iter().take(8).cloned().collect::<Vec<_>>(),
            "rationale": "Observed process spawning, stdin/stdout, or local Unix socket IPC usage."
        }));
    }

    if !signals.network_server.is_empty() {
        caps.push(json!({
            "name": "network_service_listener",
            "status": "draft",
            "advisory": true,
            "confidence": "medium",
            "evidence_refs": signals.network_server.iter().take(8).cloned().collect::<Vec<_>>(),
            "rationale": "Observed explicit HTTP/RPC/TCP server listener dependencies or bindings."
        }));
    }

    if !signals.storage.is_empty() {
        caps.push(json!({
            "name": "persistent_datastore",
            "status": "draft",
            "advisory": true,
            "confidence": "medium",
            "evidence_refs": signals.storage.iter().take(8).cloned().collect::<Vec<_>>(),
            "rationale": "Observed database driver dependencies or SQL schema definitions."
        }));
    }

    if !signals.tests.is_empty() {
        caps.push(json!({
            "name": "automated_test_suite",
            "status": "draft",
            "advisory": true,
            "confidence": "high",
            "evidence_refs": signals.tests.iter().take(8).cloned().collect::<Vec<_>>(),
            "rationale": "Observed test directories or test source files."
        }));
    }

    if caps.is_empty() && !manifests.is_empty() {
        caps.push(json!({
            "name": "software_package_unknown_runtime",
            "status": "draft",
            "advisory": true,
            "confidence": "low",
            "evidence_refs": manifests.iter().take(6).cloned().collect::<Vec<_>>(),
            "rationale": "Build manifests were observed without enough runtime markers to infer architecture."
        }));
    }

    caps
}

/// Analyze a repository strictly read-only without creating `.decapod/`, sessions,
/// locks, worktrees, or running repository code.
pub fn analyze_repository(root: &Path, options: AnalyzeOptions) -> Result<Value, DecapodError> {
    let options = options.validate()?;
    let target = root.canonicalize().map_err(|e| {
        DecapodError::ValidationError(format!(
            "ANALYZE_TARGET_INVALID: cannot inspect '{}': {e}",
            root.display()
        ))
    })?;
    if !target.is_dir() {
        return Err(DecapodError::ValidationError(format!(
            "ANALYZE_TARGET_INVALID: '{}' is not a directory",
            target.display()
        )));
    }

    let deadline = Instant::now() + Duration::from_millis(options.timeout_ms);
    let mut stack: Vec<(PathBuf, String)> = vec![(target.clone(), String::new())];
    let mut languages: BTreeMap<String, usize> = BTreeMap::new();
    let mut manifests: Vec<String> = Vec::new();
    let mut entrypoints: Vec<String> = Vec::new();
    let mut signals = SurfaceSignals::default();

    let mut files_inspected: usize = 0;
    let mut bytes_read: u64 = 0;
    let mut skipped_symlinks: usize = 0;
    let mut excluded_sensitive_files: usize = 0;
    let mut truncation_reasons: BTreeSet<&'static str> = BTreeSet::new();

    while let Some((dir_path, rel_prefix)) = stack.pop() {
        if Instant::now() >= deadline {
            truncation_reasons.insert("timeout_reached");
            break;
        }
        let Ok(entries) = fs::read_dir(&dir_path) else {
            continue;
        };
        let mut children: Vec<(String, PathBuf)> = entries
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
            .collect();
        children.sort_by(|a, b| a.0.cmp(&b.0));

        // Push subdirectories in reverse so lexicographically earlier dirs pop first.
        let mut subdirs = Vec::new();
        for (name, path) in children {
            if Instant::now() >= deadline {
                truncation_reasons.insert("timeout_reached");
                break;
            }
            let rel = if rel_prefix.is_empty() {
                name.clone()
            } else {
                format!("{rel_prefix}/{name}")
            };
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                skipped_symlinks += 1;
                continue;
            }
            if meta.is_dir() {
                if !should_skip_dir(&rel, &name) {
                    subdirs.push((path, rel));
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if is_sensitive_filename(&rel) {
                excluded_sensitive_files += 1;
                continue;
            }
            if files_inspected >= options.max_files {
                truncation_reasons.insert("max_files_reached");
                break;
            }
            files_inspected += 1;

            if let Some(ext) = path.extension().and_then(|e| e.to_str())
                && let Some(lang) = language_for_ext(ext)
            {
                *languages.entry(lang.to_string()).or_insert(0) += 1;
            }
            if is_manifest_name(&name) {
                manifests.push(rel.clone());
            }
            if is_entrypoint_path(&rel) {
                entrypoints.push(rel.clone());
            }
            if is_scannable_text(&rel) {
                let remaining = options.max_bytes.saturating_sub(bytes_read);
                if remaining == 0 {
                    truncation_reasons.insert("max_bytes_reached");
                } else if let Some((text, n)) = read_bounded_utf8(&path, remaining) {
                    bytes_read = bytes_read.saturating_add(n);
                    scan_surface_signals(&rel, &text, &mut signals);
                }
            }
        }
        for sub in subdirs.into_iter().rev() {
            stack.push(sub);
        }
        if truncation_reasons.contains("max_files_reached")
            || truncation_reasons.contains("timeout_reached")
        {
            break;
        }
    }

    manifests.sort();
    entrypoints.sort();

    let git_obs = observe_git(&target);
    let prompt_safety = scan_instruction_prompt_safety(&target);
    let governance_obs = observe_governance(&target, &git_obs);
    let inferred = infer_capabilities(&manifests, &entrypoints, &signals);
    let truncated = !truncation_reasons.is_empty();

    let mut unavailable_checks = vec![
        json!({
            "check": "code_execution_and_tests",
            "status": "unavailable",
            "reason": "AnalyzeReadOnly never executes repository code, build scripts, or git hooks."
        }),
        json!({
            "check": "network_and_remote_ci",
            "status": "unavailable",
            "reason": "AnalyzeReadOnly operates strictly offline without credentials or network access."
        }),
    ];
    if git_obs["available"] != true {
        unavailable_checks.push(json!({
            "check": "git_metadata",
            "status": "unavailable",
            "reason": "git binary absent or target is not inside a git work tree."
        }));
    }

    let draft_spec_report = options.draft_spec.then(|| {
        json!({
            "authoritative": false,
            "status": "draft",
            "written_to_disk": false,
            "notice": "DRAFT ADVISORY REPORT ONLY — never treated as authoritative specs until explicitly promoted under GovernedChange.",
            "detected_languages": languages,
            "manifests": manifests,
            "entrypoints": entrypoints,
            "proposed_capabilities": inferred,
        })
    });

    let report = json!({
        "schema_version": ANALYZE_SCHEMA_VERSION,
        "operation_class": OperationClass::AnalyzeReadOnly.as_str(),
        "read_only": true,
        "mutations_performed": 0,
        "status": if truncated { "partial" } else { "ok" },
        "target": target.display().to_string(),
        "bounds": {
            "max_files": options.max_files,
            "max_bytes": options.max_bytes,
            "timeout_ms": options.timeout_ms,
            "files_inspected": files_inspected,
            "bytes_read": bytes_read,
            "skipped_symlinks": skipped_symlinks,
            "excluded_sensitive_files": excluded_sensitive_files,
            "truncated": truncated,
            "truncation_reasons": truncation_reasons.into_iter().collect::<Vec<_>>(),
        },
        "observed_facts": {
            "languages": languages,
            "manifests": manifests,
            "entrypoints": entrypoints,
            "git": git_obs,
            "surface_evidence": {
                "cli_signals": signals.cli.into_iter().take(12).collect::<Vec<_>>(),
                "ipc_process_signals": signals.ipc_process.into_iter().take(12).collect::<Vec<_>>(),
                "network_server_signals": signals.network_server.into_iter().take(12).collect::<Vec<_>>(),
                "storage_signals": signals.storage.into_iter().take(12).collect::<Vec<_>>(),
                "test_files_observed": signals.tests.len(),
            }
        },
        "inferred_capabilities": inferred,
        "prompt_safety": prompt_safety,
        "governance_observations": governance_obs,
        "unavailable_checks": unavailable_checks,
        "draft_spec_report": draft_spec_report,
        "transition_guidance": {
            "automatic_promotion": false,
            "next_steps_for_governed_change": [
                "Request explicit user approval before mutating the repository.",
                "Initialize if absent: decapod init",
                "Acquire session and claim task: decapod session acquire && decapod todo add \"...\" && decapod todo claim --id <id>",
                "Create isolated worktree: decapod workspace ensure"
            ]
        }
    });

    Ok(crate::core::trace::redact(report))
}

pub(crate) fn run_analyze_cli(cli: AnalyzeCli) -> Result<(), DecapodError> {
    let _read_only_asserted = cli.read_only;
    let target = match cli.path {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let options = AnalyzeOptions {
        max_files: cli.max_files,
        max_bytes: cli.max_bytes,
        timeout_ms: cli.timeout_ms,
        draft_spec: cli.draft_spec,
    };
    let report = analyze_repository(&target, options)?;
    if cli.format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(std::io::Error::other)?
        );
    } else {
        println!("=== DECAPOD READ-ONLY ANALYSIS (AnalyzeReadOnly) ===");
        println!("Target: {}", report["target"].as_str().unwrap_or("unknown"));
        println!(
            "Status: {} (mutations_performed=0, truncated={})",
            report["status"].as_str().unwrap_or("unknown"),
            report["bounds"]["truncated"]
        );
        println!(
            "Bounds: files_inspected={}, bytes_read={}, skipped_symlinks={}, excluded_sensitive_files={}",
            report["bounds"]["files_inspected"],
            report["bounds"]["bytes_read"],
            report["bounds"]["skipped_symlinks"],
            report["bounds"]["excluded_sensitive_files"],
        );
        if let Some(langs) = report["observed_facts"]["languages"].as_object() {
            let summary: Vec<String> = langs.iter().map(|(k, v)| format!("{k}({v})")).collect();
            println!(
                "Observed languages: {}",
                if summary.is_empty() {
                    "none".to_string()
                } else {
                    summary.join(", ")
                }
            );
        }
        if let Some(manifests) = report["observed_facts"]["manifests"].as_array() {
            let items: Vec<&str> = manifests.iter().filter_map(Value::as_str).collect();
            println!(
                "Observed manifests: {}",
                if items.is_empty() {
                    "none".to_string()
                } else {
                    items.join(", ")
                }
            );
        }
        println!(
            "Prompt safety: {} (findings={})",
            report["prompt_safety"]["overall_status"]
                .as_str()
                .unwrap_or("unknown"),
            report["prompt_safety"]["total_findings"]
        );
        println!(
            "Governance observations (non-blocking): initialized={}, protected_branch={}",
            report["governance_observations"]["initialized"],
            report["governance_observations"]["protected_branch_observed"],
        );
        if let Some(caps) = report["inferred_capabilities"].as_array() {
            println!("Inferred capabilities (DRAFT / advisory only):");
            if caps.is_empty() {
                println!("  - none");
            } else {
                for cap in caps {
                    println!(
                        "  - {} [draft, confidence={}]",
                        cap["name"].as_str().unwrap_or("unknown"),
                        cap["confidence"].as_str().unwrap_or("unknown")
                    );
                }
            }
        }
    }
    Ok(())
}
