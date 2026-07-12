use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir)
        .args(args)
        .env("XDG_CONFIG_HOME", dir.join(".xdg-config"));
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let acquire_stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = acquire_stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password in output");

    (tmp, dir, password)
}

fn set_capabilities(dir: &Path, capabilities: &[&str]) {
    let config_path = dir.join(".decapod/config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config.toml");
    let mut config: toml::Value = toml::from_str(&config_content).expect("parse config.toml");
    let repo_table = config["repo"].as_table_mut().expect("repo table");
    repo_table.insert(
        "capabilities".to_string(),
        toml::Value::Array(
            capabilities
                .iter()
                .map(|capability| toml::Value::String((*capability).to_string()))
                .collect(),
        ),
    );
    fs::write(
        config_path,
        toml::to_string_pretty(&config).expect("serialize config"),
    )
    .expect("write config.toml");
}

fn run_rpc(dir: &Path, op: &str, params: &str, password: &str) -> std::process::Output {
    run_decapod(
        dir,
        &["rpc", "--op", op, "--params", params],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    )
}

fn run_validate(dir: &Path, password: &str, projections: bool) -> std::process::Output {
    let mut args = vec!["validate", "--format", "json"];
    if projections {
        args.push("--projections");
    }
    run_decapod(
        dir,
        &args,
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    )
}

fn get_valid_context_capsule(dir: &Path, password: &str) -> serde_json::Value {
    let out = run_rpc(
        dir,
        "context.capsule.query",
        r#"{"topic":"test","scope":"core","limit":1}"#,
        password,
    );
    assert!(
        out.status.success(),
        "context capsule query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse capsule result");
    result["result"].clone()
}

fn write_context_capsule(dir: &Path, capsule: &serde_json::Value) {
    let context_dir = dir.join(".decapod/generated/context");
    fs::create_dir_all(&context_dir).expect("create context dir");
    let capsule_path = dir.join(".decapod/generated/context/test_task.json");
    fs::write(
        &capsule_path,
        serde_json::to_string_pretty(capsule).expect("serialize capsule"),
    )
    .expect("write capsule");
}

#[test]
fn projection_validation_catches_stale_context_capsule_while_normal_passes() {
    let (_tmp, dir, password) = setup_repo();

    // Get a valid context capsule from decapod
    let capsule = get_valid_context_capsule(&dir, &password);
    // Write the valid capsule (with correct current fingerprint and hash)
    write_context_capsule(&dir, &capsule);

    // Normal validate should pass with valid capsule
    let normal = run_validate(&dir, &password, false);
    assert!(
        normal.status.success(),
        "normal validate should pass with valid context capsule; stderr:\n{}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Now mutate a tracked file so the fingerprint changes, making the capsule stale
    // Create a src/lib.rs file which is a tracked path
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        src_dir.join("lib.rs"),
        "// This changes the repo fingerprint\npub fn hello() {}\n",
    )
    .expect("write src/lib.rs to mutate repo");

    // Refresh specs so the manifest fingerprint matches the new repo state
    // This isolates the context capsule test from the specs validation
    let specs_refresh = run_decapod(
        &dir,
        &["rpc", "--op", "specs.refresh"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        specs_refresh.status.success(),
        "specs refresh failed: {}",
        String::from_utf8_lossy(&specs_refresh.stderr)
    );

    // Normal validate should still pass (capsule hash is still valid for its original fingerprint)
    let normal = run_validate(&dir, &password, false);
    assert!(
        normal.status.success(),
        "normal validate should pass with stale context capsule fingerprint; stderr:\n{}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Validate with --projections should fail (fingerprint is now stale)
    let projections = run_validate(&dir, &password, true);
    assert!(
        !projections.status.success(),
        "validate --projections should fail with stale context capsule; stdout:\n{}",
        String::from_utf8_lossy(&projections.stdout)
    );

    let payload: Value =
        serde_json::from_slice(&projections.stdout).expect("validate json payload");
    assert_eq!(payload["report"]["status"], "fail");
    assert!(payload["report"]["fail_count"].as_u64().unwrap_or(0) > 0);

    let failures = payload["report"]["failures"]
        .as_array()
        .expect("failures array");
    let projection_failure = failures.iter().find(|f| {
        let msg = f.as_str().unwrap_or("");
        msg.contains("[PROJECTION]")
            && msg.contains(".decapod/generated/context/")
            && msg.contains("repo_signal_fingerprint")
    });
    assert!(
        projection_failure.is_some(),
        "expected projection failure with typed finding; got: {failures:?}"
    );
    let finding = projection_failure.unwrap().as_str().unwrap();
    assert!(finding.contains("[PROJECTION]"));
    assert!(finding.contains("expected: repo_signal_fingerprint"));
    assert!(finding.contains("observed: stale fingerprint"));
    assert!(finding.contains("remediation"));
    assert!(finding.contains("Regenerate context capsule"));
}

#[test]
fn capability_survives_config_context_spec_deterministically() {
    let (_tmp, dir, _password) = setup_repo();

    set_capabilities(&dir, &["houseboat", "public-api"]);

    // Re-init to pick up new capabilities
    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify both capabilities appear in context
    let out = run_decapod(
        &dir,
        &[
            "rpc",
            "--op",
            "context.capsule.query",
            "--params",
            r#"{"topic":"test","scope":"core","limit":1}"#,
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "context capsule query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse capsule result");
    let capsule = result["result"].clone();
    let context_capabilities = capsule["capabilities"]
        .as_array()
        .unwrap_or_else(|| panic!("capabilities in context capsule: {capsule}"));
    assert!(
        context_capabilities
            .iter()
            .any(|value| value == "houseboat")
    );
    assert!(
        context_capabilities
            .iter()
            .any(|value| value == "public-api")
    );

    // Both capabilities should appear in generated specs
    let normal = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        normal.status.success(),
        "normal validate failed: {}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Check that generated specs reflect both capabilities
    let intent_path = dir.join(".decapod/generated/specs/INTENT.md");
    let intent = fs::read_to_string(&intent_path).expect("read INTENT.md");
    assert!(
        intent.contains("houseboat") || intent.contains("persistent-state"),
        "caps should appear in INTENT.md"
    );

    // Validate with projections should pass (both capabilities known)
    let projections = run_decapod(
        &dir,
        &["validate", "--projections", "--format", "json"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        projections.status.success(),
        "projections validate failed: {}",
        String::from_utf8_lossy(&projections.stdout)
    );

    // Generated surfaces, rather than wall-clock validation timing, must be
    // byte-identical on the second run.
    let manifest_path = dir.join(".decapod/generated/specs/.manifest.json");
    let first_manifest = fs::read_to_string(&manifest_path).expect("read first manifest");
    let first_intent = fs::read_to_string(dir.join(".decapod/generated/specs/INTENT.md"))
        .expect("read first intent");
    let second = run_decapod(
        &dir,
        &["validate", "--projections", "--format", "json"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(second.status.success());
    assert_eq!(
        first_manifest,
        fs::read_to_string(&manifest_path).expect("read second manifest")
    );
    assert_eq!(
        first_intent,
        fs::read_to_string(dir.join(".decapod/generated/specs/INTENT.md"))
            .expect("read second intent")
    );
}

#[test]
fn capability_regeneration_preserves_authorship() {
    let (_tmp, dir, _password) = setup_repo();

    // Add persistent-state capability
    set_capabilities(&dir, &["persistent-state"]);

    // First init
    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Human edits INTENT.md with custom content
    let intent_path = dir.join(".decapod/generated/specs/INTENT.md");
    let mut intent = fs::read_to_string(&intent_path).expect("read INTENT.md");
    let custom_section = "\n## Human Decision\n\nWe chose PostgreSQL for durability.\n";
    if !intent.contains("## Human Decision") {
        intent.push_str(custom_section);
        fs::write(&intent_path, &intent).expect("write INTENT.md");
    }

    // Refresh specs (should preserve human edit)
    let refresh = run_decapod(
        &dir,
        &["rpc", "--op", "specs.refresh"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        refresh.status.success(),
        "specs refresh failed: {}",
        String::from_utf8_lossy(&refresh.stderr)
    );

    // Verify human content preserved
    let intent_after = fs::read_to_string(&intent_path).expect("read INTENT.md after refresh");
    assert!(
        intent_after.contains("We chose PostgreSQL for durability"),
        "human content must survive regeneration"
    );

    // Second refresh should be byte-identical
    let manifest_path = dir.join(".decapod/generated/specs/.manifest.json");
    let first_manifest = fs::read_to_string(&manifest_path).expect("read manifest");
    let second_refresh = run_decapod(
        &dir,
        &["rpc", "--op", "specs.refresh"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        second_refresh.status.success(),
        "second refresh failed: {}",
        String::from_utf8_lossy(&second_refresh.stderr)
    );
    let second_manifest =
        fs::read_to_string(&manifest_path).expect("read manifest after second refresh");
    assert_eq!(
        first_manifest, second_manifest,
        "second refresh must be byte-identical to first"
    );
}

#[test]
fn manifest_provenance_records_capabilities() {
    let (_tmp, dir, _password) = setup_repo();

    set_capabilities(&dir, &["houseboat", "persistent-state"]);

    // Re-init to pick up capabilities
    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Check manifest records capabilities
    // Check manifest records capabilities
    let manifest_path = dir.join(".decapod/generated/specs/.manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_content).expect("parse manifest");

    assert!(
        manifest.get("declared_capabilities").is_some(),
        "manifest must have declared_capabilities"
    );
    let declared = manifest["declared_capabilities"]
        .as_array()
        .expect("declared_capabilities array");
    assert!(
        declared.iter().any(|v| v == "houseboat"),
        "houseboat must be in declared_capabilities"
    );
    assert!(
        declared.iter().any(|v| v == "persistent-state"),
        "persistent-state must be in declared_capabilities"
    );

    // capability_definition_version must be present
    assert!(
        manifest.get("capability_definition_version").is_some(),
        "capability_definition_version must be present"
    );
    let version = manifest["capability_definition_version"]
        .as_str()
        .expect("version string");
    assert!(
        !version.is_empty(),
        "capability_definition_version must not be empty"
    );
}

#[test]
fn persistent_state_activates_executable_migration_gate() {
    let (_tmp, dir, password) = setup_repo();

    set_capabilities(&dir, &["persistent-state"]);

    // Re-init to pick up capability
    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Run validation with --projections (should fail without migration artifacts)
    let projections = run_validate(&dir, &password, true);
    assert!(
        !projections.status.success(),
        "validate --projections should fail without migration artifacts; stdout:\n{}",
        String::from_utf8_lossy(&projections.stdout)
    );

    let payload: serde_json::Value =
        serde_json::from_slice(&projections.stdout).expect("validate json payload");
    let failures = payload["report"]["failures"]
        .as_array()
        .expect("failures array");
    let migration_failure = failures.iter().find(|f| {
        let msg = f.as_str().unwrap_or("");
        msg.contains("migration") && msg.contains("persistent-state")
    });
    assert!(
        migration_failure.is_some(),
        "should have migration-related failure for persistent-state; got: {failures:?}"
    );

    // Create migration artifacts to satisfy the gate
    // Create migration artifacts to satisfy the gate
    let migrations_dir = dir.join("migrations");
    fs::create_dir_all(&migrations_dir).expect("create migrations dir");
    fs::write(
        migrations_dir.join("001_initial.sql"),
        "-- initial migration\nCREATE TABLE test (id INT);\n",
    )
    .expect("write migration");

    let refresh = run_decapod(
        &dir,
        &["rpc", "--op", "specs.refresh"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        refresh.status.success(),
        "spec refresh failed after migration: {}",
        String::from_utf8_lossy(&refresh.stderr)
    );

    // Re-validate should now pass the persistent-state gate
    let projections2 = run_validate(&dir, &password, true);
    assert!(
        projections2.status.success(),
        "validate --projections should pass with migration artifacts; stdout:\n{}",
        String::from_utf8_lossy(&projections2.stdout)
    );
}
