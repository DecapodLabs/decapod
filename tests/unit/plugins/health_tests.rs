// Moved from src/decapod/plugins/health.rs
use super::*;
use crate::core::store::StoreKind;
use tempfile::tempdir;

#[test]
fn summary_observes_canonical_watcher_event_without_jsonl() {
    let dir = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    let ts = crate::core::time::now_epoch_z();
    events::append(
        &store.root,
        events::WATCHER,
        &serde_json::json!({
            "event_id": "watcher-health",
            "ts": ts,
            "event_type": "watcher.run",
            "actor": "watcher"
        }),
    )
    .unwrap();

    let summary = get_summary(&store).unwrap();
    assert_eq!(summary.watcher_last_run.as_deref(), Some(ts.as_str()));
    assert!(!summary.watcher_stale);
    assert!(!dir.path().join("watcher.events.jsonl").exists());
}
