use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn run_decapod(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run decapod")
}

#[test]
fn init_with_writes_config_toml_with_schema_and_diagram_style() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(
        tmp.path(),
        &["init", "with", "--force", "--diagram-style", "mermaid"],
    );
    assert!(
        out.status.success(),
        "decapod init with failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = tmp.path().join(".decapod/config.toml");
    assert!(
        config_path.exists(),
        "expected .decapod/config.toml to exist"
    );
    let config = fs::read_to_string(config_path).expect("read config.toml");
    assert!(config.contains("schema_version = \"1.0.0\""));
    assert!(config.contains("diagram_style = \"mermaid\""));
    assert!(config.contains("[repo]"));
    assert!(config.contains("[init]"));
    assert!(config.contains("product_summary = "));
    assert!(config.contains("architecture_direction = "));

    let intent = fs::read_to_string(tmp.path().join(".decapod/generated/specs/INTENT.md"))
        .expect("read .decapod/generated/specs/INTENT.md");
    assert!(
        !intent.contains("Define the user-visible outcome in one paragraph."),
        "intent scaffold should be seeded with non-placeholder outcome"
    );
    let version_counter =
        fs::read_to_string(tmp.path().join(".decapod/generated/version_counter.json"))
            .expect("read .decapod/generated/version_counter.json");
    let version_counter: serde_json::Value =
        serde_json::from_str(&version_counter).expect("parse version_counter json");
    assert_eq!(version_counter["version_count"], 1);
    assert_eq!(version_counter["schema_version"], "1.0.0");
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
    let intent = fs::read_to_string(project.join(".decapod/generated/specs/INTENT.md"))
        .expect("read .decapod/generated/specs/INTENT.md");
    assert!(
        intent.contains("Initialize a named project directory."),
        "intent spec should be written under the created project directory"
    );
}

#[test]
fn init_uses_existing_config_for_noninteractive_defaults() {
    let tmp = tempdir().expect("tempdir");
    let out1 = run_decapod(
        tmp.path(),
        &["init", "with", "--force", "--diagram-style", "mermaid"],
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
        fs::read_to_string(tmp.path().join(".decapod/generated/specs/ARCHITECTURE.md"))
            .expect("read .decapod/generated/specs/ARCHITECTURE.md");
    assert!(
        architecture.contains("```mermaid"),
        "existing config should keep mermaid diagram style"
    );

    let intent = fs::read_to_string(tmp.path().join(".decapod/generated/specs/INTENT.md"))
        .expect("read .decapod/generated/specs/INTENT.md");
    assert!(
        !intent.contains("Define the user-visible outcome in one paragraph."),
        "re-init should preserve intent-first seeded outcome"
    );
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

    let intent = fs::read_to_string(tmp.path().join(".decapod/generated/specs/INTENT.md"))
        .expect("read intent");
    assert!(
        intent.contains("Track brokerage intents with deterministic proofs."),
        "intent spec should include seeded summary"
    );
    let architecture =
        fs::read_to_string(tmp.path().join(".decapod/generated/specs/ARCHITECTURE.md"))
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
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
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

    let intent = fs::read_to_string(tmp.path().join(".decapod/generated/specs/INTENT.md"))
        .expect("read intent");
    assert!(
        intent.contains("Seed from env for non-interactive init."),
        "intent spec should include env-seeded summary"
    );
    let architecture =
        fs::read_to_string(tmp.path().join(".decapod/generated/specs/ARCHITECTURE.md"))
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

    // 4. Check if custom content is in .decapod/OVERRIDE.md
    let override_path = repo_dir.join(".decapod/OVERRIDE.md");
    assert!(
        override_path.exists(),
        ".decapod/OVERRIDE.md should be created"
    );
    let override_content = fs::read_to_string(override_path).expect("read OVERRIDE.md");

    assert!(
        override_content.contains("Custom Agents"),
        "OVERRIDE.md should contain custom AGENTS.md content"
    );
    assert!(
        override_content.contains("This is my custom agent configuration."),
        "OVERRIDE.md should contain custom AGENTS.md content"
    );
    assert!(
        override_content.contains("Agent X"),
        "OVERRIDE.md should contain custom AGENTS.md content"
    );
    assert!(
        override_content.contains("## PENDING CONSOLIDATION (ADOPTED INTENT)"),
        "OVERRIDE.md should have a header for pending consolidation"
    );
    assert!(
        override_content.contains("### adoption/AGENTS.md"),
        "OVERRIDE.md should have a sub-header for adopted content"
    );
}

#[test]
fn init_blends_all_agent_entrypoints_when_forced() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path();

    fs::write(repo_dir.join("CLAUDE.md"), "# Custom Claude").expect("write CLAUDE.md");
    fs::write(repo_dir.join("GEMINI.md"), "# Custom Gemini").expect("write GEMINI.md");
    fs::write(repo_dir.join("CODEX.md"), "# Custom Codex").expect("write CODEX.md");

    let out = run_decapod(repo_dir, &["init", "--force", "--all"]);
    assert!(out.status.success(), "decapod init failed");

    let override_content =
        fs::read_to_string(repo_dir.join(".decapod/OVERRIDE.md")).expect("read OVERRIDE.md");

    assert!(override_content.contains("## PENDING CONSOLIDATION (ADOPTED INTENT)"));
    assert!(override_content.contains("### adoption/CLAUDE.md"));
    assert!(override_content.contains("# Custom Claude"));
    assert!(override_content.contains("### adoption/GEMINI.md"));
    assert!(override_content.contains("# Custom Gemini"));
    assert!(override_content.contains("### adoption/CODEX.md"));
    assert!(override_content.contains("# Custom Codex"));
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

    // 5. Verify adoption in OVERRIDE.md
    let override_content =
        fs::read_to_string(repo_dir.join(".decapod/OVERRIDE.md")).expect("read OVERRIDE.md");
    assert!(override_content.contains("## PENDING CONSOLIDATION (ADOPTED INTENT)"));
    assert!(override_content.contains("### adoption/CLAUDE.md"));
    assert!(override_content.contains("# Original Claude Intent"));
}
