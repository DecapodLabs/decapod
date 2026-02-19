use decapod::core::broker::DbBroker;
use decapod::core::error::DecapodError;

#[test]
fn test_demonstrate_crash_divergence_risk() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db_path = root.join("test.db");
    
    // Initialize DB
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)", []).unwrap();
    }

    let broker = DbBroker::new(root);
    
    // Simulate a crash by panicking inside the closure.
    let result = std::panic::catch_unwind(|| {
        let _: Result<(), DecapodError> = broker.with_conn(&db_path, "test-actor", None, "test.op", |conn| {
            conn.execute("INSERT INTO kv (key, value) VALUES ('k1', 'v1')", []).unwrap();
            // "Crash"
            panic!("SIMULATED CRASH");
        });
    });

    assert!(result.is_err());

    // Verify DB has the data
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let val: String = conn.query_row("SELECT value FROM kv WHERE key = 'k1'", [], |r| r.get(0)).unwrap();
        assert_eq!(val, "v1");
    }

    // Verify log DOES have the 'pending' event, but NO terminal event
    let report = broker.verify_replay().unwrap();
    assert_eq!(report.divergences.len(), 1, "Should detect one divergence from the simulated crash");
    assert_eq!(report.divergences[0].op, "test.op");
    assert_eq!(report.divergences[0].reason, "Pending event without terminal status (potential crash)");
}
