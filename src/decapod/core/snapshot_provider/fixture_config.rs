//! Synthetic provider fixtures own local repositories, not the CI host's Git
//! authentication configuration. Query real fixture config through GitLocal;
//! production GitTransport continues to inspect and reject all URL rewrites.
use super::*;

const CONFIG_QUERY: &[&str] = &[
    "config",
    "--name-only",
    "--get-regexp",
    "^url\\..*\\.(insteadof|pushinsteadof)$",
];

pub(crate) fn execute(
    official: &mut OfficialCli,
    spec: &CommandSpec,
    budget: &Budget<'_>,
) -> Result<Reply> {
    let expected = git_spec(&spec.cwd, CONFIG_QUERY, true);
    ensure(
        spec.program == Program::GitTransport
            && spec.args == expected.args
            && spec.organization.is_none()
            && spec.provider_request.is_none(),
        "fixture configuration adapter accepts only the exact read-only rewrite query",
    )?;
    official.execute(&git_spec(&spec.cwd, CONFIG_QUERY, false), budget)
}

#[test]
fn fixture_config_isolates_host_rewrites_but_preserves_local_rejection() {
    const CHILD: &str = "DECAPOD_FIXTURE_CONFIG_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let root = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("/usr/bin/git")
                .current_dir(root.path())
                .env_clear()
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "fixture git command failed");
        };
        git(&["init", "--quiet"]);
        let mut official = OfficialCli::git_only("/usr/bin/git".into(), |_, _| Ok(())).unwrap();
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_secs(10), 1024 * 1024, &cancel).unwrap();
        let spec = git_spec(root.path(), CONFIG_QUERY, true);
        // The real production transport still sees the synthetic host rewrite.
        let host = official.execute(&spec, &budget).unwrap();
        assert_eq!(host.exit_code, 0);
        assert!(!host.stdout.is_empty());
        let isolated = execute(&mut official, &spec, &budget).unwrap();
        assert_eq!(isolated.exit_code, 1);
        assert!(isolated.stdout.is_empty());
        git(&[
            "config",
            "url.https://fixture.invalid/local/.insteadOf",
            "https://fixture.invalid/source/",
        ]);
        let local = execute(&mut official, &spec, &budget).unwrap();
        assert_eq!(local.exit_code, 0);
        assert!(
            !local.stdout.is_empty(),
            "local rewrite must remain rejected by the guard"
        );
        assert!(
            execute(
                &mut official,
                &git_spec(root.path(), &["config", "user.name", "unauthorized"], true),
                &budget,
            )
            .is_err()
        );
        assert!(
            execute(
                &mut official,
                &git_spec(
                    root.path(),
                    &["ls-remote", "https://fixture.invalid/repo"],
                    true
                ),
                &budget,
            )
            .is_err()
        );
        println!("FIXTURE_CONFIG_ISOLATION_CHECKED");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join(".gitconfig"),
        "[url \"https://fixture.invalid/host/\"]\n\tinsteadOf = https://fixture.invalid/source/\n",
    )
    .unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::snapshot_provider::fixture_config::fixture_config_isolates_host_rewrites_but_preserves_local_rejection",
            "--nocapture",
        ])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env(CHILD, "1")
        .bounded_output(Duration::from_secs(20))
        .unwrap();
    assert!(
        output.status.success(),
        "fixture configuration child failed"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("FIXTURE_CONFIG_ISOLATION_CHECKED"));
}
