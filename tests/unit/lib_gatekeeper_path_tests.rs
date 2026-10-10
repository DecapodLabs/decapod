use super::*;

#[test]
fn explicit_missing_directory_and_comma_joined_paths_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("one.rs"), "fn main() {}\n").unwrap();
    fs::write(root.path().join("two.rs"), "fn main() {}\n").unwrap();
    fs::create_dir(root.path().join("directory")).unwrap();
    fs::write(root.path().join("unreadable.rs"), [0xff, 0xfe]).unwrap();
    for path in ["missing.rs", "directory", "one.rs,two.rs", "unreadable.rs"] {
        let error = collect_gatekeeper_paths(root.path(), Some(vec![path.to_string()]))
            .expect_err("unscanned explicit paths must fail");
        assert!(error.to_string().contains("explicit scan path"));
    }
    assert_eq!(
        collect_gatekeeper_paths(
            root.path(),
            Some(vec!["one.rs".to_string(), "two.rs".to_string()])
        )
        .unwrap(),
        vec![PathBuf::from("one.rs"), PathBuf::from("two.rs")]
    );
    fs::write(root.path().join("one,two.rs"), "fn main() {}\n").unwrap();
    assert_eq!(
        collect_gatekeeper_paths(root.path(), Some(vec!["one,two.rs".to_string()])).unwrap(),
        vec![PathBuf::from("one,two.rs")]
    );
}

#[test]
fn git_enumeration_failure_is_not_an_empty_successful_scan() {
    let root = tempfile::tempdir().unwrap();
    let error = collect_gatekeeper_paths(root.path(), None).unwrap_err();
    assert!(error.to_string().contains("no scan was performed"));
}

#[test]
fn staged_paths_preserve_whitespace_and_deleted_path_guards() {
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "Git fixture command failed");
    };
    git(&["init", "-q"]);
    fs::write(root.path().join("removed.rs"), "fn removed() {}\n").unwrap();
    git(&["add", "removed.rs"]);
    git(&[
        "-c",
        "user.name=Gatekeeper Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    git(&["rm", "-q", "removed.rs"]);
    let unusual = "space and\nnewline.rs";
    fs::write(root.path().join(unusual), "fn main() {}\n").unwrap();
    git(&["add", unusual]);
    let paths = collect_gatekeeper_paths(root.path(), None).unwrap();
    assert_eq!(
        paths,
        vec![PathBuf::from("removed.rs"), PathBuf::from(unusual)]
    );
    let config = core::gatekeeper::GatekeeperConfig {
        protected_paths: vec!["removed.rs".to_string()],
        ..Default::default()
    };
    let result = core::gatekeeper::run_gatekeeper(root.path(), &paths, 0, &config).unwrap();
    assert!(
        !result.passed,
        "deletion must not bypass protected-path policy"
    );
}

#[test]
fn renamed_protected_source_path_is_preserved_for_policy_checks() {
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    fs::write(root.path().join("protected.rs"), "fn protected() {}\n").unwrap();
    git(&["add", "protected.rs"]);
    git(&[
        "-c",
        "user.name=Gatekeeper Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    git(&["mv", "protected.rs", "renamed.rs"]);
    let paths = collect_gatekeeper_paths(root.path(), None).unwrap();
    assert!(paths.contains(&PathBuf::from("protected.rs")));
    assert!(paths.contains(&PathBuf::from("renamed.rs")));
}
