use decapod::{core::db::Connection, plugins::aptitude::initialize_aptitude_db};
use serde_json::{Value, json};
use std::{fs, time::Instant};

fn prompts(conn: &Connection) -> Vec<Value> {
    let mut stmt=conn.prepare("SELECT id,context,prompt_text,priority,active,usage_count,last_shown_at,created_at,updated_at FROM agent_prompts ORDER BY id").unwrap();
    let mut rows = stmt.query([]).unwrap();
    let mut result = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        result.push(serde_json::to_value(row.parameters()).unwrap());
    }
    result
}

#[test]
fn repeated_initialization_retains_ids_disabled_state_and_custom_history() {
    let root = tempfile::tempdir().unwrap();
    initialize_aptitude_db(root.path()).unwrap();
    let conn = Connection::open(root.path().join("decapod.db")).unwrap();
    assert_eq!(prompts(&conn).len(), 4);
    conn.execute_batch("UPDATE agent_prompts SET active=0,usage_count=17,priority=3,last_shown_at='historical',updated_at='retained' WHERE context='workflow';
        INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,created_at) VALUES('custom','workflow','Retain custom configuration',7,0,9,'original');
        INSERT INTO agent_prompts(id,context,prompt_text,priority,active,usage_count,created_at) SELECT 'historical-duplicate',context,prompt_text,4,0,22,'original' FROM agent_prompts WHERE context='code_style';").unwrap();
    let before = prompts(&conn);
    for _ in 0..8 {
        initialize_aptitude_db(root.path()).unwrap();
    }
    assert_eq!(prompts(&conn), before);
}

#[test]
fn initialization_adds_only_a_missing_default() {
    let root = tempfile::tempdir().unwrap();
    initialize_aptitude_db(root.path()).unwrap();
    let conn = Connection::open(root.path().join("decapod.db")).unwrap();
    conn.execute("DELETE FROM agent_prompts WHERE context='workflow'", [])
        .unwrap();
    conn.execute_batch("INSERT INTO agent_prompts(id,context,prompt_text,created_at) VALUES('same-context-custom','workflow','Different custom text','original')").unwrap();
    let retained = prompts(&conn);
    initialize_aptitude_db(root.path()).unwrap();
    let current = prompts(&conn);
    assert_eq!(current.len(), 5);
    for row in retained {
        assert!(current.contains(&row));
    }
}

fn physical(conn: &Connection, path: &std::path::Path) -> Value {
    let mut stmt=conn.prepare("SELECT name,sum(pgsize) FROM dbstat WHERE name='agent_prompts' OR name='idx_agent_prompts_context' OR name='sqlite_autoindex_agent_prompts_1' GROUP BY name ORDER BY name").unwrap();
    let objects = stmt
        .query_map([], |r| {
            Ok(json!({"name":r.get::<_,String>(0)?,"bytes":r.get::<_,u64>(1)?}))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    json!({"file_bytes":fs::metadata(path).unwrap().len(),"prompt_objects":objects})
}

#[test]
#[ignore = "explicit repeated-initializer before/after measurement"]
fn measure_repeated_initializer() {
    let root = tempfile::tempdir().unwrap();
    let start = Instant::now();
    initialize_aptitude_db(root.path()).unwrap();
    let first_ns = start.elapsed().as_nanos();
    let path = root.path().join("decapod.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("VACUUM").unwrap();
    let initial = physical(&conn, &path);
    let mut ns = Vec::new();
    for _ in 0..128 {
        let start = Instant::now();
        initialize_aptitude_db(root.path()).unwrap();
        ns.push(start.elapsed().as_nanos());
    }
    let prompts = prompts(&conn);
    let allocated = physical(&conn, &path);
    conn.execute_batch("VACUUM").unwrap();
    let packed = physical(&conn, &path);
    ns.sort_unstable();
    let sqlite: String = conn
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    let journal: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let report = json!({"schema_version":1,"invocations":128,"sqlite_version":sqlite,"journal_mode":journal,
        "initialization_first_ns":first_ns,"p50_ns":ns[63],"p95_ns":ns[121],"p99_ns":ns[126],
        "initial_prompts":4,"final_prompts":prompts.len(),"new_prompt_rows":prompts.len()-4,
        "initial":initial,"allocated_after":allocated,"packed_after":packed,
        "limits":"Full in-process aptitude initializer including TODO schema and broker audit; shared OS cache; no hosted or process cold-start claim. No runtime pragma changes. VACUUM is fixture-only."});
    if let Ok(path) = std::env::var("DECAPOD_APTITUDE_REPORT") {
        use std::io::Write;
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap()
            .write_all(&serde_json::to_vec_pretty(&report).unwrap())
            .unwrap();
    }
    println!("{}", report);
}
