//! Executable identities, artifact corruption, and exact shard-result coverage.
#![allow(dead_code)]
#[path = "../assets/tools/integration_custody.rs"]
mod custody;
use custody::{Context, aggregate, file_sha256, prepare, record, sha256, verify};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const CI: &str = include_str!("../.github/workflows/ci.yml");
const PREPARE_RECIPE: &str = include_str!("../assets/tools/source_ci/integration-prepare.sh");
const WORKER_RECIPE: &str = include_str!("../assets/tools/source_ci/integration-worker.sh");
const STATUS_RECIPE: &str = include_str!("../assets/tools/source_ci/integration-status.sh");
struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    archive: PathBuf,
    receipts: PathBuf,
    context: Context,
    sha: String,
    helper_sha: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "integration-custody-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let artifact = root.join("artifact");
        fs::create_dir(&artifact).unwrap();
        let archive = root.join("integration.tar");
        fs::write(&archive, b"measured archive fixture").unwrap();
        let helper = root.join("helper");
        fs::write(&helper, b"measured verifier fixture").unwrap();
        let context = Context {
            event: "pull_request".into(),
            base: "a".repeat(40),
            head: "b".repeat(40),
            merge_base: "a".repeat(40),
            validation_head: "c".repeat(40),
            tree: "d".repeat(40),
            shards: vec!["gatling-0".into(), "release-policies".into()],
        };
        prepare(&artifact, &helper, &context).unwrap();
        Self {
            receipts: root.join("receipts"),
            sha: file_sha256(&archive).unwrap(),
            helper_sha: file_sha256(&helper).unwrap(),
            root,
            artifact,
            archive,
            context,
        }
    }
    fn verify(&self) -> Result<(), String> {
        verify(
            &self.artifact,
            &self.archive,
            &self.context,
            &self.sha,
            &self.helper_sha,
        )
    }
    fn receipts(&self) {
        for shard in &self.context.shards {
            record(&self.receipts, &self.context, &self.sha, shard).unwrap();
        }
    }
    fn aggregate(&self) -> Result<(), String> {
        aggregate(
            &self.receipts,
            &self.context,
            &self.context.encode().unwrap(),
            &self.sha,
            "2",
            "success",
            "success",
        )
    }
}
#[test]
fn standard_sha256_vectors_and_streaming_boundaries() {
    for (bytes, expected) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ] {
        assert_eq!(sha256(bytes).unwrap(), expected);
    }
    assert_eq!(
        sha256(&vec![b'a'; 1_000_000][..]).unwrap(),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    let f = Fixture::new();
    for size in [1, 55, 56, 63, 64, 65, 127, 128, 129, 8191, 8192, 8193] {
        fs::write(&f.archive, vec![b'x'; size]).unwrap();
        let system = std::process::Command::new("sha256sum")
            .arg(&f.archive)
            .output()
            .unwrap();
        assert!(system.status.success());
        assert_eq!(
            file_sha256(&f.archive).unwrap(),
            String::from_utf8(system.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
        );
    }
}
#[test]
fn exact_context_and_artifacts_verify_without_mutation() {
    let f = Fixture::new();
    let before = fs::read(&f.archive).unwrap();
    f.verify().unwrap();
    assert_eq!(
        Context::parse(&f.context.encode().unwrap()).unwrap(),
        f.context
    );
    assert_eq!(fs::read(&f.archive).unwrap(), before);
    f.receipts();
    f.aggregate().unwrap();
}
#[test]
fn corrupted_archive_verifier_or_manifest_fails_closed() {
    for name in ["archive", "verifier", "manifest"] {
        let f = Fixture::new();
        let path = match name {
            "archive" => f.archive.clone(),
            "verifier" => f.artifact.join("integration-custody"),
            _ => f.artifact.join("integration-context.v1"),
        };
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b'!');
        fs::write(path, bytes).unwrap();
        assert!(f.verify().is_err(), "{name}");
    }
}
#[test]
fn incomplete_forged_or_reordered_contexts_are_rejected() {
    let f = Fixture::new();
    let original = f.context.encode().unwrap();
    for text in [
        original.clone() + "|extra",
        original.clone() + "\n",
        original.replace(".v1|", ".v2|"),
        original.replace("pull_request", "push"),
        original.replace(&f.context.head, "HEAD"),
        original.replace("gatling-0,release-policies", "release-policies,gatling-0"),
        original.replace("gatling-0,release-policies", "gatling-0,gatling-0"),
        original.replace("gatling-0,release-policies", "unknown-shard"),
    ] {
        assert!(Context::parse(&text).is_err(), "{text}");
    }
    for field in [
        "base",
        "head",
        "merge_base",
        "validation_head",
        "tree",
        "shards",
    ] {
        let mut different = f.context.clone();
        match field {
            "base" => different.base = "e".repeat(40),
            "head" => different.head = "e".repeat(40),
            "merge_base" => different.merge_base = "e".repeat(40),
            "validation_head" => different.validation_head = "e".repeat(40),
            "tree" => different.tree = "e".repeat(40),
            _ => different.shards = vec!["gatling-1".into()],
        }
        assert!(
            verify(&f.artifact, &f.archive, &different, &f.sha, &f.helper_sha).is_err(),
            "{field}"
        );
    }
}
#[test]
fn every_selected_receipt_is_required_despite_aggregate_worker_success() {
    let f = Fixture::new();
    assert!(f.aggregate().is_err());
    record(&f.receipts, &f.context, &f.sha, "gatling-0").unwrap();
    assert!(f.aggregate().is_err());
    record(&f.receipts, &f.context, &f.sha, "release-policies").unwrap();
    f.aggregate().unwrap();
}
#[test]
fn skipped_missing_failed_cancelled_or_timed_out_results_are_never_success() {
    let f = Fixture::new();
    f.receipts();
    let context = f.context.encode().unwrap();
    for result in [
        "",
        "skipped",
        "failure",
        "cancelled",
        "timed_out",
        "pending",
    ] {
        assert!(
            aggregate(
                &f.receipts,
                &f.context,
                &context,
                &f.sha,
                "2",
                result,
                "success"
            )
            .is_err()
        );
        assert!(
            aggregate(
                &f.receipts,
                &f.context,
                &context,
                &f.sha,
                "2",
                "success",
                result
            )
            .is_err()
        );
    }
    for count in ["", "0", "1", "3", "02", "two"] {
        assert!(
            aggregate(
                &f.receipts,
                &f.context,
                &context,
                &f.sha,
                count,
                "success",
                "success"
            )
            .is_err()
        );
    }
}
#[test]
fn malformed_stale_duplicate_and_unexpected_receipts_are_rejected() {
    for mutation in [
        "head",
        "base",
        "tree",
        "digest",
        "shard",
        "status",
        "duplicate",
        "extra",
        "missing",
    ] {
        let f = Fixture::new();
        f.receipts();
        let path = f.receipts.join("gatling-0.receipt");
        let text = fs::read_to_string(&path).unwrap();
        match mutation {
            "head" => fs::write(&path, text.replace(&f.context.head, &"e".repeat(40))).unwrap(),
            "base" => fs::write(&path, text.replace(&f.context.base, &"e".repeat(40))).unwrap(),
            "tree" => fs::write(&path, text.replace(&f.context.tree, &"e".repeat(40))).unwrap(),
            "digest" => fs::write(&path, text.replace(&f.sha, &"e".repeat(64))).unwrap(),
            "shard" => {
                fs::write(&path, text.replace("shard=gatling-0", "shard=gatling-1")).unwrap()
            }
            "status" => fs::write(&path, text.replace("status=passed", "status=skipped")).unwrap(),
            "duplicate" => fs::write(&path, text + "status=passed\n").unwrap(),
            "extra" => fs::write(f.receipts.join("gatling-1.receipt"), text).unwrap(),
            _ => fs::remove_file(&path).unwrap(),
        }
        assert!(f.aggregate().is_err(), "{mutation}");
    }
}
#[test]
fn empty_selection_requires_proven_preparation_and_explicitly_skipped_workers() {
    let mut f = Fixture::new();
    f.context.shards.clear();
    let context = f.context.encode().unwrap();
    aggregate(
        &f.receipts,
        &f.context,
        &context,
        "",
        "0",
        "success",
        "skipped",
    )
    .unwrap();
    for (sha, prepare, worker) in [
        (&f.sha[..], "success", "skipped"),
        ("", "failure", "skipped"),
        ("", "success", "success"),
        ("", "success", ""),
    ] {
        assert!(aggregate(&f.receipts, &f.context, &context, sha, "0", prepare, worker).is_err());
    }
    fs::create_dir(&f.receipts).unwrap();
    fs::write(f.receipts.join("unselected.receipt"), "extra").unwrap();
    assert!(
        aggregate(
            &f.receipts,
            &f.context,
            &context,
            "",
            "0",
            "success",
            "skipped"
        )
        .is_err()
    );
}
#[test]
fn duplicate_or_unselected_recording_does_not_overwrite_success() {
    let f = Fixture::new();
    f.receipts();
    let before = fs::read(f.receipts.join("gatling-0.receipt")).unwrap();
    assert!(record(&f.receipts, &f.context, &f.sha, "gatling-0").is_err());
    assert!(record(&f.receipts, &f.context, &f.sha, "gatling-1").is_err());
    assert_eq!(
        fs::read(f.receipts.join("gatling-0.receipt")).unwrap(),
        before
    );
}
#[cfg(unix)]
#[test]
fn symlinked_receipts_are_not_independent_evidence() {
    let f = Fixture::new();
    f.receipts();
    let path = f.receipts.join("gatling-0.receipt");
    let outside = f.root.join("outside");
    fs::rename(&path, &outside).unwrap();
    std::os::unix::fs::symlink(outside, path).unwrap();
    assert!(f.aggregate().is_err());
}
fn job(name: &str) -> &'static str {
    let rest = CI.split_once(&format!("\n  {name}:\n")).unwrap().1;
    let mut end = 0;
    for line in rest.split_inclusive('\n') {
        if line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#')
        {
            break;
        }
        end += line.len();
    }
    &rest[..end]
}
#[test]
fn actual_workflow_measures_before_execution_and_records_after_the_real_shard_command() {
    let prep = job("integration-prepare");
    let worker = job("test-integration");
    let aggregate = job("integration-status");
    assert!(prep.contains("run-ci --check integration-prepare"));
    assert!(worker.contains("integration-bootstrap/source-ci-profile run-ci --check"));
    assert!(worker.contains("integration-${{ matrix.shard }}"));
    assert!(
        aggregate
            .contains("integration-bootstrap/source-ci-profile run-ci --check integration-status")
    );
    assert_eq!(
        PREPARE_RECIPE
            .matches("rustc --edition=2024 -O assets/tools/integration_custody.rs")
            .count(),
        1
    );
    assert!(PREPARE_RECIPE.contains("integration-custody\" prepare --artifact"));
    assert!(
        PREPARE_RECIPE.contains("digest --file target/integration-artifacts/integration-tests.tar")
    );
    for body in [worker, aggregate] {
        assert!(!body.contains("actions/checkout"));
        assert!(!body.contains("rustc "));
        assert!(body.find("sha256sum --check --status").unwrap() < body.find("tar -xf").unwrap());
        assert!(body.find("tar -xf").unwrap() < body.find("source-ci-profile run-ci").unwrap());
    }
    assert!(
        WORKER_RECIPE.find("sha256sum --check --status").unwrap()
            < WORKER_RECIPE.find("tar -xf").unwrap()
    );
    assert!(
        WORKER_RECIPE.find("integration-custody\" verify").unwrap()
            < WORKER_RECIPE
                .find("integration-tests/integration-shards\" run")
                .unwrap()
    );
    assert!(
        WORKER_RECIPE
            .find("integration-tests/integration-shards\" run")
            .unwrap()
            < WORKER_RECIPE.find("integration-custody\" record").unwrap()
    );
    assert!(!worker.contains("continue-on-error"));
    assert_eq!(worker.matches("if: always()").count(), 1);
    assert!(worker.contains("Upload measured source-CI custody"));
    assert!(!worker.contains("Record measured integration shard success"));
    assert!(
        STATUS_RECIPE.find("sha256sum --check --status").unwrap()
            < STATUS_RECIPE.find("integration-custody aggregate").unwrap()
    );
    assert!(worker.contains("CUSTODY_SHARD: ${{ matrix.shard }}"));
    for shard in custody::SHARDS {
        assert!(aggregate.contains(&format!(
            "if: contains(fromJSON(needs.integration-prepare.outputs.shards), '{shard}')"
        )));
        assert!(aggregate.contains(&format!(
            "name: decapod-integration-receipt-${{{{ github.sha }}}}-{shard}"
        )));
    }
    assert_eq!(
        aggregate
            .matches("name: decapod-integration-receipt-")
            .count(),
        custody::SHARDS.len()
    );
    assert!(
        aggregate
            .contains("CUSTODY_PREPARED_CONTEXT: ${{ needs.integration-prepare.outputs.context }}")
    );
    assert!(aggregate.contains("CUSTODY_WORKER_RESULT: ${{ needs.test-integration.result }}"));
}
#[test]
fn source_workflow_gates_use_verified_context_and_keep_non_readiness_paths_explicit() {
    assert!(CI.contains("branches: [ master, 'decapod/preflight/**' ]"));
    assert!(!CI.contains("github.event.before"));
    assert!(!CI.contains("github.base_ref"));
    assert!(!CI.contains("git fetch"));
    for id in [
        "commit-lint",
        "governance-artifacts",
        "material-specs",
        "detect-migration-script-changes",
        "test",
        "integration-prepare",
        "test-integration",
        "integration-status",
        "migration-script-tests",
    ] {
        assert!(
            job(id).contains("needs.release-metadata.outputs.pr_like == 'true'"),
            "{id}"
        );
    }
    for id in [
        "release-metadata",
        "commit-lint",
        "governance-artifacts",
        "material-specs",
        "detect-migration-script-changes",
        "supabase-consumer",
        "workspace-lifecycle-docker",
        "musl-publication",
        "fmt",
        "clippy",
        "deps-lint",
        "health",
        "state-commit-golden-vectors",
        "setup",
        "test",
        "integration-prepare",
        "test-master",
        "test-rpc-suite",
        "chaos-replay",
        "daemonless-lifecycle",
        "contracts",
        "migration-script-tests",
        "release-build",
        "propodus-live",
    ] {
        let body = job(id);
        assert!(
            body.contains("fetch-depth: 0") && body.contains("persist-credentials: false"),
            "{id}"
        );
        assert!(
            body.find("uses: ./.github/actions/ci-context").unwrap()
                < body.find("        run:").unwrap_or(usize::MAX),
            "{id}"
        );
    }
    assert!(job("propodus-live").contains("needs.release-metadata.outputs.pre_pr == 'false'"));
    assert!(job("release-build").contains("needs.release-metadata.outputs.pre_pr == 'false'"));
    assert!(job("test-master").contains("needs.release-metadata.outputs.event == 'push'"));
    assert!(
        include_str!("../assets/tools/source_ci/governance-artifacts.sh")
            .contains("--base-branch \"$GOVERNANCE_BASE\"")
    );
    assert!(
        include_str!("../assets/tools/source_ci/governance-artifacts.sh")
            .contains("--target-base master")
    );
    assert!(
        include_str!("../assets/tools/source_ci/material-specs.sh")
            .contains("head_mat = material(head_text)")
    );
    assert!(
        include_str!("../assets/tools/source_ci/health.sh")
            .contains("decapod govern project-check --refresh-specs")
    );
}

fn workflow_script(job_id: &str, step: &str) -> String {
    let body = job(job_id)
        .split_once(&format!("      - name: {step}\n"))
        .unwrap()
        .1;
    body.split_once("        run: |\n")
        .unwrap()
        .1
        .lines()
        .take_while(|line| line.starts_with("          ") || line.is_empty())
        .map(|line| line.strip_prefix("          ").unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("${{ matrix.shard }}", "gatling-0")
}

#[cfg(unix)]
#[test]
fn actual_workflow_shell_custody_boundaries_use_the_real_verifier_and_fail_closed() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let mut f = Fixture::new();
    f.context.shards = vec!["gatling-0".into()];
    let executable = if let Some(packaged) =
        option_env!("DECAPOD_CUSTODY_TEST_FIXTURE").filter(|p| !p.is_empty())
    {
        let mut candidates = vec![PathBuf::from(packaged)];
        for variable in ["RUNFILES_DIR", "TEST_SRCDIR"] {
            if let Some(root) = std::env::var_os(variable) {
                candidates.push(
                    PathBuf::from(root)
                        .join("_main")
                        .join(packaged.trim_start_matches("./")),
                );
            }
        }
        candidates
            .into_iter()
            .find_map(|p| p.canonicalize().ok())
            .expect("Bazel must provide the declared real custody verifier")
    } else {
        let source = f.root.join("custody.rs");
        fs::write(
            &source,
            include_str!("../assets/tools/integration_custody.rs"),
        )
        .unwrap();
        let executable = f.root.join("native-custody");
        let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let compiled = Command::new(compiler)
            .args(["--edition=2024", "-O", "-D", "warnings"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(compiled.status.success(), "{compiled:?}");
        executable
    };
    fs::remove_dir_all(&f.artifact).unwrap();
    fs::create_dir(&f.artifact).unwrap();
    prepare(&f.artifact, &executable, &f.context).unwrap();
    // The expensive test executable is an explicit fixture double. Everything
    // else is the actual workflow Bash and the compiled production verifier.
    let runner = f.artifact.join("integration-shards");
    fs::write(
        &runner,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$FIXTURE_INVOCATION\"\nif [ \"$FIXTURE_EXIT_CODE\" = sleep ]; then\n  printf started > \"$FIXTURE_STARTED\"\n  sleep 30\n  exit 0\nfi\nexit \"$FIXTURE_EXIT_CODE\"\n",
    )
    .unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        Command::new("tar")
            .arg("-cf")
            .arg(&f.archive)
            .arg("-C")
            .arg(&f.artifact)
            .arg(".")
            .status()
            .unwrap()
            .success()
    );
    f.sha = file_sha256(&f.archive).unwrap();
    f.helper_sha = file_sha256(&executable).unwrap();
    let worker = f.root.join("worker");
    fs::create_dir_all(worker.join("target/integration-download")).unwrap();
    fs::create_dir_all(worker.join("target/integration-custody")).unwrap();
    fs::copy(
        &executable,
        worker.join("target/integration-custody/integration-custody"),
    )
    .unwrap();
    let downloaded = worker.join("target/integration-download/integration-tests.tar");
    fs::copy(&f.archive, &downloaded).unwrap();
    let temp = f.root.join("worker-temp");
    fs::create_dir(&temp).unwrap();
    let invocation = f.root.join("invocation");
    let next = std::cell::Cell::new(0usize);
    let command = |script: &str, exit_code: &str| {
        let invocation_temp = temp.join(format!("run-{}", next.get()));
        next.set(next.get() + 1);
        fs::create_dir(&invocation_temp).unwrap();
        let mut command = Command::new("bash");
        command
            .args(["-c", script])
            .current_dir(&worker)
            .env("RUNNER_TEMP", &invocation_temp)
            .env("CUSTODY_EVENT", &f.context.event)
            .env("CUSTODY_BASE", &f.context.base)
            .env("CUSTODY_HEAD", &f.context.head)
            .env("CUSTODY_MERGE_BASE", &f.context.merge_base)
            .env("CUSTODY_VALIDATION_HEAD", &f.context.validation_head)
            .env("CUSTODY_TREE", &f.context.tree)
            .env("CUSTODY_SHARDS", "[\"gatling-0\"]")
            .env("CUSTODY_ARCHIVE_SHA256", &f.sha)
            .env("CUSTODY_HELPER_SHA256", &f.helper_sha)
            .env("CUSTODY_PREPARED_CONTEXT", f.context.encode().unwrap())
            .env("CUSTODY_COUNT", "1")
            .env("CUSTODY_PREPARE_RESULT", "success")
            .env("CUSTODY_WORKER_RESULT", "success")
            .env("CUSTODY_SHARD", "gatling-0")
            .env("FIXTURE_STARTED", f.root.join("started"))
            .env("FIXTURE_INVOCATION", &invocation)
            .env("FIXTURE_EXIT_CODE", exit_code);
        command
    };
    let run = |script: &str, exit_code: &str| command(script, exit_code).output().unwrap();
    // Each invocation gets a fresh extraction directory, matching independent
    // workers/retries. The exact checked-in recipe always runs before recording.
    fs::write(&downloaded, b"corrupted before extraction").unwrap();
    assert!(!run(WORKER_RECIPE, "0").status.success());
    assert!(
        !temp.join("run-0/integration-tests").exists(),
        "unmeasured archive was extracted"
    );
    fs::copy(&f.archive, &downloaded).unwrap();
    assert!(!run(WORKER_RECIPE, "17").status.success());
    assert!(!worker.join("target/integration-receipts").exists());
    assert!(
        !run(STATUS_RECIPE, "0").status.success(),
        "provider success concealed missing selected receipt"
    );
    // Cancellation must reach timeout's independent process group. Prove the
    // real Bash trap reaps the test process and never emits a success receipt.
    let started = f.root.join("started");
    let mut child = command(WORKER_RECIPE, "sleep")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !started.exists() && std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let did_start = started.exists();
    let signalled = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cancelled worker did not terminate within the fixture budget");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(did_start && signalled.success());
    assert!(!status.success());
    assert!(!worker.join("target/integration-receipts").exists());
    let result = run(WORKER_RECIPE, "0");
    assert!(result.status.success(), "{result:?}");
    let args = fs::read_to_string(&invocation).unwrap();
    assert!(args.starts_with("run\n--artifact\n"));
    assert!(args.ends_with("--shard\ngatling-0\n"));
    let result = run(STATUS_RECIPE, "0");
    assert!(result.status.success(), "{result:?}");
    fs::write(
        worker.join("target/integration-receipts/extra.receipt"),
        "unexpected",
    )
    .unwrap();
    assert!(!run(STATUS_RECIPE, "0").status.success());
}

#[path = "../assets/tools/ci_event_context.rs"]
mod event_context;

#[test]
fn authored_range_commands_consume_real_verified_head_not_synthetic_merge_content() {
    use std::process::Command;
    let f = Fixture::new();
    let repo = f.root.join("source-repo");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| -> String {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Context Fixture",
                "-c",
                "user.email=context@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-q", "-b", "master"]);
    fs::create_dir_all(repo.join(".decapod/managed/specs")).unwrap();
    fs::write(
        repo.join(".decapod/managed/specs/INTENT.md"),
        "# Original authored intent\n",
    )
    .unwrap();
    fs::write(repo.join("source.rs"), "pub fn before() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "baseline"]);
    let common = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-q", "-b", "candidate"]);
    fs::write(repo.join("source.rs"), "pub fn after() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "original candidate"]);
    let head = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-q", "master"]);
    fs::write(
        repo.join(".decapod/managed/specs/INTENT.md"),
        "# Target-only authored intent\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "target advanced"]);
    let base = git(&["rev-parse", "HEAD"]);
    git(&[
        "merge",
        "-q",
        "--no-ff",
        "candidate",
        "-m",
        "synthetic PR merge",
    ]);
    let resolve = |head: &str| {
        event_context::resolve(&event_context::Inputs {
            repo: repo.clone(),
            event: "pull_request".into(),
            base: base.clone(),
            head: head.into(),
            pre_pr: false,
            accepted_base_ref: None,
            snapshot: false,
            trigger_ref: None,
        })
        .unwrap()
    };
    let context = resolve(&head);
    assert_eq!(context.merge_base, common);
    assert_ne!(context.checkout_head, context.head);
    let material = include_str!("../assets/tools/source_ci/material-specs.sh");
    let lint = include_str!("../assets/tools/source_ci/commit-lint.sh");
    let run = |script: &str, context: &event_context::Context| {
        Command::new("bash")
            .args(["-c", script])
            .current_dir(&repo)
            .env("SOURCE_CI_EVENT", "pull_request")
            .env("SOURCE_CI_MERGE_BASE", &context.merge_base)
            .env("SOURCE_CI_HEAD", &context.head)
            .output()
            .unwrap()
    };
    let result = run(material, &context);
    assert!(
        !result.status.success(),
        "target-only prose was mistaken for authored candidate work"
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("FINGERPRINT_ONLY_SPECS"));
    assert!(run(lint, &context).status.success());
    git(&["checkout", "-q", "candidate"]);
    fs::write(
        repo.join(".decapod/managed/specs/INTENT.md"),
        "# Candidate-authored semantic change\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "material candidate spec"]);
    let context = resolve(&git(&["rev-parse", "HEAD"]));
    let result = run(material, &context);
    assert!(result.status.success(), "{result:?}");
    git(&[
        "commit",
        "--allow-empty",
        "-qm",
        "wip: incomplete candidate",
    ]);
    let context = resolve(&git(&["rev-parse", "HEAD"]));
    assert!(!run(lint, &context).status.success());
    let result = Command::new("bash")
        .args(["-c", lint])
        .current_dir(&repo)
        .env("SOURCE_CI_EVENT", "pull_request")
        .env("SOURCE_CI_MERGE_BASE", "f".repeat(40))
        .env("SOURCE_CI_HEAD", &context.head)
        .output()
        .unwrap();
    assert!(
        !result.status.success(),
        "failed git log was hidden by a pipeline success"
    );
}

#[test]
fn bounded_preparation_dependencies_require_exact_context_inventory_and_worker_bytes() {
    let mut f = Fixture::new();
    f.context.shards = vec![
        "gatling-0".into(),
        "gatling-1".into(),
        "gatling-2".into(),
        "gatling-3".into(),
    ];
    let hash = "1".repeat(64);
    let mut inventory = std::collections::BTreeMap::new();
    for name in [
        "build_lifecycle",
        "integration-shards",
        "integration-custody",
        "integration-context.v1",
        "changed-paths.txt",
        "matrix.json",
        "gatling",
        "gatling-inventory.txt",
    ] {
        inventory.insert(name.to_string(), (32, hash.clone()));
    }
    inventory.get_mut("integration-context.v1").unwrap().1 =
        custody::sha256((f.context.encode().unwrap() + "\n").as_bytes()).unwrap();
    let descriptor = custody::PreparedDependencies {
        context: f.context.clone(),
        archive_sha256: "2".repeat(64),
        archive_bytes: 1024 * 1024 * 1024,
        helper_sha256: hash.clone(),
        bootstrap_sha256: "3".repeat(64),
        executor_sha256: "4".repeat(64),
        worker_script_sha256: "5".repeat(64),
        status_script_sha256: "6".repeat(64),
        inventory,
    };
    let encoded = descriptor.encode().unwrap();
    assert!(encoded.len() < 4096);
    assert_eq!(
        custody::PreparedDependencies::parse(encoded.as_bytes()).unwrap(),
        descriptor
    );
    descriptor
        .verify_selection(&f.context, &["gatling"])
        .unwrap();
    descriptor
        .verify_worker_input(
            &descriptor.archive_sha256,
            descriptor.archive_bytes,
            &hash,
            &descriptor.executor_sha256,
        )
        .unwrap();
    assert!(
        descriptor
            .verify_worker_input(
                &descriptor.archive_sha256,
                1,
                &hash,
                &descriptor.executor_sha256
            )
            .is_err()
    );
    assert!(
        descriptor
            .verify_worker_input(
                &"7".repeat(64),
                descriptor.archive_bytes,
                &hash,
                &descriptor.executor_sha256
            )
            .is_err()
    );
    assert!(
        descriptor
            .verify_selection(&f.context, &["other-test"])
            .is_err()
    );
    assert!(
        custody::PreparedDependencies::parse(
            format!("{encoded}file|gatling|32|{hash}\n").as_bytes()
        )
        .is_err()
    );
    let mut wrong = f.context.clone();
    wrong.tree = "7".repeat(40);
    assert!(descriptor.verify_selection(&wrong, &["gatling"]).is_err());
    assert!(
        custody::verify_manifest_bytes(&f.context, (f.context.encode().unwrap() + "\n").as_bytes())
            .is_ok()
    );
    assert!(
        custody::verify_manifest_bytes(&wrong, (f.context.encode().unwrap() + "\n").as_bytes())
            .is_err()
    );
}

#[test]
fn pure_authenticated_receipt_set_reuses_exact_existing_receipt_contract() {
    let mut f = Fixture::new();
    f.context.shards = vec!["gatling-0".into()];
    let archive = "2".repeat(64);
    let bytes = format!("decapod.integration-receipt.v1\ncontext={}\narchive_sha256={archive}\nshard=gatling-0\nstatus=passed\n", f.context.encode().unwrap()).into_bytes();
    let mut rows = std::collections::BTreeMap::from([("gatling-0.receipt".into(), bytes.clone())]);
    custody::verify_receipt_set(&f.context, &archive, &rows).unwrap();
    assert!(
        custody::verify_receipt_bytes(&f.context, &"3".repeat(64), "gatling-0", &bytes).is_err()
    );
    rows.insert("gatling-1.receipt".into(), bytes.clone());
    assert!(custody::verify_receipt_set(&f.context, &archive, &rows).is_err());
    rows.clear();
    assert!(custody::verify_receipt_set(&f.context, &archive, &rows).is_err());
    let values = std::collections::BTreeMap::from([
        ("CUSTODY_EVENT".into(), f.context.event.clone()),
        ("CUSTODY_BASE".into(), f.context.base.clone()),
        ("CUSTODY_HEAD".into(), f.context.head.clone()),
        ("CUSTODY_MERGE_BASE".into(), f.context.merge_base.clone()),
        (
            "CUSTODY_VALIDATION_HEAD".into(),
            f.context.validation_head.clone(),
        ),
        ("CUSTODY_TREE".into(), f.context.tree.clone()),
        ("CUSTODY_SHARDS".into(), "[\"gatling-0\"]".into()),
    ]);
    assert_eq!(custody::Context::from_bindings(&values).unwrap(), f.context);
}
