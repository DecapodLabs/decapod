// Moved from src/decapod/core/todo.rs
#[test]
fn claim_container_error_summary_hides_preflight_dump() {
    let summary = super::summarize_claim_container_error(
        "Validation error: AUTOREMEDIABLE_VALIDATION_ERROR code=container_runtime_preflight_failed\nstderr:\nvery long host-specific output",
    );

    assert_eq!(
        summary,
        "Container runtime preflight failed. Check Docker/Podman availability and permissions."
    );
    assert!(!summary.contains("AUTOREMEDIABLE"));
    assert!(!summary.contains("stderr"));
}

#[test]
fn completion_dependency_gate_rechecks_proof_readiness() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    super::initialize_todo_db(&root).unwrap();
    let add = |title: &str, depends_on: &str| {
        super::add_task(
            &root,
            &super::TodoCommand::Add {
                title: title.to_string(),
                description: String::new(),
                priority: "medium".to_string(),
                tags: "houseboat,dependency".to_string(),
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: String::new(),
                dir: Some(root.to_string_lossy().to_string()),
                depends_on: depends_on.to_string(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let dependency_id = add("Completion dependency unique", "");
    let target_id = add("Completion target unique", &dependency_id);

    let blocked = super::dependency_completion_gate(&root, &target_id)
        .unwrap()
        .expect("unfinished dependency must block completion");
    assert_eq!(
        blocked["result"]["dependency_readiness"]["state"],
        "waiting"
    );

    let db = root.join(crate::core::schemas::LOCAL_DB_NAME);
    let conn = rusqlite::Connection::open(db).unwrap();
    conn.execute(
        "UPDATE tasks SET status = 'done' WHERE id = ?1",
        [&dependency_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, last_verified_at, last_verified_status,
            last_verified_notes, verification_policy_days, updated_at
         ) VALUES(?1, '[]', '100Z', 'passed', '', 90, '100Z')",
        [&dependency_id],
    )
    .unwrap();
    assert!(
        super::dependency_completion_gate(&root, &target_id)
            .unwrap()
            .is_none()
    );
}
