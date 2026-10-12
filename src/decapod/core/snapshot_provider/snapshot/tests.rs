use super::*;
use std::process::Command;
fn setup() -> (tempfile::TempDir, SnapshotRequest) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let run = |args: &[&str]| {
        let r = Command::new("/usr/bin/git")
            .current_dir(root)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "{:?}",
            String::from_utf8_lossy(&r.stderr)
        );
        String::from_utf8(r.stdout).unwrap().trim().to_string()
    };
    run(&["init", "-b", "work"]);
    run(&["config", "user.name", "Test"]);
    run(&["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("source"), "base").unwrap();
    run(&["add", "source"]);
    run(&["commit", "-m", "base"]);
    let base = run(&["rev-parse", "HEAD"]);
    std::fs::write(root.join("source"), "candidate").unwrap();
    run(&["add", "source"]);
    run(&["commit", "-m", "candidate"]);
    let head = run(&["rev-parse", "HEAD"]);
    let tree = run(&["rev-parse", "HEAD^{tree}"]);
    let r = SnapshotRequest {
        repository: Repository::new("DecapodLabs", "decapod").unwrap(),
        request_id: "11111111-2222-3333-4444-555555555555".into(),
        source_ref: "refs/heads/work".into(),
        source_head: head,
        source_tree: tree,
        accepted_base: base,
        accepted_base_ref: "refs/heads/master".into(),
        tags: Default::default(),
    };
    (d, r)
}
fn local() -> OfficialCli {
    OfficialCli::new(
        "/usr/bin/git".into(),
        "/usr/bin/true".into(),
        None,
        |_, _| Ok(()),
    )
    .unwrap()
}
#[test]
fn legacy_metadata_is_not_a_transport_object() {
    let (_, r) = setup();
    assert_eq!(r.message().unwrap().lines().count(), 7);
    assert!(r.message().unwrap().ends_with('\n'));
}
#[test]
fn prepare_preserves_branch_index_and_content() {
    let (d, r) = setup();
    let cancel = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &cancel).unwrap();
    let index = std::fs::read(d.path().join(".git/index")).unwrap();
    let refs = std::fs::read(d.path().join(".git/refs/heads/work")).unwrap();
    let objects = git(&mut local(), d.path(), &["count-objects", "-v"], &b).unwrap();
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    assert_eq!(
        objects,
        git(&mut local(), d.path(), &["count-objects", "-v"], &b).unwrap()
    );
    assert_eq!(p.transport_head(), r.source_head);
    assert_eq!(p.request().transport_ref(), r.source_ref);
    assert_eq!(index, std::fs::read(d.path().join(".git/index")).unwrap());
    assert_eq!(
        refs,
        std::fs::read(d.path().join(".git/refs/heads/work")).unwrap()
    );
    assert_eq!(
        std::fs::read(d.path().join("source")).unwrap(),
        b"candidate"
    );
    assert!(!d.path().join(".git/refs/heads/decapod").exists());
}
#[test]
fn dirty_and_untracked_material_never_ignored() {
    for untracked in [false, true] {
        let (d, r) = setup();
        std::fs::write(
            d.path().join(if untracked { "new" } else { "source" }),
            "dirty",
        )
        .unwrap();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
        assert!(prepare(&mut local(), d.path(), r, &b).is_err());
    }
}
#[test]
fn staged_material_and_hidden_index_rejected() {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let (d, r) = setup();
        assert!(
            Command::new("/usr/bin/git")
                .current_dir(d.path())
                .args(["update-index", flag, "source"])
                .status()
                .unwrap()
                .success()
        );
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
        assert!(prepare(&mut local(), d.path(), r, &b).is_err());
    }
    let (d, r) = setup();
    std::fs::write(d.path().join("source"), "stage").unwrap();
    assert!(
        Command::new("/usr/bin/git")
            .current_dir(d.path())
            .args(["add", "source"])
            .status()
            .unwrap()
            .success()
    );
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    assert!(prepare(&mut local(), d.path(), r, &b).is_err());
}
#[test]
fn metadata_inputs_reject_ref_or_argument_injection() {
    let (_, r) = setup();
    for v in [
        "-bad",
        "refs/heads/a\nb",
        "refs/heads/a..b",
        "refs/heads/x.lock",
        "refs/heads/decapod/preflight/evil",
    ] {
        let mut x = r.clone();
        x.source_ref = v.into();
        assert!(x.validate().is_err());
    }
    let mut x = r.clone();
    x.source_ref = x.accepted_base_ref.clone();
    assert!(x.validate().is_err());
    let mut x = r;
    x.request_id = x.request_id.to_uppercase().replace('1', "A");
    assert!(x.validate().is_err());
}
#[test]
fn readback_requires_one_exact_immutable_ref() {
    let r = "refs/heads/decapod/preflight/11111111-2222-3333-4444-555555555555";
    let sha = "a".repeat(40);
    assert_eq!(
        parse_readback(format!("{sha}\t{r}\n").as_bytes(), r).unwrap(),
        Some(sha.clone())
    );
    assert_eq!(parse_readback(b"", r).unwrap(), None);
    for s in [
        format!("{sha}\t{r}\n{sha}\t{r}\n"),
        format!("{sha}\trefs/heads/master\n"),
        format!("short\t{r}\n"),
    ] {
        assert!(parse_readback(s.as_bytes(), r).is_err());
    }
}
struct Sim {
    calls: Vec<Vec<String>>,
    ref_id: Option<String>,
}
impl Runner for Sim {
    fn execute(&mut self, c: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
        if c.args.iter().any(|v| v == "config") {
            return Ok(Reply {
                exit_code: 1,
                stdout: vec![],
            });
        }
        self.calls.push(c.args.clone());
        assert!(c.args.iter().any(|v| v == "ls-remote"));
        Ok(Reply {
            exit_code: 0,
            stdout: self
                .ref_id
                .as_ref()
                .map(|id| format!("{id}\t{}\n", c.args.last().unwrap()).into_bytes())
                .unwrap_or_default(),
        })
    }
}
#[test]
fn cleanup_retains_real_workspace_without_any_remote_operation() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let p = prepare(&mut local(), d.path(), r, &b).unwrap();
    let mut sim = Sim {
        calls: vec![],
        ref_id: None,
    };
    assert_eq!(
        cleanup(&mut sim, &p, &b).unwrap(),
        Cleanup::RetainedWorkbranch
    );
    assert!(sim.calls.is_empty());
}
#[test]
fn cancellation_prevents_preparation_spawn() {
    let (d, r) = setup();
    let c = Cancellation::default();
    c.cancel();
    let b = Budget::new(Duration::from_secs(10), 1024, &c).unwrap();
    assert_eq!(
        prepare(&mut local(), d.path(), r, &b).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}

struct PublishFake {
    local: OfficialCli,
    base: String,
    remote: Option<String>,
    pushes: Vec<Vec<String>>,
    race: bool,
}
impl Runner for PublishFake {
    fn execute(&mut self, c: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
        if c.program == Program::GitLocal {
            return self.local.execute(c, b);
        }
        assert_eq!(c.program, Program::GitTransport);
        let a = &c.args;
        if a.iter().any(|v| v == "config") {
            return Ok(Reply {
                exit_code: 1,
                stdout: vec![],
            });
        }
        if a.iter().any(|v| v == "--tags") {
            return Ok(Reply {
                exit_code: 0,
                stdout: vec![],
            });
        }
        if a.iter().any(|v| v == "ls-remote") {
            let reference = a.last().unwrap();
            let id = if reference == "refs/heads/master" {
                Some(&self.base)
            } else {
                self.remote.as_ref()
            };
            return Ok(Reply {
                exit_code: 0,
                stdout: id
                    .map(|id| format!("{id}\t{reference}\n").into_bytes())
                    .unwrap_or_default(),
            });
        }
        if a.iter().any(|v| v == "fetch") {
            assert!(a.iter().any(|v| v == "--no-write-fetch-head"));
            assert_eq!(a.last(), self.remote.as_ref());
            return Ok(Reply {
                exit_code: 0,
                stdout: vec![],
            });
        }
        assert!(a.iter().any(|v| v == "push"));
        let (id, reference) = a.last().unwrap().split_once(':').unwrap();
        assert!(oid(id) && reference == "refs/heads/work");
        assert!(!a.iter().any(|v| v.starts_with("--force")
            || v == "--all"
            || v == "--mirror"
            || v.starts_with('+')));
        self.pushes.push(a.clone());
        if self.race {
            return Ok(Reply {
                exit_code: 1,
                stdout: vec![],
            });
        }
        self.remote = Some(id.into());
        Ok(Reply {
            exit_code: 0,
            stdout: vec![],
        })
    }
}
#[test]
fn first_publication_is_only_exact_existing_local_branch_without_force() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    let mut fake = PublishFake {
        local: local(),
        base: r.accepted_base,
        remote: None,
        pushes: vec![],
        race: false,
    };
    let attempt = publish(&mut fake, p, &b);
    assert!(attempt.outcome.is_ok());
    assert_eq!(fake.pushes.len(), 1);
    assert_eq!(
        fake.remote.as_deref(),
        Some(attempt.snapshot.transport_head())
    );
}
#[test]
fn missing_ancestor_object_or_changed_base_prevents_any_push() {
    for changed_base in [false, true] {
        let (d, r) = setup();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
        let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
        let mut fake = PublishFake {
            local: local(),
            base: if changed_base {
                "f".repeat(40)
            } else {
                r.accepted_base
            },
            remote: if changed_base {
                None
            } else {
                Some("e".repeat(40))
            },
            pushes: vec![],
            race: false,
        };
        assert!(publish(&mut fake, p, &b).outcome.is_err());
        assert!(fake.pushes.is_empty());
    }
}
#[test]
fn uncertain_push_retains_exact_recovery_snapshot() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    let head = p.transport_head().to_string();
    let mut fake = PublishFake {
        local: local(),
        base: r.accepted_base,
        remote: None,
        pushes: vec![],
        race: true,
    };
    let attempt = publish(&mut fake, p, &b);
    assert!(attempt.outcome.is_err());
    assert_eq!(attempt.snapshot.transport_head(), head);
    assert_eq!(fake.pushes.len(), 1);
}

#[test]
fn full_tag_inventory_preserves_annotated_objects_and_rejects_local_change() {
    let (d, mut r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    assert!(
        Command::new("/usr/bin/git")
            .current_dir(d.path())
            .args(["tag", "-a", "v1", "-m", "release tag"])
            .status()
            .unwrap()
            .success()
    );
    assert!(prepare(&mut local(), d.path(), r.clone(), &b).is_err());
    r.tags = capture_local_tags(&mut local(), d.path(), &b).unwrap();
    assert_eq!(r.tags.len(), 1);
    assert_ne!(r.tags["refs/tags/v1"], r.source_head);
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    let mut remote = PublishFake {
        local: local(),
        base: r.accepted_base,
        remote: None,
        pushes: vec![],
        race: false,
    };
    assert!(publish(&mut remote, p, &b).outcome.is_err());
    assert!(remote.pushes.is_empty());
}
#[test]
fn remote_tag_comparison_uses_real_refs_and_validates_peeled_entries() {
    let tag = "a".repeat(40);
    let commit = "b".repeat(40);
    let raw = format!("{tag}\trefs/tags/v1\n{commit}\trefs/tags/v1^{{}}\n");
    let parsed = parse_tags(raw.as_bytes(), true).unwrap();
    assert_eq!(parsed, BTreeMap::from([("refs/tags/v1".into(), tag)]));
    assert!(parse_tags(raw.as_bytes(), false).is_err());
    assert!(
        parse_tags(
            format!("{commit}\trefs/tags/missing^{{}}\n").as_bytes(),
            true
        )
        .is_err()
    );
}
#[test]
fn authorization_denial_happens_before_any_process() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let mut runner = OfficialCli::new(
        "/usr/bin/git".into(),
        "/usr/bin/true".into(),
        None,
        |_, _| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "existing policy requires authorization",
            ))
        },
    )
    .unwrap();
    assert_eq!(
        prepare(&mut runner, d.path(), r, &b).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}
#[test]
fn actual_byte_check_defeats_a_trusted_index_stat_cache() {
    let (d, r) = setup();
    let root = d.path();
    let old = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    let set_time = || {
        std::fs::File::options()
            .write(true)
            .open(root.join("source"))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap()
    };
    assert!(
        Command::new("/usr/bin/git")
            .current_dir(root)
            .args(["config", "core.trustctime", "false"])
            .status()
            .unwrap()
            .success()
    );
    set_time();
    assert!(
        Command::new("/usr/bin/git")
            .current_dir(root)
            .args(["update-index", "--refresh"])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(root.join("source"), "different").unwrap();
    set_time();
    assert!(
        Command::new("/usr/bin/git")
            .current_dir(root)
            .args(["diff", "--quiet", "HEAD"])
            .status()
            .unwrap()
            .success(),
        "fixture must demonstrate the Git stat-cache shortcut"
    );
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let err = prepare(&mut local(), root, r, &b).unwrap_err();
    assert!(
        err.to_string().contains("actual source bytes differ"),
        "{err}"
    );
}
#[test]
fn resume_retains_exact_transport_without_recreating_commit_or_changing_refs() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    let resumed = resume(&mut local(), d.path(), r.clone(), p.transport_head(), &b).unwrap();
    assert_eq!(resumed.transport_head(), p.transport_head());
    let mut wrong = r.clone();
    wrong.source_tree = "e".repeat(40);
    assert!(resume(&mut local(), d.path(), wrong, p.transport_head(), &b).is_err());
    assert!(resume(&mut local(), d.path(), r.clone(), &r.accepted_base, &b).is_err());
    std::fs::write(d.path().join("source"), "dirty").unwrap();
    assert!(resume(&mut local(), d.path(), r, p.transport_head(), &b).is_err());
}
#[test]
fn accepted_target_lookup_is_readonly_and_rejects_temp_or_malformed_refs() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let mut sim = Sim {
        calls: vec![],
        ref_id: Some(r.accepted_base.clone()),
    };
    assert_eq!(
        accepted_target(&mut sim, d.path(), &r.repository, &r.accepted_base_ref, &b).unwrap(),
        Some(r.accepted_base)
    );
    assert!(sim.calls.iter().all(|a| a.iter().any(|a| a == "ls-remote")));
    let count = sim.calls.len();
    for reference in [
        "--upload-pack=unsafe",
        "refs/heads/decapod/preflight/11111111-1111-1111-1111-111111111111",
    ] {
        assert!(accepted_target(&mut sim, d.path(), &r.repository, reference, &b).is_err());
    }
    assert_eq!(sim.calls.len(), count);
}

#[test]
fn existing_equal_or_ancestor_workspace_can_be_reused_or_fast_forwarded() {
    for equal in [false, true] {
        let (d, r) = setup();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
        let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
        let mut fake = PublishFake {
            local: local(),
            base: r.accepted_base.clone(),
            remote: Some(if equal {
                r.source_head.clone()
            } else {
                r.accepted_base
            }),
            pushes: vec![],
            race: false,
        };
        let attempt = publish(&mut fake, p, &b);
        assert!(attempt.outcome.is_ok(), "{:?}", attempt.outcome);
        assert_eq!(fake.pushes.len(), usize::from(!equal));
        assert_eq!(fake.remote.as_deref(), Some(r.source_head.as_str()));
    }
}

#[test]
fn known_diverged_workspace_is_never_force_pushed() {
    let (d, r) = setup();
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(10), 1024 * 1024, &c).unwrap();
    let other = git(
        &mut local(),
        d.path(),
        &[
            "commit-tree",
            &r.source_tree,
            "-p",
            &r.accepted_base,
            "-m",
            "other",
        ],
        &b,
    )
    .unwrap();
    let p = prepare(&mut local(), d.path(), r.clone(), &b).unwrap();
    let mut fake = PublishFake {
        local: local(),
        base: r.accepted_base,
        remote: Some(other),
        pushes: vec![],
        race: false,
    };
    assert!(publish(&mut fake, p, &b).outcome.is_err());
    assert!(fake.pushes.is_empty());
}
