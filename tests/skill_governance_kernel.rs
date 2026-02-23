use serde_json::Value;
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

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn setup_repo() -> (TempDir, std::path::PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&decapod_init.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

fn write_sample_skill(dir: &Path) -> std::path::PathBuf {
    let skill_path = dir.join("skills/sample/SKILL.md");
    fs::create_dir_all(skill_path.parent().expect("skill dir")).expect("mkdir skill dir");
    fs::write(
        &skill_path,
        "---\nname: web-service-reliability\ndescription: Build and validate robust backend/frontend services\n---\n\n# Overview\nA test skill.\n\n## Dependencies\n- pytest\n- playwright\n\n## Workflow\n1. Define invariants\n2. Implement\n3. Validate\n",
    )
    .expect("write sample skill");
    skill_path
}

#[test]
fn skill_import_writes_deterministic_card_hash() {
    let (_tmp, dir, password) = setup_repo();
    let skill_path = write_sample_skill(&dir);

    let import = run_decapod(
        &dir,
        &[
            "data",
            "aptitude",
            "skill",
            "import",
            "--path",
            skill_path.to_str().expect("skill path utf8"),
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        import.status.success(),
        "skill import failed: {}",
        String::from_utf8_lossy(&import.stderr)
    );

    let payload: Value = serde_json::from_slice(&import.stdout).expect("parse import payload");
    let card_hash_1 = payload["card"]["card_hash"]
        .as_str()
        .expect("card hash in payload")
        .to_string();

    let import_again = run_decapod(
        &dir,
        &[
            "data",
            "aptitude",
            "skill",
            "import",
            "--path",
            skill_path.to_str().expect("skill path utf8"),
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        import_again.status.success(),
        "second skill import failed: {}",
        String::from_utf8_lossy(&import_again.stderr)
    );
    let payload_2: Value = serde_json::from_slice(&import_again.stdout).expect("parse payload2");
    let card_hash_2 = payload_2["card"]["card_hash"]
        .as_str()
        .expect("card hash2")
        .to_string();

    assert_eq!(
        card_hash_1, card_hash_2,
        "skill card hash should be deterministic for same SKILL.md"
    );
}

#[test]
fn skill_resolution_is_deterministic_for_same_query() {
    let (_tmp, dir, password) = setup_repo();
    let skill_path = write_sample_skill(&dir);

    let import = run_decapod(
        &dir,
        &[
            "data",
            "aptitude",
            "skill",
            "import",
            "--path",
            skill_path.to_str().expect("skill path utf8"),
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(import.status.success(), "import failed");

    let run_resolve = || {
        run_decapod(
            &dir,
            &[
                "data",
                "aptitude",
                "skill",
                "resolve",
                "--query",
                "backend reliability",
                "--limit",
                "3",
                "--write",
            ],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        )
    };

    let first = run_resolve();
    assert!(
        first.status.success(),
        "first resolve failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_payload: Value = serde_json::from_slice(&first.stdout).expect("parse first");

    let second = run_resolve();
    assert!(
        second.status.success(),
        "second resolve failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_payload: Value = serde_json::from_slice(&second.stdout).expect("parse second");

    assert_eq!(
        first_payload["resolution"]["resolution_hash"],
        second_payload["resolution"]["resolution_hash"],
        "resolution hash should be deterministic for same query and state"
    );

    let out_path = first_payload["path"]
        .as_str()
        .expect("resolution artifact path");
    assert!(
        Path::new(out_path).exists(),
        "resolution artifact should exist"
    );
}

#[test]
fn validate_fails_on_skill_card_hash_mismatch_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let skill_path = write_sample_skill(&dir);

    let import = run_decapod(
        &dir,
        &[
            "data",
            "aptitude",
            "skill",
            "import",
            "--path",
            skill_path.to_str().expect("skill path utf8"),
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(import.status.success(), "import failed");
    let payload: Value = serde_json::from_slice(&import.stdout).expect("parse import payload");
    let card_path = payload["card_path"].as_str().expect("card path in output");

    let mut card_json: Value =
        serde_json::from_slice(&fs::read(card_path).expect("read skill card artifact"))
            .expect("parse skill card artifact");
    card_json["card_hash"] = Value::String("tampered-hash".to_string());
    fs::write(
        card_path,
        serde_json::to_string_pretty(&card_json).expect("serialize tampered card"),
    )
    .expect("write tampered card");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail when skill card hash is tampered"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("skill card hash mismatch"),
        "expected skill card hash mismatch error, got:\n{}",
        stderr
    );
}
