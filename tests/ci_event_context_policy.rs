//! Executable Git fixtures for the CI-only immutable context boundary.
//! Run without Cargo/network:
//! `rustc --edition=2024 --test tests/ci_event_context_policy.rs -o /tmp/ci-context-tests`
//! `/tmp/ci-context-tests`
#![allow(dead_code)]

#[path = "../assets/tools/ci_event_context.rs"]
mod ci_event_context;
#[path = "../src/decapod/core/release_metadata.rs"]
mod release_metadata;

use ci_event_context::{Context, Inputs, parse_args, resolve};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const ACCEPTED: &str = "refs/remotes/origin/master";

struct Repo(PathBuf);
impl Drop for Repo {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned fixture");
    }
}
impl Repo {
    fn new() -> Self {
        Self::with_format("sha1")
    }
    fn with_format(format: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "decapod-ci-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create private fixture");
        let repo = Self(root);
        repo.git(&[
            "init",
            "-q",
            "-b",
            "master",
            &format!("--object-format={format}"),
        ]);
        repo.git(&["config", "user.name", "CI context fixture"]);
        repo.git(&["config", "user.email", "ci-context@example.invalid"]);
        repo.version("0.107.4");
        fs::write(repo.0.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        fs::create_dir(repo.0.join("src")).unwrap();
        fs::write(repo.0.join("src/lib.rs"), "pub fn original() {}\n").unwrap();
        let base = repo.commit("base");
        repo.git(&["update-ref", ACCEPTED, &base]);
        repo
    }
    fn git(&self, args: &[&str]) -> String {
        let result = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_string()
    }
    fn version(&self, version: &str) {
        fs::write(self.0.join("Cargo.toml"), format!(
            "[package]\nname = \"decapod\"\ndefault-run = \"decapod\"\nversion = \"{version}\"\nedition = \"2024\"\n"
        )).unwrap();
        fs::write(
            self.0.join("Cargo.lock"),
            format!("version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"{version}\"\n"),
        )
        .unwrap();
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.head()
    }
    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
    fn source(&self) -> String {
        fs::write(self.0.join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        self.commit("source change")
    }
    fn inputs(&self, event: &str, base: &str, head: &str) -> Inputs {
        Inputs {
            repo: self.0.clone(),
            event: event.into(),
            base: base.into(),
            head: head.into(),
            pre_pr: false,
            created: false,
            accepted_base_ref: None,
            snapshot: false,
            trigger_ref: None,
        }
    }
    fn pre_pr(&self, base: &str, head: &str) -> Inputs {
        Inputs {
            pre_pr: true,
            accepted_base_ref: Some(ACCEPTED.into()),
            ..self.inputs("workflow_dispatch", base, head)
        }
    }
    fn classify(&self, context: &Context) -> bool {
        release_metadata::classify(&self.0, &context.event, &context.base, &context.head).unwrap()
    }
}

#[test]
fn source_push_pr_and_explicit_pre_pr_share_the_exact_range() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    for input in [
        repo.inputs("push", &base, &head),
        repo.inputs("pull_request", &base, &head),
        repo.pre_pr(&base, &head),
    ] {
        let context = resolve(&input).unwrap();
        assert_eq!(context.base, base);
        assert_eq!(context.head, head);
        assert_eq!(context.merge_base, base);
        assert_eq!(context.checkout_head, head);
        assert_eq!(context.checkout_tree, context.head_tree);
        assert!(!repo.classify(&context));
    }
    let context = resolve(&repo.pre_pr(&base, &head)).unwrap();
    assert_eq!(context.event, "pull_request");
    assert_eq!(context.trigger_event, "workflow_dispatch");
    assert_eq!(context.accepted_base, base);
    assert!(context.pre_pr);
}

#[test]
fn release_only_ranges_interoperate_with_the_shared_classifier() {
    let repo = Repo::new();
    let base = repo.head();
    repo.version("0.107.5");
    let head = repo.commit("release");
    for input in [
        repo.inputs("push", &base, &head),
        repo.inputs("pull_request", &base, &head),
        repo.pre_pr(&base, &head),
    ] {
        let context = resolve(&input).unwrap();
        assert!(repo.classify(&context));
    }
}

#[test]
fn a_source_then_release_push_never_drops_the_source_commit() {
    let repo = Repo::new();
    let base = repo.head();
    let source = repo.source();
    repo.version("0.107.5");
    let head = repo.commit("release");
    let context = resolve(&repo.inputs("push", &base, &head)).unwrap();
    assert_eq!(context.merge_base, base);
    assert_ne!(context.merge_base, source);
    assert!(!repo.classify(&context));
    assert!(release_metadata::classify(&repo.0, "push", &source, &head).unwrap());
}

#[test]
fn plain_manual_dispatch_remains_manual_and_never_implies_pre_pr() {
    let repo = Repo::new();
    let head = repo.head();
    let context = resolve(&repo.inputs("workflow_dispatch", "", &head)).unwrap();
    assert_eq!(context.event, "workflow_dispatch");
    assert!(!context.pre_pr);
    assert!(context.base.is_empty());
    assert!(context.merge_base.is_empty());
    assert!(context.accepted_base.is_empty());
    assert!(!repo.classify(&context));
    let base = head;
    let head = repo.source();
    assert!(resolve(&repo.inputs("workflow_dispatch", &base, &head)).is_err());
    let mut input = repo.inputs("workflow_dispatch", "", &head);
    input.accepted_base_ref = Some(ACCEPTED.into());
    assert!(resolve(&input).is_err());
}

#[test]
fn pre_pr_requires_explicit_dispatch_and_accepted_ref() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    for event in [
        "pull_request",
        "push",
        "pull_request_target",
        "schedule",
        "",
        "push\npre_pr=true",
    ] {
        let mut input = repo.pre_pr(&base, &head);
        input.event = event.into();
        assert!(resolve(&input).is_err(), "accepted {event:?}");
    }
    let mut input = repo.pre_pr(&base, &head);
    input.accepted_base_ref = None;
    assert!(resolve(&input).is_err());
    let mut input = repo.inputs("push", &base, &head);
    input.accepted_base_ref = Some(ACCEPTED.into());
    assert!(resolve(&input).is_err());
}

#[test]
fn malformed_missing_zero_and_ref_like_object_ids_fail_closed() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    let invalid = [
        "".into(),
        "0".repeat(40),
        "0".repeat(64),
        "g".repeat(40),
        "a".repeat(39),
        "a".repeat(41),
        "A".repeat(40),
        "HEAD".into(),
        "master".into(),
        "HEAD~1".into(),
        "--all".into(),
        format!("{head}\npre_pr=true"),
    ];
    for bad in invalid {
        assert!(
            resolve(&repo.inputs("push", &bad, &head)).is_err(),
            "base {bad:?}"
        );
        assert!(
            resolve(&repo.inputs("pull_request", &base, &bad)).is_err(),
            "head {bad:?}"
        );
        assert!(
            resolve(&repo.pre_pr(&bad, &head)).is_err(),
            "pre-PR base {bad:?}"
        );
    }
    for event in ["push", "pull_request"] {
        assert!(resolve(&repo.inputs(event, &head, &head)).is_err());
    }
}

#[test]
fn missing_objects_and_noncommit_objects_are_rejected() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    let blob = repo.git(&["rev-parse", "HEAD:Cargo.toml"]);
    repo.git(&["tag", "-a", "fixture-tag", "-m", "tag", &base]);
    let tag = repo.git(&["rev-parse", "refs/tags/fixture-tag"]);
    for bad in ["f".repeat(40), "f".repeat(64), tree, blob, tag] {
        assert!(resolve(&repo.inputs("push", &bad, &head)).is_err());
        assert!(resolve(&repo.inputs("push", &base, &bad)).is_err());
    }
}

#[test]
fn stale_head_and_same_tree_wrong_commit_are_rejected() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    repo.commit("same tree newer commit");
    for input in [
        repo.inputs("push", &base, &head),
        repo.inputs("pull_request", &base, &head),
        repo.pre_pr(&base, &head),
        repo.inputs("workflow_dispatch", "", &head),
    ] {
        assert!(resolve(&input).is_err());
    }
}

#[test]
fn unrelated_base_and_head_are_rejected_for_every_range_mode() {
    let repo = Repo::new();
    let base = repo.head();
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    let head = repo.git(&["commit-tree", &tree, "-m", "unrelated root"]);
    repo.git(&["checkout", "-q", "--detach", &head]);
    for input in [
        repo.inputs("push", &base, &head),
        repo.inputs("pull_request", &base, &head),
        repo.pre_pr(&base, &head),
    ] {
        assert!(resolve(&input).is_err());
    }
}

#[test]
fn divergent_pr_uses_merge_base_but_cannot_claim_pre_pr_equivalence() {
    let repo = Repo::new();
    let common = repo.head();
    repo.git(&["checkout", "-q", "-b", "candidate"]);
    let head = repo.source();
    repo.git(&["checkout", "-q", "master"]);
    fs::write(repo.0.join("base-only.txt"), "new accepted change\n").unwrap();
    let base = repo.commit("accepted target advanced");
    repo.git(&["update-ref", ACCEPTED, &base]);
    repo.git(&["checkout", "-q", "candidate"]);
    let context = resolve(&repo.inputs("pull_request", &base, &head)).unwrap();
    assert_eq!(context.merge_base, common);
    assert_ne!(context.merge_base, base);
    assert!(resolve(&repo.inputs("push", &base, &head)).is_err());
    assert!(resolve(&repo.pre_pr(&base, &head)).is_err());
}

#[test]
fn advanced_accepted_ref_rejects_stale_base_even_when_both_are_ancestors() {
    let repo = Repo::new();
    let stale = repo.head();
    let current = repo.source();
    repo.git(&["update-ref", ACCEPTED, &current]);
    repo.version("0.107.5");
    let head = repo.commit("candidate");
    assert!(resolve(&repo.pre_pr(&stale, &head)).is_err());
    let context = resolve(&repo.pre_pr(&current, &head)).unwrap();
    assert_eq!(context.accepted_base, current);
    assert_eq!(context.merge_base, current);
    assert!(
        context
            .github_outputs()
            .contains("provider_freshness=not_checked\n")
    );
}

#[test]
fn accepted_ref_must_be_an_explicit_existing_branch_capture() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    repo.git(&["branch", "accepted-local", &base]);
    let mut input = repo.pre_pr(&base, &head);
    input.accepted_base_ref = Some("refs/heads/accepted-local".into());
    assert!(resolve(&input).is_ok());
    for reference in [
        "",
        "HEAD",
        "origin/master",
        "refs/tags/master",
        "refs/remotes/origin/missing",
        "refs/remotes/origin/master~1",
        "refs/remotes/origin/master\npre_pr=true",
        "refs/remotes/origin/../master",
    ] {
        input.accepted_base_ref = Some(reference.into());
        assert!(resolve(&input).is_err(), "accepted ref {reference:?}");
    }
}

#[test]
fn exact_synthetic_pr_merge_is_accepted_with_its_actual_checkout_tree() {
    let repo = Repo::new();
    let common = repo.head();
    repo.git(&["checkout", "-q", "-b", "candidate"]);
    repo.version("0.107.5");
    let head = repo.commit("release");
    repo.git(&["checkout", "-q", "master"]);
    let base = repo.source();
    repo.git(&[
        "merge",
        "-q",
        "--no-ff",
        "candidate",
        "-m",
        "synthetic merge",
    ]);
    let context = resolve(&repo.inputs("pull_request", &base, &head)).unwrap();
    assert_eq!(context.checkout_head, repo.head());
    assert_ne!(context.checkout_head, head);
    assert_ne!(context.checkout_tree, context.head_tree);
    assert_eq!(context.merge_base, common);
    assert!(!repo.classify(&context));
    assert!(resolve(&repo.pre_pr(&base, &head)).is_err());
    assert!(resolve(&repo.inputs("push", &base, &head)).is_err());
}

#[test]
fn exact_release_pr_merge_preserves_release_classification() {
    let repo = Repo::new();
    let base = repo.head();
    repo.git(&["checkout", "-q", "-b", "candidate"]);
    repo.version("0.107.5");
    let head = repo.commit("release");
    repo.git(&["checkout", "-q", "master"]);
    repo.git(&[
        "merge",
        "-q",
        "--no-ff",
        "candidate",
        "-m",
        "synthetic merge",
    ]);
    let context = resolve(&repo.inputs("pull_request", &base, &head)).unwrap();
    assert_eq!(context.checkout_tree, context.head_tree);
    assert!(repo.classify(&context));
}

#[test]
fn synthetic_merge_rejects_wrong_base_reversed_parents_and_extra_parents() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    let other = repo.commit("another candidate");
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    for parents in [
        vec![&head, &base],
        vec![&other, &head],
        vec![&base, &other],
        vec![&base, &head, &other],
        vec![&head],
    ] {
        let mut args = vec!["commit-tree", &tree, "-m", "invalid synthetic merge"];
        for parent in parents {
            args.extend(["-p", parent]);
        }
        let merge = repo.git(&args);
        repo.git(&["checkout", "-q", "--detach", &merge]);
        assert!(resolve(&repo.inputs("pull_request", &base, &head)).is_err());
    }
}

#[test]
fn dirty_worktree_index_and_untracked_files_cannot_attest_a_clean_tree() {
    for kind in ["worktree", "index", "untracked"] {
        let repo = Repo::new();
        let base = repo.head();
        let head = repo.source();
        let path = if kind == "untracked" {
            "untracked.rs"
        } else {
            "src/lib.rs"
        };
        fs::write(repo.0.join(path), "unexpected content\n").unwrap();
        if kind == "index" {
            repo.git(&["add", path]);
        }
        assert!(
            resolve(&repo.pre_pr(&base, &head)).is_err(),
            "accepted {kind}"
        );
    }
}

#[test]
fn incomplete_history_and_legacy_grafts_fail_closed() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    fs::write(repo.0.join(".git/shallow"), format!("{head}\n")).unwrap();
    assert!(resolve(&repo.pre_pr(&base, &head)).is_err());
    fs::remove_file(repo.0.join(".git/shallow")).unwrap();
    fs::write(repo.0.join(".git/info/grafts"), format!("{head}\n")).unwrap();
    assert!(resolve(&repo.pre_pr(&base, &head)).is_err());
}

#[test]
fn replacement_objects_cannot_forge_ancestry() {
    let repo = Repo::new();
    let base = repo.head();
    let source = repo.source();
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    let unrelated = repo.git(&["commit-tree", &tree, "-m", "unrelated root"]);
    repo.git(&["replace", &unrelated, &source]);
    repo.git(&["checkout", "-q", "--detach", &unrelated]);
    // A Git process that follows replacements would wrongly accept ancestry.
    repo.git(&["merge-base", "--is-ancestor", &base, &unrelated]);
    assert!(resolve(&repo.pre_pr(&base, &unrelated)).is_err());
}

#[test]
fn github_outputs_are_fixed_bounded_single_line_records() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    let context = resolve(&repo.pre_pr(&base, &head)).unwrap();
    let output = context.github_outputs();
    assert!(output.len() < 1024);
    let records: Vec<_> = output
        .lines()
        .map(|line| line.split_once('=').unwrap())
        .collect();
    assert_eq!(records.len(), 13);
    let expected = [
        "event",
        "trigger_event",
        "base",
        "head",
        "merge_base",
        "checkout_head",
        "checkout_tree",
        "head_tree",
        "accepted_base",
        "pre_pr",
        "pr_like",
        "local_base_verified",
        "provider_freshness",
    ];
    for ((key, value), expected_key) in records.into_iter().zip(expected) {
        assert_eq!(key, expected_key);
        assert!(
            value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        );
    }
    assert!(output.contains("local_base_verified=true\n"));
    assert!(output.ends_with("provider_freshness=not_checked\n"));
}

#[test]
fn explicit_parser_rejects_missing_duplicate_unknown_and_malformed_flags() {
    let args = [
        "--repo", ".", "--event", "push", "--base", "a", "--head", "b", "--pre-pr", "false",
    ];
    assert!(parse_args(args.map(String::from)).is_ok());
    for start in (0..args.len()).step_by(2) {
        let mut missing = args.to_vec();
        missing.drain(start..start + 2);
        assert!(parse_args(missing.into_iter().map(String::from)).is_err());
    }
    for tail in [
        vec!["--pre-pr", "true"],
        vec!["--unknown", "value"],
        vec!["--accepted-base-ref"],
    ] {
        assert!(parse_args(args.into_iter().chain(tail).map(String::from)).is_err());
    }
    for bad in ["", "1", "yes", "TRUE", "false\npre_pr=true"] {
        let mut changed = args.to_vec();
        changed[9] = bad;
        assert!(parse_args(changed.into_iter().map(String::from)).is_err());
    }
}

#[test]
fn verification_is_read_only_and_never_runs_fsmonitor_configuration() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    repo.git(&[
        "config",
        "core.fsmonitor",
        "an-executable-that-must-not-be-called",
    ]);
    let before_index = fs::read(repo.0.join(".git/index")).unwrap();
    let before_ref = fs::read(repo.0.join(".git/refs/heads/master")).unwrap();
    resolve(&repo.pre_pr(&base, &head)).unwrap();
    assert_eq!(fs::read(repo.0.join(".git/index")).unwrap(), before_index);
    assert_eq!(
        fs::read(repo.0.join(".git/refs/heads/master")).unwrap(),
        before_ref
    );
}

#[test]
fn root_must_be_an_available_checkout_root() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    for path in [
        repo.0.join("src"),
        repo.0.join("absent"),
        repo.0.join(".git"),
    ] {
        let mut input = repo.pre_pr(&base, &head);
        input.repo = path;
        assert!(resolve(&input).is_err());
    }
}

#[test]
fn sha256_repositories_use_full_sha256_context_ids() {
    let repo = Repo::with_format("sha256");
    let base = repo.head();
    let head = repo.source();
    let context = resolve(&repo.pre_pr(&base, &head)).unwrap();
    assert_eq!(context.base.len(), 64);
    assert_eq!(context.head.len(), 64);
    assert_eq!(context.checkout_tree.len(), 64);
    assert!(context.github_outputs().len() < 1024);
}

#[test]
fn repository_content_filters_are_rejected_without_execution() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    repo.git(&[
        "config",
        "filter.fixture.clean",
        "an-executable-that-must-not-be-called",
    ]);
    let error = resolve(&repo.pre_pr(&base, &head)).unwrap_err();
    assert!(error.contains("content filters"));
}

const REQUEST: &str = "a81bc02e-dc82-4b9f-8f15-2c389a012345";
const SNAPSHOT_REF: &str = "refs/heads/decapod/preflight/a81bc02e-dc82-4b9f-8f15-2c389a012345";

impl Repo {
    fn metadata(&self, base: &str, source: &str) -> ci_event_context::SnapshotMetadata {
        ci_event_context::SnapshotMetadata {
            request: REQUEST.into(),
            source: source.into(),
            base: base.into(),
            source_ref: "refs/heads/feature/immutable-ci".into(),
        }
    }
    fn transport(&self, tree: &str, parents: &[&str], message: &str) -> String {
        let mut args = vec!["commit-tree", tree, "-m", message];
        for parent in parents {
            args.extend(["-p", parent]);
        }
        let snapshot = self.git(&args);
        self.git(&["checkout", "-q", "--detach", &snapshot]);
        snapshot
    }
    fn snapshot(&self, metadata: &ci_event_context::SnapshotMetadata) -> String {
        let tree = self.git(&["rev-parse", &format!("{}^{{tree}}", metadata.source)]);
        self.transport(&tree, &[&metadata.source], &metadata.encode().unwrap())
    }
    fn snapshot_inputs(&self, snapshot: &str) -> Inputs {
        Inputs {
            pre_pr: true,
            snapshot: true,
            accepted_base_ref: Some(ACCEPTED.into()),
            trigger_ref: Some(SNAPSHOT_REF.into()),
            ..self.inputs("push", "", snapshot)
        }
    }
}

fn snapshot_fixture() -> (Repo, ci_event_context::SnapshotMetadata, Inputs) {
    let repo = Repo::new();
    let base = repo.head();
    let source = repo.source();
    let metadata = repo.metadata(&base, &source);
    let snapshot = repo.snapshot(&metadata);
    let input = repo.snapshot_inputs(&snapshot);
    (repo, metadata, input)
}

#[test]
fn snapshot_roundtrips_exact_metadata_and_emits_original_candidate_context() {
    let (repo, metadata, input) = snapshot_fixture();
    let text = metadata.encode().unwrap();
    assert_eq!(
        ci_event_context::SnapshotMetadata::parse(&text).unwrap(),
        metadata
    );
    assert_eq!(text.split('\n').count(), 8);
    let context = resolve(&input).unwrap();
    assert_eq!(context.event, "pull_request");
    assert_eq!(context.trigger_event, "push");
    assert_eq!(context.head, metadata.source);
    assert_eq!(context.base, metadata.base);
    assert_eq!(context.merge_base, metadata.base);
    assert_eq!(context.checkout_head, input.head);
    assert_ne!(context.head, context.checkout_head);
    assert_eq!(context.checkout_tree, context.head_tree);
    assert!(context.pre_pr);
    assert_eq!(context.snapshot, Some(metadata.clone()));
    let output = context.github_outputs();
    assert!(output.contains(&format!("transport_head={}\n", input.head)));
    assert!(output.contains(&format!("request_id={REQUEST}\n")));
    assert!(output.contains("source_ref=refs/heads/feature/immutable-ci\n"));
    assert!(output.contains("provider_freshness=not_checked\n"));
    assert!(output.len() < 2048);
    assert!(!repo.classify(&context));
    let mut explicit_base = input;
    explicit_base.base = metadata.base;
    assert_eq!(resolve(&explicit_base).unwrap(), context);
}

#[test]
fn snapshot_release_range_classifies_the_original_source_not_transport_parent() {
    let repo = Repo::new();
    let base = repo.head();
    repo.version("0.107.5");
    let source = repo.commit("release candidate");
    let metadata = repo.metadata(&base, &source);
    let snapshot = repo.snapshot(&metadata);
    let context = resolve(&repo.snapshot_inputs(&snapshot)).unwrap();
    assert_eq!(context.head, source);
    assert_eq!(context.base, base);
    assert!(repo.classify(&context));
    // Source work before a version bump must remain part of the checked range.
    let (repo, mut metadata, _) = snapshot_fixture();
    repo.git(&["checkout", "-q", "--detach", &metadata.source]);
    repo.version("0.107.5");
    metadata.source = repo.commit("version after source work");
    let snapshot = repo.snapshot(&metadata);
    let context = resolve(&repo.snapshot_inputs(&snapshot)).unwrap();
    assert!(!repo.classify(&context));
}

#[test]
fn snapshot_parser_rejects_extra_duplicate_reordered_and_malformed_fields() {
    let (repo, metadata, _) = snapshot_fixture();
    let text = metadata.encode().unwrap();
    for bad in [
        text.trim_end_matches('\n').to_string(),
        format!("{text}\n"),
        format!("{text}base={}\n", metadata.base),
        text.replace("schema=1", "schema=2"),
        text.replace("decapod CI preflight snapshot", "ordinary commit"),
        text.replace("source_ref=", "base="),
        text.replace("source=", "source_ref="),
        text.replace("request=", "request ="),
        text.replace('\n', "\r\n"),
        text.replace(REQUEST, "A81BC02E-DC82-4B9F-8F15-2C389A012345"),
        text.replace(REQUEST, "00000000-0000-0000-0000-000000000000"),
        text.replace(&metadata.source, "HEAD"),
        text.replace(&metadata.base, &"0".repeat(40)),
        text.replace(&metadata.base, &metadata.source),
        text.replace("refs/heads/feature/immutable-ci", "origin/master"),
        text.replace("refs/heads/feature/immutable-ci", SNAPSHOT_REF),
        text.replace("refs/heads/feature/immutable-ci", "refs/heads/bad name"),
        text.replace("refs/heads/feature/immutable-ci", "refs/heads/bad.lock"),
        text.replace("refs/heads/feature/immutable-ci", "refs/heads/../master"),
    ] {
        assert!(
            ci_event_context::SnapshotMetadata::parse(&bad).is_err(),
            "accepted {bad:?}"
        );
        let tree = repo.git(&["rev-parse", &format!("{}^{{tree}}", metadata.source)]);
        let snapshot = repo.transport(&tree, &[&metadata.source], &bad);
        // Git normalizes a missing final LF itself; test that parser boundary
        // directly above, and all other encodings through the actual resolver.
        if !bad.ends_with('\n') {
            continue;
        }
        assert!(
            resolve(&repo.snapshot_inputs(&snapshot)).is_err(),
            "accepted commit {bad:?}"
        );
    }
}

#[test]
fn snapshot_encoder_rejects_invalid_or_unbounded_public_metadata() {
    let (_, metadata, _) = snapshot_fixture();
    for field in ["request", "source", "base", "source_ref"] {
        let mut bad = metadata.clone();
        match field {
            "request" => bad.request = "not-a-uuid".into(),
            "source" => bad.source = "HEAD".into(),
            "base" => bad.base = "a\nbase=injected".into(),
            _ => bad.source_ref = format!("refs/heads/{}", "a".repeat(513)),
        }
        assert!(bad.encode().is_err());
    }
}

#[test]
fn snapshot_requires_explicit_push_profile_and_exact_provider_ref() {
    let (_repo, _, input) = snapshot_fixture();
    for event in ["pull_request", "workflow_dispatch", "schedule"] {
        let mut bad = Inputs {
            event: event.into(),
            ..copy_inputs(&input)
        };
        assert!(resolve(&bad).is_err());
        bad.pre_pr = false;
        assert!(resolve(&bad).is_err());
    }
    let mut bad = copy_inputs(&input);
    bad.pre_pr = false;
    assert!(resolve(&bad).is_err());
    for reference in [
        None,
        Some("refs/heads/master"),
        Some("refs/heads/preflight/a81bc02e-dc82-4b9f-8f15-2c389a012345"),
        Some("refs/heads/decapod/preflight/A81BC02E-DC82-4B9F-8F15-2C389A012345"),
        Some("refs/heads/decapod/preflight/a81bc02e-dc82-4b9f-8f15-2c389a012346"),
        Some("refs/heads/decapod/preflight/a81bc02e-dc82-4b9f-8f15-2c389a012345/extra"),
        Some("refs/heads/decapod/preflight/a81bc02e-dc82-4b9f-8f15-2c389a012345\npre_pr=true"),
    ] {
        let mut bad = copy_inputs(&input);
        bad.trigger_ref = reference.map(String::from);
        assert!(resolve(&bad).is_err(), "accepted {reference:?}");
    }
}

fn copy_inputs(input: &Inputs) -> Inputs {
    Inputs {
        repo: input.repo.clone(),
        event: input.event.clone(),
        base: input.base.clone(),
        head: input.head.clone(),
        pre_pr: input.pre_pr,
        created: input.created,
        accepted_base_ref: input.accepted_base_ref.clone(),
        snapshot: input.snapshot,
        trigger_ref: input.trigger_ref.clone(),
    }
}

fn created_push(repo: &Repo, head: &str) -> Inputs {
    Inputs {
        created: true,
        accepted_base_ref: Some(ACCEPTED.into()),
        trigger_ref: Some("refs/heads/new-candidate".into()),
        ..repo.inputs("push", &"0".repeat(head.len()), head)
    }
}

#[test]
fn created_push_binds_accepted_tip_without_claiming_pr_readiness() {
    for format in ["sha1", "sha256"] {
        let repo = Repo::with_format(format);
        let base = repo.head();
        let head = repo.source();
        let context = resolve(&created_push(&repo, &head)).unwrap();
        assert_eq!(context.event, "push");
        assert_eq!(context.trigger_event, "push");
        assert_eq!(context.base, base);
        assert_eq!(context.accepted_base, base);
        assert_eq!(context.merge_base, base);
        assert_eq!(context.head, head);
        assert!(!context.pre_pr);
        assert!(context.snapshot.is_none());
        let outputs = context.github_outputs();
        assert!(outputs.contains("pr_like=false\n"));
        assert!(outputs.contains("local_base_verified=true\n"));
        assert!(outputs.contains("provider_freshness=not_checked\n"));
        assert!(!repo.classify(&context));
    }
}

#[test]
fn created_push_requires_authoritative_flag_exact_zero_before_and_branch_scope() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    let mut absent = created_push(&repo, &head);
    absent.created = false;
    assert!(resolve(&absent).is_err());
    for before in ["".to_string(), "0".repeat(39), "0".repeat(64), base] {
        let mut input = created_push(&repo, &head);
        input.base = before;
        assert!(resolve(&input).is_err());
    }
    for reference in [
        None,
        Some("refs/tags/new"),
        Some(SNAPSHOT_REF),
        Some("new-candidate"),
    ] {
        let mut input = created_push(&repo, &head);
        input.trigger_ref = reference.map(str::to_string);
        assert!(resolve(&input).is_err());
    }
    for mode in [
        "pull_request",
        "workflow_dispatch",
        "schedule",
        "pre_pr",
        "snapshot",
    ] {
        let mut input = created_push(&repo, &head);
        match mode {
            "pre_pr" => input.pre_pr = true,
            "snapshot" => input.snapshot = true,
            _ => input.event = mode.into(),
        }
        assert!(resolve(&input).is_err(), "accepted {mode}");
    }
}

#[test]
fn created_push_rejects_missing_advanced_unrelated_or_dirty_accepted_context() {
    for case in [
        "missing",
        "advanced",
        "unrelated",
        "dirty",
        "wrong-head",
        "no-ref",
    ] {
        let repo = Repo::new();
        let base = repo.head();
        let head = repo.source();
        let mut input = created_push(&repo, &head);
        match case {
            "missing" => {
                repo.git(&["update-ref", "-d", ACCEPTED]);
            }
            "advanced" => {
                let advanced = repo.commit("accepted tip advanced");
                repo.git(&["update-ref", ACCEPTED, &advanced]);
                repo.git(&["checkout", "-q", "--detach", &head]);
            }
            "unrelated" => {
                let tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
                let unrelated = repo.git(&["commit-tree", &tree, "-m", "unrelated root"]);
                repo.git(&["update-ref", ACCEPTED, &unrelated]);
            }
            "dirty" => fs::write(repo.0.join("src/lib.rs"), "uncommitted\n").unwrap(),
            "wrong-head" => {
                repo.git(&["checkout", "-q", "--detach", &base]);
            }
            "no-ref" => input.accepted_base_ref = None,
            _ => unreachable!(),
        }
        assert!(resolve(&input).is_err(), "accepted {case}");
    }
}

#[test]
fn created_cli_flag_is_explicit_boolean_and_defaults_false() {
    let args = [
        "--repo", ".", "--event", "push", "--base", "", "--head", "", "--pre-pr", "false",
    ];
    assert!(!parse_args(args.map(str::to_string)).unwrap().created);
    let mut explicit = args.map(str::to_string).to_vec();
    explicit.extend(["--created".into(), "true".into()]);
    assert!(parse_args(explicit.clone()).unwrap().created);
    for value in ["", "1", "TRUE", "true\npre_pr=true"] {
        *explicit.last_mut().unwrap() = value.into();
        assert!(parse_args(explicit.clone()).is_err());
    }
}

#[test]
fn action_argument_vector_is_nonempty_and_preserves_event_context() {
    let action = include_str!("../.github/actions/ci-context/action.yml");
    assert!(action.contains("EVENT_CREATED: ${{ github.event.created == true }}"));
    // This structural invariant also catches the Bash 3.2 nounset regression
    // when the fixture is executed by a newer Bash that accepts empty arrays.
    assert!(action.contains("args=(--repo \"$GITHUB_WORKSPACE\" --event \"$RAW_EVENT\")"));
    assert!(action.contains("\"${args[@]}\" > \"$private_dir/outputs\""));
    let selection = action
        .split_once("        base=\"$EVENT_BASE\"")
        .unwrap()
        .1
        .split_once("        private_dir=")
        .unwrap()
        .0;
    let selection = selection
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let script = format!(
        "set -eu\nbase=\"$EVENT_BASE\"\n{selection}\nprintf '%s\\n' \"$pre_pr\" \"$snapshot\" \"${{args[@]}}\"\n"
    );
    for (event, reference, created, optional) in [
        ("pull_request", "refs/pull/1401/merge", "false", vec![]),
        ("push", "refs/heads/new-candidate", "false", vec![]),
        (
            "push",
            "refs/heads/new-candidate",
            "true",
            vec![
                "--created",
                "true",
                "--trigger-ref",
                "refs/heads/new-candidate",
                "--accepted-base-ref",
                ACCEPTED,
            ],
        ),
        (
            "push",
            SNAPSHOT_REF,
            "true",
            vec![
                "--snapshot",
                "true",
                "--trigger-ref",
                SNAPSHOT_REF,
                "--accepted-base-ref",
                ACCEPTED,
            ],
        ),
    ] {
        let output = Command::new("bash")
            .args(["-c", &script])
            .env("RAW_EVENT", event)
            .env("RAW_REF", reference)
            .env("EVENT_CREATED", created)
            .env("EVENT_BASE", "0".repeat(40))
            .env("EVENT_HEAD", "a".repeat(40))
            .env("GITHUB_WORKSPACE", "/fixture with spaces")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let snapshot = reference == SNAPSHOT_REF;
        let flag = if snapshot { "true" } else { "false" };
        let base = if snapshot {
            String::new()
        } else {
            "0".repeat(40)
        };
        let head = "a".repeat(40);
        let mut expected = vec![
            flag,
            flag,
            "--repo",
            "/fixture with spaces",
            "--event",
            event,
        ];
        expected.extend(optional);
        expected.extend(["--base", &base, "--head", &head, "--pre-pr", flag]);
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            text,
            format!("{}\n", expected.join("\n")),
            "{event} {reference} {created}"
        );
    }
}

#[test]
fn snapshot_rejects_missing_wrong_or_multiple_parents_and_mismatched_source() {
    let (repo, metadata, _) = snapshot_fixture();
    let tree = repo.git(&["rev-parse", &format!("{}^{{tree}}", metadata.source)]);
    for parents in [
        vec![],
        vec![metadata.base.as_str()],
        vec![metadata.base.as_str(), metadata.source.as_str()],
        vec![metadata.source.as_str(), metadata.base.as_str()],
    ] {
        let snapshot = repo.transport(&tree, &parents, &metadata.encode().unwrap());
        assert!(resolve(&repo.snapshot_inputs(&snapshot)).is_err());
    }
    let alternate = repo.git(&[
        "commit-tree",
        &tree,
        "-p",
        &metadata.base,
        "-m",
        "different source, identical tree",
    ]);
    let snapshot = repo.transport(&tree, &[&alternate], &metadata.encode().unwrap());
    assert!(resolve(&repo.snapshot_inputs(&snapshot)).is_err());
}

#[test]
fn snapshot_rejects_event_head_or_base_substitution() {
    let (repo, metadata, input) = snapshot_fixture();
    let mut bad = copy_inputs(&input);
    bad.head = metadata.source.clone();
    assert!(resolve(&bad).is_err());
    for base in [
        &metadata.source,
        &"HEAD".to_string(),
        &"f".repeat(40),
        &format!("{}\nhead=injected", metadata.base),
    ] {
        bad = copy_inputs(&input);
        bad.base = base.clone();
        assert!(resolve(&bad).is_err());
    }
    repo.commit("a later checkout");
    assert!(resolve(&input).is_err());
}

#[test]
fn snapshot_rejects_dirty_or_untracked_exports_and_different_committed_trees() {
    for dirty in ["worktree", "index", "untracked"] {
        let (repo, metadata, input) = snapshot_fixture();
        let path = if dirty == "untracked" {
            "new-source.rs"
        } else {
            "src/lib.rs"
        };
        fs::write(repo.0.join(path), "not in the original candidate\n").unwrap();
        if dirty == "index" {
            repo.git(&["add", path]);
        }
        assert!(resolve(&input).is_err(), "accepted {dirty}");
        repo.git(&["add", "."]);
        let exported_tree = repo.git(&["write-tree"]);
        let changed = repo.transport(
            &exported_tree,
            &[&metadata.source],
            &metadata.encode().unwrap(),
        );
        assert!(
            resolve(&repo.snapshot_inputs(&changed)).is_err(),
            "accepted {dirty} tree export"
        );
    }
}

#[test]
fn snapshot_rejects_stale_accepted_tip_even_when_new_tip_is_an_ancestor() {
    let repo = Repo::new();
    let old_base = repo.head();
    let current_base = repo.source();
    repo.git(&["update-ref", ACCEPTED, &current_base]);
    repo.version("0.107.5");
    let source = repo.commit("candidate above current accepted tip");
    let mut metadata = repo.metadata(&old_base, &source);
    let stale_snapshot = repo.snapshot(&metadata);
    assert!(resolve(&repo.snapshot_inputs(&stale_snapshot)).is_err());
    metadata.base = current_base;
    let current_snapshot = repo.snapshot(&metadata);
    assert!(resolve(&repo.snapshot_inputs(&current_snapshot)).is_ok());
}

#[test]
fn snapshot_rejects_unrelated_or_nonancestor_accepted_base() {
    let (repo, mut metadata, _) = snapshot_fixture();
    let tree = repo.git(&["rev-parse", &format!("{}^{{tree}}", metadata.source)]);
    let unrelated = repo.git(&["commit-tree", &tree, "-m", "unrelated base"]);
    let descendant = repo.git(&[
        "commit-tree",
        &tree,
        "-p",
        &metadata.source,
        "-m",
        "base after candidate",
    ]);
    for base in [unrelated, descendant] {
        metadata.base = base;
        repo.git(&["update-ref", ACCEPTED, &metadata.base]);
        let snapshot = repo.snapshot(&metadata);
        assert!(resolve(&repo.snapshot_inputs(&snapshot)).is_err());
    }
}

#[test]
fn snapshot_rejects_missing_ref_and_replacement_shallow_or_grafted_views() {
    for view in ["accepted", "replace", "shallow", "graft"] {
        let (repo, metadata, input) = snapshot_fixture();
        match view {
            "accepted" => {
                repo.git(&["update-ref", "-d", ACCEPTED]);
            }
            "replace" => {
                repo.git(&["replace", &metadata.base, &metadata.source]);
            }
            "shallow" => {
                fs::write(repo.0.join(".git/shallow"), format!("{}\n", input.head)).unwrap();
            }
            _ => {
                fs::write(repo.0.join(".git/info/grafts"), format!("{}\n", input.head)).unwrap();
            }
        }
        assert!(resolve(&input).is_err(), "accepted {view}");
    }
}

#[test]
fn snapshot_sha256_transport_preserves_exact_tree_and_original_range() {
    let repo = Repo::with_format("sha256");
    let base = repo.head();
    let source = repo.source();
    let metadata = repo.metadata(&base, &source);
    let snapshot = repo.snapshot(&metadata);
    let context = resolve(&repo.snapshot_inputs(&snapshot)).unwrap();
    assert_eq!(context.head.len(), 64);
    assert_eq!(context.checkout_head.len(), 64);
    assert_eq!(context.checkout_tree, context.head_tree);
}

#[test]
fn snapshot_cli_flags_are_explicit_and_ordinary_modes_do_not_inherit_them() {
    let args = [
        "--repo",
        ".",
        "--event",
        "push",
        "--base",
        "",
        "--head",
        "a",
        "--pre-pr",
        "true",
        "--snapshot",
        "true",
        "--trigger-ref",
        SNAPSHOT_REF,
        "--accepted-base-ref",
        ACCEPTED,
    ];
    let input = parse_args(args.map(String::from)).unwrap();
    assert!(input.snapshot && input.pre_pr);
    assert_eq!(input.trigger_ref.as_deref(), Some(SNAPSHOT_REF));
    for bad in ["", "1", "TRUE", "yes", "false\nsnapshot=true"] {
        let mut bad_args = args;
        bad_args[11] = bad;
        assert!(parse_args(bad_args.map(String::from)).is_err());
    }
    let (repo, metadata, _) = snapshot_fixture();
    repo.git(&["checkout", "-q", "--detach", &metadata.source]);
    for mut input in [
        repo.inputs("push", &metadata.base, &metadata.source),
        repo.inputs("pull_request", &metadata.base, &metadata.source),
        repo.inputs("workflow_dispatch", "", &metadata.source),
        repo.pre_pr(&metadata.base, &metadata.source),
    ] {
        assert!(resolve(&input).unwrap().snapshot.is_none());
        input.trigger_ref = Some(SNAPSHOT_REF.into());
        assert!(resolve(&input).is_err());
    }
}

#[test]
fn snapshot_rejects_index_flags_that_would_hide_dirty_tracked_source() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let (repo, _, input) = snapshot_fixture();
        repo.git(&["update-index", flag, "src/lib.rs"]);
        fs::write(repo.0.join("src/lib.rs"), "hidden dirty source\n").unwrap();
        // Ordinary Git diff may suppress these paths; the snapshot boundary
        // rejects the index flags instead of claiming that state was checked.
        let error = resolve(&input).unwrap_err();
        assert!(error.contains("hidden tracked paths"));
    }
}

#[test]
fn normalized_snapshot_rechecks_transport_after_detach_and_generated_projection_refresh() {
    let (repo, metadata, input) = snapshot_fixture();
    repo.git(&["checkout", "-q", "--detach", &metadata.source]);
    // The bounded validator may refresh generated projections; immutable
    // source/range proof still comes from H/S, not those working-tree writes.
    fs::write(repo.0.join("generated-validation-projection"), "temporary").unwrap();
    ci_event_context::verify_normalized_snapshot(
        &repo.0,
        &input.head,
        SNAPSHOT_REF,
        &metadata.base,
        &metadata.source,
        ACCEPTED,
    )
    .unwrap();
    for (transport, trigger, base, head) in [
        (
            metadata.source.as_str(),
            SNAPSHOT_REF,
            metadata.base.as_str(),
            metadata.source.as_str(),
        ),
        (
            input.head.as_str(),
            "refs/heads/master",
            metadata.base.as_str(),
            metadata.source.as_str(),
        ),
        (
            input.head.as_str(),
            SNAPSHOT_REF,
            metadata.source.as_str(),
            metadata.source.as_str(),
        ),
        (
            input.head.as_str(),
            SNAPSHOT_REF,
            metadata.base.as_str(),
            metadata.base.as_str(),
        ),
    ] {
        assert!(
            ci_event_context::verify_normalized_snapshot(
                &repo.0, transport, trigger, base, head, ACCEPTED
            )
            .is_err()
        );
    }
}

#[test]
fn normalized_snapshot_rejects_changed_tree_parent_and_advanced_base() {
    for mutation in ["tree", "parent", "advanced-base"] {
        let (repo, metadata, input) = snapshot_fixture();
        repo.git(&["checkout", "-q", "--detach", &metadata.source]);
        let transport = match mutation {
            "tree" => {
                let other = repo.git(&["rev-parse", &format!("{}^{{tree}}", metadata.base)]);
                repo.transport(&other, &[&metadata.source], &metadata.encode().unwrap())
            }
            "parent" => {
                let tree = repo.git(&["rev-parse", &format!("{}^{{tree}}", metadata.source)]);
                repo.transport(&tree, &[&metadata.base], &metadata.encode().unwrap())
            }
            _ => {
                repo.git(&["update-ref", ACCEPTED, &metadata.source]);
                input.head.clone()
            }
        };
        assert!(
            ci_event_context::verify_normalized_snapshot(
                &repo.0,
                &transport,
                SNAPSHOT_REF,
                &metadata.base,
                &metadata.source,
                ACCEPTED
            )
            .is_err(),
            "{mutation}"
        );
    }
}

const WORKBRANCH: &str = "refs/heads/agent/ci-fixture/task-123";

fn workbranch(repo: &Repo, head: &str) -> Inputs {
    Inputs {
        pre_pr: true,
        trigger_ref: Some(WORKBRANCH.into()),
        accepted_base_ref: Some(ACCEPTED.into()),
        ..repo.inputs("push", "", head)
    }
}

#[test]
fn workbranch_uses_actual_source_without_synthetic_commit_or_ref() {
    for format in ["sha1", "sha256"] {
        let repo = Repo::with_format(format);
        let base = repo.head();
        repo.git(&["checkout", "-qb", "agent/ci-fixture/task-123"]);
        let head = repo.source();
        let refs_before = repo.git(&["show-ref"]);
        let context = resolve(&workbranch(&repo, &head)).unwrap();
        assert_eq!(context.event, "pull_request");
        assert_eq!(context.trigger_event, "push");
        assert_eq!(context.base, base);
        assert_eq!(context.merge_base, base);
        assert_eq!(context.accepted_base, base);
        assert_eq!(context.head, head);
        assert_eq!(context.checkout_head, head);
        assert_eq!(context.checkout_tree, context.head_tree);
        assert!(context.pre_pr);
        assert!(context.snapshot.is_none());
        let outputs = context.github_outputs();
        assert!(outputs.contains("workbranch=true\n"));
        assert!(outputs.contains(&format!("source_ref={WORKBRANCH}\n")));
        assert!(!outputs.contains("transport_head="));
        assert!(!outputs.contains("request_id="));
        assert_eq!(repo.git(&["show-ref"]), refs_before);
        assert_eq!(repo.git(&["symbolic-ref", "HEAD"]), WORKBRANCH);
    }
}

#[test]
fn workbranch_updates_compare_all_source_work_with_captured_target() {
    let repo = Repo::new();
    let base = repo.head();
    let before = repo.source();
    repo.version("0.107.5");
    let head = repo.commit("later version change on same branch");
    let context = resolve(&workbranch(&repo, &head)).unwrap();
    assert_eq!(context.base, base);
    assert_ne!(context.base, before);
    assert!(
        !repo.classify(&context),
        "source history must not get release-only exemption"
    );
    let mut wrong = workbranch(&repo, &head);
    wrong.base = before;
    assert!(resolve(&wrong).is_err());
}

#[test]
fn workbranch_requires_exact_branch_and_accepted_source_identity() {
    for case in [
        "ref",
        "missing",
        "accepted",
        "dirty",
        "untracked",
        "hidden",
        "head",
        "created",
        "replacement",
        "graft",
    ] {
        let repo = Repo::new();
        let base = repo.head();
        let head = repo.source();
        let mut input = workbranch(&repo, &head);
        match case {
            "ref" => input.trigger_ref = Some("refs/heads/master".into()),
            "missing" => input.accepted_base_ref = None,
            "accepted" => {
                repo.git(&["update-ref", ACCEPTED, &head]);
            }
            "dirty" => fs::write(repo.0.join("src/lib.rs"), "changed outside commit").unwrap(),
            "untracked" => fs::write(repo.0.join("new.rs"), "uncommitted").unwrap(),
            "hidden" => {
                repo.git(&["update-index", "--assume-unchanged", "src/lib.rs"]);
            }
            "head" => input.head = base.clone(),
            "created" => input.created = true,
            "replacement" => {
                repo.git(&["replace", &head, &base]);
            }
            "graft" => fs::write(repo.0.join(".git/info/grafts"), &head).unwrap(),
            _ => unreachable!(),
        }
        assert!(resolve(&input).is_err(), "accepted {case}");
    }
    let repo = Repo::new();
    let head = repo.source();
    for reference in [
        "refs/heads/agent",
        "refs/heads/agent/name",
        "refs/tags/agent/name/task",
        "refs/heads/agent/name/task.lock",
        SNAPSHOT_REF,
    ] {
        let mut input = workbranch(&repo, &head);
        input.trigger_ref = Some(reference.into());
        assert!(resolve(&input).is_err(), "accepted {reference}");
    }
}

#[test]
fn normalized_workbranch_rechecks_objects_without_rejecting_supported_projection_refresh() {
    let repo = Repo::new();
    let base = repo.head();
    let head = repo.source();
    resolve(&workbranch(&repo, &head)).unwrap();
    fs::write(
        repo.0.join("projection.txt"),
        "supported generated projection",
    )
    .unwrap();
    ci_event_context::verify_normalized_workbranch(&repo.0, WORKBRANCH, &base, &head, ACCEPTED)
        .unwrap();
    for (reference, supplied_base, supplied_head) in [
        ("refs/heads/master", base.as_str(), head.as_str()),
        (WORKBRANCH, head.as_str(), head.as_str()),
        (WORKBRANCH, base.as_str(), base.as_str()),
    ] {
        assert!(
            ci_event_context::verify_normalized_workbranch(
                &repo.0,
                reference,
                supplied_base,
                supplied_head,
                ACCEPTED
            )
            .is_err()
        );
    }
    repo.git(&["update-ref", ACCEPTED, &head]);
    assert!(
        ci_event_context::verify_normalized_workbranch(&repo.0, WORKBRANCH, &base, &head, ACCEPTED)
            .is_err()
    );
}

// This runs in //:ci_event_context_policy as well as Cargo: the new config
// custody boundary is not dependent on manually running the Python fixture suite.
#[test]
#[cfg(unix)]
fn action_workbranch_target_config_is_bound_to_regular_source_blob() {
    let action = include_str!("../.github/actions/ci-context/action.yml");
    let selection = action
        .split_once("        base=\"$EVENT_BASE\"")
        .unwrap()
        .1
        .split_once("        private_dir=")
        .unwrap()
        .0;
    let selection = selection
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let script = format!(
        "set -eu\nbase=\"$EVENT_BASE\"\n{selection}\nprintf '%s\\n' \"$pre_pr\" \"$snapshot\" \"${{args[@]}}\"\n"
    );
    for case in [
        "regular",
        "ignored",
        "symlink",
        "missing",
        "malformed",
        "integer",
        "duplicate",
    ] {
        let repo = Repo::new();
        fs::create_dir(repo.0.join(".decapod")).unwrap();
        let config = repo.0.join(".decapod/config.toml");
        let text = match case {
            "malformed" => "not TOML",
            "integer" => "[repo]\nbase_branch = 1\n",
            "duplicate" => "[repo]\nbase_branch = 'master'\nbase_branch = 'trunk'\n",
            _ => "[repo]\nbase_branch = 'trunk'\n",
        };
        match case {
            "ignored" => {
                fs::write(repo.0.join(".gitignore"), ".decapod/\n").unwrap();
                fs::write(&config, text).unwrap();
            }
            "symlink" => std::os::unix::fs::symlink("../Cargo.toml", &config).unwrap(),
            "missing" => {}
            _ => fs::write(&config, text).unwrap(),
        }
        let head = repo.commit("config fixture");
        let output = Command::new("bash")
            .args(["-c", &script])
            .env("RAW_EVENT", "push")
            .env("RAW_REF", WORKBRANCH)
            .env("EVENT_CREATED", "true")
            .env("EVENT_BASE", "0".repeat(40))
            .env("EVENT_HEAD", &head)
            .env("GITHUB_WORKSPACE", &repo.0)
            .env("GIT_DIR", "/not/the/repository")
            .env("GIT_INDEX_FILE", "/not/the/index")
            .output()
            .unwrap();
        if case == "regular" {
            assert!(output.status.success(), "{output:?}");
            let text = String::from_utf8(output.stdout).unwrap();
            assert_eq!(
                text,
                format!(
                    "true\nfalse\n--repo\n{}\n--event\npush\n--trigger-ref\n{WORKBRANCH}\n\
                     --accepted-base-ref\nrefs/remotes/origin/trunk\n--base\n\n\
                     --head\n{head}\n--pre-pr\ntrue\n",
                    repo.0.display()
                )
            );
        } else {
            assert!(!output.status.success(), "accepted {case}");
            assert!(
                output.stdout.is_empty(),
                "partial context escaped for {case}"
            );
        }
    }
}
