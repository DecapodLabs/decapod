use decapod::core::bounded_process::BoundedCommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

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
        .and_then(|exe| exe.parent().map(|path| path.to_path_buf()))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

// These children own unrelated temporary repositories, not the CI source checkout.
// Keep the parent provider context untouched; explicit test overrides are applied last.
const PARENT_CI_CONTEXT: &[&str] = &[
    "GITHUB_ACTIONS",
    "BUILDKITE",
    "GITHUB_EVENT_NAME",
    "GITHUB_WORKSPACE",
    "BUILDKITE_BUILD_CHECKOUT_PATH",
    "CHANGE_EVENT",
    "CHANGE_BASE",
    "CHANGE_HEAD",
    "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT",
];

fn decapod_command(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(resolve_decapod_bin());
    for name in PARENT_CI_CONTEXT {
        cmd.env_remove(name);
    }
    cmd.current_dir(dir)
        .args(args)
        .env("XDG_CONFIG_HOME", dir.join(".config"));
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd
}

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    decapod_command(dir, args, envs)
        .output()
        .expect("run decapod")
}

#[test]
fn scratch_children_remove_parent_ci_context_and_allow_explicit_test_context() {
    let command = decapod_command(Path::new("fixture"), &["govern", "project-check"], &[]);
    for name in PARENT_CI_CONTEXT {
        assert_eq!(
            command.get_envs().find(|(key, _)| key == name),
            Some((std::ffi::OsStr::new(name), None)),
            "scratch child must remove inherited {name}"
        );
    }
    let command = decapod_command(
        Path::new("fixture"),
        &["govern", "project-check"],
        &[("DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT", "1")],
    );
    assert_eq!(
        command
            .get_envs()
            .find(|(name, _)| *name == "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT")
            .and_then(|(_, value)| value),
        Some(std::ffi::OsStr::new("1"))
    );
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

    // 1. The initial project configuration passes its prerequisite checks.
    // Corrupt configuration below must still be rejected by public validate.
    let out = run_decapod(dir, &["govern", "project-check"], &envs);
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
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("fail=")
            || combined.contains("Invalid .decapod/config.toml schema")
            || combined.contains("Failed to parse config.toml"),
        "Should report the config failure: {combined}"
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
        content.contains("External task managers are allowed")
            && content.contains("Decapod todos are still mandatory"),
        "Regenerated AGENTS.md should use updated template. Content was:\n{content}"
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
        .join("managed")
        .join("specs")
        .join("INTENT.md");
    let intent_content = fs::read_to_string(&intent_path).unwrap();
    assert!(
        intent_content.contains("New Summary"),
        "INTENT.md should be updated with new summary from config.toml. Content was:\n{intent_content}"
    );
}

#[test]
fn config_change_after_refresh_does_not_turn_authored_specs_into_templates() {
    let tmp = TempDir::new().unwrap();
    let (password, agent_id) = setup_repo(&tmp);
    let dir = tmp.path();
    let envs = [
        ("DECAPOD_AGENT_ID", agent_id.as_str()),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
    ];
    let validation = dir.join(".decapod/managed/specs/VALIDATION.md");
    let authored = fs::read_to_string(&validation).unwrap().replace(
        "## Proof Surfaces\n",
        "## Proof Surfaces\n- Project-specific doc-contract obligation.\n",
    );
    fs::write(&validation, &authored).unwrap();
    // A refresh records authored content_hash; this is not evidence that the
    // file is an untouched template, even if config_input_hash later changes.
    decapod::core::project_specs::refresh_specs_manifest(dir, &[]).unwrap();
    let config_path = dir.join(".decapod/config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("Original Summary", "Changed summary");
    fs::write(&config_path, config).unwrap();
    for _ in 0..2 {
        let out = run_decapod(dir, &["init", "--force"], &envs);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let preserved = fs::read_to_string(&validation).unwrap();
        assert_eq!(
            decapod::core::project_specs::material_spec_body(&preserved).trim(),
            decapod::core::project_specs::material_spec_body(&authored).trim(),
        );
    }
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
    let heading_index = override_content
        .find("### specs/INTENT")
        .expect("INTENT override section");
    let placeholder = "Replace this line with this directive's override. Use Markdown or any documentation style you prefer.";
    let relative_body_index = override_content[heading_index..]
        .find(placeholder)
        .expect("INTENT override body placeholder");
    let body_index = heading_index + relative_body_index;
    override_content.replace_range(
        body_index..body_index + placeholder.len(),
        "This intent comes from OVERRIDE.md",
    );
    fs::write(&override_path, &override_content).unwrap();

    // 2. Run init --force
    let out = run_decapod(dir, &["init", "--force"], &envs);
    assert!(out.status.success(), "Init --force should succeed");

    // 3. Check INTENT.md
    let intent_path = dir
        .join(".decapod")
        .join("managed")
        .join("specs")
        .join("INTENT.md");
    let intent_content = fs::read_to_string(&intent_path).unwrap();
    assert!(
        intent_content.contains("This intent comes from OVERRIDE.md"),
        "INTENT.md should be updated with intent from OVERRIDE.md. Content was:\n{intent_content}"
    );
}

#[test]
fn test_regression_against_known_good_substrate() {
    // Check the known-good configuration/override substrate, not publication readiness.
    // This prerequisite result is not full publication-readiness proof.

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
    let out = run_decapod(dir, &["govern", "project-check"], &envs);
    assert!(
        out.status.success(),
        "Validate should pass against known-good substrate. Output: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // 4. Init --force should respect these files
    let out = run_decapod(dir, &["init", "--force"], &envs);
    assert!(out.status.success(), "Init --force should succeed");

    // 5. Verify INTENT.md used the OVERRIDE
    let intent_path = decapod_dir.join("managed").join("specs").join("INTENT.md");
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

/// Replay the version-only release failure through the actual candidate CLI,
/// not a recorder executable or a second implementation of the material gate.
struct MaterialReleaseCommand {
    _scratch: TempDir,
    repo: PathBuf,
    auth: PathBuf,
    password: String,
    base: String,
    head: String,
}

impl MaterialReleaseCommand {
    fn version(repo: &Path, value: &str) {
        fs::write(
            repo.join("Cargo.toml"),
            format!("[package]\nname = \"decapod\"\nversion = \"{value}\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        fs::write(
            repo.join("Cargo.lock"),
            format!("version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"{value}\"\n"),
        )
        .unwrap();
    }

    fn revision(repo: &Path) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo)
            .bounded_output(std::time::Duration::from_secs(90))
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn new() -> Self {
        use decapod::core::{entrypoint_integrity as entrypoints, project_specs as specs};
        const OLD: &str = "0.0.1";
        let scratch = TempDir::new().unwrap();
        let repo = scratch.path().join("release source");
        let auth = scratch.path().join("private auth");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&auth).unwrap();
        git_command(&repo, &["init", "-q", "-b", "master"]);
        git_command(&repo, &["config", "user.name", "Material command fixture"]);
        git_command(&repo, &["config", "user.email", "material@example.invalid"]);
        Self::version(&repo, OLD);
        fs::create_dir(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), "pub fn unchanged() {}\n").unwrap();
        let init = decapod_command(
            &repo,
            &[
                "init",
                "--force",
                "--no-container-workspaces",
                "--backend",
                "local",
                "--product-name",
                "Release fixture",
                "--product-summary",
                "A fixed Rust fixture for immutable release validation.",
            ],
            &[
                ("XDG_CONFIG_HOME", auth.to_str().unwrap()),
                ("DECAPOD_AGENT_ID", "material-init"),
            ],
        )
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .bounded_output(std::time::Duration::from_secs(90))
        .unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let acquired = decapod_command(
            &repo,
            &["session", "acquire"],
            &[
                ("XDG_CONFIG_HOME", auth.to_str().unwrap()),
                ("DECAPOD_AGENT_ID", "material-command"),
            ],
        )
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .bounded_output(std::time::Duration::from_secs(90))
        .unwrap();
        assert!(
            acquired.status.success(),
            "{}",
            String::from_utf8_lossy(&acquired.stderr)
        );
        let password = String::from_utf8_lossy(&acquired.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("Password: "))
            .unwrap()
            .to_owned();

        // A version-only exception starts from an already-attested release,
        // rather than asking the exception to invent missing baseline proof.
        let refreshed = decapod_command(
            &repo,
            &["rpc", "--op", "specs.refresh"],
            &[
                ("XDG_CONFIG_HOME", auth.to_str().unwrap()),
                ("DECAPOD_AGENT_ID", "material-command"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        )
        .bounded_output(std::time::Duration::from_secs(90))
        .unwrap();
        assert!(
            refreshed.status.success(),
            "baseline attestation: {}",
            String::from_utf8_lossy(&refreshed.stderr)
        );

        // Controlled prior-release fixtures use the production renderers and
        // exact manifest schema; no authored prose is changed by the release.
        for name in entrypoints::ENTRYPOINT_FILES {
            fs::write(
                repo.join(name),
                entrypoints::render_entrypoint_for_release(name, OLD).unwrap(),
            )
            .unwrap();
        }
        let docker = repo.join(".decapod/managed/Dockerfile.decapod");
        fs::write(
            &docker,
            fs::read_to_string(&docker)
                .unwrap()
                .replace(entrypoints::RELEASE_VERSION, OLD),
        )
        .unwrap();
        let path = repo.join(specs::LOCAL_PROJECT_SPECS_MANIFEST);
        let attested: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let capabilities: Vec<String> = attested["declared_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        specs::refresh_specs_manifest(&repo, &capabilities).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        manifest["decapod_release"] = OLD.into();
        manifest["entrypoints"] = serde_json::json!(entrypoints::ENTRYPOINT_FILES.map(|name| {
            let body = fs::read_to_string(repo.join(name)).unwrap();
            let fingerprint = body
                .lines()
                .nth(1)
                .unwrap()
                .strip_prefix("<!-- decapod-fingerprint: ")
                .unwrap()
                .strip_suffix(" -->")
                .unwrap();
            serde_json::json!({"path": name, "template_hash": specs::hash_text(&body),
                "content_hash": specs::hash_text(&body), "fingerprint": fingerprint})
        }));
        fs::write(path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        git_command(&repo, &["add", "."]);
        git_command(&repo, &["commit", "-qm", "prior release source"]);
        let base = Self::revision(&repo);
        Self::version(&repo, entrypoints::RELEASE_VERSION);
        git_command(&repo, &["add", "Cargo.toml", "Cargo.lock"]);
        git_command(&repo, &["commit", "-qm", "version-only release"]);
        let head = Self::revision(&repo);
        git_command(&repo, &["update-ref", "refs/remotes/origin/master", &base]);
        git_command(&repo, &["checkout", "-q", "--detach", &head]);
        git_command(&repo, &["branch", "-D", "master"]);
        Self {
            _scratch: scratch,
            repo,
            auth,
            password,
            base,
            head,
        }
    }

    fn check(&self, event: &str, base: Option<&str>, head: Option<&str>) -> std::process::Output {
        let mut command = decapod_command(
            &self.repo,
            &[
                "govern",
                "project-check",
                "--refresh-specs",
                "--format",
                "json",
            ],
            &[
                ("XDG_CONFIG_HOME", self.auth.to_str().unwrap()),
                ("DECAPOD_AGENT_ID", "material-command"),
                ("DECAPOD_SESSION_PASSWORD", &self.password),
                // These are the same project-check scope/context controls as the
                // reviewed CI recipes. This test does not claim full readiness.
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT", "1"),
                ("GITHUB_ACTIONS", "true"),
                ("GITHUB_EVENT_NAME", event),
                ("GITHUB_WORKSPACE", self.repo.to_str().unwrap()),
                ("CHANGE_EVENT", event),
            ],
        );
        for (key, value) in [("CHANGE_BASE", base), ("CHANGE_HEAD", head)] {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        command
            .bounded_output(std::time::Duration::from_secs(90))
            .unwrap()
    }
}

fn git_command(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .bounded_output(std::time::Duration::from_secs(90))
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn actual_release_command_accepts_detached_remote_base_and_postmerge_context() {
    let fixture = MaterialReleaseCommand::new();
    for event in ["pull_request", "push"] {
        if event == "push" {
            // A postmerge accepted ref equal to HEAD must not skip the original
            // immutable push range's actual material/projection validation.
            git_command(
                &fixture.repo,
                &["update-ref", "refs/remotes/origin/master", &fixture.head],
            );
        }
        let out = fixture.check(event, Some(&fixture.base), Some(&fixture.head));
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let projection = decapod::core::release_projection_drift::verify(
            &fixture.repo,
            event,
            &fixture.base,
            &fixture.head,
        );
        assert!(
            out.status.success(),
            "actual release project-check: {text}\nExact projection diagnostic: {projection:?}"
        );
        let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(result["publication_ready"], false);
        assert!(
            result["report"]["notes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|note| note.as_str()
                    == Some("material_specs: outcome=passed proof=version_only_exact_projections")),
            "material gate did not record actual release proof: {text}"
        );
        assert!(projection.is_ok(), "{projection:?}");
    }
}

#[test]
fn actual_release_command_rejects_missing_malformed_and_mixed_evidence() {
    let mut fixture = MaterialReleaseCommand::new();
    // A postmerge accepted tip equal to HEAD still requires complete context.
    git_command(
        &fixture.repo,
        &["update-ref", "refs/remotes/origin/master", &fixture.head],
    );
    for (base, head) in [
        (None, Some(fixture.head.as_str())),
        (Some(fixture.base.as_str()), None),
        (Some("HEAD"), Some(fixture.head.as_str())),
        (
            Some(fixture.base.as_str()),
            Some("0000000000000000000000000000000000000000"),
        ),
    ] {
        let out = fixture.check("pull_request", base, head);
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.status.success(), "malformed context was accepted");
        assert!(
            text.contains("CI_CHANGE_CONTEXT_INVALID"),
            "wrong failure: {text}"
        );
    }
    fs::write(
        fixture.repo.join("src/lib.rs"),
        "pub fn mixed_source_change() {}\n",
    )
    .unwrap();
    git_command(&fixture.repo, &["add", "src/lib.rs"]);
    git_command(
        &fixture.repo,
        &["commit", "-qm", "source without authored specification"],
    );
    fixture.head = MaterialReleaseCommand::revision(&fixture.repo);
    let out = fixture.check("pull_request", Some(&fixture.base), Some(&fixture.head));
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "mixed source was accepted as release-only"
    );
    assert!(
        text.contains("FINGERPRINT_ONLY_SPECS"),
        "wrong mixed-source failure: {text}"
    );
}
