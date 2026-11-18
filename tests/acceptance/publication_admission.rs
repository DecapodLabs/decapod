//! Opt-in real GitHub acceptance. No forge response or proof runner is replaced.
//! Run only against an explicitly supplied, empty disposable repository.
use decapod::core::bounded_process::BoundedCommand;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_evidence::{Conclusion, Observation, SuiteEvidence};
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{self, RepositoryContract, RepositoryExecution};
use decapod::core::validation_runtime::{self, Options};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

struct Demo {
    binary: PathBuf,
    directory: PathBuf,
    repository: String,
    sequence: std::cell::Cell<u32>,
}

impl Demo {
    fn run(&self, root: &Path, program: &Path, args: &[&str]) -> Output {
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        let output = Command::new(program)
            .current_dir(root)
            .args(args)
            .env("DECAPOD_AGENT_ID", "real-forge-admission")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .bounded_output(Duration::from_secs(300))
            .unwrap();
        // Raw transcripts are private local diagnostics, never Git artifacts.
        fs::write(
            self.directory.join(format!("{sequence:03}.stdout")),
            &output.stdout,
        )
        .unwrap();
        fs::write(
            self.directory.join(format!("{sequence:03}.stderr")),
            &output.stderr,
        )
        .unwrap();
        output
    }
    fn ok(&self, root: &Path, program: &Path, args: &[&str]) -> Output {
        let output = self.run(root, program, args);
        assert!(
            output.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    fn cli(&self, root: &Path, args: &[&str]) -> Value {
        let output = self.ok(root, &self.binary, args);
        serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
    }
    fn git(&self, root: &Path, args: &[&str]) -> String {
        String::from_utf8(self.ok(root, Path::new("/usr/bin/git"), args).stdout)
            .unwrap()
            .trim()
            .into()
    }
    fn gh(&self, root: &Path, args: &[&str]) -> Value {
        serde_json::from_slice(&self.ok(root, Path::new("gh"), args).stdout).unwrap()
    }
    fn receipt(&self, report: &Value, label: &str) -> Value {
        assert_eq!(report["publication_ready"], true, "{report}");
        let path = PathBuf::from(report["readiness"]["evidence_path"].as_str().unwrap());
        let bytes = fs::read(path.join("completion/native-readiness-receipt.json")).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(!text.contains(self.directory.to_str().unwrap()));
        fs::write(self.directory.join(format!("{label}-receipt.json")), &bytes).unwrap();
        fs::copy(
            path.join("observations.json"),
            self.directory.join(format!("{label}-observations.json")),
        )
        .unwrap();
        let observed: SuiteEvidence =
            serde_json::from_slice(&fs::read(path.join("observations.json")).unwrap()).unwrap();
        assert_eq!(observed.results.len(), 2);
        assert!(observed.results.iter().all(|row| matches!(
            row.observation,
            Observation::Executed {
                conclusion: Conclusion::Success,
                exit_code: Some(0),
                ..
            }
        )));
        serde_json::from_slice(&bytes).unwrap()
    }
    fn reject(&self, root: &Path, args: &[&str]) -> String {
        let output = self.run(root, &self.binary, args);
        assert!(
            !output.status.success(),
            "rejected action succeeded: {args:?}"
        );
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
    fn checkpoint(&self, root: &Path, id: &str) {
        self.cli(root, &["rpc", "--op", "specs.refresh"]);
        for spec in ["ARCHITECTURE", "INTERFACES", "SECURITY"] {
            self.cli(root, &["govern", "plan", "review-spec", "--path", &format!(".decapod/managed/specs/{spec}.md"), "--disposition", "updated", "--reason", "Reviewed the current literal source assertions and exact-publication contract against the accepted fixture baseline"]);
        }
        self.git(root, &["add", "-A"]);
        self.cli(
            root,
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                id,
                "--summary",
                id,
                "--proof-ref",
                "acceptance:source-contract-control",
            ],
        );
        self.git(root, &["add", ".decapod/governance.json"]);
        self.git(root, &["commit", "-m", &format!("test: {id}")]);
    }
}

fn codes(reason: &str) -> Vec<String> {
    let mut result: Vec<_> = reason
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| {
            word.len() > 5
                && word.contains('_')
                && word
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        })
        .map(str::to_owned)
        .collect();
    result.sort();
    result.dedup();
    result
}

fn configure_accepted_fixture(root: &Path, output: &Path, repository: &str) {
    // Initial fixture policy is authored before acceptance on main. Subsequent
    // agent work cannot weaken or repin these independent required assertions.
    let program = Path::new("/usr/bin/grep").canonicalize().unwrap();
    let loader = if cfg!(target_os = "macos") {
        "/usr/lib/dyld"
    } else {
        "/lib64/ld-linux-x86-64.so.2"
    };
    let execution = RepositoryExecution {
        program: program.to_str().unwrap().into(),
        toolchain_files: BTreeMap::from([(
            "declared-loader".into(),
            Path::new(loader)
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
                .into(),
        )]),
        environment: BTreeMap::from([("LC_ALL".into(), "C".into())]),
        directory: ".".into(),
        timeout_seconds: 10,
    };
    let commands: Vec<_> = [
        ("guard-contract", "required", "src/guard.txt"),
        ("payload-contract", "ready", "src/payload.txt"),
    ]
    .into_iter()
    .map(|(name, expected, path)| ProofDef {
        name: name.into(),
        command: execution.program.clone(),
        args: ["-F", "-x", "--", expected, path].map(str::to_owned).into(),
        description:
            "Require the actual committed source to satisfy its literal accepted invariant".into(),
        required: true,
    })
    .collect();
    let mut contract = RepositoryContract {
        canonical_local_target: false,
        reviewed_commands_digest: String::new(),
        source_coverage: vec![],
        executions: commands
            .iter()
            .map(|command| (command.name.clone(), execution.clone()))
            .collect(),
        targets: BTreeMap::new(),
    };
    contract.reviewed_commands_digest =
        repository_profile::command_contract_digest(&commands, &contract).unwrap();
    let options = Options {
        root: root.canonicalize().unwrap(),
        git: Path::new("/usr/bin/git").canonicalize().unwrap(),
        repository: repository.into(),
        base_branch: "main".into(),
        accepted_base_ref: "refs/heads/main".into(),
        evidence_parent: output.canonicalize().unwrap(),
        timeout: Duration::from_secs(120),
        max_input_bytes: 64 * 1024 * 1024,
        max_files: 10_000,
        max_output_bytes: 1024 * 1024,
        target: None,
    };
    let sources =
        validation_runtime::discover_source_inventory(&options, &Default::default()).unwrap();
    let proof = ProjectProofConfig {
        validation_buildkite_preflights: false,
        required_profile: Some(ProfileDeclaration {
            id: repository_profile::REQUIRED_PROFILE.into(),
            reviewed_sources_digest: sources.digest().unwrap(),
        }),
        repository_contract: Some(contract),
        commands,
        ..Default::default()
    };
    let path = root.join(".decapod/config.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["proof"] = toml::Value::try_from(proof).unwrap();
    config["repo"]["base_branch"] = "main".into();
    fs::write(path, toml::to_string_pretty(&config).unwrap()).unwrap();
}

#[test]
#[ignore = "requires explicit empty disposable GitHub repository and local evidence destination"]
fn real_forge_reject_repair_admit_and_invalidate() {
    assert!(
        std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
            && std::env::var("BUILDKITE").as_deref() != Ok("true"),
        "run acceptance outside CI"
    );
    let repository =
        std::env::var("DECAPOD_ACCEPTANCE_REPOSITORY").expect("explicit disposable repository");
    let (owner, name) = repository.split_once('/').unwrap();
    assert!(
        name.starts_with("decapod-admission-acceptance-")
            && repository
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b)),
        "disposable repository only"
    );
    let directory = PathBuf::from(
        std::env::var_os("DECAPOD_ACCEPTANCE_OUTPUT").expect("new private output directory"),
    );
    fs::create_dir(&directory).expect("output must not exist; preserve all earlier attempts");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let directory = directory.canonicalize().unwrap();
    let root = directory.join("repository");
    fs::create_dir(&root).unwrap();
    let binary = super::resolve_decapod_bin().canonicalize().unwrap();
    let demo = Demo {
        binary,
        directory: directory.clone(),
        repository: repository.clone(),
        sequence: Default::default(),
    };
    let source_root = super::resolve_repo_root();
    let source_head = demo.git(&source_root, &["rev-parse", "HEAD"]);
    assert!(
        demo.git(&source_root, &["status", "--porcelain"])
            .is_empty(),
        "acceptance requires a clean exact source candidate"
    );
    let endpoint = format!("repos/{repository}");
    let branches = demo.gh(&root, &["api", &format!("{endpoint}/branches")]);
    assert!(
        branches.as_array().unwrap().is_empty(),
        "initial fixture must be empty"
    );
    let binary_digest = format!(
        "sha256:{:x}",
        Sha256::digest(fs::read(&demo.binary).unwrap())
    );
    demo.git(&root, &["init", "-b", "main"]);
    demo.git(&root, &["config", "user.name", "Decapod acceptance"]);
    demo.git(
        &root,
        &["config", "user.email", "acceptance@example.invalid"],
    );
    demo.git(&root, &["config", "commit.gpgsign", "false"]);
    demo.cli(&root, &["eval", "--prompt", "Demonstrate rejection repair and exact-evidence PR admission in this disposable fixture", "--format", "json"]);
    demo.cli(&root, &["init", "--proof", "--no-ci", "--no-container-workspaces", "--product-name", "Admission acceptance", "--product-summary", "A disposable source contract demonstrates governed rejection repair and exact-artifact PR admission", "--product-type", "service_or_library", "--architecture-direction", "cli", "--done-criteria", "Both accepted source assertions and exact-head/base PR admission pass; merge remains unauthorized", "--primary-language", "text"]);
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/payload.txt"), "ready\n").unwrap();
    fs::write(root.join("src/guard.txt"), "required\n").unwrap();
    fs::write(root.join("Justfile"), "check:\n    /usr/bin/grep -F -x -- ready src/payload.txt\n    /usr/bin/grep -F -x -- required src/guard.txt\n").unwrap();
    configure_accepted_fixture(&root, &directory, &repository);
    demo.git(&root, &["add", "."]);
    demo.git(
        &root,
        &[
            "commit",
            "-m",
            "test: accept independent fixture source contract",
        ],
    );
    demo.git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            &format!("git@github.com:{repository}.git"),
        ],
    );
    demo.git(&root, &["push", "-u", "origin", "main"]);
    let base = demo.git(&root, &["rev-parse", "HEAD"]);
    demo.ok(&root, &demo.binary, &["session", "acquire"]);
    let task = demo.cli(
        &root,
        &[
            "todo",
            "add",
            "Repair source and earn exact-proof PR admission",
            "--format",
            "json",
        ],
    );
    let task = task["id"].as_str().unwrap();
    demo.cli(
        &root,
        &["todo", "claim", "--id", task, "--lease-seconds", "86400"],
    );
    let branch = format!("agent/real-forge-admission/{task}-repair");
    let workspace = demo.cli(&root, &["workspace", "ensure", "--branch", &branch]);
    let workspace = PathBuf::from(workspace["worktree_path"].as_str().unwrap());
    demo.cli(&workspace, &["infer", "orientation", "--task-id", task]);
    demo.cli(&workspace, &["govern", "plan", "update", "--intent", "Repair actual source failure while preserving both accepted assertions, obtain fresh full native proof, and admit only the exact GitHub head/base; admission does not authorize merge", "--clear-unknowns"]);
    demo.cli(&workspace, &["govern", "plan", "approve"]);
    for spec in [
        "INTENT",
        "ARCHITECTURE",
        "INTERFACES",
        "SECURITY",
        "VALIDATION",
    ] {
        let path = workspace.join(format!(".decapod/managed/specs/{spec}.md"));
        let mut body = fs::read_to_string(&path).unwrap();
        body.push_str("\n## Exact source admission contract\n\nThe accepted payload is ready and the independent guard is required. Both literal source assertions must execute and pass for the exact candidate and GitHub main target. A draft PR can exist after rejected readback; creation is not admission. Later source or accepted-target changes invalidate applicability and never authorize merge.\n");
        fs::write(path, body).unwrap();
    }
    fs::write(workspace.join("src/payload.txt"), "invalid work\n").unwrap();
    demo.checkpoint(&workspace, "agent-introduces-source-failure");
    let invalid_output = demo.run(&workspace, &demo.binary, &["validate", "--format", "json"]);
    assert!(
        !invalid_output.status.success(),
        "invalid source passed validation"
    );
    let invalid = format!(
        "{} {}",
        String::from_utf8_lossy(&invalid_output.stdout),
        String::from_utf8_lossy(&invalid_output.stderr)
    );
    let failed_report: Value = serde_json::from_slice(&invalid_output.stdout).unwrap();
    let failed_directory = PathBuf::from(
        failed_report["readiness"]["evidence_path"]
            .as_str()
            .expect("source failure must retain actual execution evidence"),
    );
    let failed_evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(failed_directory.join("observations.json")).unwrap())
            .unwrap();
    fs::copy(
        failed_directory.join("observations.json"),
        directory.join("invalid-source-observations.json"),
    )
    .unwrap();
    assert!(
        failed_evidence
            .results
            .iter()
            .any(|row| row.key.job == "payload-contract"
                && matches!(
                    row.observation,
                    Observation::Executed {
                        conclusion: Conclusion::Failure,
                        ..
                    }
                ))
    );
    let invalid_publish = demo.reject(
        &workspace,
        &[
            "workspace",
            "publish",
            "--title",
            "Acceptance source repair",
        ],
    );
    assert!(failed_evidence.results.iter().any(|row| {
        row.key.job == "guard-contract"
            && matches!(
                row.observation,
                Observation::Executed {
                    conclusion: Conclusion::Success,
                    exit_code: Some(0),
                    ..
                }
            )
    }));
    // A timeout, lock or transport failure proves no source-policy enforcement.
    // Require the actual readiness rejection before repairing the work.
    assert!(
        invalid_publish.contains("PUBLICATION_NOT_READY")
            && invalid_publish.contains("READINESS_BLOCKED")
            && !invalid_publish.contains("VALIDATE_TIMEOUT_OR_LOCK"),
        "invalid work must be rejected by the source-proof gate: {invalid_publish}"
    );
    assert!(
        demo.gh(&root, &["api", &format!("{endpoint}/pulls")])
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::write(workspace.join("src/payload.txt"), "ready\n").unwrap();
    demo.checkpoint(&workspace, "agent-repairs-actual-source-failure");
    let repaired = demo.cli(&workspace, &["validate", "--format", "json"]);
    let repaired_receipt = demo.receipt(&repaired, "repaired");
    let head = demo.git(&workspace, &["rev-parse", "HEAD"]);
    assert_eq!(repaired_receipt["material"]["head_revision"], head);
    assert_eq!(repaired_receipt["material"]["accepted_base_revision"], base);

    // Observe actual GitHub creation, then mutate source before final readback.
    // No command, response, transport or clock is replaced by this interleaving.
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker_root = workspace.clone();
    let worker_endpoint = format!("{endpoint}/pulls");
    let query = format!("head={owner}:{branch}");
    let watcher = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(120);
        while !worker_stop.load(Ordering::Acquire) && Instant::now() < until {
            let output = Command::new("gh")
                .args([
                    "api",
                    "--method",
                    "GET",
                    &worker_endpoint,
                    "-f",
                    "state=open",
                    "-f",
                    &query,
                    "--jq",
                    ".[0].number // empty",
                ])
                .bounded_output(Duration::from_secs(10))
                .unwrap();
            if output.status.success()
                && let Ok(number) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
            {
                fs::write(
                    worker_root.join("src/payload.txt"),
                    "mutated after real GitHub creation\n",
                )
                .unwrap();
                return Some(number);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        None
    });
    let interrupted = demo.run(&workspace, &demo.binary, &["workspace", "publish", "--title", "Acceptance source repair", "--description", "Real-forge acceptance for DecapodLabs/decapod #1747 and #1748; PR admission does not authorize merge"]);
    stop.store(true, Ordering::Release);
    let number = watcher
        .join()
        .unwrap()
        .expect("real creation must be observed during publication");
    let interrupted_reason = format!(
        "{} {}",
        String::from_utf8_lossy(&interrupted.stdout),
        String::from_utf8_lossy(&interrupted.stderr)
    );
    assert!(
        !interrupted.status.success() && interrupted_reason.contains("PR_CREATED_NOT_ADMITTED"),
        "{interrupted_reason}"
    );
    let rejection_path = interrupted_reason
        .lines()
        .find_map(|line| line.strip_prefix("Rejection evidence: "))
        .expect("post-creation rejection must retain its native evidence");
    fs::copy(
        rejection_path,
        directory.join("created-not-admitted-rejection.json"),
    )
    .unwrap();
    let pr_endpoint = format!("{endpoint}/pulls/{number}");
    let quarantined = demo.gh(&root, &["api", &pr_endpoint]);
    assert_eq!(quarantined["draft"], true);
    assert_eq!(
        fs::read_to_string(workspace.join("src/payload.txt")).unwrap(),
        "mutated after real GitHub creation\n"
    );
    fs::write(workspace.join("src/payload.txt"), "ready\n").unwrap();
    let fresh = demo.cli(&workspace, &["validate", "--format", "json"]);
    let fresh_receipt = demo.receipt(&fresh, "fresh-after-quarantine");
    assert_ne!(
        fresh["readiness"]["evidence_path"],
        repaired["readiness"]["evidence_path"]
    );
    let admitted = demo.cli(
        &workspace,
        &[
            "workspace",
            "publish",
            "--title",
            "Acceptance source repair",
        ],
    );
    assert_eq!(admitted["pr_verified"], true, "{admitted}");
    let observed = demo.gh(&root, &["api", &pr_endpoint]);
    assert_eq!(observed["head"]["sha"], head);
    assert_eq!(observed["base"]["sha"], base);
    assert_eq!(fresh_receipt["material"]["head_revision"], head);
    fs::write(
        workspace.join("src/payload.txt"),
        "changed after admission\n",
    )
    .unwrap();
    let stale_source = demo.reject(&workspace, &["workspace", "publish"]);
    assert_eq!(demo.git(&workspace, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(workspace.join("src/payload.txt")).unwrap(),
        "changed after admission\n"
    );
    fs::write(workspace.join("src/payload.txt"), "ready\n").unwrap();
    // The publisher stages authored changes before checking their checkpoint.
    // Restore only this fixture-owned path in both worktree and index.
    demo.git(&workspace, &["add", "src/payload.txt"]);
    // The entrypoint also regenerated specs for the temporarily changed source.
    // Repair through the governed owner and obtain proof for the resulting new
    // commit; an earlier admission cannot authorize this changed artifact.
    let first_admitted_head = head;
    demo.checkpoint(&workspace, "repair-stale-source-projections");
    let head = demo.git(&workspace, &["rev-parse", "HEAD"]);
    assert_ne!(head, first_admitted_head);
    let revalidated = demo.cli(&workspace, &["validate", "--format", "json"]);
    let revalidated_receipt = demo.receipt(&revalidated, "fresh-after-stale-source");
    assert_eq!(revalidated_receipt["material"]["head_revision"], head);
    let readmitted = demo.cli(
        &workspace,
        &[
            "workspace",
            "publish",
            "--title",
            "Acceptance source repair",
        ],
    );
    assert_eq!(readmitted["pr_verified"], true);
    let observed = demo.gh(&root, &["api", &pr_endpoint]);
    assert_eq!(observed["head"]["sha"], head);
    assert_eq!(observed["base"]["sha"], base);
    // Advance only the disposable remote target, under explicit acceptance authority.
    demo.gh(
        &root,
        &[
            "api",
            "--method",
            "PUT",
            &format!("{endpoint}/contents/accepted-target-change.txt"),
            "-f",
            "branch=main",
            "-f",
            "message=test: advance independent acceptance target",
            "-f",
            "content=YWR2YW5jZWQK",
        ],
    );
    let stale_base = demo.reject(&workspace, &["workspace", "publish"]);
    assert!(stale_base.contains("STALE_ACCEPTED_BASE"), "{stale_base}");
    let repeated_base = demo.reject(&workspace, &["validate", "--format", "json"]);
    assert!(
        repeated_base.contains("STALE_ACCEPTED_BASE"),
        "{repeated_base}"
    );
    let current_pr = demo.gh(&root, &["api", &pr_endpoint]);
    // PR base metadata can retain the admission-era SHA after the branch moves.
    // Read the actual branch ref independently; stale PR metadata cannot prove
    // current target applicability.
    let current_target = demo.gh(&root, &["api", &format!("{endpoint}/git/ref/heads/main")]);
    let advanced_base = current_target["object"]["sha"].as_str().unwrap();
    assert_eq!(current_pr["head"]["sha"], head);
    assert_ne!(advanced_base, base);
    assert_eq!(demo.git(&source_root, &["rev-parse", "HEAD"]), source_head);
    let summary = json!({"schema":"decapod.real-forge-acceptance.v1", "repository":demo.repository,
        "pr_url":observed["html_url"], "validated_head":head, "validated_base":base,
        "current_head":current_pr["head"]["sha"], "advanced_base":advanced_base, "pr_base_after_target_advance":current_pr["base"]["sha"],
        "controller_sha256":binary_digest, "controller_source_head":source_head, "first_admitted_head":first_admitted_head,
        "admission_observed":true, "current_admission_applicable":false, "merge_authorized":false,
        "negative_controls":{"invalid_source":codes(&invalid),"invalid_publication":codes(&invalid_publish),"created_not_admitted":codes(&interrupted_reason),"stale_source":codes(&stale_source),"stale_base":codes(&stale_base),"repeated_stale_base":codes(&repeated_base)},
        "receipts":["repaired-receipt.json","fresh-after-quarantine-receipt.json","fresh-after-stale-source-receipt.json"], "quarantine_confirmed":true});
    fs::write(
        directory.join("acceptance.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    // Publish unchanged, machine-state-free receipts to a separate evidence-only
    // branch. Archival is not fresh publication or merge authority for the PR.
    let mut entries = Vec::new();
    for file in [
        "acceptance.json",
        "invalid-source-observations.json",
        "created-not-admitted-rejection.json",
        "repaired-receipt.json",
        "repaired-observations.json",
        "fresh-after-quarantine-receipt.json",
        "fresh-after-quarantine-observations.json",
        "fresh-after-stale-source-receipt.json",
        "fresh-after-stale-source-observations.json",
    ] {
        let content = fs::read_to_string(directory.join(file)).unwrap();
        assert!(!content.contains(directory.to_str().unwrap()));
        if let Some(home) = std::env::var_os("HOME") {
            assert!(!content.contains(&home.to_string_lossy().to_string()));
        }
        let blob = demo.gh(
            &root,
            &[
                "api",
                "--method",
                "POST",
                &format!("{endpoint}/git/blobs"),
                "-f",
                "encoding=utf-8",
                "-f",
                &format!("content={content}"),
            ],
        );
        entries.push(json!({"path":file,"mode":"100644","type":"blob","sha":blob["sha"]}));
    }
    let request = directory.join("evidence-tree-request.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({"tree":entries})).unwrap(),
    )
    .unwrap();
    let tree = demo.gh(
        &root,
        &[
            "api",
            "--method",
            "POST",
            &format!("{endpoint}/git/trees"),
            "--input",
            request.to_str().unwrap(),
        ],
    );
    let commit = demo.gh(
        &root,
        &[
            "api",
            "--method",
            "POST",
            &format!("{endpoint}/git/commits"),
            "-f",
            "message=test: preserve exact real-forge admission observations; no merge authority",
            "-f",
            &format!("tree={}", tree["sha"].as_str().unwrap()),
        ],
    );
    demo.gh(
        &root,
        &[
            "api",
            "--method",
            "POST",
            &format!("{endpoint}/git/refs"),
            "-f",
            "ref=refs/heads/acceptance-evidence",
            "-f",
            &format!("sha={}", commit["sha"].as_str().unwrap()),
        ],
    );
    fs::write(
        directory.join("evidence-reference.json"),
        serde_json::to_vec_pretty(
            &json!({"repository":repository,"commit":commit["sha"],"branch":"acceptance-evidence"}),
        )
        .unwrap(),
    )
    .unwrap();
    eprintln!(
        "Real-forge acceptance evidence: {}\n{summary}",
        directory.display()
    );
}
