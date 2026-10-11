use super::*;
use std::process::{Child, Stdio};
use std::time::Instant;

struct Fixture(Scratch);

impl Fixture {
    fn new() -> Self {
        Self(Scratch::create(&env::temp_dir()).expect("create fixture"))
    }

    fn path(&self) -> &Path {
        &self.0.path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.0.clean().expect("clean fixture");
    }
}

fn supervisor(parent: &Path, record: &Path, behavior: &str) -> Command {
    let mut command = Command::new(env::current_exe().unwrap());
    command
        .args(["--exact", "tests::supervisor_fixture", "--nocapture"])
        .env("DECAPOD_BUILD_TEST_PARENT", parent)
        .env("DECAPOD_BUILD_TEST_RECORD", record)
        .env("DECAPOD_BUILD_TEST_BEHAVIOR", behavior)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn wait_record(record: &Path) -> PathBuf {
    let start = Instant::now();
    loop {
        if let Ok(path) = fs::read_to_string(record)
            && !path.is_empty()
        {
            return PathBuf::from(path);
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "child did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_child(child: &mut Child) -> ExitStatus {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().unwrap();
            panic!("supervisor did not terminate");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

// Subprocess fixtures execute the production supervisor. The environment is set
// only on new Commands, never mutated in the parallel test process.
#[test]
fn supervisor_fixture() {
    let Some(parent) = env::var_os("DECAPOD_BUILD_TEST_PARENT") else {
        return;
    };
    let executable = if env::var("DECAPOD_BUILD_TEST_BEHAVIOR").unwrap() == "spawn-error" {
        Path::new(&parent).join("nonexistent-executable")
    } else {
        env::current_exe().unwrap()
    };
    let mut arguments = if env::var_os("DECAPOD_BUILD_TEST_BAZEL").is_some()
        || env::var_os("DECAPOD_BUILD_TEST_AUTOMATIC").is_some()
    {
        Vec::new()
    } else {
        vec![
            OsString::from("--scratch-parent"),
            parent,
            OsString::from("--"),
        ]
    };
    arguments.extend([
        executable.into_os_string(),
        OsString::from("--exact"),
        OsString::from("tests::child_fixture"),
        OsString::from("--nocapture"),
    ]);
    let code = run(arguments).unwrap_or(1);
    std::process::exit(code);
}

#[test]
fn child_fixture() {
    let Ok(behavior) = env::var("DECAPOD_BUILD_TEST_BEHAVIOR") else {
        return;
    };
    let scratch = env::temp_dir();
    if let Some(expected) = env::var_os("DECAPOD_BUILD_TEST_CWD") {
        assert_eq!(
            env::current_dir().unwrap(),
            Path::new(&expected).canonicalize().unwrap()
        );
    }
    #[cfg(unix)]
    if behavior == "ignore-term" {
        unsafe extern "C" {
            fn signal(number: i32, handler: usize) -> usize;
        }
        // SAFETY: this isolated fixture ignores SIGTERM to exercise escalation.
        unsafe { signal(15, 1) };
    }
    assert_eq!(env::var_os("TMP"), Some(scratch.clone().into_os_string()));
    assert_eq!(env::var_os("TEMP"), Some(scratch.clone().into_os_string()));
    if env::var_os("DECAPOD_BUILD_TEST_BAZEL").is_some() {
        assert_eq!(
            env::var_os("TEST_TMPDIR"),
            Some(scratch.clone().into_os_string())
        );
        assert_eq!(env::var_os("HOME"), Some(scratch.clone().into_os_string()));
    }
    fs::create_dir(scratch.join("transient")).unwrap();
    fs::write(scratch.join("transient/generated"), "build scratch").unwrap();
    let record = PathBuf::from(env::var_os("DECAPOD_BUILD_TEST_RECORD").unwrap());
    fs::write(&record, scratch.to_str().unwrap()).unwrap();
    #[cfg(unix)]
    if behavior == "unix-socket" {
        use std::os::unix::net::{UnixListener, UnixStream};
        for relative in [
            ".tmpABCDEF/capability.sock",
            ".tmpABCDEF/.decapod/data/broker-runtime/broker.sock",
        ] {
            let path = scratch.join(relative);
            // sockaddr_un.sun_path is 104 bytes on macOS (108 on Linux),
            // including its terminator. Check this even if AF_UNIX is denied.
            assert!(
                path.as_os_str().as_encoded_bytes().len() < 104,
                "Unix socket path exceeds portable SUN_LEN: {}",
                path.display()
            );
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let listener = match UnixListener::bind(&path) {
                Ok(listener) => listener,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                    eprintln!(
                        "AF_UNIX capability unavailable ({error}); path-length assertion passed for {}, live bind/connect proof requires a socket-capable executor",
                        path.display()
                    );
                    continue;
                }
                Err(error) => panic!("real Unix socket {}: {error}", path.display()),
            };
            let client = UnixStream::connect(&path).unwrap();
            let (accepted, _) = listener.accept().unwrap();
            drop((client, accepted, listener));
        }
    }
    if behavior == "wait" || behavior == "ignore-term" {
        while !record.with_extension("release").exists() {
            thread::sleep(Duration::from_millis(10));
        }
    }
    std::process::exit(if behavior == "fail" { 37 } else { 0 });
}

#[test]
fn successful_and_failed_runs_clean_only_owned_scratch() {
    for behavior in ["success", "fail"] {
        let fixture = Fixture::new();
        let parent = fixture.path();
        for path in [
            "target/debug/cache",
            "bazel-out/binary",
            ".decapod/governance.json",
            "src/main.rs",
            "testlogs/proof.log",
        ] {
            let path = parent.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "preserve").unwrap();
        }
        let record = parent.join("record");
        let result = supervisor(parent, &record, behavior).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(if behavior == "fail" { 37 } else { 0 })
        );
        assert!(!wait_record(&record).exists());
        for path in [
            "target/debug/cache",
            "bazel-out/binary",
            ".decapod/governance.json",
            "src/main.rs",
            "testlogs/proof.log",
        ] {
            assert_eq!(fs::read_to_string(parent.join(path)).unwrap(), "preserve");
        }
    }
}

#[test]
fn failed_spawn_cleans_allocated_scratch() {
    let fixture = Fixture::new();
    let result = supervisor(
        fixture.path(),
        &fixture.path().join("record"),
        "spawn-error",
    )
    .output()
    .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(fs::read_dir(fixture.path()).unwrap().count(), 0);
}

#[test]
fn bazel_test_tmpdir_and_disposable_home_are_owned_but_proof_is_preserved() {
    let fixture = Fixture::new();
    let record = fixture.path().join("record");
    let proof = fixture.path().join("test.xml");
    fs::write(&proof, "preserve test evidence").unwrap();
    let result = supervisor(fixture.path(), &record, "success")
        .env("TEST_TMPDIR", fixture.path())
        .env("HOME", fixture.path())
        .env("DECAPOD_BUILD_TEST_BAZEL", "1")
        .env("XML_OUTPUT_FILE", &proof)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(!wait_record(&record).exists());
    assert_eq!(fs::read_to_string(proof).unwrap(), "preserve test evidence");
}

#[cfg(unix)]
#[test]
fn long_bazel_parent_uses_owned_short_scratch_for_real_unix_sockets() {
    let fixture = Fixture::new();
    let long_parent = fixture.path().join("bazel-output-base-".repeat(12));
    let process_temp = fixture.path().join("tmp");
    fs::create_dir_all(&long_parent).unwrap();
    fs::create_dir(&process_temp).unwrap();
    fs::write(long_parent.join("proof"), "keep Bazel evidence").unwrap();
    fs::write(process_temp.join("neighbor"), "keep another run").unwrap();
    let record = fixture.path().join("record");
    let output = supervisor(&long_parent, &record, "unix-socket")
        .env("TEST_TMPDIR", &long_parent)
        .env("HOME", &long_parent)
        .env("TMPDIR", &process_temp)
        .env("DECAPOD_BUILD_TEST_BAZEL", "1")
        .output()
        .unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scratch = wait_record(&record);
    assert!(scratch.is_absolute());
    assert_eq!(scratch.parent(), Some(process_temp.as_path()));
    assert!(!scratch.exists());
    assert_eq!(
        fs::read_to_string(long_parent.join("proof")).unwrap(),
        "keep Bazel evidence"
    );
    assert_eq!(
        fs::read_to_string(process_temp.join("neighbor")).unwrap(),
        "keep another run"
    );
}

#[test]
fn explicit_long_scratch_parent_is_never_silently_relocated() {
    let fixture = Fixture::new();
    let long_parent = fixture.path().join("explicit-build-parent-".repeat(10));
    let record = fixture.path().join("record");
    let output = supervisor(&long_parent, &record, "success")
        .env("TMPDIR", fixture.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let scratch = wait_record(&record);
    assert_eq!(scratch.parent(), Some(long_parent.as_path()));
    assert!(!scratch.exists());
    assert!(long_parent.exists());
}

#[cfg(unix)]
#[test]
fn automatic_cargo_mode_keeps_scratch_short_from_a_deep_checkout() {
    let fixture = Fixture::new();
    let checkout = fixture.path().join("deep-governed-worktree-".repeat(10));
    let process_temp = fixture.path().join("tmp");
    fs::create_dir_all(checkout.join("target")).unwrap();
    fs::create_dir(&process_temp).unwrap();
    fs::write(checkout.join("target/cache"), "reusable output").unwrap();
    fs::write(checkout.join("proof"), "preserve evidence").unwrap();
    fs::write(process_temp.join("neighbor"), "other invocation").unwrap();
    let record = fixture.path().join("record");
    let output = supervisor(&checkout, &record, "unix-socket")
        .current_dir(&checkout)
        .env_remove("TEST_TMPDIR")
        .env_remove("DECAPOD_BUILD_TEST_BAZEL")
        .env("DECAPOD_BUILD_TEST_AUTOMATIC", "1")
        .env("DECAPOD_BUILD_TEST_CWD", &checkout)
        .env("TMPDIR", &process_temp)
        .output()
        .unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scratch = wait_record(&record);
    assert!(scratch.is_absolute());
    assert_eq!(scratch.parent(), Some(process_temp.as_path()));
    assert!(!scratch.exists());
    for (path, expected) in [
        (checkout.join("target/cache"), "reusable output"),
        (checkout.join("proof"), "preserve evidence"),
        (process_temp.join("neighbor"), "other invocation"),
    ] {
        assert_eq!(fs::read_to_string(path).unwrap(), expected);
    }
}

#[cfg(unix)]
#[test]
fn short_parent_alias_stays_short_and_supports_real_unix_sockets() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let long_parent = fixture.path().join("bazel-sandbox-output-".repeat(10));
    fs::create_dir_all(&long_parent).unwrap();
    let alias = fixture.path().join("s");
    symlink(&long_parent, &alias).unwrap();
    let record = fixture.path().join("record");
    let output = supervisor(&alias, &record, "unix-socket").output().unwrap();
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scratch = wait_record(&record);
    assert!(scratch.is_absolute());
    assert_eq!(scratch.parent(), Some(alias.as_path()));
    assert!(!scratch.exists());
    assert!(alias.exists());
    assert!(long_parent.exists());
}

#[cfg(unix)]
#[test]
fn retargeted_parent_alias_never_deletes_the_new_targets_files() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let original = fixture.path().join("original");
    let replacement = fixture.path().join("replacement");
    fs::create_dir(&original).unwrap();
    fs::create_dir(&replacement).unwrap();
    let alias = fixture.path().join("s");
    symlink(&original, &alias).unwrap();
    let scratch = Scratch::create(&alias).unwrap();
    let name = scratch.path.file_name().unwrap();
    let unrelated = replacement.join(name);
    fs::remove_file(&alias).unwrap();
    symlink(&replacement, &alias).unwrap();
    // A missing path through a retargeted alias is not evidence of cleanup.
    assert!(scratch.clean().is_err());
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep"), "another run").unwrap();
    assert!(scratch.clean().is_err());
    assert!(original.join(name).exists());
    assert_eq!(
        fs::read_to_string(unrelated.join("keep")).unwrap(),
        "another run"
    );
}

#[test]
fn concurrent_runs_never_delete_each_others_live_scratch() {
    let fixture = Fixture::new();
    let first_record = fixture.path().join("first");
    let second_record = fixture.path().join("second");
    let mut first = supervisor(fixture.path(), &first_record, "wait")
        .spawn()
        .unwrap();
    let mut second = supervisor(fixture.path(), &second_record, "wait")
        .spawn()
        .unwrap();
    let first_scratch = wait_record(&first_record);
    let second_scratch = wait_record(&second_record);
    assert_ne!(first_scratch, second_scratch);
    fs::write(first_record.with_extension("release"), "finish").unwrap();
    assert!(wait_child(&mut first).success());
    assert!(!first_scratch.exists());
    assert!(second_scratch.join("transient/generated").is_file());
    fs::write(second_record.with_extension("release"), "finish").unwrap();
    assert!(wait_child(&mut second).success());
    assert!(!second_scratch.exists());
}

#[cfg(unix)]
#[test]
fn handled_interrupts_wait_for_the_child_and_clean_scratch() {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    for signal in [1, 2, 15] {
        let fixture = Fixture::new();
        let record = fixture.path().join("record");
        let mut child = supervisor(fixture.path(), &record, "wait").spawn().unwrap();
        let scratch = wait_record(&record);
        // SAFETY: signal the known live supervisor fixture, not its process group.
        assert_eq!(unsafe { kill(child.id() as i32, signal) }, 0);
        assert_eq!(wait_child(&mut child).code(), Some(128 + signal));
        assert!(!scratch.exists());
    }
}

#[cfg(unix)]
#[test]
fn ignored_termination_is_bounded_and_cleans_after_child_exit() {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let fixture = Fixture::new();
    let record = fixture.path().join("record");
    let mut child = supervisor(fixture.path(), &record, "ignore-term")
        .spawn()
        .unwrap();
    let scratch = wait_record(&record);
    // SAFETY: signal this known live supervisor; it owns the child process group.
    assert_eq!(unsafe { kill(child.id() as i32, 15) }, 0);
    assert_eq!(wait_child(&mut child).code(), Some(143));
    assert!(!scratch.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_does_not_follow_symlinks_or_delete_a_replaced_root() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let scratch = Scratch::create(fixture.path()).unwrap();
    let external = fixture.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep"), "proof").unwrap();
    symlink(&external, scratch.path.join("link")).unwrap();
    scratch.clean().unwrap();
    assert_eq!(fs::read_to_string(external.join("keep")).unwrap(), "proof");

    let scratch = Scratch::create(fixture.path()).unwrap();
    let moved = fixture.path().join("moved");
    fs::rename(&scratch.path, &moved).unwrap();
    symlink(&external, &scratch.path).unwrap();
    assert!(scratch.clean().is_err());
    assert!(moved.exists());
    assert!(external.join("keep").exists());
}

#[test]
fn existing_scratch_is_never_adopted_or_swept() {
    let fixture = Fixture::new();
    let first = Scratch::create(fixture.path()).unwrap();
    fs::write(first.path.join("keep"), "another run").unwrap();
    let second = Scratch::create(fixture.path()).unwrap();
    assert_ne!(first.path, second.path);
    second.clean().unwrap();
    assert!(first.path.join("keep").exists());
    first.clean().unwrap();
    first.clean().unwrap();
}

#[cfg(unix)]
#[test]
fn cleanup_diagnostics_report_readonly_paths_without_changing_or_following_them() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let scratch = Scratch::create(fixture.path()).unwrap();
    let readonly = scratch.path.join("readonly");
    fs::create_dir(&readonly).unwrap();
    fs::write(readonly.join("keep"), "fixture evidence").unwrap();
    fs::set_permissions(&readonly, fs::Permissions::from_mode(0o555)).unwrap();
    let external = fixture.path().join("external-must-not-be-traversed");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep"), "external evidence").unwrap();
    symlink(&external, scratch.path.join("link")).unwrap();
    let diagnostic = cleanup_diagnostics(&scratch.path);
    assert!(
        diagnostic.contains(readonly.to_str().unwrap()),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("mode=555"), "{diagnostic}");
    assert!(diagnostic.contains("uid="), "{diagnostic}");
    assert!(!diagnostic.contains("external-must-not-be-traversed"));
    assert_eq!(
        fs::metadata(&readonly).unwrap().permissions().mode() & 0o777,
        0o555
    );
    assert_eq!(
        fs::read_to_string(readonly.join("keep")).unwrap(),
        "fixture evidence"
    );
    assert_eq!(
        fs::read_to_string(external.join("keep")).unwrap(),
        "external evidence"
    );
    fs::set_permissions(&readonly, fs::Permissions::from_mode(0o755)).unwrap();
    scratch.clean().unwrap();
}

#[test]
fn main_build_and_test_configuration_enables_owned_cleanup() {
    let bazel = include_str!("../../.config/build/bazelrc");
    assert!(
        bazel
            .lines()
            .any(|line| line == "build --reuse_sandbox_directories=false")
    );
    assert!(
        bazel
            .lines()
            .any(|line| line == "build --experimental_sandbox_async_tree_delete_idle_threads=0")
    );
    assert!(
        bazel
            .lines()
            .any(|line| line == "test --run_under=//:build_lifecycle")
    );
    let cargo = include_str!("../../.cargo/config.toml");
    for profile in ["install", "dev", "test"] {
        let section = cargo
            .split(&format!("[profile.{profile}]"))
            .nth(1)
            .expect("profile exists")
            .split('\n')
            .take_while(|line| !line.starts_with('['))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            section.contains("incremental = false"),
            "{profile} must not accumulate incremental scratch by default"
        );
    }
    for command in ["build", "test"] {
        assert!(cargo.contains(&format!("{command}-main = \"run --quiet --features build-tools --bin decapod-build -- cargo {command}\"")));
    }
}

#[test]
fn bazel_lifecycle_inputs_use_exported_subpackage_labels() {
    let rules = include_str!("../../.config/build/decapod.BUILD.bzl");
    let lifecycle = rules
        .split("name = \"build_lifecycle_tests\"")
        .nth(1)
        .expect("lifecycle test target exists")
        .split("cargo_toml_env_vars(")
        .next()
        .unwrap();
    let package = include_str!("../../.config/build/BUILD.bazel");
    let exports = package
        .split("exports_files([")
        .nth(1)
        .expect("build package explicitly exports its source inputs")
        .split("])")
        .next()
        .unwrap();

    // These include_str! inputs live in a distinct Bazel package. Root-relative
    // file labels such as ".config/build/bazelrc" cannot cross that boundary.
    for name in ["BUILD.bazel", "bazelrc", "decapod.BUILD.bzl"] {
        assert!(
            lifecycle.contains(&format!("\"//.config/build:{name}\"")),
            "lifecycle input {name} must use its owning package label"
        );
        assert!(!lifecycle.contains(&format!("\".config/build/{name}\"")));
        assert!(!lifecycle.contains(&format!("\"//:.config/build/{name}\"")));
        assert!(
            exports.contains(&format!("\"{name}\"")),
            "build package must export {name} to the root test target"
        );
    }
    // .cargo has no BUILD file and is part of the root package.
    assert!(lifecycle.contains("\"//:.cargo/config.toml\""));
}
