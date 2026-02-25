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
