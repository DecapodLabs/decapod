use super::*;
use crate::core::db::Connection;
use std::sync::{Arc, Barrier};

#[test]
fn default_seeding_is_atomic_under_64_independent_sqlite_connections() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("seeds.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(schemas::APTITUDE_DB_SCHEMA_AGENT_PROMPTS)
        .unwrap();
    conn.execute_batch(schemas::APTITUDE_DB_SCHEMA_INDEX_PROMPT_CONTEXT)
        .unwrap();
    let barrier = Arc::new(Barrier::new(64));
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..64 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                let conn = Connection::open(path).unwrap();
                barrier.wait();
                seed_default_agent_prompts(&conn, "fixture-time").unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let count: u64 = conn
        .query_row("SELECT COUNT(*) FROM agent_prompts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, DEFAULT_AGENT_PROMPTS.len() as u64);
    for (context, prompt, _) in DEFAULT_AGENT_PROMPTS {
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_prompts WHERE context=?1 AND prompt_text=?2",
                params![context, prompt],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    // Public custom insertion remains free to retain identical user entries.
    conn.execute_batch("INSERT INTO agent_prompts(id,context,prompt_text,created_at) VALUES('custom-a','custom','same text','original'),('custom-b','custom','same text','original')").unwrap();
    seed_default_agent_prompts(&conn, "later").unwrap();
    let count: u64 = conn
        .query_row("SELECT COUNT(*) FROM agent_prompts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, DEFAULT_AGENT_PROMPTS.len() as u64 + 2);
}
