// Moved from src/decapod/core/flight_recorder.rs
use super::*;
use crate::core::store::StoreKind;
use tempfile::tempdir;

#[test]
fn timeline_observes_watcher_event_from_canonical_store_only() {
    let dir = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    events::append(
        &store.root,
        events::WATCHER,
        &serde_json::json!({
            "event_id": "watcher-canonical",
            "ts": "1785620000Z",
            "event_type": "watcher.run",
            "actor": "watcher"
        }),
    )
    .unwrap();

    let (timeline, sources, gaps) = read_timeline_events(&store, 10);
    assert!(gaps.is_empty(), "unexpected canonical read gaps: {gaps:?}");
    assert!(sources.contains(&"watcher".to_string()));
    assert!(
        timeline
            .iter()
            .any(|event| event.event_id == "watcher-canonical")
    );
    assert!(!dir.path().join("watcher.events.jsonl").exists());
}
