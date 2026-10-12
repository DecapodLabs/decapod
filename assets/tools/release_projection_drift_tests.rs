use super::*;
use serde_json::json;

const OLD: &str = "0.0.1";

pub(crate) struct Fixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) base: String,
    pub(crate) head: String,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path();
        git(repo, &["init", "-q", "-b", "master"]).unwrap();
        git(repo, &["config", "user.name", "Projection Test"]).unwrap();
        git(
            repo,
            &["config", "user.email", "projection@example.invalid"],
        )
        .unwrap();
        fs::create_dir_all(repo.join(specs::LOCAL_PROJECT_SPECS_DIR)).unwrap();
        fs::create_dir(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), "pub fn unchanged() {}\n").unwrap();
        fs::write(
            repo.join(".decapod/config.toml"),
            "[repo]\nproduct_name = \"fixture\"\n",
        )
        .unwrap();
        for path in entrypoints::ENTRYPOINT_FILES {
            fs::write(
                repo.join(path),
                entrypoints::render_entrypoint_for_release(path, OLD).unwrap(),
            )
            .unwrap();
        }
        fs::write(
            repo.join(DOCKERFILE),
            container::generated_dockerfile_for_repo(repo)
                .replace(entrypoints::RELEASE_VERSION, OLD),
        )
        .unwrap();
        for spec in specs::LOCAL_PROJECT_SPECS {
            fs::write(repo.join(spec.path), format!("# Fixture\n\nAuthored obligation stays byte-identical.\n\n{ATTESTATION_START}\n## Codebase Attestation\n\n- Repository signal fingerprint: `{}`\n- Significant implementation surfaces: `src/` (1 files)\n- Refreshed from the current codebase by `decapod rpc --op specs.refresh`\n{ATTESTATION_END}\n", "0".repeat(64))).unwrap();
        }
        fs::write(repo.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        Self::version(repo, OLD);
        let manifest = specs::refresh_specs_manifest(repo, &[]).unwrap();
        let mut manifest = serde_json::to_value(manifest).unwrap();
        manifest["decapod_release"] = OLD.into();
        manifest["entrypoints"] = json!(entrypoints::ENTRYPOINT_FILES.map(|path| {
            let body = fs::read_to_string(repo.join(path)).unwrap();
            json!({"path": path, "template_hash": specs::hash_text(&body), "content_hash": specs::hash_text(&body), "fingerprint": body.lines().nth(1).unwrap().strip_prefix("<!-- decapod-fingerprint: ").unwrap().strip_suffix(" -->").unwrap()})
        }));
        fs::write(
            repo.join(specs::LOCAL_PROJECT_SPECS_MANIFEST),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        git(repo, &["add", "."]).unwrap();
        git(repo, &["commit", "-qm", "source baseline"]).unwrap();
        let base = git(repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        Self::version(repo, entrypoints::RELEASE_VERSION);
        git(repo, &["add", "Cargo.toml", "Cargo.lock"]).unwrap();
        git(repo, &["commit", "-qm", "version-only release"]).unwrap();
        let head = git(repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        let fixture = Self { root, base, head };
        fixture.refresh();
        fixture
    }

    fn version(repo: &Path, version: &str) {
        fs::write(
            repo.join("Cargo.toml"),
            format!("[package]\nname = \"decapod\"\nversion = \"{version}\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        fs::write(
            repo.join("Cargo.lock"),
            format!("version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
    }

    pub(crate) fn refresh(&self) {
        let repo = self.root.path();
        entrypoints::refresh_entrypoint_metadata(repo).unwrap();
        container::refresh_managed_dockerfile_release(repo).unwrap();
        let signal = specs::repo_signal_fingerprint(repo).unwrap();
        for spec in specs::LOCAL_PROJECT_SPECS {
            let path = repo.join(spec.path);
            fs::write(
                &path,
                expected_spec(&fs::read_to_string(&path).unwrap(), &signal).unwrap(),
            )
            .unwrap();
        }
        specs::refresh_specs_manifest(repo, &[]).unwrap();
    }

    fn verify(&self) -> Check<()> {
        verify(self.root.path(), "push", &self.base, &self.head)
    }

    fn edit(&self, path: &str, edit: impl FnOnce(String) -> String) {
        let path = self.root.path().join(path);
        fs::write(&path, edit(fs::read_to_string(&path).unwrap())).unwrap();
    }

    fn manifest(&self, edit: impl FnOnce(&mut Value)) {
        self.edit(specs::LOCAL_PROJECT_SPECS_MANIFEST, |text| {
            let mut manifest: Value = serde_json::from_str(&text).unwrap();
            edit(&mut manifest);
            serde_json::to_string_pretty(&manifest).unwrap()
        });
    }
}

#[test]
fn accepts_only_exact_current_release_projections_without_mutating_checkout() {
    let fixture = Fixture::new();
    let before = git(fixture.root.path(), &["diff", "HEAD"]).unwrap();
    fixture.verify().unwrap();
    assert_eq!(before, git(fixture.root.path(), &["diff", "HEAD"]).unwrap());
    fixture.refresh();
    fixture.verify().unwrap();
}

#[test]
fn rejects_authored_prose_and_generated_inventory_changes_even_with_fresh_hashes() {
    for (old, new) in [
        ("Authored obligation", "Changed obligation"),
        ("`src/` (1 files)", "`src/` (2 files)"),
        ("## Codebase Attestation", "## Different heading"),
    ] {
        let fixture = Fixture::new();
        fixture.edit(specs::LOCAL_PROJECT_SPECS_INTENT, |text| {
            text.replace(old, new)
        });
        specs::refresh_specs_manifest(fixture.root.path(), &[]).unwrap();
        assert!(fixture.verify().is_err(), "accepted {new}");
    }
}

#[test]
fn rejects_manifest_template_config_capability_and_inventory_changes() {
    for (pointer, value) in [
        ("/schema_version", json!("2")),
        ("/template_version", json!("changed")),
        ("/config_input_hash", json!("0".repeat(64))),
        ("/capability_definition_version", json!("changed")),
        ("/declared_capabilities", json!(["new-capability"])),
        ("/files/0/template_hash", json!("0".repeat(64))),
        ("/files/0/path", json!("other.md")),
        ("/entrypoints/0/path", json!("OTHER.md")),
        ("/entrypoints/0/template_hash", json!("0".repeat(64))),
    ] {
        let fixture = Fixture::new();
        fixture.manifest(|manifest| *manifest.pointer_mut(pointer).unwrap() = value);
        assert!(fixture.verify().is_err(), "accepted {pointer}");
    }
}

#[test]
fn rejects_plausible_but_incorrect_generated_hashes() {
    for pointer in [
        "/repo_signal_fingerprint",
        "/spec_input_hash",
        "/files/0/content_hash",
        "/entrypoints/0/fingerprint",
    ] {
        let fixture = Fixture::new();
        fixture
            .manifest(|manifest| *manifest.pointer_mut(pointer).unwrap() = json!("a".repeat(64)));
        assert!(fixture.verify().is_err(), "accepted {pointer}");
    }
    let fixture = Fixture::new();
    let signal = specs::repo_signal_fingerprint(fixture.root.path()).unwrap();
    fixture.edit(specs::LOCAL_PROJECT_SPECS_INTENT, |text| {
        text.replace(&signal, &"a".repeat(64))
    });
    specs::refresh_specs_manifest(fixture.root.path(), &[]).unwrap();
    assert!(fixture.verify().is_err());
}

#[test]
fn rejects_entrypoint_payload_and_dockerfile_body_changes() {
    for path in ["AGENTS.md", DOCKERFILE] {
        let fixture = Fixture::new();
        fixture.edit(path, |text| text + "\nUnexpected command or prose\n");
        assert!(fixture.verify().is_err(), "accepted {path}");
    }
}

#[test]
fn rejects_source_configuration_and_untracked_inventory_changes() {
    for path in ["src/lib.rs", ".decapod/config.toml", "new.rs"] {
        let fixture = Fixture::new();
        fs::write(fixture.root.path().join(path), "unexpected\n").unwrap();
        assert!(fixture.verify().is_err(), "accepted {path}");
    }
}

#[test]
fn rejects_missing_projection_and_non_release_event() {
    let fixture = Fixture::new();
    assert!(
        verify(
            fixture.root.path(),
            "workflow_dispatch",
            &fixture.base,
            &fixture.head
        )
        .is_err()
    );
    fs::remove_file(fixture.root.path().join("CODEX.md")).unwrap();
    assert!(fixture.verify().is_err());
}

#[test]
fn rejects_ignored_source_files_but_allows_runtime_and_build_outputs() {
    let fixture = Fixture::new();
    for path in [
        "target/debug/projection-checker",
        ".decapod/data/runtime.json",
    ] {
        let path = fixture.root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "runtime output").unwrap();
    }
    fixture.verify().unwrap();
    fs::write(
        fixture.root.path().join(".git/info/exclude"),
        "ignored.rs\n",
    )
    .unwrap();
    fs::write(
        fixture.root.path().join("ignored.rs"),
        "pub fn unexpected() {}\n",
    )
    .unwrap();
    fixture.refresh();
    assert!(fixture.verify().is_err());
}

#[cfg(unix)]
#[test]
fn rejects_mode_changes_and_symlink_projections() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fs::set_permissions(
        fixture.root.path().join("CODEX.md"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(fixture.verify().is_err());
    let fixture = Fixture::new();
    fs::remove_file(fixture.root.path().join("CODEX.md")).unwrap();
    std::os::unix::fs::symlink("AGENTS.md", fixture.root.path().join("CODEX.md")).unwrap();
    assert!(fixture.verify().is_err());
}

#[test]
fn rejects_ambiguous_attestations() {
    let valid = format!(
        "{ATTESTATION_START}\n- Repository signal fingerprint: `{}`\n{ATTESTATION_END}\n",
        "0".repeat(64)
    );
    assert!(expected_spec(&(valid.clone() + &valid), &"a".repeat(64)).is_err());
    assert!(expected_spec(&valid.replace(ATTESTATION_END, ""), &"a".repeat(64)).is_err());
}

#[test]
fn rejects_staged_source_hidden_by_a_worktree_revert() {
    let fixture = Fixture::new();
    let repo = fixture.root.path();
    let original = fs::read(repo.join("src/lib.rs")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn staged_change() {}\n").unwrap();
    git(repo, &["add", "src/lib.rs"]).unwrap();
    fs::write(repo.join("src/lib.rs"), original).unwrap();
    assert!(git(repo, &["diff", "--quiet", "HEAD", "--", "src/lib.rs"]).is_ok());
    assert!(fixture.verify().is_err());
}

#[test]
fn validation_release_evidence_is_complete_and_base_bound() {
    let fixture = Fixture::new();
    let repo = fixture.root.path();
    let valid = [
        Some("pull_request".into()),
        Some(fixture.base.clone()),
        Some(fixture.head.clone()),
    ];
    verify_for_validation(repo, &fixture.base, valid.clone()).unwrap();
    verify_for_validation(repo, &fixture.base, [None, None, None]).unwrap();
    for missing in 0..3 {
        let mut partial = valid.clone();
        partial[missing] = None;
        assert!(verify_for_validation(repo, &fixture.base, partial).is_err());
    }
    for bad in ["", "master", "0000000000000000000000000000000000000000"] {
        let mut invalid = valid.clone();
        invalid[1] = Some(bad.into());
        assert!(verify_for_validation(repo, &fixture.base, invalid).is_err());
    }
    let mut unknown = valid;
    unknown[0] = Some("workflow_dispatch".into());
    assert!(verify_for_validation(repo, &fixture.base, unknown).is_err());
    assert!(verify_for_validation(repo, "missing-base", [None, None, None]).is_err());
}

#[test]
fn validation_rejects_a_synthetic_merge_with_new_base_code() {
    let fixture = Fixture::new();
    let repo = fixture.root.path();
    // Discard only this disposable fixture's generated projection changes.
    git(repo, &["reset", "--hard", "HEAD"]).unwrap();
    git(repo, &["checkout", "-qb", "base-advance", &fixture.base]).unwrap();
    fs::write(repo.join("src/new.rs"), "pub fn new_base_code() {}\n").unwrap();
    git(repo, &["add", "src/new.rs"]).unwrap();
    git(repo, &["commit", "-qm", "new accepted base code"]).unwrap();
    let base = git(repo, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    git(repo, &["checkout", "-qb", "synthetic", &fixture.head]).unwrap();
    git(
        repo,
        &["merge", "--no-ff", "-m", "synthetic PR merge", &base],
    )
    .unwrap();
    fixture.refresh();
    assert!(
        verify_for_validation(
            repo,
            &base,
            [
                Some("pull_request".into()),
                Some(base.clone()),
                Some(fixture.head.clone())
            ]
        )
        .is_err()
    );
    assert!(verify_for_validation(repo, &base, [None, None, None]).is_err());
}

#[test]
fn validation_accepts_equal_tree_synthetic_merge_with_explicit_event_evidence() {
    let fixture = Fixture::new();
    let repo = fixture.root.path();
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let merge = git(
        repo,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            &fixture.base,
            "-p",
            &fixture.head,
            "-m",
            "synthetic release PR merge",
        ],
    )
    .unwrap();
    git(repo, &["reset", "--soft", merge.trim()]).unwrap();
    verify_for_validation(
        repo,
        &fixture.base,
        [
            Some("pull_request".into()),
            Some(fixture.base.clone()),
            Some(fixture.head.clone()),
        ],
    )
    .unwrap();
    assert!(verify_for_validation(repo, &fixture.base, [None, None, None]).is_err());
}

#[test]
fn validation_rejects_valid_release_evidence_outside_configured_base_ancestry() {
    let fixture = Fixture::new();
    let repo = fixture.root.path();
    let tree = git(repo, &["rev-parse", &format!("{}^{{tree}}", fixture.base)]).unwrap();
    let unrelated = git(
        repo,
        &[
            "commit-tree",
            tree.trim(),
            "-m",
            "unrelated configured base",
        ],
    )
    .unwrap();
    assert!(
        verify_for_validation(
            repo,
            unrelated.trim(),
            [
                Some("pull_request".into()),
                Some(fixture.base.clone()),
                Some(fixture.head.clone())
            ]
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn expected_dockerfile_uses_private_scratch_under_permissive_umask() {
    const CHILD: &str = "DECAPOD_PROJECTION_UMASK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Only the process-local child inherits the deliberately permissive
        // umask. Do not change permissions on any existing checkout or state.
        let root = tempfile::tempdir().unwrap();
        let old = container::generated_dockerfile_for_repo(root.path())
            .replace(entrypoints::RELEASE_VERSION, OLD);
        let refreshed = expected_dockerfile(&old, OLD).unwrap();
        container::managed_dockerfile_matches_release(&refreshed, entrypoints::RELEASE_VERSION)
            .unwrap();
        println!("private scratch refresh completed");
        return;
    }

    let test_name = std::thread::current()
        .name()
        .expect("libtest names this test thread")
        .to_string();
    for mask in ["000", "002"] {
        let output = Command::new("/bin/sh")
            .args([
                "-c",
                "umask \"$1\"; shift; exec \"$@\"",
                "projection-umask",
                mask,
            ])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", &test_name, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env_remove("DECAPOD_STORAGE_SHARED_GROUP")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "umask {mask}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("private scratch refresh completed"),
            "umask {mask}: child must execute the exact helper regression"
        );
    }
}
