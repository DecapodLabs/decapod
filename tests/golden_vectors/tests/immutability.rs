//! Exercise the actual CLI against disposable real Git history. Expected v1
//! bytes come from the checked-in vector; the verifier must never rewrite them.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const OUTPUTS: [&str; 4] = [
    "scope_record.cbor",
    "scope_record.cbor.hex",
    "scope_record_hash.txt",
    "state_commit_root.txt",
];

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "decapod-golden-v1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        fs::create_dir_all(fixture.cwd()).unwrap();
        fs::create_dir_all(fixture.repo()).unwrap();
        fs::create_dir_all(fixture.goldens()).unwrap();
        let checked_in =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/state_commit_repo");
        for name in [
            "initial.txt",
            "target.txt",
            ".ignored_test.txt",
            "exec_test.rs",
            "lfs_pointer.txt",
            "regular.txt",
            "日本語ファイル.txt",
        ] {
            fs::copy(checked_in.join(name), fixture.repo().join(name)).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            fs::set_permissions(
                fixture.repo().join("exec_test.rs"),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            symlink("target.txt", fixture.repo().join("symlink_test")).unwrap();
        }
        for name in OUTPUTS {
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../golden/state_commit/v1")
                    .join(name),
                fixture.goldens().join(name),
            )
            .unwrap();
        }
        git(&fixture.repo(), &["init", "-q", "--initial-branch=master"]);
        git(&fixture.repo(), &["config", "user.name", "Golden fixture"]);
        git(
            &fixture.repo(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        git(&fixture.repo(), &["config", "core.autocrlf", "false"]);
        git(&fixture.repo(), &["config", "core.filemode", "true"]);
        git(&fixture.repo(), &["add", "initial.txt", "target.txt"]);
        git(&fixture.repo(), &["commit", "-qm", "base commit"]);
        git(&fixture.repo(), &["add", "."]);
        git(&fixture.repo(), &["commit", "-qm", "fixture commit"]);
        fixture
    }
    fn repo(&self) -> PathBuf {
        self.0.join("fixtures/state_commit_repo")
    }
    fn cwd(&self) -> PathBuf {
        self.0.join("golden_vectors")
    }
    fn goldens(&self) -> PathBuf {
        self.0.join("golden/state_commit/v1")
    }
    fn bytes(&self) -> Vec<Option<Vec<u8>>> {
        OUTPUTS
            .iter()
            .map(|name| fs::read(self.goldens().join(name)).ok())
            .collect()
    }
    fn check(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_golden_vectors"))
            .arg("--check")
            .current_dir(self.cwd())
            .output()
            .unwrap()
    }
    fn reject_without_writes(&self, diagnostic: &str) {
        let before = self.bytes();
        let out = self.check();
        assert!(!out.status.success(), "unexpected success: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(diagnostic),
            "{out:?}"
        );
        assert_eq!(
            self.bytes(),
            before,
            "verification must not repair its expectations"
        );
    }
}

#[test]
#[cfg(unix)]
fn canonical_v1_fixture_is_reproducible_without_writing_goldens() {
    let fixture = Fixture::new();
    let before = fixture.bytes();
    let changed = git(
        &fixture.repo(),
        &[
            "-c",
            "core.quotepath=false",
            "diff",
            "--name-only",
            "HEAD~1",
            "HEAD",
        ],
    );
    assert_eq!(changed.lines().count(), 6);
    assert!(!changed.lines().any(|path| path == "target.txt"));
    assert_eq!(
        git(&fixture.repo(), &["show", "HEAD~1:target.txt"]),
        "target"
    );
    for _ in 0..2 {
        let out = fixture.check();
        assert!(out.status.success(), "{out:?}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("Verified all four immutable vectors")
        );
        assert_eq!(fixture.bytes(), before);
    }
}

#[test]
#[cfg(unix)]
fn changed_fixture_content_exec_symlink_or_delta_is_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for kind in ["content", "exec", "symlink", "delta"] {
        let fixture = Fixture::new();
        match kind {
            "content" => {
                fs::write(fixture.repo().join("regular.txt"), "changed content\n").unwrap()
            }
            "exec" => fs::set_permissions(
                fixture.repo().join("exec_test.rs"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap(),
            "symlink" => {
                fs::remove_file(fixture.repo().join("symlink_test")).unwrap();
                symlink("initial.txt", fixture.repo().join("symlink_test")).unwrap();
            }
            "delta" => fs::write(fixture.repo().join("target.txt"), "changed target\n").unwrap(),
            _ => unreachable!(),
        }
        git(&fixture.repo(), &["add", "."]);
        git(&fixture.repo(), &["commit", "--amend", "--no-edit", "-q"]);
        fixture.reject_without_writes("immutable v1 golden vector differs");
    }
}

#[test]
#[cfg(unix)]
fn each_missing_or_changed_expected_output_is_rejected_without_repair() {
    for name in OUTPUTS {
        for missing in [true, false] {
            let fixture = Fixture::new();
            if missing {
                fs::remove_file(fixture.goldens().join(name)).unwrap();
            } else {
                fs::write(fixture.goldens().join(name), "changed expectation").unwrap();
            }
            fixture.reject_without_writes(if missing {
                "missing golden vector"
            } else {
                "immutable v1 golden vector differs"
            });
        }
    }
}

#[test]
#[cfg(unix)]
fn absent_git_history_is_an_error_not_an_empty_generated_vector() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.repo().join(".git")).unwrap();
    fixture.reject_without_writes("fixture git");
}
