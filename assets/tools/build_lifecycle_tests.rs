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
    let mut arguments = if env::var_os("DECAPOD_BUILD_TEST_BAZEL").is_some() {
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
        assert!(cargo.contains(&format!("{command}-main = \"run --quiet --features build-tools --bin decapod-build -- --scratch-parent target -- cargo {command}\"")));
    }
}
