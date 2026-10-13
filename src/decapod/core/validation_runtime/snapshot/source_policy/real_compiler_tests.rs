//! Real local compiler/executor/consumer regression. Only provider archive
//! authentication is a fixture; this never establishes hosted or whole-PR proof.
use super::*;
use provider::artifact_archive::ArtifactFiles;
use provider::source_execution::{self as consumer, ExecutorExpectation};
use source::custody::{self, Context, ExecutorContext};

const TEST_NAME: &str = "core::validation_runtime::snapshot::source_policy::real_compiler_tests::accepted_executor_builds_and_tests_edited_candidate_with_independent_custody";
const CHILD: &str = "DECAPOD_REAL_DUAL_ROOT_CHILD";
const CHECK: &str = "golden-vectors-linux";
const ARTIFACT: &str = "tests/golden_vectors/target/release/golden_vectors";
const ORIGIN: &str = "github:fixture/decapod/runs/12/attempts/1/artifacts/local-fixture";

// This dependency-free miniature exercises the fixed golden-vector recipe. Its
// checksums and Git assertions describe these fixtures, not Decapod's real CBOR
// vectors. Both Cargo tests and the compiled --check consumer do real work.
const CANDIDATE: &str = r#"
use std::{fs, process::Command};
const CANDIDATE_VALUE: u32 = 42;
fn fingerprint(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
    format!("{value:016x}")
}
fn vectors() -> (String, String) {
    let base = "../golden/state_commit/v1";
    let bytes = fs::read(format!("{base}/scope_record.cbor")).unwrap();
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(hex, fs::read_to_string(format!("{base}/scope_record.cbor.hex")).unwrap().trim());
    let scope = fingerprint(&bytes);
    assert_eq!(scope, fs::read_to_string(format!("{base}/scope_record_hash.txt")).unwrap().trim());
    let mut state = Vec::new();
    for name in ["initial.txt", "target.txt", "change.txt"] {
        state.extend(fs::read(format!("../fixtures/state_commit_repo/{name}")).unwrap());
    }
    let root = fingerprint(&state);
    assert_eq!(root, fs::read_to_string(format!("{base}/state_commit_root.txt")).unwrap().trim());
    (scope, root)
}
fn main() {
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["--check"]);
    let (scope, root) = vectors();
    let git = |args: &[&str]| {
        let output = Command::new("git").args(args).current_dir("../fixtures/state_commit_repo").output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    assert_eq!(git(&["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(git(&["diff", "--name-only", "HEAD^", "HEAD"]), "change.txt");
    assert_eq!(git(&["status", "--porcelain"]), "");
    println!("scope_record_hash: {scope}\nstate_commit_root: {root}\ncandidate_value: {CANDIDATE_VALUE}");
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn candidate_value_is_real() { assert_eq!(CANDIDATE_VALUE, 42); }
    #[test] fn checked_in_vectors_match() { vectors(); }
}
"#;

fn output(command: &mut Command) -> std::process::Output {
    let result = command.bounded_output(Duration::from_secs(15)).unwrap();
    assert!(result.status.success(), "{command:?}: {result:?}");
    result
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = output(Command::new("/usr/bin/git").current_dir(root).args(args));
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn write(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn installed_rust_bin() -> PathBuf {
    // `which` only resolves an already-installed toolchain. No installer,
    // download, executable substitute, version relaxation or successful skip.
    let tool = |name: &str| {
        let result = output(Command::new("rustup").args(["which", "--toolchain", "1.96.1", name]));
        let path = PathBuf::from(String::from_utf8(result.stdout).unwrap().trim());
        let path = path.canonicalize().unwrap();
        let version = output(Command::new(&path).arg("--version"));
        assert!(String::from_utf8_lossy(&version.stdout).starts_with(&format!("{name} 1.96.1 ")));
        path
    };
    let rustc = tool("rustc");
    let cargo = tool("cargo");
    assert_eq!(rustc.parent(), cargo.parent());
    rustc.parent().unwrap().into()
}

#[test]
fn accepted_executor_builds_and_tests_edited_candidate_with_independent_custody() {
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let rust_bin = installed_rust_bin();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir(home.path().join("cargo-home")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", TEST_NAME, "--nocapture"]);
        // Test-runner/compiler/provider overrides must not become source policy.
        // Only this child's environment changes; parallel tests remain untouched.
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if custody::declared_toolchain::ENVIRONMENT_KEYS.contains(&name.as_ref())
                || [
                    "CARGO_",
                    "GIT_",
                    "SOURCE_CI_",
                    "CUSTODY_",
                    "GITHUB_",
                    "BUILDKITE_",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                command.env_remove(key);
            }
        }
        command
            .env(CHILD, "1")
            .env("HOME", home.path())
            .env("CARGO_HOME", home.path().join("cargo-home"))
            .env("CARGO_NET_OFFLINE", "true")
            .env("PATH", format!("{}:/usr/bin:/bin", rust_bin.display()))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "core.hooksPath")
            .env("GIT_CONFIG_VALUE_0", "/dev/null")
            .env("GIT_CONFIG_KEY_1", "commit.gpgsign")
            .env("GIT_CONFIG_VALUE_1", "false");
        let result = command
            .bounded_capture(
                ProcessLimits {
                    timeout: Duration::from_secs(300),
                    max_output_bytes: 8 * 1024 * 1024,
                },
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(result.termination, Termination::Exited, "{result:?}");
        assert!(result.output.status.success(), "{result:?}");
        assert!(
            String::from_utf8_lossy(&result.output.stdout).contains("1 passed"),
            "{result:?}"
        );
        return;
    }
    for case in ["passed", "test-failure", "build-failure"] {
        exercise(case);
    }
}

fn exercise(case: &str) {
    let storage = tempfile::tempdir().unwrap();
    let subject = storage.path().join("subject");
    let executor_root = storage.path().join("accepted-executor");
    fs::create_dir(&subject).unwrap();
    for (name, bytes) in custody::profile_sources() {
        let path = custody::profile_source_path(name);
        write(&subject, path.to_str().unwrap(), bytes);
    }
    write(&subject, ".gitignore", "**/target/\n");
    write(
        &subject,
        "tests/golden_vectors/Cargo.toml",
        "[package]\nname = \"golden_vectors\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
    );
    write(
        &subject,
        "tests/golden_vectors/src/main.rs",
        CANDIDATE.replace("42", "7"),
    );
    // Freeze the installed pinned Cargo's canonical dependency-free lockfile
    // before R is committed. Execution must still reject every tracked change.
    output(
        Command::new("cargo")
            .current_dir(subject.join("tests/golden_vectors"))
            .args(["generate-lockfile", "--offline"]),
    );
    for (name, contents) in [
        ("scope_record.cbor", "candidate H input\n"),
        (
            "scope_record.cbor.hex",
            "63616e646964617465204820696e7075740a\n",
        ),
        ("scope_record_hash.txt", "5d85267cc8996fa0\n"),
        ("state_commit_root.txt", "58a633bff4c4d913\n"),
    ] {
        write(
            &subject,
            &format!("tests/golden/state_commit/v1/{name}"),
            contents,
        );
    }
    for (name, contents) in [
        ("initial.txt", "initial\n"),
        ("target.txt", "target\n"),
        ("change.txt", "candidate delta\n"),
    ] {
        write(
            &subject,
            &format!("tests/fixtures/state_commit_repo/{name}"),
            contents,
        );
    }
    git(&subject, &["init", "-q", "-b", "work"]);
    git(&subject, &["config", "user.name", "Real compiler fixture"]);
    git(
        &subject,
        &["config", "user.email", "compiler@example.invalid"],
    );
    git(&subject, &["add", "."]);
    git(&subject, &["commit", "-qm", "accepted executor R"]);
    let base = git(&subject, &["rev-parse", "HEAD"]);
    let executor_tree = git(&subject, &["rev-parse", "HEAD^{tree}"]);
    git(
        &subject,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            executor_root.to_str().unwrap(),
            &base,
        ],
    );
    // H cannot supply the executor program. Its incompatible helper is tracked
    // input while the in-process compiled R catalog retains full recipe custody.
    let changed_helper = "assets/tools/source_ci_profile.rs";
    write(
        &subject,
        changed_helper,
        "compile_error!(\"candidate helper must never select the recipe\");\n",
    );
    let source = match case {
        "build-failure" => {
            format!("{CANDIDATE}\ncompile_error!(\"real candidate build failure\");\n")
        }
        "test-failure" => CANDIDATE.replace(
            "assert_eq!(CANDIDATE_VALUE, 42)",
            "assert_eq!(CANDIDATE_VALUE, 43)",
        ),
        _ => CANDIDATE.into(),
    };
    write(&subject, "tests/golden_vectors/src/main.rs", source);
    git(&subject, &["add", "."]);
    git(&subject, &["commit", "-qm", "edited candidate H"]);
    let head = git(&subject, &["rev-parse", "HEAD"]);
    let tree = git(&subject, &["rev-parse", "HEAD^{tree}"]);
    assert_ne!(base, head);
    assert_ne!(executor_tree, tree);
    let check = source::find_check(CHECK).unwrap();
    let paths = source::changed_paths(&subject, &base, &head).unwrap();
    let recipe = source::commands(check, &paths).unwrap();
    assert_eq!(recipe[0].args, ["build", "--release"]);
    assert_eq!(recipe[1].args, ["test", "--release"]);
    assert_eq!(
        recipe.last().unwrap().program,
        "./target/release/golden_vectors"
    );
    let profile_digest = custody::profile_digest().unwrap();
    let mut project = super::super::tests::project(&subject);
    project.facts.changed_paths = paths.iter().cloned().collect();
    let captured = profile::definition_files()
        .into_iter()
        .map(|(path, _)| {
            let raw = output(
                Command::new("/usr/bin/git")
                    .current_dir(&executor_root)
                    .args(["show", &format!("{base}:{path}")]),
            );
            (path.to_string(), hash(&raw.stdout))
        })
        .collect();
    project.executor = Some(
        profile::AcceptedExecutor::from_captured(
            base.clone(),
            executor_tree.clone(),
            SourceInventory { files: captured },
        )
        .unwrap(),
    );
    let expected = SourceExpectation {
        workflow: check.workflow.into(),
        job: check.job.into(),
        check: CHECK.into(),
        matrix: check.matrix.map(str::to_string),
        platform: check.platform.name().into(),
        base: base.clone(),
        head: head.clone(),
        snapshot: head.clone(),
        execution_revision: head.clone(),
        tree: tree.clone(),
        provider: "github".into(),
        run_id: "12".into(),
        run_attempt: "1".into(),
        job_id: check.job.into(),
        step_key: String::new(),
        profile_digest: profile_digest.clone(),
        invocations: custody::recipe_json(&recipe).into_bytes(),
        artifact_source_paths: BTreeSet::from([ARTIFACT.into()]),
        execution_mode: "source_checkout".into(),
        executor: Some(ExecutorExpectation {
            revision: base.clone(),
            tree: executor_tree.clone(),
            profile_digest: profile_digest.clone(),
        }),
    };
    let mut context = Context {
        snapshot: head.clone(),
        validation_head: head.clone(),
        tree: tree.clone(),
        provider: expected.provider.clone(),
        run_id: expected.run_id.clone(),
        run_attempt: expected.run_attempt.clone(),
        job_id: expected.job_id.clone(),
        step_key: String::new(),
        expected_profile: profile_digest,
        expected_invocation: custody::invocation_digest(&recipe).unwrap(),
        expected_toolchain: None,
        preparation_file: None,
        evidence_dir: storage.path().join("preparation"),
        executor: Some(ExecutorContext {
            root: executor_root.clone(),
            revision: base.clone(),
            tree: executor_tree,
        }),
    };
    let originals: BTreeMap<_, _> = git(&subject, &["ls-files"])
        .lines()
        .map(|name| (name.to_string(), fs::read(subject.join(name)).unwrap()))
        .collect();
    assert!(!subject.join(ARTIFACT).exists());
    assert!(
        custody::run(&subject, check, &paths, &base, &head, &context, true).unwrap(),
        "{case}: preparation"
    );
    let preparation_dir = context.evidence_dir.clone();
    let mut files = read_evidence(&preparation_dir, "preparation");
    let transport = ArtifactFiles::fixture(ORIGIN, files.clone());
    let dependencies = integration::Dependencies::default();
    // Full production policy approves measured versions, sysroot, native linker,
    // configuration, capabilities and source identity before substantive work.
    let prepared = consumer::verify_preparation(&transport, &expected, |measured| {
        verify(&project, check, &expected, measured, &dependencies)
    })
    .unwrap();
    context.expected_toolchain = Some(prepared.toolchain_digest());
    context.preparation_file = Some(preparation_dir.join("preparation.json"));
    context.evidence_dir = storage.path().join("execution");
    let passed = custody::run(&subject, check, &paths, &base, &head, &context, false).unwrap();
    assert_eq!(passed, case == "passed", "{case}");
    files.extend(read_evidence(&context.evidence_dir, "execution"));
    let transport = ArtifactFiles::fixture(ORIGIN, files.clone());
    let verified = consumer::verify_execution(&transport, &prepared);
    let execution: Value =
        serde_json::from_slice(&files[&format!("{CHECK}/execution/execution.json")]).unwrap();
    assert_eq!(execution["authenticated"], false);
    assert_eq!(execution["complete_pr_profile"], false);
    assert_eq!(execution["execution_complete"], passed);
    let results = execution["results"].as_array().unwrap();
    if passed {
        let verified = verified.unwrap();
        assert_eq!(results.len(), recipe.len());
        assert_eq!(verified.artifacts().len(), 1);
        let artifact = &files[&format!("{CHECK}/execution/artifact-0")];
        assert!(artifact.starts_with(b"\x7fELF"));
        assert_eq!(artifact, &fs::read(subject.join(ARTIFACT)).unwrap());
        assert_eq!(verified.artifacts()[0].source_path, ARTIFACT);
        assert_eq!(verified.artifacts()[0].digest, hash(artifact));
        assert_eq!(verified.artifacts()[0].bytes, artifact.len() as u64);
        assert_eq!(results.last().unwrap()["executable_sha256"], hash(artifact));
        let test_stdout =
            String::from_utf8_lossy(&files[&format!("{CHECK}/execution/command-1.stdout.log")]);
        assert!(
            test_stdout.contains("tests::candidate_value_is_real ... ok"),
            "{test_stdout}"
        );
        assert!(
            test_stdout.contains("tests::checked_in_vectors_match ... ok"),
            "{test_stdout}"
        );
        let binary_stdout =
            &files[&format!("{CHECK}/execution/command-{}.stdout.log", recipe.len() - 1)];
        assert_eq!(binary_stdout, b"scope_record_hash: 5d85267cc8996fa0\nstate_commit_root: 58a633bff4c4d913\ncandidate_value: 42\n");
        for member in [
            "artifact-0",
            "command-1.stdout.log",
            "frozen-preparation.json",
        ] {
            let mut changed = files.clone();
            changed
                .get_mut(&format!("{CHECK}/execution/{member}"))
                .unwrap()
                .push(b'!');
            assert!(
                consumer::verify_execution(&ArtifactFiles::fixture(ORIGIN, changed), &prepared)
                    .is_err(),
                "accepted changed {member}"
            );
        }
        let mut substituted = expected.clone();
        substituted.executor.as_mut().unwrap().revision = head.clone();
        assert!(
            consumer::verify_preparation(&transport, &substituted, |measured| verify(
                &project,
                check,
                &substituted,
                measured,
                &dependencies
            ))
            .is_err()
        );
        // R must still be the immutable full closure after its receipt is frozen.
        let original = fs::read(executor_root.join(changed_helper)).unwrap();
        write(
            &executor_root,
            changed_helper,
            "changed after preparation\n",
        );
        context.evidence_dir = storage.path().join("tampered-executor");
        assert!(custody::run(&subject, check, &paths, &base, &head, &context, false).is_err());
        assert!(!context.evidence_dir.exists());
        write(&executor_root, changed_helper, original);
    } else {
        assert!(
            verified.is_err(),
            "{case}: independent consumer accepted failed execution"
        );
        assert_eq!(execution["status"], "failed");
        assert_eq!(results.len(), if case == "build-failure" { 1 } else { 2 });
        assert_eq!(results.last().unwrap()["exit_code"], 101);
        assert_eq!(results.last().unwrap()["success"], false);
        assert_eq!(subject.join(ARTIFACT).exists(), case == "test-failure");
        assert!(!context.evidence_dir.join("artifact-0").exists());
        let failed_index = results.len() - 1;
        let mut diagnostic =
            files[&format!("{CHECK}/execution/command-{failed_index}.stdout.log")].clone();
        diagnostic.extend_from_slice(
            &files[&format!("{CHECK}/execution/command-{failed_index}.stderr.log")],
        );
        let diagnostic = String::from_utf8_lossy(&diagnostic);
        if case == "test-failure" {
            assert!(
                diagnostic.contains("tests::candidate_value_is_real ... FAILED")
                    && diagnostic.contains("left: 42")
                    && diagnostic.contains("right: 43"),
                "{diagnostic}"
            );
        } else {
            assert!(
                diagnostic.contains("real candidate build failure"),
                "{diagnostic}"
            );
        }
        assert!(
            !subject
                .join("tests/fixtures/state_commit_repo/.git")
                .exists()
        );
    }
    for (path, original) in originals {
        assert_eq!(
            fs::read(subject.join(&path)).unwrap(),
            original,
            "{case}: changed {path}"
        );
    }
    assert_eq!(git(&subject, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&subject, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(git(&subject, &["status", "--porcelain"]), "");
    assert_eq!(git(&executor_root, &["status", "--porcelain"]), "");
    assert!(!executor_root.join("tests/golden_vectors/target").exists());
}

fn read_evidence(directory: &Path, phase: &str) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            (
                format!("{CHECK}/{phase}/{}", entry.file_name().to_str().unwrap()),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}
