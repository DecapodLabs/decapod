use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo(tmp: &TempDir) -> (String, String) {
    let dir = tmp.path();
    Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "master"])
        .output()
        .unwrap();

    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--product-name",
            "IntegrityTest",
            "--product-summary",
            "Original Summary",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "test-agent")],
    );
    assert!(
        acquire.status.success(),
        "acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );

    let stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = stdout
        .lines()
        .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
        .unwrap();

    (password, "test-agent".to_string())
}

#[test]
fn test_decapod_uses_config_toml_for_validation() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let (password, agent_id) = setup_repo(&tmp);

    let envs = [
        ("DECAPOD_AGENT_ID", agent_id.as_str()),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
    ];

    // 1. Initial validate should pass
    let out = run_decapod(dir, &["validate"], &envs);
    assert!(
        out.status.success(),
        "Initial validate should pass, output: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // 2. Corrupt config.toml
    let config_path = dir.join(".decapod").join("config.toml");
    fs::write(&config_path, "this is not toml").unwrap();

    let out = run_decapod(dir, &["validate"], &envs);
    assert!(
        !out.status.success(),
        "Validate should fail with corrupted config.toml"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("fail="),
        "Should report failure in stdout"
    );

    // 3. Restore config.toml but change schema version
    fs::write(
        &config_path,
        "schema_version = \"9.9.9\"\n[repo]\nproduct_name = \"Test\"",
    )
    .unwrap();
    let out = run_decapod(dir, &["validate"], &envs);
    assert!(
        !out.status.success(),
        "Validate should fail with wrong schema version"
    );
}

#[test]
fn test_decapod_init_regenerates_from_templates() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let (password, agent_id) = setup_repo(&tmp);

    let envs = [
        ("DECAPOD_AGENT_ID", agent_id.as_str()),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
    ];

    let agents_path = dir.join("AGENTS.md");
    fs::remove_file(&agents_path).unwrap();

    // Run init to regenerate
    let out = run_decapod(dir, &["init", "with", "--agents", "--force"], &envs);
    assert!(out.status.success(), "Init should regenerate AGENTS.md");

    let content = fs::read_to_string(&agents_path).unwrap();
    assert!(
        content.contains("External task managers may be used for user workflow")
            && content.contains("Decapod todos are still mandatory"),
        "Regenerated AGENTS.md should use updated template. Content was:\n{}",
        content
    );
}

#[test]
fn test_config_toml_changes_flow_to_specs() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let (password, agent_id) = setup_repo(&tmp);

    let envs = [
        ("DECAPOD_AGENT_ID", agent_id.as_str()),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
    ];

    // 1. Change product name and summary in config.toml
    let config_path = dir.join(".decapod").join("config.toml");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config = config
        .replace("IntegrityTest", "NewProduct")
        .replace("Original Summary", "New Summary");
    fs::write(&config_path, config).unwrap();

    // 2. Run init --force (should load config.toml)
    let out = run_decapod(dir, &["init", "--force"], &envs);
    assert!(out.status.success(), "Init --force should succeed");

    // 3. Check INTENT.md
    let intent_path = dir
        .join(".decapod")
        .join("generated")
        .join("specs")
        .join("INTENT.md");
    let intent_content = fs::read_to_string(&intent_path).unwrap();
    assert!(
        intent_content.contains("New Summary"),
        "INTENT.md should be updated with new summary from config.toml. Content was:\n{}",
        intent_content
    );
}

#[test]
fn test_override_md_changes_flow_to_specs() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let (password, agent_id) = setup_repo(&tmp);

    let envs = [
        ("DECAPOD_AGENT_ID", agent_id.as_str()),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
    ];

    // 1. Add override for INTENT in OVERRIDE.md
    let override_path = dir.join(".decapod").join("OVERRIDE.md");
    let mut override_content = fs::read_to_string(&override_path).unwrap();
    override_content = override_content.replace(
        "### specs/INTENT.md",
        "### specs/INTENT.md\n\nThis intent comes from OVERRIDE.md",
    );
    fs::write(&override_path, &override_content).unwrap();

    // 2. Run init --force
    let out = run_decapod(dir, &["init", "--force"], &envs);
    assert!(out.status.success(), "Init --force should succeed");

    // 3. Check INTENT.md
    let intent_path = dir
        .join(".decapod")
        .join("generated")
        .join("specs")
        .join("INTENT.md");
    let intent_content = fs::read_to_string(&intent_path).unwrap();
    assert!(
        intent_content.contains("This intent comes from OVERRIDE.md"),
        "INTENT.md should be updated with intent from OVERRIDE.md. Content was:\n{}",
        intent_content
    );
}

#[test]
fn test_regression_against_known_good_substrate() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "master"])
        .output()
        .unwrap();

    // 1. Manually craft a "known good" .decapod directory
    let decapod_dir = dir.join(".decapod");
    fs::create_dir_all(&decapod_dir).unwrap();

    let config_toml = r#"schema_version = "1.0.0"
[init]
specs = true
diagram_style = "mermaid"
entrypoints = ["AGENTS.md"]

[repo]
product_name = "RegressionTest"
product_summary = "A summary that must be preserved."
architecture_direction = "Architecture that must be preserved."
product_type = "service_or_library"
done_criteria = "Criteria that must be preserved."
primary_languages = ["rust"]
detected_surfaces = ["cargo"]
"#;
    fs::write(decapod_dir.join("config.toml"), config_toml).unwrap();

    let override_md = r#"# OVERRIDE.md
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
### specs/INTENT.md
This intent from OVERRIDE must win.
"#;
    fs::write(decapod_dir.join("OVERRIDE.md"), override_md).unwrap();

    // 2. Run session acquire to get a valid session
    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "smoke-agent")],
    );
    let stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = stdout
        .lines()
        .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
        .unwrap();

    let envs = [
        ("DECAPOD_AGENT_ID", "smoke-agent"),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
    ];

    // 3. Validate should pass against this substrate
    let out = run_decapod(dir, &["validate"], &envs);
    assert!(
        out.status.success(),
        "Validate should pass against known-good substrate. Output: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // 4. Init --force should respect these files
    let out = run_decapod(dir, &["init", "--force"], &envs);
    assert!(out.status.success(), "Init --force should succeed");

    // 5. Verify INTENT.md used the OVERRIDE
    let intent_path = decapod_dir
        .join("generated")
        .join("specs")
        .join("INTENT.md");
    let intent_content = fs::read_to_string(&intent_path).unwrap();
    assert!(
        intent_content.contains("This intent from OVERRIDE must win"),
        "Regenerated spec should respect existing OVERRIDE.md"
    );

    // 6. Verify config.toml was not mangled (should still have our product name)
    let final_config = fs::read_to_string(decapod_dir.join("config.toml")).unwrap();
    assert!(
        final_config.contains("RegressionTest"),
        "config.toml should preserve product name"
    );
}
