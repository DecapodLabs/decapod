//! Real CLI source discovery against a tracked, Rust-only consumer fixture.
//! This proves inventory/index binding, not full publication readiness.
use decapod::core::bounded_process::BoundedCommand;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

// Compile the same fixture tests into the required CLI suite; no nested Cargo
// process, interpreter, network fetch, or optional test selector is needed.
#[path = "../fixtures/justfile_consumer/src/lib.rs"]
mod consumer;

const FILES: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        include_str!("../fixtures/justfile_consumer/Cargo.toml"),
    ),
    (
        "Cargo.lock",
        include_str!("../fixtures/justfile_consumer/Cargo.lock"),
    ),
    (
        "Justfile",
        include_str!("../fixtures/justfile_consumer/Justfile"),
    ),
    (
        "src/lib.rs",
        include_str!("../fixtures/justfile_consumer/src/lib.rs"),
    ),
    (
        "src/distance.rs",
        include_str!("../fixtures/justfile_consumer/src/distance.rs"),
    ),
    (
        "src/temperature.rs",
        include_str!("../fixtures/justfile_consumer/src/temperature.rs"),
    ),
];

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .bounded_output(Duration::from_secs(10))
        .expect("bounded fixture Git command")
}

fn cli(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(super::resolve_decapod_bin())
        .current_dir(root)
        .args(args)
        .env("XDG_CONFIG_HOME", root.join("machine-config"))
        .env("XDG_DATA_HOME", root.join("machine-data"))
        .env("DECAPOD_AGENT_ID", "justfile-inventory")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .bounded_output(Duration::from_secs(40))
        .expect("bounded fixture CLI command");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if args[0] == "rpc" {
        serde_json::from_slice(&output.stdout).expect("actual agent.init JSON")
    } else {
        serde_json::Value::Null
    }
}

fn check_inventory(root: &Path) {
    for (path, contents) in FILES {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    assert!(git(root, &["init", "-b", "main"]).status.success());
    assert!(
        git(
            root,
            &["add", "Cargo.toml", "Cargo.lock", "Justfile", "src"]
        )
        .status
        .success()
    );
    let tracked = git(root, &["ls-files", "-z"]);
    assert!(tracked.status.success());
    let spellings: Vec<_> = tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| path.eq_ignore_ascii_case(b"justfile"))
        .collect();
    assert_eq!(spellings, vec![b"Justfile".as_slice()]);
    assert!(
        !git(root, &["ls-files", "--error-unmatch", "justfile"])
            .status
            .success(),
        "the lowercase probe must not have a tracked index entry"
    );

    cli(
        root,
        &[
            "init",
            "--proof",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
    );
    let response = cli(root, &["rpc", "--op", "agent.init"]);
    assert_eq!(response["success"], true, "{response}");
    let report = &response["result"]["reconciliation"];
    let sources = report["discovered_sources"].as_object().unwrap();
    let discovered: Vec<_> = sources
        .keys()
        .filter(|name| name.eq_ignore_ascii_case("justfile"))
        .map(String::as_str)
        .collect();
    assert_eq!(discovered, vec!["Justfile"], "{report}");

    // Bind the inventory digest to the exact tracked bytes, rather than
    // accepting a filesystem lookup that silently resolves an alias.
    let indexed = git(root, &["show", ":Justfile"]);
    assert!(indexed.status.success());
    assert_eq!(
        sources["Justfile"],
        format!("sha256:{:x}", Sha256::digest(&indexed.stdout))
    );
    assert_eq!(report["proof_status"], "not_run");
    assert_eq!(report["merge_ready"], false);
}

#[test]
fn tracked_rust_consumer_justfile_binds_exact_index_spelling() {
    let root = super::contract_fixture();
    check_inventory(root.path());
}

#[cfg(target_os = "macos")]
#[test]
fn case_insensitive_apfs_cli_inventory_has_no_phantom_justfile() {
    use std::ffi::{CStr, CString};
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let root = super::contract_fixture();
    let path = CString::new(root.path().as_os_str().as_bytes()).unwrap();
    let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: path is NUL-terminated and filesystem is writable storage for
    // statfs. Its fields are read only after the successful syscall.
    assert_eq!(
        unsafe { libc::statfs(path.as_ptr(), filesystem.as_mut_ptr()) },
        0
    );
    let filesystem = unsafe { filesystem.assume_init() };
    // SAFETY: successful Darwin statfs supplies a NUL-terminated type name.
    let kind = unsafe { CStr::from_ptr(filesystem.f_fstypename.as_ptr()) };
    assert_eq!(
        kind.to_bytes(),
        b"apfs",
        "this required acceptance needs APFS"
    );
    fs::write(root.path().join("Justfile"), "case probe").unwrap();
    let upper = fs::metadata(root.path().join("Justfile")).unwrap();
    let lower = fs::metadata(root.path().join("justfile"))
        .expect("this required acceptance needs case-insensitive APFS");
    assert_eq!((upper.dev(), upper.ino()), (lower.dev(), lower.ino()));
    check_inventory(root.path());
}
