// Moved from src/decapod/plugins/container.rs
use super::*;

fn disable_container_runtime_override(
    repo_root: &Path,
    reason: &str,
    remediation: &str,
) -> Result<bool, error::DecapodError> {
    let path = override_file_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let mut content = if path.exists() {
        fs::read_to_string(&path).map_err(error::DecapodError::IoError)?
    } else {
        String::new()
    };
    if content.contains(CONTAINER_DISABLE_MARKER) {
        return Ok(false);
    }
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(
        "\n### plugins/CONTAINER\n\
## Runtime Guard Override (auto-generated)\n\
",
    );
    content.push_str(CONTAINER_DISABLE_MARKER);
    content.push('\n');
    content.push_str(&format!("reason: {reason}\n"));
    content.push_str(&format!("remediation: {remediation}\n"));
    content.push_str("warning: disabling isolated containers increases risk of concurrent agents stepping on each other.\n");
    fs::write(path, content).map_err(error::DecapodError::IoError)?;
    Ok(true)
}

#[test]
fn docker_spec_contains_safety_flags_and_sdlc_steps() {
    let repo = PathBuf::from("/tmp/repo");
    let workspace = PathBuf::from("/tmp/repo/.decapod/workspaces/w1");
    let spec = build_docker_spec(
        "docker",
        &repo,
        &workspace,
        "rust:1.96.1",
        "agent-a",
        "cargo test -q",
        "ahr/branch",
        "master",
        "2g",
        "2.0",
        Some("R_123"),
        false,
        false,
    )
    .expect("spec");

    let joined = spec.args.join(" ");
    assert!(joined.contains("--rm"));
    assert!(joined.contains("--cap-drop ALL"));
    assert!(joined.contains("--security-opt no-new-privileges:true"));
    assert!(!joined.contains("-e PATH="));
    assert!(
        joined.contains("-v /tmp/repo/.decapod/workspaces/w1:/tmp/repo/.decapod/workspaces/w1")
    );
    assert!(joined.contains("-v /tmp/repo/.decapod:/tmp/repo/.decapod/workspaces/w1/.decapod"));
    assert!(joined.contains("DECAPOD_LOCAL_ONLY=1"));
    assert!(joined.contains("decapod() { cargo run --quiet --bin decapod -- \"$@\"; }"));
    assert!(joined.contains("git_safe checkout -B 'ahr/branch'"));
    assert!(!joined.contains("git_safe fetch --no-write-fetch-head origin 'master'"));
    assert!(!joined.contains("git_safe rebase origin/'master'"));
    assert!(joined.contains("decapod update"));
    assert!(!joined.contains("git_safe push -u origin HEAD"));
    assert!(!joined.contains("gh auth status"));
    assert!(!joined.contains("gh pr create --base 'master' --head 'ahr/branch'"));
}

#[test]
fn docker_spec_local_only_avoids_remote_git_operations() {
    let repo = PathBuf::from("/tmp/repo");
    let workspace = PathBuf::from("/tmp/repo/.decapod/workspaces/w1");
    let spec = build_docker_spec(
        "docker",
        &repo,
        &workspace,
        "rust:1.96.1",
        "agent-a",
        "cargo test -q",
        "ahr/branch",
        "master",
        "2g",
        "2.0",
        Some("R_123"),
        false,
        true,
    )
    .expect("spec");

    let joined = spec.args.join(" ");
    assert!(joined.contains("DECAPOD_LOCAL_ONLY=1"));
    assert!(!joined.contains("git_safe fetch --no-write-fetch-head origin 'master'"));
    assert!(!joined.contains("git_safe rebase origin/'master'"));
    assert!(!joined.contains("git_safe push -u origin"));
    assert!(!joined.contains("gh pr create --base 'master' --head 'ahr/branch'"));
    assert!(!joined.contains("ssh-keyscan -t ed25519 github.com"));
    assert!(joined.contains("git_safe checkout -B 'ahr/branch' 'master'"));
}

#[test]
fn podman_spec_does_not_force_host_uid_mapping() {
    let repo = PathBuf::from("/tmp/repo");
    let workspace = PathBuf::from("/tmp/repo/.decapod/workspaces/w1");
    let spec = build_docker_spec(
        "podman",
        &repo,
        &workspace,
        "rust:1.96.1",
        "agent-a",
        "decapod validate",
        "ahr/branch",
        "master",
        "2g",
        "2.0",
        Some("R_123"),
        false,
        true,
    )
    .expect("spec");

    assert!(
        !spec.args.iter().any(|arg| arg == "--user"),
        "rootless podman should use its default user namespace for mounted worktree writes"
    );
}

#[test]
fn sanitize_name_normalizes_agent_identifiers() {
    assert_eq!(sanitize_name("Agent_One"), "agent-one");
    assert_eq!(sanitize_name("  team/a  "), "team-a");
}

#[test]
fn default_branch_name_includes_agent_and_task() {
    let branch = default_branch_name("Agent_One", Some("R_ABC-123"));
    assert_eq!(branch, "agent/agent-one/r-abc-123");
}

#[test]
fn configured_base_branch_is_used_when_cli_override_is_absent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".decapod")).expect("config dir");
    std::fs::write(
            tmp.path().join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n\n[init]\nspecs = true\nci = true\ndiagram_style = \"ascii\"\nentrypoints = []\n\n[repo]\nbase_branch = \"main\"\n",
        )
        .expect("write config");

    assert_eq!(resolve_base_branch(tmp.path(), None), "main");
    assert_eq!(resolve_base_branch(tmp.path(), Some("release")), "release");
}

#[test]
fn generated_workspace_dockerfile_includes_decapod_and_rust_when_needed() {
    let content = render_generated_dockerfile(&ProjectCapabilities {
        primary_language: Some(ProjectLanguage::Rust),
        rust: true,
        node: false,
        python: false,
        go: false,
    });
    assert!(content.contains(&format!(
        "ARG DECAPOD_IMAGE=ghcr.io/decapodlabs/decapod:v{}-debian",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(content.contains("FROM $DECAPOD_IMAGE"));
    assert!(content.contains("ARG DECAPOD_USE_LOCAL_BINARY=0"));
    assert!(content.contains("ARG DECAPOD_WORKSPACE_PATH=unknown"));
    assert!(content.contains("LABEL org.decapod.managed=\"workspace\""));
    assert!(content.contains("LABEL org.decapod.workspace.path=\"$DECAPOD_WORKSPACE_PATH\""));
    assert!(content.contains("git"));
    assert!(content.contains("openssh-client"));
    assert!(content.contains("coreutils"));
    assert!(content.contains("sqlite-libs"));
    assert!(content.contains("libsqlite3-0"));
    assert!(content.contains("rust"));
    assert!(content.contains("rustc"));
    assert!(content.contains("cargo"));
    assert!(!content.contains("sqlite-dev"));
    assert!(!content.contains("sqlite-static"));
    assert!(content.contains(&format!(
        "ARG DECAPOD_VERSION={}",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(content.contains("COPY .decapod/managed/decapod /usr/local/bin/decapod.local"));
    assert!(content.contains("DECAPOD_USE_LOCAL_BINARY"));
    assert!(content.contains("cp /usr/local/bin/decapod.local /usr/local/bin/decapod"));
    assert!(content.contains("/usr/local/bin/decapod --help >/dev/null"));
}

#[test]
fn generated_workspace_dockerfile_layers_project_packages_on_decapod_image() {
    let content = render_generated_dockerfile(&ProjectCapabilities {
        primary_language: Some(ProjectLanguage::Python),
        rust: false,
        node: false,
        python: false,
        go: false,
    });
    assert!(content.contains("FROM $DECAPOD_IMAGE"));
    assert!(content.contains("git"));
    assert!(content.contains("coreutils"));
    assert!(content.contains("python3"));
    assert!(content.contains("py3-pip"));
    assert!(content.contains("python3-pip"));
    assert!(content.contains("decapod.local"));
    assert!(!content.contains("nodejs"));
    assert!(!content.contains(" go "));
    assert!(!content.contains(" golang-go "));
}

#[test]
fn generated_profile_stages_current_decapod_binary() {
    let root = std::env::temp_dir().join(format!(
        "decapod-generated-profile-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    let dockerfile = prepare_generated_container_profile(&root).expect("prepare profile");
    assert_eq!(
        dockerfile,
        root.join(".decapod")
            .join("managed")
            .join("Dockerfile.decapod")
    );
    assert!(
        root.join(".decapod")
            .join("managed")
            .join("decapod")
            .exists(),
        "current decapod binary should be staged into the ignored generated build context"
    );
    let content = fs::read_to_string(dockerfile).expect("read Dockerfile");
    assert!(content.contains("COPY .decapod/managed/decapod /usr/local/bin/decapod.local"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn generated_profile_migrates_legacy_dockerfile_without_overwriting_it() {
    let root = std::env::temp_dir().join(format!(
        "decapod-legacy-profile-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    let managed = root.join(".decapod").join("managed");
    fs::create_dir_all(&managed).expect("mkdir managed");
    let legacy = managed.join("Dockerfile");
    let legacy_content = "# project-specific workspace package\nRUN echo keep-me\n";
    fs::write(&legacy, legacy_content).expect("write legacy Dockerfile");

    let current = prepare_generated_container_profile(&root).expect("migrate profile");

    assert_eq!(
        current,
        root.join(".decapod")
            .join("managed")
            .join("Dockerfile.decapod")
    );
    assert!(!legacy.exists(), "legacy Dockerfile should be renamed");
    assert_eq!(
        fs::read_to_string(&current).expect("read migrated Dockerfile"),
        legacy_content,
        "migration must preserve project-specific Dockerfile content"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn generated_dockerfile_final_stage_uses_configured_primary_language() {
    let root = std::env::temp_dir().join(format!(
        "decapod-primary-language-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    fs::create_dir_all(root.join(".decapod")).expect("mkdir .decapod");
    fs::write(
        root.join(".decapod").join("config.toml"),
        r#"
[repo]
primary_languages = ["Python"]
"#,
    )
    .expect("write config");

    let content = generated_dockerfile_for_repo(&root);
    assert!(content.contains("FROM $DECAPOD_IMAGE"));
    assert!(content.contains("python3"));
    assert!(content.contains("py3-pip"));
    assert!(content.contains("decapod.local"));

    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn local_generated_image_builds_generated_dockerfile_from_repo_context() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "decapod-local-image-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    fs::create_dir_all(&root).expect("mkdir repo");
    let fake_runtime = root.join("fake-runtime");
    let args_file = root.join("runtime-args.txt");
    fs::write(
        &fake_runtime,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            args_file.display()
        ),
    )
    .expect("write fake runtime");
    let mut perms = fs::metadata(&fake_runtime).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_runtime, perms).expect("chmod");

    let image = ensure_local_generated_workspace_image(fake_runtime.to_str().unwrap(), &root)
        .expect("local image");

    assert_eq!(
        image,
        format!(
            "decapod-local-{}:workspace",
            root.file_name().unwrap().to_string_lossy()
        )
    );
    let args = fs::read_to_string(&args_file).expect("runtime args");
    assert!(args.contains("build\n"));
    assert!(args.contains(&format!(
            "{}\n",
            root.join(".decapod").join("managed").join("Dockerfile.decapod").display()
        )));
    assert!(
        args.ends_with(&format!("{}\n", root.display())),
        "build context should be the repository root, got: {args}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn container_failures_are_classified_for_common_validation_causes() {
    assert_eq!(
        classify_container_failure("", "fatal: unable to create index.lock: Permission denied").0,
        "container_workspace_permission_denied"
    );
    assert_eq!(
        classify_container_failure("", "fatal: refusing to fetch into branch").0,
        "container_branch_sync_checked_out"
    );
    assert_eq!(
        classify_container_failure("", "clang: invalid linker name in argument '-fuse-ld=lld'").0,
        "rust_toolchain_linker_config"
    );
}

#[test]
fn generated_dockerfile_expands_with_detected_stacks() {
    let content = render_generated_dockerfile(&ProjectCapabilities {
        primary_language: Some(ProjectLanguage::Go),
        rust: false,
        node: true,
        python: true,
        go: true,
    });
    assert!(content.contains("FROM $DECAPOD_IMAGE"));
    assert!(content.contains("nodejs"));
    assert!(content.contains("python3"));
    assert!(content.contains("go"));
}

#[test]
fn container_schema_includes_dockerfile_template_component() {
    let schema = schema();
    let component = schema
        .get("components")
        .and_then(|v| v.get("dockerfile_template"))
        .expect("dockerfile_template component exists");
    assert_eq!(
        component.get("path").and_then(|v| v.as_str()),
        Some(".decapod/managed/Dockerfile.decapod")
    );
    assert_eq!(
        component.get("extra_packages_env").and_then(|v| v.as_str()),
        Some("DECAPOD_CONTAINER_SYSTEM_PACKAGES")
    );
    assert_eq!(
        component
            .get("legacy_extra_packages_env")
            .and_then(|v| v.as_str()),
        Some("DECAPOD_CONTAINER_APK_PACKAGES")
    );
}

#[test]
fn disable_override_marks_container_runtime_disabled() {
    let root = std::env::temp_dir().join(format!(
        "decapod-container-override-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    disable_container_runtime_override(&root, "test-reason", "test-remediation").expect("write");
    let override_path = root.join(".decapod").join("OVERRIDE.md");
    let content = fs::read_to_string(&override_path).expect("override");
    assert!(content.contains(CONTAINER_DISABLE_MARKER));
    assert!(content.contains("warning: disabling isolated containers"));
    assert!(container_runtime_disabled(&root).expect("disabled check"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clear_override_strips_container_runtime_disabled_marker() {
    let root = std::env::temp_dir().join(format!(
        "decapod-container-clear-{}",
        crate::core::ulid::new_ulid().to_lowercase()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    let wrote = disable_container_runtime_override(&root, "test-reason", "test-remediation")
        .expect("disable override");
    assert!(wrote, "override should be written");
    let cleared = clear_container_runtime_override(&root).expect("clear override");
    assert!(cleared, "disable marker should be removed");
    assert!(
        !container_runtime_disabled(&root).expect("disabled check"),
        "container disable marker should be cleared"
    );
    let content = fs::read_to_string(root.join(".decapod").join("OVERRIDE.md")).expect("read");
    assert!(
        !content.contains(CONTAINER_DISABLE_MARKER),
        "override should no longer contain the disable marker"
    );

    let _ = fs::remove_dir_all(root);
}
