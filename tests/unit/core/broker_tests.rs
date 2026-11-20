use super::*;

#[test]
fn standalone_audit_acquires_pool_before_waiting_for_audit_mutex() {
    let root = tempfile::tempdir().unwrap();
    crate::core::todo::initialize_todo_db(root.path()).unwrap();
    let path = events::canonical_db_path(root.path());
    let broker = DbBroker::new(root.path());
    let observed = std::thread::scope(|scope| {
        let audit_guard = get_audit_lock().lock().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let worker = scope.spawn(move || {
            let _ = started_tx.send(());
            broker.log_event("lock-order-test", None, "test.get", "decapod.db", "success")
        });
        let started = started_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        let deadline = Instant::now() + Duration::from_secs(2);
        let observation = loop {
            match pool::global_pool().operation_lock_is_held_for_test(&path) {
                Ok(true) => break Ok(true),
                Ok(false) if !started || Instant::now() >= deadline => break Ok(false),
                Ok(false) => std::thread::sleep(Duration::from_millis(5)),
                Err(error) => break Err(error),
            }
        };
        // Release before joining or asserting on every observation outcome.
        // Otherwise a failed ordering check could poison the shared mutex.
        drop(audit_guard);
        worker.join().unwrap().unwrap();
        observation.unwrap()
    });
    assert!(
        observed,
        "standalone audit waited for the audit mutex without holding its database operation lock"
    );
    let audit = events::query(root.path(), events::BROKER, usize::MAX).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|event| event.payload["actor"] == "lock-order-test")
            .count(),
        1
    );
}

struct StubIncarnationProbe {
    boot_id: Option<String>,
    host_id: Option<String>,
    pid_ns: Option<String>,
    process: ProcessProbe,
}

impl IncarnationProbe for StubIncarnationProbe {
    fn current_boot_id(&self) -> Option<String> {
        self.boot_id.clone()
    }

    fn current_host_id(&self) -> Option<String> {
        self.host_id.clone()
    }

    fn current_pid_ns(&self) -> Option<String> {
        self.pid_ns.clone()
    }

    fn probe_process(&self, _pid: u32) -> ProcessProbe {
        self.process.clone()
    }
}

fn seed_custom_pending(
    root: &std::path::Path,
    event_id: &str,
    ts: &str,
    incarnation: WriterIncarnation,
) {
    if !events::canonical_db_path(root).exists() {
        crate::core::todo::initialize_todo_db(root).unwrap();
    }
    let mut payload = serde_json::json!({
        "schema_version": "1.1.0",
        "request_id": event_id,
        "ts": ts,
        "event_id": event_id,
        "actor": "operator",
        "actor_id": "operator",
        "caller_agent_id": "test-agent",
        "op": "todo.init",
        "db_id": "decapod.db",
        "status": "pending",
        "intent_ref": format!("intent:{event_id}"),
    });
    if let Some(pid) = incarnation.writer_pid {
        payload["writer_pid"] = serde_json::json!(pid);
    }
    if let Some(start_id) = incarnation.writer_start_id {
        payload["writer_start_id"] = serde_json::json!(start_id);
    }
    if let Some(boot_id) = incarnation.writer_boot_id {
        payload["writer_boot_id"] = serde_json::json!(boot_id);
    }
    if let Some(host_id) = incarnation.writer_host_id {
        payload["writer_host_id"] = serde_json::json!(host_id);
    }
    if let Some(pid_ns) = incarnation.writer_pid_ns {
        payload["writer_pid_ns"] = serde_json::json!(pid_ns);
    }
    events::append(root, events::BROKER, &payload).unwrap();
}

#[test]
fn build_event_captures_writer_incarnation_on_pending_only() {
    let tmp = tempfile::tempdir().unwrap();
    crate::core::todo::initialize_todo_db(tmp.path()).unwrap();
    let db_path = events::canonical_db_path(tmp.path());
    let broker = DbBroker::new(tmp.path());
    let _: Result<(), error::DecapodError> =
        broker.with_conn(&db_path, "operator", None, "todo.add", |_| Ok(()));

    let stored = events::query(tmp.path(), events::BROKER, usize::MAX).unwrap();
    let decoded: Vec<BrokerEvent> = decode_events(stored).unwrap();
    let pending = decoded
        .iter()
        .find(|ev| ev.op == "todo.add" && ev.status == "pending")
        .expect("pending event recorded");
    let terminal = decoded
        .iter()
        .find(|ev| ev.op == "todo.add" && ev.status == "success")
        .expect("terminal event recorded");

    assert_eq!(pending.writer_pid, Some(std::process::id()));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        assert!(pending.writer_start_id.is_some());
        assert!(pending.writer_boot_id.is_some());
    }
    assert_eq!(terminal.writer_pid, None);
    assert_eq!(terminal.writer_start_id, None);
    assert_eq!(terminal.writer_boot_id, None);
}

#[test]
fn verify_and_repair_refuse_active_writer_with_matching_start_id() {
    let tmp = tempfile::tempdir().unwrap();
    seed_custom_pending(
        tmp.path(),
        "live-evt",
        "1Z",
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-1".into()),
            writer_host_id: Some("host-1".into()),
            writer_pid_ns: None,
        },
    );
    let probe = StubIncarnationProbe {
        boot_id: Some("boot-1".into()),
        host_id: Some("host-1".into()),
        pid_ns: None,
        process: ProcessProbe::Running {
            start_id: Some("start-1".into()),
        },
    };
    let stored =
        decode_events(events::query(tmp.path(), events::BROKER, usize::MAX).unwrap()).unwrap();
    let replay = replay_report_with_probe(&stored, &probe);
    assert_eq!(replay.divergences.len(), 1);
    assert_eq!(
        replay.divergences[0].writer_liveness,
        WriterLiveness::Active
    );
    assert!(!replay.divergences[0].human_review_required);

    let broker = DbBroker::new(tmp.path());
    assert!(
        broker
            .repair_with_probe("live-evt", "Still live", false, false, "operator", &probe)
            .is_err()
    );
    assert!(
        broker
            .repair_with_probe("live-evt", "Still live", true, true, "operator", &probe)
            .is_err()
    );
}

#[test]
fn verify_and_repair_detect_pid_reuse_as_stopped_and_repairable() {
    let tmp = tempfile::tempdir().unwrap();
    seed_custom_pending(
        tmp.path(),
        "reused-evt",
        &time::now_epoch_z(),
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-old".into()),
            writer_boot_id: Some("boot-1".into()),
            writer_host_id: Some("host-1".into()),
            writer_pid_ns: None,
        },
    );
    let probe = StubIncarnationProbe {
        boot_id: Some("boot-1".into()),
        host_id: Some("host-1".into()),
        pid_ns: None,
        process: ProcessProbe::Running {
            start_id: Some("start-new".into()),
        },
    };
    let stored =
        decode_events(events::query(tmp.path(), events::BROKER, usize::MAX).unwrap()).unwrap();
    let replay = replay_report_with_probe(&stored, &probe);
    assert_eq!(replay.divergences.len(), 1);
    assert_eq!(
        replay.divergences[0].writer_liveness,
        WriterLiveness::Stopped
    );
    assert!(!replay.divergences[0].human_review_required);
    assert!(replay.divergences[0].liveness_detail.contains("reused"));

    let broker = DbBroker::new(tmp.path());
    let preview = broker
        .repair_with_probe("reused-evt", "PID reused", false, false, "operator", &probe)
        .unwrap();
    assert_eq!(preview.status, "preview");
    assert_eq!(preview.writer_liveness, WriterLiveness::Stopped);
    assert!(!preview.human_review_required);

    let applied = broker
        .repair_with_probe("reused-evt", "PID reused", true, false, "operator", &probe)
        .unwrap();
    assert_eq!(applied.status, "abandoned");
    assert!(!applied.acknowledgment_required);
}

#[test]
fn verify_and_repair_detect_absent_pid_and_prior_boot_same_host_as_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    seed_custom_pending(
        tmp.path(),
        "absent-evt",
        &time::now_epoch_z(),
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-1".into()),
            writer_host_id: Some("host-1".into()),
            writer_pid_ns: None,
        },
    );
    seed_custom_pending(
        tmp.path(),
        "prior-boot-evt",
        &time::now_epoch_z(),
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-old".into()),
            writer_host_id: Some("host-1".into()),
            writer_pid_ns: None,
        },
    );
    let probe = StubIncarnationProbe {
        boot_id: Some("boot-1".into()),
        host_id: Some("host-1".into()),
        pid_ns: None,
        process: ProcessProbe::Absent,
    };
    let broker = DbBroker::new(tmp.path());
    assert_eq!(
        broker
            .repair_with_probe("absent-evt", "Exited", true, false, "operator", &probe)
            .unwrap()
            .status,
        "abandoned"
    );
    assert_eq!(
        broker
            .repair_with_probe(
                "prior-boot-evt",
                "Host rebooted",
                true,
                false,
                "operator",
                &probe
            )
            .unwrap()
            .status,
        "abandoned"
    );
}

#[test]
fn verify_and_repair_fail_closed_for_legacy_cross_host_and_partial_provenance() {
    let tmp = tempfile::tempdir().unwrap();
    // 1. Legacy provenance-free event (very old timestamp "1Z")
    seed_custom_pending(tmp.path(), "legacy-evt", "1Z", WriterIncarnation::default());
    // 2. Partial provenance: bare PID without start_id / boot_id (must NOT be trusted!)
    seed_custom_pending(
        tmp.path(),
        "bare-pid-evt",
        "1Z",
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: None,
            writer_boot_id: None,
            writer_host_id: None,
            writer_pid_ns: None,
        },
    );
    // 3. Cross-host event (different boot_id AND different host_id)
    seed_custom_pending(
        tmp.path(),
        "cross-host-evt",
        "1Z",
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-remote".into()),
            writer_host_id: Some("host-remote".into()),
            writer_pid_ns: None,
        },
    );
    // 4. Different boot_id when host_id is unavailable
    seed_custom_pending(
        tmp.path(),
        "no-host-id-evt",
        "1Z",
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-remote".into()),
            writer_host_id: None,
            writer_pid_ns: None,
        },
    );
    // 5. Different PID namespace on same boot
    seed_custom_pending(
        tmp.path(),
        "diff-ns-evt",
        "1Z",
        WriterIncarnation {
            writer_pid: Some(4242),
            writer_start_id: Some("start-1".into()),
            writer_boot_id: Some("boot-local".into()),
            writer_host_id: Some("host-local".into()),
            writer_pid_ns: Some("pid:[111]".into()),
        },
    );

    let probe = StubIncarnationProbe {
        boot_id: Some("boot-local".into()),
        host_id: Some("host-local".into()),
        pid_ns: Some("pid:[222]".into()),
        process: ProcessProbe::Absent,
    };

    let stored =
        decode_events(events::query(tmp.path(), events::BROKER, usize::MAX).unwrap()).unwrap();
    let replay = replay_report_with_probe(&stored, &probe);
    assert_eq!(replay.divergences.len(), 5);
    for div in &replay.divergences {
        assert_eq!(
            div.writer_liveness,
            WriterLiveness::Unknown,
            "{}",
            div.event_id
        );
        assert!(div.human_review_required, "{}", div.event_id);
    }

    let broker = DbBroker::new(tmp.path());
    for id in [
        "legacy-evt",
        "bare-pid-evt",
        "cross-host-evt",
        "no-host-id-evt",
        "diff-ns-evt",
    ] {
        let preview = broker
            .repair_with_probe(id, "Human inspected", false, false, "operator", &probe)
            .unwrap();
        assert_eq!(preview.status, "preview");
        assert_eq!(preview.writer_liveness, WriterLiveness::Unknown);
        assert!(preview.human_review_required);
        assert!(preview.acknowledgment_required);

        // Silent --apply without acknowledge_unknown_writer MUST fail closed
        assert!(
            broker
                .repair_with_probe(id, "Human inspected", true, false, "operator", &probe)
                .is_err(),
            "silent apply should fail for {id}"
        );

        // Explicit human-reviewed acknowledgment succeeds
        let applied = broker
            .repair_with_probe(id, "Human inspected", true, true, "operator", &probe)
            .unwrap();
        assert_eq!(applied.status, "abandoned");
        assert!(applied.acknowledged_unknown_writer);
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn real_os_probe_distinguishes_live_child_pid_reuse_and_dead_child() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep child");
    let pid = child.id();
    let probe = RealIncarnationProbe;
    let ProcessProbe::Running {
        start_id: Some(real_start_id),
    } = probe.probe_process(pid)
    else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("expected live child start_id");
    };
    let real_boot_id = probe.current_boot_id().expect("boot_id available");
    let real_host_id = probe.current_host_id();
    let real_pid_ns = probe.current_pid_ns();

    seed_custom_pending(
        tmp.path(),
        "real-live",
        &time::now_epoch_z(),
        WriterIncarnation {
            writer_pid: Some(pid),
            writer_start_id: Some(real_start_id),
            writer_boot_id: Some(real_boot_id.clone()),
            writer_host_id: real_host_id.clone(),
            writer_pid_ns: real_pid_ns.clone(),
        },
    );
    // Simulate PID reuse on the exact same live OS PID with a different start_id
    seed_custom_pending(
        tmp.path(),
        "real-pid-reused",
        &time::now_epoch_z(),
        WriterIncarnation {
            writer_pid: Some(pid),
            writer_start_id: Some("different-prior-start-id".into()),
            writer_boot_id: Some(real_boot_id),
            writer_host_id: real_host_id,
            writer_pid_ns: real_pid_ns,
        },
    );

    let broker = DbBroker::new(tmp.path());
    let report = broker.verify_replay().unwrap();
    let live_div = report
        .divergences
        .iter()
        .find(|d| d.event_id == "real-live")
        .unwrap();
    let reuse_div = report
        .divergences
        .iter()
        .find(|d| d.event_id == "real-pid-reused")
        .unwrap();
    assert_eq!(live_div.writer_liveness, WriterLiveness::Active);
    assert_eq!(reuse_div.writer_liveness, WriterLiveness::Stopped);

    // Live writer refused even with acknowledge_unknown_writer
    assert!(
        broker
            .repair_with_options("real-live", "Attempt", true, true, "operator")
            .is_err()
    );
    // Reused PID repairable without acknowledge_unknown_writer
    let repaired_reuse = broker
        .repair_with_options("real-pid-reused", "PID reused", true, false, "operator")
        .unwrap();
    assert_eq!(repaired_reuse.status, "abandoned");

    // Terminate and reap the child process -> original event is now Stopped and repairable
    child.kill().unwrap();
    child.wait().unwrap();

    let after_exit = broker.verify_replay().unwrap();
    assert_eq!(after_exit.divergences.len(), 1);
    assert_eq!(
        after_exit.divergences[0].writer_liveness,
        WriterLiveness::Stopped
    );
    let repaired_dead = broker
        .repair_with_options("real-live", "Child exited", true, false, "operator")
        .unwrap();
    assert_eq!(repaired_dead.status, "abandoned");
    assert!(broker.verify_replay().unwrap().divergences.is_empty());
}
