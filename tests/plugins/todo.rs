use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::plugins::todo::{
    TodoCommand, add_task, get_task, initialize_todo_db, list_tasks, rebuild_from_events,
    todo_db_path, update_status,
};
use serde_json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_todo_lifecycle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // 1. Add task
    let add_args = TodoCommand::Add {
        title: "Test task".to_string(),
        tags: "tag1".to_string(),
        owner: "arx".to_string(),
        due: None,
        r#ref: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
    };
    let res = add_task(&root, &add_args).unwrap();
    let task_id = res.get("id").unwrap().as_str().unwrap();
    assert!(task_id.contains("_"));

    // 2. Get task
    let task = get_task(&root, task_id).unwrap().expect("Task not found");
    assert_eq!(task.title, "Test task");
    assert_eq!(task.status, "open");

    // 3. Mark done
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    update_status(&store, task_id, "done", "task.done", serde_json::json!({})).unwrap();
    let task = get_task(&root, task_id).unwrap().unwrap();
    assert_eq!(task.status, "done");

    // 4. List tasks
    let tasks = list_tasks(&root, Some("done".to_string()), None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
}

#[test]
fn test_todo_rebuild() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Add some tasks
    for i in 0..3 {
        let add_args = TodoCommand::Add {
            title: format!("Task {}", i),
            tags: "".to_string(),
            owner: "".to_string(),
            due: None,
            r#ref: "".to_string(),
            dir: Some(tmp.path().to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
        };
        add_task(&root, &add_args).unwrap();
    }

    // Corrupt/Delete DB
    let db_path = todo_db_path(&root);
    fs::remove_file(&db_path).unwrap();

    // Rebuild
    rebuild_from_events(&root).unwrap();

    // Verify
    let tasks = list_tasks(&root, None, None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 3);
}
