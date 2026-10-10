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
