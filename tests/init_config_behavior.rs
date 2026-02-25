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
