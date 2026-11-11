use super::*;

fn git(root: &Path, arguments: &[&str]) -> String {
    let mut command = Command::new("git");
    command.current_dir(root).args(arguments);
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(name);
    }
    let output = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Historical proof test")
        .env("GIT_AUTHOR_EMAIL", "historical-proof@example.invalid")
        .env("GIT_COMMITTER_NAME", "Historical proof test")
        .env("GIT_COMMITTER_EMAIL", "historical-proof@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn source_tree(root: &Path, content: &str) -> String {
    std::fs::write(root.join("subject.txt"), content).unwrap();
    git(root, &["add", "--", "subject.txt"]);
    git(root, &["write-tree"])
}

fn commit(root: &Path, tree: &str, parents: &[&str], message: &str) -> String {
    let mut arguments = vec!["commit-tree", tree, "-m", message];
    for parent in parents {
        arguments.extend(["-p", *parent]);
    }
    git(root, &arguments)
}

fn fixture() -> (tempfile::TempDir, MaterialIdentity) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch=master"]);
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    let baseline = source_tree(root, "accepted baseline\n");
    let base = commit(root, &baseline, &[], "accepted baseline");
    git(root, &["update-ref", "refs/heads/master", &base]);
    let candidate_tree = source_tree(root, "validated candidate\n");
    let candidate = commit(root, &candidate_tree, &[&base], "original candidate");
    git(root, &["update-ref", "refs/heads/candidate", &candidate]);

    let mut material = test_fixture::record().proof.material;
    material.head_revision = candidate;
    material.accepted_base_revision = base;
    (dir, material)
}

#[test]
fn accepted_fast_forward_descendant_and_merge_bind_the_original_candidate() {
    let (dir, material) = fixture();
    let root = dir.path();
    let candidate = &material.head_revision;
    let candidate_tree = tree(root, candidate).unwrap();
    let descendant = commit(root, &candidate_tree, &[candidate], "later accepted commit");
    let merge = commit(
        root,
        &candidate_tree,
        &[&material.accepted_base_revision, candidate],
        "accepted merge",
    );
    for accepted in [candidate.as_str(), descendant.as_str(), merge.as_str()] {
        let witness = integration_for_target(root, &material, accepted).unwrap();
        assert_eq!(witness.mode, "ancestor");
        assert_eq!(witness.candidate_sha, *candidate);
        assert_eq!(witness.integration_sha, *candidate);
        assert_eq!(witness.accepted_sha, accepted);
        assert_eq!(witness.accepted_base, material.accepted_base_revision);
        witness.recheck_local(root).unwrap();
    }
}

#[test]
fn exact_tree_squash_and_rebase_keep_candidate_and_integration_identities_distinct() {
    let (dir, material) = fixture();
    let root = dir.path();
    let candidate_tree = tree(root, &material.head_revision).unwrap();
    let base_tree = tree(root, &material.accepted_base_revision).unwrap();
    let updated_base = commit(
        root,
        &base_tree,
        &[&material.accepted_base_revision],
        "accepted history advanced without material changes",
    );
    for (parent, message) in [
        (
            material.accepted_base_revision.as_str(),
            "squashed candidate",
        ),
        (updated_base.as_str(), "rebased candidate"),
    ] {
        let integrated = commit(root, &candidate_tree, &[parent], message);
        assert_ne!(integrated, material.head_revision);
        assert!(!ancestor(root, &material.head_revision, &integrated).unwrap());
        let later_tree = source_tree(root, "subsequent accepted work\n");
        let accepted = commit(root, &later_tree, &[&integrated], "later accepted work");
        let witness = integration_for_target(root, &material, &accepted).unwrap();
        assert_eq!(witness.mode, "equivalent_tree");
        assert_eq!(witness.candidate_sha, material.head_revision);
        assert_eq!(witness.integration_sha, integrated);
        assert_eq!(witness.accepted_sha, accepted);
        assert_ne!(witness.candidate_sha, witness.integration_sha);
        witness.recheck_local(root).unwrap();
    }
}

#[test]
fn matching_changed_file_or_patch_is_insufficient_without_the_complete_candidate_tree() {
    let (dir, material) = fixture();
    let root = dir.path();
    source_tree(root, "validated candidate\n");
    std::fs::write(
        root.join("unvalidated.txt"),
        "different accepted material\n",
    )
    .unwrap();
    git(root, &["add", "--", "unvalidated.txt"]);
    let changed_tree = git(root, &["write-tree"]);
    let accepted = commit(
        root,
        &changed_tree,
        &[&material.accepted_base_revision],
        "same task patch combined with different material",
    );
    assert_ne!(changed_tree, tree(root, &material.head_revision).unwrap());
    assert!(integration_for_target(root, &material, &accepted).is_err());
}

#[test]
fn an_identical_tree_on_unrelated_history_does_not_prove_accepted_integration() {
    let (dir, material) = fixture();
    let root = dir.path();
    let candidate_tree = tree(root, &material.head_revision).unwrap();
    let unrelated = commit(
        root,
        &candidate_tree,
        &[],
        "unrelated root with identical files",
    );
    assert_eq!(tree(root, &unrelated).unwrap(), candidate_tree);
    assert!(integration_for_target(root, &material, &unrelated).is_err());
    assert!(integration_for_target(root, &material, &material.accepted_base_revision).is_err());
}

#[test]
fn a_matching_tree_before_the_validated_base_is_not_a_new_integration() {
    let (dir, mut material) = fixture();
    let root = dir.path();
    let candidate_tree = tree(root, &material.head_revision).unwrap();
    let earlier_match = commit(
        root,
        &candidate_tree,
        &[&material.accepted_base_revision],
        "old accepted tree that resembles this candidate",
    );
    let newer_tree = source_tree(root, "new accepted baseline\n");
    let newer_base = commit(
        root,
        &newer_tree,
        &[&earlier_match],
        "new accepted baseline",
    );
    material.accepted_base_revision = newer_base.clone();
    material.head_revision = commit(root, &candidate_tree, &[&newer_base], "new candidate");
    let accepted = commit(
        root,
        &newer_tree,
        &[&newer_base],
        "accepted unrelated history",
    );
    assert!(ancestor(root, &earlier_match, &accepted).unwrap());
    assert!(integration_for_target(root, &material, &accepted).is_err());
}

#[test]
fn witness_recheck_rejects_substituted_identity_history_and_mode() {
    let (dir, material) = fixture();
    let root = dir.path();
    let original = integration_for_target(root, &material, &material.head_revision).unwrap();
    let orphan_tree = source_tree(root, "unrelated history\n");
    let orphan = commit(root, &orphan_tree, &[], "unrelated root");
    let mut invalid_witnesses = Vec::new();
    let mut witness = original.clone();
    witness.candidate_sha = material.accepted_base_revision.clone();
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.integration_sha = material.accepted_base_revision.clone();
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.accepted_sha = material.accepted_base_revision.clone();
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.accepted_base = orphan;
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.repository = "AnotherOwner/another-repository".into();
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.mode = "merged_pr_label".into();
    invalid_witnesses.push(witness);
    let mut witness = original.clone();
    witness.integration_sha = "HEAD".into();
    invalid_witnesses.push(witness);
    let mut witness = original;
    witness.accepted_sha = "f".repeat(40);
    invalid_witnesses.push(witness);
    for witness in invalid_witnesses {
        assert!(witness.recheck_local(root).is_err(), "{witness:?}");
    }
}

#[test]
fn replacement_refs_cannot_forge_candidate_ancestry() {
    let (dir, material) = fixture();
    let root = dir.path();
    let different_tree = source_tree(root, "accepted work without candidate\n");
    let accepted = commit(
        root,
        &different_tree,
        &[&material.accepted_base_revision],
        "real accepted history",
    );
    let fake_parentage = commit(
        root,
        &different_tree,
        &[&material.head_revision],
        "replacement claims original candidate as parent",
    );
    git(root, &["replace", &accepted, &fake_parentage]);
    // Ordinary Git observes the attacker-controlled replacement graph.
    git(
        root,
        &[
            "merge-base",
            "--is-ancestor",
            &material.head_revision,
            &accepted,
        ],
    );
    // Historical proof must inspect the actual commit objects instead.
    assert!(!ancestor(root, &material.head_revision, &accepted).unwrap());
    assert!(integration_for_target(root, &material, &accepted).is_err());
}

#[test]
fn replay_uses_original_git_objects_after_branch_removal_and_later_checkout() {
    let (dir, material) = fixture();
    let root = dir.path();
    let candidate_tree = tree(root, &material.head_revision).unwrap();
    let integrated = commit(
        root,
        &candidate_tree,
        &[&material.accepted_base_revision],
        "accepted squash",
    );
    let later_tree = source_tree(root, "later accepted material\n");
    let accepted = commit(root, &later_tree, &[&integrated], "accepted after squash");
    let witness = integration_for_target(root, &material, &accepted).unwrap();
    git(root, &["update-ref", "-d", "refs/heads/candidate"]);
    git(
        root,
        &["checkout", "--quiet", "--force", "--detach", &accepted],
    );
    std::fs::write(
        root.join("subject.txt"),
        "unrelated current worktree edits\n",
    )
    .unwrap();
    let before = git(root, &["status", "--porcelain=v1"]);
    let current_head = git(root, &["rev-parse", "HEAD"]);
    for _ in 0..3 {
        witness.recheck_local(root).unwrap();
    }
    assert_eq!(git(root, &["status", "--porcelain=v1"]), before);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), current_head);
    assert_ne!(current_head, material.head_revision);
}

#[test]
fn repeated_commit_validation_is_unambiguous_only_for_the_same_task_contract() {
    let (dir, material) = fixture();
    let root = dir.path();
    let store = Store {
        kind: super::super::store::StoreKind::Repo,
        root: root.join(".decapod/data"),
    };
    std::fs::create_dir_all(&store.root).unwrap();
    std::fs::write(
        root.join(".decapod/config.toml"),
        toml::to_string(&crate::cli::DecapodProjectConfig::default()).unwrap(),
    )
    .unwrap();
    let conn = Connection::open(events::canonical_db_path(&store.root)).unwrap();
    let seed = test_fixture::record();
    let original = test_fixture::persist(
        &conn,
        &store.root,
        material.clone(),
        seed.task_binding.clone(),
        seed.validation_epoch.clone(),
    )
    .unwrap();
    let repeated = test_fixture::persist(
        &conn,
        &store.root,
        material.clone(),
        seed.task_binding.clone(),
        seed.validation_epoch.clone(),
    )
    .unwrap();
    assert_ne!(original.record_id(), repeated.record_id());
    assert_eq!(
        lookup(root, &store, &material.head_revision)
            .unwrap()
            .record_id(),
        repeated.record_id()
    );
    assert_eq!(
        lookup_record(root, &store, &material.head_revision, original.record_id())
            .unwrap()
            .record_digest(),
        original.record_digest()
    );

    let mut changed_task = seed.task_binding;
    changed_task.contract_digest = digest(&"different task obligations").unwrap();
    test_fixture::persist(
        &conn,
        &store.root,
        material.clone(),
        changed_task,
        seed.validation_epoch,
    )
    .unwrap();
    assert!(lookup(root, &store, &material.head_revision).is_err());
    // Exact replay never silently switches to a later invocation or contract.
    assert_eq!(
        lookup_record(root, &store, &material.head_revision, original.record_id())
            .unwrap()
            .record_digest(),
        original.record_digest()
    );

    conn.execute(
        "UPDATE events SET payload='{}' WHERE stream=?1 AND event_id=?2",
        db::params![events::VERIFICATION, original.record_id()],
    )
    .unwrap();
    assert!(lookup(root, &store, &material.head_revision).is_err());
    assert!(lookup_record(root, &store, &material.head_revision, original.record_id()).is_err());
    assert_eq!(
        lookup_record(root, &store, &material.head_revision, repeated.record_id())
            .unwrap()
            .record_digest(),
        repeated.record_digest()
    );
}
