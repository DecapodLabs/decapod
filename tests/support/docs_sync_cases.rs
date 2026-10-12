use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    path: PathBuf,
    base: String,
    head: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "decapod-docs-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

impl Fixture {
    fn new() -> Self {
        let path = scratch("fixture");
        let result = Command::new("git")
            .args(["init", "--quiet", "-b", "fixture-base"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let mut fixture = Self {
            path,
            base: String::new(),
            head: String::new(),
        };
        fixture.git(&["config", "user.email", "docs-fixture@example.invalid"]);
        fixture.git(&["config", "user.name", "Docs Fixture"]);
        fixture.write(".gitignore", "/.fixture-*\n");
        fixture.write(GENERATED_DOCS[0], "generated contracts\n");
        fixture.write(GENERATED_DOCS[1], "generated schema\n");
        fixture.write("README.md", "original\n");
        let base = fixture.commit("base");
        fixture.git(&["checkout", "-b", "candidate"]);
        fixture.base = base.clone();
        fixture.head = base;
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        git_text(&self.path, args).unwrap()
    }

    fn write(&self, path: impl AsRef<Path>, text: &str) {
        let path = self.path.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn change(&mut self, path: impl AsRef<Path>) {
        self.write(path, "changed\n");
        self.head = self.commit("candidate");
    }

    fn select(&self) -> Check<()> {
        select(&self.path, &self.base, &self.head)
    }

    fn verify(&self) -> Check<()> {
        verify(&self.path, &self.base, &self.head, fake_binary())
    }

    fn mode(&self, mode: &str) {
        self.write(".fixture-mode", mode);
    }
}

fn fake_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        // Bazel and relocated shards carry this test-only executable in runfiles.
        // They intentionally do not depend on an ambient rustup home or rustc.
        if let Some(packaged) =
            option_env!("DECAPOD_DOCS_SYNC_TEST_FIXTURE").filter(|path| !path.is_empty())
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
            return candidates
                .into_iter()
                .filter(|path| path.is_file())
                .find_map(|path| path.canonicalize().ok())
                .expect("Bazel must provide its declared test-only docs fixture in runfiles");
        }
        // Cargo retains the same compiled fixture without adding a shipped bin.
        let root = scratch("fake-binary");
        let source = root.join("fake_docs.rs");
        fs::write(&source, include_str!("../fixtures/docs_sync_fixture.rs")).unwrap();
        let binary = root.join("decapod");
        let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let result = Command::new(compiler)
            .args(["--edition=2024"])
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        binary
    })
}

#[test]
fn full_sha_only_and_missing_objects_fail_closed() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    for bad in [
        "",
        "master",
        "HEAD",
        "HEAD~1",
        "--help",
        &f.head[..12],
        &"0".repeat(40),
        &"A".repeat(40),
    ] {
        assert!(select(&f.path, bad, &f.head).is_err(), "base {bad:?}");
        assert!(select(&f.path, &f.base, bad).is_err(), "head {bad:?}");
    }
    for (base, head) in [(&"1".repeat(40), &f.head), (&f.base, &"1".repeat(40))] {
        assert!(select(&f.path, base, head).is_err());
    }
    assert_eq!(f.git(&["rev-parse", "HEAD"]), f.head);
}

#[test]
fn detached_head_selection_and_branch_movement_keep_captured_range() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    f.select().unwrap();
    let later = f.git(&[
        "commit-tree",
        &format!("{}^{{tree}}", f.head),
        "-p",
        &f.head,
        "-m",
        "branch moved",
    ]);
    f.git(&["update-ref", "refs/heads/fixture-base", &f.head]);
    f.git(&["update-ref", "refs/heads/candidate", &later]);
    f.verify().unwrap();
    assert_eq!(f.git(&["rev-parse", "HEAD"]), f.head);
    assert_eq!(
        fs::read(f.path.join(".fixture-argv")).unwrap(),
        b"docs\0build\0--touched\0./src/decapod/cli.rs\0"
    );
}

#[test]
fn exact_default_merge_is_accepted_but_other_head_or_base_is_rejected() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    let merge = f.git(&[
        "commit-tree",
        &format!("{}^{{tree}}", f.head),
        "-p",
        &f.base,
        "-p",
        &f.head,
        "-m",
        "event merge",
    ]);
    f.git(&["checkout", "--detach", &merge]);
    assert!(select(&f.path, &f.head, &f.head).is_err());
    assert!(select(&f.path, &f.base, &f.base).is_err());
    f.select().unwrap();
    assert_eq!(f.git(&["rev-parse", "HEAD"]), f.head);
    f.git(&["checkout", "--detach", &f.base]);
    assert!(f.select().is_err());
    assert!(f.verify().is_err());
}

#[test]
fn shallow_or_unrelated_range_is_rejected_without_fetch() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    fs::write(f.path.join(".git/shallow"), format!("{}\n", f.base)).unwrap();
    assert!(f.select().unwrap_err().contains("full local Git history"));
    fs::remove_file(f.path.join(".git/shallow")).unwrap();
    let other = f.git(&[
        "commit-tree",
        &format!("{}^{{tree}}", f.head),
        "-m",
        "unrelated",
    ]);
    assert!(select(&f.path, &other, &f.head).is_err());
}

#[test]
fn staged_input_is_rejected_before_running_docs() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    f.select().unwrap();
    f.write(GENERATED_DOCS[0], "hidden staged edit\n");
    f.git(&["add", "--", GENERATED_DOCS[0]]);
    assert!(f.verify().is_err());
    assert!(!f.path.join(".fixture-argv").exists());
    assert!(f.select().is_err());
}

#[test]
fn generator_failure_drift_and_staged_hidden_drift_are_rejected() {
    for mode in ["fail", "drift", "stage", "stage-restore", "switch-head"] {
        let mut f = Fixture::new();
        f.change("src/decapod/cli.rs");
        f.select().unwrap();
        f.mode(mode);
        let error = f.verify().unwrap_err();
        if mode == "fail" {
            assert!(error.contains("37"), "{error}");
        }
        assert!(f.path.join(".fixture-argv").exists(), "{mode}");
    }
}

#[test]
fn source_paths_drive_existing_cli_selection_and_empty_range_builds_all() {
    for (changed, contracts, schema) in [
        ("src/decapod/cli.rs", true, true),
        ("src/decapod/core/docs_cli.rs", true, false),
        ("assets/schemas/trajectory.schema.json", true, false),
        ("src/other.rs", false, false),
        ("docs/ordinary.md", false, false),
    ] {
        for (document, should_regenerate) in
            [(GENERATED_DOCS[0], contracts), (GENERATED_DOCS[1], schema)]
        {
            let mut f = Fixture::new();
            f.write(document, "stale committed documentation\n");
            f.change(changed);
            f.select().unwrap();
            assert_eq!(
                f.verify().is_err(),
                should_regenerate,
                "{changed}: {document}"
            );
        }
    }
    let f = Fixture::new();
    f.select().unwrap();
    f.verify().unwrap();
    assert_eq!(
        fs::read(f.path.join(".fixture-argv")).unwrap(),
        b"docs\0build\0"
    );
}

#[test]
fn changed_filenames_are_byte_preserving_argv_never_shell_source() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut f = Fixture::new();
    let names = [
        OsString::from("space and\ttab\nnewline.rs"),
        OsString::from("$(touch SHOULD_NOT_EXIST);`touch ALSO_NOT_EXIST`.rs"),
        OsString::from("--help"),
        OsString::from("quote'\"\\*.rs"),
        OsString::from_vec(b"non-utf8-\xff.rs".to_vec()),
    ];
    for name in &names {
        f.write(name, "changed\n");
    }
    f.head = f.commit("unsafe filename fixtures");
    f.select().unwrap();
    f.verify().unwrap();
    let bytes = fs::read(f.path.join(".fixture-argv")).unwrap();
    let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    assert_eq!(
        &fields[..3],
        &[
            b"docs".as_slice(),
            b"build".as_slice(),
            b"--touched".as_slice()
        ]
    );
    assert_eq!(fields.len(), names.len() + 4);
    for name in names {
        let mut expected = b"./".to_vec();
        expected.extend(name.as_bytes());
        assert!(fields.contains(&expected.as_slice()), "{name:?}");
    }
    assert!(!f.path.join("SHOULD_NOT_EXIST").exists());
    assert!(!f.path.join("ALSO_NOT_EXIST").exists());
}

#[test]
fn owned_cli_rejects_extra_commands_and_missing_identity() {
    for args in [
        vec!["tool"],
        vec![
            "tool", "execute", "--repo", ".", "--base", "abc", "--head", "abc",
        ],
        vec![
            "tool",
            "select",
            "--repo",
            ".",
            "--base",
            "abc",
            "--head",
            "abc",
            "--command",
            "echo",
        ],
    ] {
        assert!(run(&args.into_iter().map(OsString::from).collect::<Vec<_>>()).is_err());
    }
}

#[test]
fn blobs_tags_and_missing_generated_documents_fail_closed() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    let blob = f.git(&["rev-parse", &format!("{}:README.md", f.head)]);
    assert!(select(&f.path, &blob, &f.head).is_err());
    assert!(select(&f.path, &f.base, &blob).is_err());
    f.git(&["tag", "-a", "fixture-tag", &f.head, "-m", "tag object"]);
    let tag = f.git(&["rev-parse", "fixture-tag"]);
    assert!(select(&f.path, &f.base, &tag).is_err());
    fs::remove_file(f.path.join(GENERATED_DOCS[1])).unwrap();
    f.head = f.commit("missing generated document");
    f.select().unwrap();
    assert!(
        f.verify()
            .unwrap_err()
            .contains("must exist in the captured commit")
    );
    assert!(!f.path.join(".fixture-argv").exists());
}

#[test]
fn an_attached_head_is_rejected_until_exact_local_selection() {
    let mut f = Fixture::new();
    f.change("src/decapod/cli.rs");
    assert!(f.verify().unwrap_err().contains("detached immutable HEAD"));
    assert!(!f.path.join(".fixture-argv").exists());
    f.select().unwrap();
    f.verify().unwrap();
}
