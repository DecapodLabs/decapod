use super::*;
use serde_json::json;

const OLD: &str = "0.0.1";

struct Fixture {
    root: tempfile::TempDir,
    base: String,
    head: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path();
        git(repo, &["init", "-q"]).unwrap();
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

    fn refresh(&self) {
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
