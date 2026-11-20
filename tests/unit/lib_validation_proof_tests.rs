use super::{
    heal_validation_scaffold, scaffold, trajectory_has_validation_epoch, validation_epoch_evidence,
};

#[test]
fn validation_epoch_evidence_matches_only_the_bound_epoch() {
    let evidence = vec![validation_epoch_evidence("ve_bound")];

    assert!(trajectory_has_validation_epoch(&evidence, "ve_bound"));
    assert!(!trajectory_has_validation_epoch(&evidence, "ve_changed"));
}

fn scaffolded_project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    scaffold::scaffold_project_entrypoints(&scaffold::ScaffoldOptions {
        target_dir: root.path().to_path_buf(),
        force: false,
        dry_run: false,
        agent_files: vec!["AGENTS.md".into()],
        created_backups: false,
        all: false,
        preserved_agent_content: vec![],
        generate_specs: true,
        generate_ci: true,
        diagram_style: scaffold::DiagramStyle::Ascii,
        specs_seed: None,
        capabilities: vec![],
    })
    .unwrap();
    // Init creates this ignored cache; a fresh Git worktree omits it.
    std::fs::remove_file(
        root.path()
            .join(crate::core::capsule_policy::GENERATED_POLICY_REL_PATH),
    )
    .unwrap();
    root
}

#[test]
fn absent_runtime_policy_cache_does_not_rewrite_tracked_scaffolding() {
    let root = scaffolded_project();
    assert!(
        !root
            .path()
            .join(crate::core::capsule_policy::GENERATED_POLICY_REL_PATH)
            .exists()
    );
    let manifest = root.path().join(".decapod/managed/specs/.manifest.json");
    let before = std::fs::read(&manifest).unwrap();
    assert!(heal_validation_scaffold(root.path()).unwrap().is_none());
    assert_eq!(std::fs::read(manifest).unwrap(), before);
}

#[test]
fn missing_tracked_living_spec_still_requires_scaffold_repair() {
    let root = scaffolded_project();
    let intent = root.path().join(".decapod/managed/specs/INTENT.md");
    std::fs::remove_file(&intent).unwrap();
    assert!(heal_validation_scaffold(root.path()).unwrap().is_some());
    assert!(intent.is_file());
}
