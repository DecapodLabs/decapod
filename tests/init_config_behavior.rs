use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{NamedTempFile, tempdir};

fn resolve_decapod_bin() -> PathBuf {
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

fn run_decapod(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(resolve_decapod_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run decapod")
}

fn run_git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_persists_remote_default_base_branch() {
    let tmp = tempdir().expect("tempdir");
    run_git(tmp.path(), &["init", "-q"]);
    run_git(tmp.path(), &["config", "user.email", "test@test.com"]);
    run_git(tmp.path(), &["config", "user.name", "Test"]);
    fs::write(tmp.path().join("README.md"), "# project\n").expect("write readme");
    run_git(tmp.path(), &["add", "README.md"]);
    run_git(tmp.path(), &["commit", "-m", "initial"]);
    run_git(tmp.path(), &["branch", "-M", "main"]);
    run_git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:example/project.git",
        ],
    );
    run_git(
        tmp.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    run_git(
        tmp.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );

    let out = run_decapod(tmp.path(), &["init", "with", "--force"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let config =
        fs::read_to_string(tmp.path().join(".decapod/config.toml")).expect("read config.toml");
    assert!(
        config.contains("base_branch = \"main\""),
        "init should persist the remote default branch: {config}"
    );
}

#[test]
fn init_with_backend_cloud_saves_to_config() {
    let tmp = tempdir().expect("tempdir");
    let out = Command::new(resolve_decapod_bin())
        .args(["init", "with", "--backend", "cloud", "--force"])
        .current_dir(tmp.path())
        .output()
        .expect("run decapod");

    assert!(
        out.status.success(),
        "decapod init with --backend cloud failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = tmp.path().join(".decapod/config.toml");
    let config = fs::read_to_string(config_path).expect("read config.toml");
    assert!(
        config.contains("backend = \"cloud\""),
        "explicit cloud backend must be recorded: {config}"
    );
    assert!(!config.contains("[cloud]"));
    assert!(!config.contains("api_url"));
    assert!(!config.contains("project_id"));
    assert!(!config.contains("repo_id"));
    assert!(
        !config.contains("supabase") && !config.contains("token") && !config.contains("secret"),
        "repo config must not contain credentials or legacy backend secrets: {config}"
    );

    let registration_path = tmp
        .path()
        .join(".decapod/managed/cloud/init-registration.json");
    let registration =
        fs::read_to_string(registration_path).expect("read cloud init registration payload");
    let registration: serde_json::Value =
        serde_json::from_str(&registration).expect("parse cloud init registration");
    assert_eq!(registration["provider"], "vercel");
    assert_eq!(
        registration["route"],
        "POST /query for task reads/writes; POST /batch for atomic task operations",
        "registration should target the Dactyl storage contract"
    );
    assert!(
        registration["writes"]
            .as_array()
            .expect("writes array")
            .iter()
            .any(|write| {
                write["table"] == "tasks" && write["operation"] == "Dactyl query list/add"
            }),
        "registration should model repo-scoped todo access"
    );
}

#[test]
fn init_with_backend_local_is_default() {
    let tmp = tempdir().expect("tempdir");
    let _ = run_decapod(tmp.path(), &["init", "with", "--force"]);

    let config_path = tmp.path().join(".decapod/config.toml");
    let config = fs::read_to_string(config_path).expect("read config.toml");
    assert!(config.contains("backend = \"local\""));
    assert!(
        !config.contains("[decision]"),
        "retired provider configuration must not be generated: {config}"
    );
    assert!(!config.contains("mode = \"local\""));
    assert!(
        !config.contains("[cloud]"),
        "cloud service details must not be serialized: {config}"
    );
}

#[test]
fn init_rejects_retired_provider_flag_without_persisting_credentials() {
    let project = tempdir().expect("project tempdir");
    let data_home = tempdir().expect("data tempdir");
    let output = Command::new(resolve_decapod_bin())
        .args(["init", "with", "--force", "--decision-provider", "jev"])
        .current_dir(project.path())
        .env("XDG_DATA_HOME", data_home.path())
        .env("TYPESAFE_API_KEY", "jev-test-key")
        .output()
        .expect("run decapod");
    assert!(
        !output.status.success(),
        "retired provider flag must be rejected: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!data_home.path().join("decapod/secrets.json").exists());
}

#[test]
fn init_refresh_ignores_retired_provider_configuration_and_leaves_secrets_untouched() {
    let project = tempdir().expect("project tempdir");
    let data_home = tempdir().expect("data tempdir");
    let first = run_decapod(project.path(), &["init", "with", "--force"]);
    assert!(
        first.status.success(),
        "initial init failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let config_path = project.path().join(".decapod/config.toml");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config.push_str("\n[decision]\nprovider = \"jev\"\n");
    fs::write(&config_path, config).unwrap();
    let secret_path = data_home.path().join("decapod/secrets.json");
    fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    // Deliberately malformed retired credentials cannot block initialization.
    fs::write(&secret_path, "retired-secret-sentinel").unwrap();
    let refresh = Command::new(resolve_decapod_bin())
        .args(["init", "--refresh"])
        .current_dir(project.path())
        .env("XDG_DATA_HOME", data_home.path())
        .env("TYPESAFE_API_KEY", "ignored-retired-key")
        .output()
        .expect("refresh");
    assert!(
        refresh.status.success(),
        "refresh init failed: {}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    let config = fs::read_to_string(project.path().join(".decapod/config.toml"))
        .expect("read refreshed project config");
    assert!(!config.contains("[decision]"));
    assert!(!config.contains("ignored-retired-key"));
    assert_eq!(
        fs::read_to_string(secret_path).unwrap(),
        "retired-secret-sentinel"
    );
}

#[test]
fn init_refresh_preserves_explicit_vega_installation_without_executing_it() {
    let project = tempdir().expect("project tempdir");
    let first = run_decapod(project.path(), &["init", "with", "--force"]);
    assert!(first.status.success());
    let config_path = project.path().join(".decapod/config.toml");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config.push_str(&format!(
        "\n[vega]\nmode = \"required\"\nalpha = \"0.05\"\nrisk = \"0.1\"\n\
         [vega.installation]\ndirectory = \"/unavailable-model\"\n\
         manifest_digest = \"sha256:{}\"\nruntime = \"/unavailable-worker\"\n\
         runtime_sha256 = \"sha256:{}\"\n",
        "a".repeat(64),
        "b".repeat(64)
    ));
    let before: toml::Value = toml::from_str(&config).unwrap();
    fs::write(&config_path, config).unwrap();
    let refresh = run_decapod(project.path(), &["init", "--refresh"]);
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    let after: toml::Value = toml::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
    assert_eq!(before["vega"], after["vega"]);
}

#[test]
fn init_refresh_requires_an_existing_project() {
    let project = tempdir().expect("project tempdir");
    let refresh = run_decapod(project.path(), &["init", "--refresh"]);
    assert!(!refresh.status.success());
    let stderr = String::from_utf8_lossy(&refresh.stderr);
    assert!(
        stderr.contains("Cannot refresh an uninitialized project"),
        "{stderr}"
    );
}

#[test]
fn init_cloud_opt_in_does_not_store_secret_environment_values() {
    let tmp = tempdir().expect("tempdir");
    let out = Command::new(resolve_decapod_bin())
        .args(["init", "with", "--backend", "cloud", "--force"])
        .current_dir(tmp.path())
        .env("SUPABASE_URL", "https://private.supabase.local")
        .env("SUPABASE_KEY", "super-secret-service-role")
        .env("DECAPOD_ACCESS_TOKEN", "repo-config-must-not-store-this")
        .output()
        .expect("run decapod");

    assert!(
        out.status.success(),
        "decapod init with secret-like env failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = tmp.path().join(".decapod/config.toml");
    let config = fs::read_to_string(config_path).expect("read config.toml");
    assert!(config.contains("backend = \"cloud\""));
    assert!(!config.contains("[cloud]"));
    assert!(!config.contains("api_url"));
    assert!(!config.contains("project_id"));
    assert!(!config.contains("repo_id"));
    assert!(!config.contains("private.supabase.local"));
    assert!(!config.contains("super-secret-service-role"));
    assert!(!config.contains("repo-config-must-not-store-this"));
    assert!(!config.contains("supabase_key"));
    assert!(!config.contains("access_token"));

    let registration = fs::read_to_string(
        tmp.path()
            .join(".decapod/managed/cloud/init-registration.json"),
    )
    .expect("read cloud init registration");
    assert!(!registration.contains("private.supabase.local"));
    assert!(!registration.contains("super-secret-service-role"));
    assert!(!registration.contains("repo-config-must-not-store-this"));
}

#[test]
fn session_acquire_uses_machine_local_config_not_repo_session_file() {
    let tmp = tempdir().expect("tempdir");
    let config_home = tempdir().expect("config home");
    let out = run_decapod(tmp.path(), &["init", "with", "--force"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = Command::new(resolve_decapod_bin())
        .args(["session", "acquire"])
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("DECAPOD_AGENT_ID", "machine-local-agent")
        .output()
        .expect("session acquire");
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );

    assert!(
        !tmp.path().join(".decapod/session_token").exists(),
        "repo-local session_token should not be created"
    );
    assert!(
        !tmp.path().join(".decapod/managed/sessions").exists(),
        "session credentials should not be written under repo generated state"
    );

    let machine_sessions = config_home.path().join("decapod/sessions");
    assert!(
        machine_sessions.exists(),
        "machine-local session directory missing"
    );
    let mut found_session = false;
    for project_entry in fs::read_dir(machine_sessions).expect("read machine sessions") {
        let project_entry = project_entry.expect("project session dir");
        for agent_entry in fs::read_dir(project_entry.path()).expect("read project session dir") {
            let agent_entry = agent_entry.expect("agent session file");
            if agent_entry.file_name() == "machine-local-agent.json" {
                found_session = true;
            }
        }
    }
    assert!(found_session, "machine-local agent session file missing");
}

#[test]
fn session_acquire_falls_back_to_workspace_when_machine_config_is_unusable() {
    let tmp = tempdir().expect("tempdir");
    let config_blocker = NamedTempFile::new().expect("config blocker");
    let out = run_decapod(tmp.path(), &["init", "with", "--force"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = Command::new(resolve_decapod_bin())
        .args(["session", "acquire"])
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", config_blocker.path())
        .env("DECAPOD_AGENT_ID", "workspace-fallback-agent")
        .output()
        .expect("session acquire");
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    assert!(
        String::from_utf8_lossy(&acquire.stderr).contains("workspace-local session state"),
        "fallback diagnostic missing: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );

    let fallback_session = tmp
        .path()
        .join(".decapod/managed/sessions/workspace-fallback-agent.json");
    assert!(
        fallback_session.is_file(),
        "workspace-local session file missing"
    );
    assert!(
        !config_blocker.path().join("decapod").exists(),
        "unusable machine config path should not be replaced"
    );

    let status = Command::new(resolve_decapod_bin())
        .args(["session", "status"])
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", config_blocker.path())
        .env("DECAPOD_AGENT_ID", "workspace-fallback-agent")
        .output()
        .expect("session status");
    assert!(
        status.status.success(),
        "session status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("Session active"),
        "fallback session was not readable: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}

#[test]
fn init_project_dir_creates_directory_and_initializes_inside_it() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "--project-dir",
            "pincher",
            "--product-name",
            "pincher",
            "--force",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init --project-dir failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let project = tmp.path().join("pincher");
    assert!(project.is_dir(), "expected project directory to be created");
    assert!(
        project.join(".decapod/config.toml").exists(),
        "expected .decapod/config.toml in project directory"
    );
    assert!(
        !tmp.path().join(".decapod").exists(),
        "parent directory should not be initialized"
    );
}

#[test]
fn init_with_project_dir_creates_directory_and_initializes_inside_it() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--project-dir",
            "pincher-with",
            "--product-summary",
            "Initialize a named project directory.",
            "--force",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init with --project-dir failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let project = tmp.path().join("pincher-with");
    assert!(project.is_dir(), "expected project directory to be created");
    let intent = fs::read_to_string(project.join(".decapod/managed/specs/INTENT.md"))
        .expect("read .decapod/managed/specs/INTENT.md");
    assert!(
        intent.contains("Initialize a named project directory."),
        "intent spec should be written under the created project directory"
    );
}

#[test]
fn init_uses_existing_config_for_noninteractive_defaults() {
    let tmp = tempdir().expect("tempdir");
    let product_summary = "Recipe Kit helps home cooks scale ingredient quantities.";
    let out1 = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--force",
            "--diagram-style",
            "mermaid",
            "--product-summary",
            product_summary,
        ],
    );
    assert!(
        out1.status.success(),
        "initial init failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );

    let out2 = run_decapod(tmp.path(), &["init", "--force"]);
    assert!(
        out2.status.success(),
        "base init should succeed with existing config: {}",
        String::from_utf8_lossy(&out2.stderr)
    );

    let architecture =
        fs::read_to_string(tmp.path().join(".decapod/managed/specs/ARCHITECTURE.md"))
            .expect("read .decapod/managed/specs/ARCHITECTURE.md");
    assert!(
        architecture.contains("```mermaid"),
        "existing config should keep mermaid diagram style"
    );

    let intent = fs::read_to_string(tmp.path().join(".decapod/managed/specs/INTENT.md"))
        .expect("read .decapod/managed/specs/INTENT.md");
    assert!(
        !intent.contains("Define the user-visible outcome in one paragraph."),
        "re-init should preserve intent-first seeded outcome"
    );
    assert!(
        intent.contains(product_summary),
        "re-init should preserve the explicit whole-project outcome"
    );
    let config =
        fs::read_to_string(tmp.path().join(".decapod/config.toml")).expect("read preserved config");
    assert!(config.contains(product_summary));
}

#[test]
fn init_with_proof_bypasses_interaction_and_initializes_cwd() {
    let tmp = tempdir().expect("tempdir");
    // Ensure it's a git repo so init works correctly
    let _ = std::process::Command::new("git")
        .arg("init")
        .current_dir(tmp.path())
        .output()
        .expect("git init");

    let out = run_decapod(tmp.path(), &["init", "--proof"]);
    assert!(
        out.status.success(),
        "decapod init --proof failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(tmp.path().join(".decapod").is_dir());
    assert!(tmp.path().join("AGENTS.md").is_file());
}

#[test]
fn init_with_accepts_noninteractive_spec_seed_flags() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--force",
            "--product-name",
            "pincher",
            "--product-summary",
            "Track brokerage intents with deterministic proofs.",
            "--architecture-direction",
            "Broker-gated mutation path with deterministic context capsules.",
            "--done-criteria",
            "validate passes and proofs are green",
            "--primary-language",
            "rust,sql",
            "--surface",
            "backend,cli",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init with flags failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let intent = fs::read_to_string(tmp.path().join(".decapod/managed/specs/INTENT.md"))
        .expect("read intent");
    assert!(
        intent.contains("Track brokerage intents with deterministic proofs."),
        "intent spec should include seeded summary"
    );
    let architecture =
        fs::read_to_string(tmp.path().join(".decapod/managed/specs/ARCHITECTURE.md"))
            .expect("read architecture");
    assert!(
        architecture.contains("Broker-gated mutation path with deterministic context capsules."),
        "architecture spec should include seeded architecture direction"
    );
}

#[test]
fn init_with_architecture_seeds_ideal_language_when_unspecified() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--force",
            "--architecture-direction",
            "microservice",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init with architecture failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config =
        fs::read_to_string(tmp.path().join(".decapod/config.toml")).expect("read config.toml");
    assert!(
        config.contains("primary_languages = [\"Go\"]"),
        "microservice architecture should seed Go as the default language: {config}"
    );
}

#[test]
fn init_with_architecture_can_recommend_zig() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--force",
            "--architecture-direction",
            "embedded systems",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init with embedded architecture failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config =
        fs::read_to_string(tmp.path().join(".decapod/config.toml")).expect("read config.toml");
    assert!(
        config.contains("primary_languages = [\"Zig\"]"),
        "embedded systems architecture should seed Zig as the default language: {config}"
    );
}

#[test]
fn init_with_mixed_scripts_repo_uses_file_inference_noninteractively() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("task.py"), "print('ok')\n").expect("python fixture");
    fs::write(tmp.path().join("deploy.sh"), "#!/usr/bin/env bash\n").expect("shell fixture");
    fs::write(tmp.path().join("env.zsh"), "printenv\n").expect("zsh fixture");
    fs::write(tmp.path().join("tool.ts"), "export const ok = true;\n").expect("ts fixture");
    fs::write(tmp.path().join("probe.go"), "package main\n").expect("go fixture");

    let out = run_decapod(tmp.path(), &["init", "with", "--force"]);
    assert!(
        out.status.success(),
        "decapod init with mixed scripts repo failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config =
        fs::read_to_string(tmp.path().join(".decapod/config.toml")).expect("read config.toml");
    assert!(config.contains("\"go\""), "expected Go inference: {config}");
    assert!(
        config.contains("\"python\""),
        "expected Python inference: {config}"
    );
    assert!(
        config.contains("\"shell\""),
        "expected shell inference: {config}"
    );
    assert!(
        config.contains("\"typescript\""),
        "expected TypeScript inference: {config}"
    );
    assert!(
        !config.contains("primary_languages = [\"Rust\"]"),
        "mixed scripts repo should not collapse to Rust: {config}"
    );
}

#[test]
fn init_with_accepts_noninteractive_spec_seed_env() {
    let tmp = tempdir().expect("tempdir");
    let out = Command::new(resolve_decapod_bin())
        .args(["init", "with", "--force"])
        .current_dir(tmp.path())
        .env("DECAPOD_INIT_PRODUCT_NAME", "pincher-env")
        .env(
            "DECAPOD_INIT_PRODUCT_SUMMARY",
            "Seed from env for non-interactive init.",
        )
        .env(
            "DECAPOD_INIT_ARCHITECTURE_DIRECTION",
            "Capsule-first architecture with broker-enforced writes.",
        )
        .output()
        .expect("run decapod");
    assert!(
        out.status.success(),
        "decapod init with env failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let intent = fs::read_to_string(tmp.path().join(".decapod/managed/specs/INTENT.md"))
        .expect("read intent");
    assert!(
        intent.contains("Seed from env for non-interactive init."),
        "intent spec should include env-seeded summary"
    );
    let architecture =
        fs::read_to_string(tmp.path().join(".decapod/managed/specs/ARCHITECTURE.md"))
            .expect("read architecture");
    assert!(
        architecture.contains("Capsule-first architecture with broker-enforced writes."),
        "architecture spec should include env-seeded architecture direction"
    );
}

#[test]
fn init_blends_existing_agent_entrypoints_into_override_md() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path();

    // 1. Create a custom AGENTS.md
    let custom_agents_content =
        "# Custom Agents\n\nThis is my custom agent configuration.\n- Agent X\n- Agent Y";
    fs::write(repo_dir.join("AGENTS.md"), custom_agents_content).expect("write AGENTS.md");

    // 2. Run decapod init (without --force, as it's a fresh repo)
    let out = run_decapod(repo_dir, &["init"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 3. Check if AGENTS.md is overwritten by template
    let new_agents_content =
        fs::read_to_string(repo_dir.join("AGENTS.md")).expect("read new AGENTS.md");
    assert!(
        new_agents_content.contains("Universal Agent Contract"),
        "AGENTS.md should be overwritten by template"
    );
    assert!(
        !new_agents_content.contains("Custom Agents"),
        "AGENTS.md should not contain custom content anymore"
    );

    // 4. Check if custom content is in .bak (for agent to process)
    let bak_path = repo_dir.join("AGENTS.md.bak");
    assert!(
        bak_path.exists(),
        "AGENTS.md.bak should exist for agent processing"
    );
    let bak_content = fs::read_to_string(&bak_path).expect("read AGENTS.md.bak");
    assert!(
        bak_content.contains("Custom Agents"),
        "AGENTS.md.bak should contain custom content"
    );
    assert!(
        bak_content.contains("Agent X"),
        "AGENTS.md.bak should contain Agent X"
    );
}

#[test]
fn init_blends_all_agent_entrypoints_when_forced() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path();

    // Create custom entrypoints
    fs::write(repo_dir.join("CLAUDE.md"), "# Custom Claude").expect("write CLAUDE.md");
    fs::write(repo_dir.join("GEMINI.md"), "# Custom Gemini").expect("write GEMINI.md");
    fs::write(repo_dir.join("CODEX.md"), "# Custom Codex").expect("write CODEX.md");

    let out = run_decapod(repo_dir, &["init", "--force", "--all"]);
    assert!(out.status.success(), "decapod init failed");

    // Legacy content stays in .bak files for agent to process
    // Agent calls get_legacy_entrypoint_contents() to retrieve and manually blend
    assert!(
        repo_dir.join("CLAUDE.md.bak").exists(),
        "CLAUDE.md.bak should exist for agent"
    );
    assert!(
        repo_dir.join("GEMINI.md.bak").exists(),
        "GEMINI.md.bak should exist for agent"
    );
    assert!(
        repo_dir.join("CODEX.md.bak").exists(),
        "CODEX.md.bak should exist for agent"
    );

    // Verify .bak files contain the custom content
    let claude_bak =
        fs::read_to_string(repo_dir.join("CLAUDE.md.bak")).expect("read CLAUDE.md.bak");
    assert!(claude_bak.contains("# Custom Claude"));
}

#[test]
fn init_with_claude_only_adopts_it_and_generates_all_four_entrypoints() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path();

    // 1. Create only CLAUDE.md
    fs::write(repo_dir.join("CLAUDE.md"), "# Original Claude Intent").expect("write CLAUDE.md");

    // 2. Run decapod init
    let out = run_decapod(repo_dir, &["init"]);
    assert!(out.status.success(), "decapod init failed");

    // 3. Verify ALL four entrypoints now exist
    assert!(repo_dir.join("AGENTS.md").exists());
    assert!(repo_dir.join("CLAUDE.md").exists());
    assert!(repo_dir.join("GEMINI.md").exists());
    assert!(repo_dir.join("CODEX.md").exists());

    // 4. Verify CLAUDE.md content is the template
    let new_claude = fs::read_to_string(repo_dir.join("CLAUDE.md")).expect("read new CLAUDE.md");
    assert!(new_claude.contains("Agent Entrypoint"));
    assert!(!new_claude.contains("Original Claude Intent"));

    // 5. Verify CLAUDE.md is in .bak for agent processing
    let bak_path = repo_dir.join("CLAUDE.md.bak");
    assert!(bak_path.exists(), "CLAUDE.md.bak should exist for agent");
    let bak_content = fs::read_to_string(&bak_path).expect("read CLAUDE.md.bak");
    assert!(bak_content.contains("# Original Claude Intent"));
}

#[test]
fn init_creates_custody_directory_and_intent_has_epistemic_custody_fields() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "with", "--force"]);
    assert!(
        out.status.success(),
        "decapod init with failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let custody_dir = tmp.path().join(".decapod/managed/artifacts/custody");
    assert!(
        custody_dir.exists(),
        "expected .decapod/managed/artifacts/custody/ directory to exist"
    );
    let intent = fs::read_to_string(tmp.path().join(".decapod/managed/specs/INTENT.md"))
        .expect("read INTENT.md");
    assert!(
        intent.contains("## Epistemic Custody Fields"),
        "INTENT.md should contain Epistemic Custody Fields section"
    );
    assert!(
        intent.contains("### Active Assumptions"),
        "INTENT.md should contain Active Assumptions subsection"
    );
    assert!(
        intent.contains("### Measured vs Inferred Facts"),
        "INTENT.md should contain Measured vs Inferred Facts subsection"
    );
    assert!(
        intent.contains("### Unresolved Contradictions"),
        "INTENT.md should contain Unresolved Contradictions subsection"
    );
    assert!(
        intent.contains("### Deferred Questions"),
        "INTENT.md should contain Deferred Questions subsection"
    );
    assert!(
        intent.contains("### Stop Conditions"),
        "INTENT.md should contain Stop Conditions subsection"
    );
    assert!(
        intent.contains("### Proof Required Before Completion"),
        "INTENT.md should contain Proof Required Before Completion subsection"
    );
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn agents_md_contains_epistemic_custody_section() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "with", "--force", "--all"]);
    assert!(
        out.status.success(),
        "decapod init with failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let agents_md = fs::read_to_string(tmp.path().join("AGENTS.md")).expect("read AGENTS.md");
    assert!(
        agents_md.contains("## Epistemic Custody"),
        "AGENTS.md should contain Epistemic Custody section"
    );
    assert!(
        agents_md.contains("**Epistemic custody** is the preserved chain"),
        "AGENTS.md should define epistemic custody"
    );
    assert!(
        agents_md.contains("| Term | Meaning |"),
        "AGENTS.md should contain epistemic custody vocabulary table"
    );
    assert!(
        agents_md.contains("## Custody artifacts"),
        "AGENTS.md should describe custody artifacts directory"
    );
}

#[test]
fn init_preserves_manually_added_custody_fields_in_intent_md() {
    let tmp = tempdir().expect("tempdir");
    // 1. Initial init
    run_decapod(tmp.path(), &["init", "with", "--force"]);

    let intent_path = tmp.path().join(".decapod/managed/specs/INTENT.md");
    let mut intent_content = fs::read_to_string(&intent_path).expect("read intent");

    // 2. Manually add an assumption
    intent_content = intent_content.replace(
        "### Active Assumptions\n- [ ] List any assumptions made to proceed.",
        "### Active Assumptions\n- [ ] List any assumptions made to proceed.\n- [ ] MANUALLY_ADDED_ASSUMPTION"
    );
    fs::write(&intent_path, intent_content).expect("write modified intent");

    // 3. Re-init
    run_decapod(tmp.path(), &["init", "--force"]);

    // 4. Verify assumption is still there
    let re_init_intent = fs::read_to_string(&intent_path).expect("read re-init intent");
    assert!(
        re_init_intent.contains("MANUALLY_ADDED_ASSUMPTION"),
        "re-init should preserve manually added assumptions in INTENT.md"
    );
}

#[test]
fn init_supports_and_preserves_declared_capabilities() {
    // Check scaffolded capability prerequisites without asserting source/release proof.
    // This prerequisite result is not full publication-readiness proof.

    let tmp = tempdir().expect("tempdir");

    // 1. Init with custom declared capabilities
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--product-summary",
            "Job Ledger stores scheduled work and records its outcomes.",
            "--declared-capability",
            "persistent-state",
            "--declared-capability",
            "scheduled-jobs",
            "--force",
            "--no-git",
        ],
    );
    assert!(
        out.status.success(),
        "decapod init with custom capabilities failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = tmp.path().join(".decapod/config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config");
    assert!(
        config_content.contains("persistent-state") && config_content.contains("scheduled-jobs"),
        "config should contain custom declared capabilities: {}",
        config_content
    );

    // Verify manifest has them
    let manifest_path = tmp.path().join(".decapod/managed/specs/.manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).expect("read manifest");
    assert!(
        manifest_content.contains("persistent-state"),
        "manifest should contain persistent-state capability: {}",
        manifest_content
    );
    assert!(
        manifest_content.contains("scheduled-jobs"),
        "manifest should contain scheduled-jobs capability: {}",
        manifest_content
    );

    // 2. Re-init with --force, and verify it PRESERVES them
    let out = run_decapod(tmp.path(), &["init", "--force", "--no-git"]);
    assert!(
        out.status.success(),
        "decapod init --force failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_content = fs::read_to_string(&config_path).expect("read config");
    assert!(
        config_content.contains("persistent-state"),
        "config should preserve persistent-state capability after force re-init: {}",
        config_content
    );

    assert!(
        config_content.contains("Job Ledger stores scheduled work and records its outcomes."),
        "re-init should preserve the explicit product purpose alongside capabilities"
    );

    // 3. Verify validation passes successfully
    let out = run_decapod(tmp.path(), &["govern", "project-check"]);
    assert!(
        out.status.success(),
        "decapod validate failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn init_persists_guided_governance_and_reuses_it_on_refresh() {
    let tmp = tempdir().expect("tempdir");
    fs::write(tmp.path().join("README.md"), "# project\n").expect("write readme");

    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--protected-path",
            "README.md",
            "--approval-category",
            "policy_changes",
            "--isolation-mode",
            "worktree",
            "--tracker-provider",
            "beads",
            "--tracker-project",
            "project-1",
            "--tracker-url",
            "https://tracker.invalid/project-1",
            "--declared-context-source",
            "README.md",
            "--proof-command",
            "lint=cargo test --lib",
            "--force",
        ],
    );
    assert!(
        out.status.success(),
        "guided init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = tmp.path().join(".decapod/config.toml");
    let config = fs::read_to_string(&config_path).expect("read config");
    assert!(config.contains("[governance]"));
    assert!(config.contains("protected_paths = [\"README.md\"]"));
    assert!(config.contains("approval_categories = [\"policy_changes\"]"));
    assert!(config.contains("isolation = \"worktree\""));
    assert!(config.contains("provider = \"beads\""));
    assert!(config.contains("declared_sources = [\"README.md\"]"));
    assert!(config.contains("name = \"lint\""));

    let refresh = run_decapod(tmp.path(), &["init", "--force"]);
    assert!(
        refresh.status.success(),
        "guided init refresh failed: {}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    let refreshed = fs::read_to_string(config_path).expect("read refreshed config");
    assert!(refreshed.contains("provider = \"beads\""));
    assert!(refreshed.contains("name = \"lint\""));
    assert!(refreshed.contains("declared_sources = [\"README.md\"]"));
}

#[test]
fn init_validation_rejects_traversal_in_guided_paths() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &[
            "init",
            "with",
            "--protected-path",
            "../outside",
            "--force",
            "--no-git",
        ],
    );
    assert!(
        out.status.success(),
        "init should preserve noninteractive behavior"
    );

    let validate = run_decapod(tmp.path(), &["validate", "--format", "json"]);
    assert!(
        !validate.status.success(),
        "validate must reject traversal paths: {}",
        String::from_utf8_lossy(&validate.stdout)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&validate.stdout).expect("structured validation report");
    let failures = report["report"]["failures"]
        .as_array()
        .expect("complete validation failure list");
    assert!(
        failures.iter().any(|failure| failure
            .as_str()
            .is_some_and(|message| message.contains("Invalid guided init protected path:"))),
        "traversal rejection missing from structured failures: {failures:?}"
    );
}

fn read_init_proof_config(dir: &std::path::Path) -> toml::Value {
    let raw = fs::read_to_string(dir.join(".decapod/config.toml")).expect("read config");
    let config: toml::Value = toml::from_str(&raw).expect("parse config");
    config["proof"].clone()
}

fn assert_init_success(dir: &std::path::Path, args: &[&str]) {
    let output = run_decapod(dir, args);
    assert!(
        output.status.success(),
        "init {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_buildkite_preflights_defaults_and_overrides_on_both_surfaces() {
    for surface in [vec!["init"], vec!["init", "with"]] {
        let project = tempdir().expect("project");
        let mut args = surface.clone();
        args.extend(["--proof", "--force"]);
        assert_init_success(project.path(), &args);
        assert_eq!(
            read_init_proof_config(project.path())["validation_buildkite_preflights"].as_bool(),
            Some(false)
        );
        for value in [true, false] {
            let flag = format!("--validation-buildkite-preflights={value}");
            let mut override_args = args.clone();
            override_args.push(&flag);
            assert_init_success(project.path(), &override_args);
            assert_eq!(
                read_init_proof_config(project.path())["validation_buildkite_preflights"].as_bool(),
                Some(value)
            );
            // Unset is not an explicit false, including on init with.
            assert_init_success(project.path(), &args);
            assert_eq!(
                read_init_proof_config(project.path())["validation_buildkite_preflights"].as_bool(),
                Some(value)
            );
        }
    }
}

#[test]
fn init_refresh_preserves_buildkite_provider_and_structured_proofs() {
    for args in [
        vec!["init", "--refresh", "--force"],
        vec!["init", "with", "--force"],
    ] {
        let project = tempdir().expect("project");
        assert_init_success(
            project.path(),
            &[
                "init",
                "with",
                "--validation-buildkite-preflights",
                "--force",
            ],
        );
        let path = project.path().join(".decapod/config.toml");
        let raw = fs::read_to_string(&path).expect("read config");
        let mut config: toml::Value = toml::from_str(&raw).expect("parse config");
        config["proof"]["commands"] = toml::Value::Array(vec![
            toml::Value::try_from(serde_json::json!({
                "name": "optional",
                "command": "cargo",
                "args": ["test", "--lib", "argument with spaces,commas=equals"],
                "description": "Keep this authored description",
                "required": false
            }))
            .expect("proof definition"),
            toml::Value::try_from(serde_json::json!({
                "name": "required",
                "command": "printf 'a,b=c'",
                "args": [],
                "description": "Required proof",
                "required": true
            }))
            .expect("required proof definition"),
        ]);
        let expected = config["proof"].clone();
        fs::write(&path, toml::to_string_pretty(&config).unwrap())
            .expect("write structured config");
        assert_init_success(project.path(), &args);
        assert_eq!(read_init_proof_config(project.path()), expected);
    }
}

#[test]
fn init_refresh_upgrades_legacy_proof_config_to_native_default() {
    let project = tempdir().expect("project");
    assert_init_success(project.path(), &["init", "with", "--force"]);
    let path = project.path().join(".decapod/config.toml");
    let raw = fs::read_to_string(&path).expect("read config");
    let mut config: toml::Value = toml::from_str(&raw).expect("parse config");
    config["proof"]
        .as_table_mut()
        .unwrap()
        .remove("validation_buildkite_preflights");
    fs::write(&path, toml::to_string_pretty(&config).unwrap()).expect("write legacy config");
    assert_init_success(project.path(), &["init", "--refresh", "--force"]);
    assert_eq!(
        read_init_proof_config(project.path())["validation_buildkite_preflights"].as_bool(),
        Some(false)
    );
}

#[test]
fn both_init_surfaces_preserve_authored_config_and_accurate_dry_run() {
    for prefix in [vec!["init"], vec!["init", "with"], vec!["init", "wtih"]] {
        let project = tempdir().expect("project tempdir");
        fs::write(project.path().join("README.md"), "# Authored consumer\n").unwrap();
        let first = run_decapod(
            project.path(),
            &[
                "init",
                "with",
                "--all",
                "--no-specs",
                "--no-ci",
                "--tracker-provider",
                "beads",
                "--tracker-project",
                "consumer",
                "--tracker-url",
                "https://tracker.invalid/consumer",
                "--diagram-style",
                "mermaid",
                "--no-container-workspaces",
                "--isolation-mode",
                "worktree",
                "--protected-path",
                "README.md",
                "--approval-category",
                "policy_changes",
                "--declared-context-source",
                "README.md",
                "--proof-command",
                "lint=echo checked",
            ],
        );
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        let path = project.path().join(".decapod/config.toml");
        let mut authored = fs::read_to_string(&path).unwrap();
        authored.push_str("\n# Preserve this authored extension and comment.\n[consumer_extension]\nowner = 'consumer-team'\n");
        authored.push_str(&format!(
            "\n[proof.repository_contract]\nreviewed_commands_digest = 'sha256:{}'\nsource_coverage = []\nexecutions = {{}}\n",
            "0".repeat(64)
        ));
        fs::write(&path, &authored).unwrap();
        let mut args = prefix.clone();
        args.extend(["--all", "--force", "--no-specs", "--no-ci", "--dry-run"]);
        let dry = run_decapod(project.path(), &args);
        assert!(
            dry.status.success(),
            "{}",
            String::from_utf8_lossy(&dry.stderr)
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            authored,
            "dry-run mutated config"
        );
        assert!(String::from_utf8_lossy(&dry.stdout).contains("config.toml: unchanged"));
        assert!(
            String::from_utf8_lossy(&dry.stdout)
                .contains("Command contract digest (review aid; not approved): sha256:")
        );
        args.pop();
        let apply = run_decapod(project.path(), &args);
        assert!(
            apply.status.success(),
            "{}",
            String::from_utf8_lossy(&apply.stderr)
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            authored,
            "refresh must preserve every authored setting and no-op bytes: {prefix:?}"
        );
    }
}

#[test]
fn both_init_surfaces_apply_only_explicit_config_overrides() {
    for prefix in [vec!["init"], vec!["init", "with"]] {
        let project = tempdir().expect("project tempdir");
        let first = run_decapod(
            project.path(),
            &[
                "init",
                "with",
                "--all",
                "--no-specs",
                "--no-ci",
                "--tracker-provider",
                "beads",
                "--tracker-project",
                "consumer",
                "--tracker-url",
                "https://tracker.invalid/consumer",
                "--diagram-style",
                "mermaid",
            ],
        );
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        let path = project.path().join(".decapod/config.toml");
        let mut authored: toml::Value =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        // Neither a default-valued backend option nor a default-valued diagram
        // style may be confused with omission. Avoid any cloud execution here.
        authored["repo"]["backend"] = "cloud".into();
        authored["init"]["ci"] = true.into();
        authored["init"]["specs"] = true.into();
        authored.as_table_mut().unwrap().insert(
            "consumer_extension".into(),
            toml::Value::try_from(serde_json::json!({"owner":"consumer-team"})).unwrap(),
        );
        fs::write(&path, toml::to_string_pretty(&authored).unwrap()).unwrap();
        let before = fs::read_to_string(&path).unwrap();
        let mut args = prefix.clone();
        args.extend([
            "--force",
            "--no-specs",
            "--no-ci",
            "--tracker-provider",
            "decapod",
            "--diagram-style",
            "ascii",
            "--backend",
            "local",
            "--no-container-workspaces",
            "--dry-run",
        ]);
        let dry = run_decapod(project.path(), &args);
        assert!(
            dry.status.success(),
            "{}",
            String::from_utf8_lossy(&dry.stderr)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert!(String::from_utf8_lossy(&dry.stdout).contains("config.toml: would update"));
        args.pop();
        let apply = run_decapod(project.path(), &args);
        assert!(
            apply.status.success(),
            "{}",
            String::from_utf8_lossy(&apply.stderr)
        );
        let actual: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        authored["repo"]["backend"] = "local".into();
        authored["repo"]["external_tracker"] = false.into();
        authored["repo"]["container_workspaces"] = false.into();
        authored["tracker"]["provider"] = "decapod".into();
        authored["init"]["diagram_style"] = "ascii".into();
        authored["init"]["ci"] = false.into();
        authored["init"]["specs"] = false.into();
        assert_eq!(
            actual, authored,
            "unrequested config changes for {prefix:?}"
        );
    }
}
