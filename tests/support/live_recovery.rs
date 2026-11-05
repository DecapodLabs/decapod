//! Opt-in acceptance against an explicitly supplied, governed live fixture.
//! No provider, validation snapshot, executable, or successful receipt is mocked.

use decapod::core::bounded_process::BoundedCommand;
use decapod::core::db::{OptionalExtension, db_connect_for_validate};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

type HistoricalTasks = BTreeMap<String, (Value, PathBuf, BTreeMap<String, Vec<String>>)>;

fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
    command.current_dir(root).args(args);
    // An outer CI job must not accidentally turn this into CI-local proof, or
    // exempt tasks from the final unscoped validation. Do not set bypass flags.
    for name in [
        "GITHUB_ACTIONS",
        "BUILDKITE",
        "GITHUB_EVENT_NAME",
        "GITHUB_WORKSPACE",
        "BUILDKITE_BUILD_CHECKOUT_PATH",
        "CHANGE_EVENT",
        "CHANGE_BASE",
        "CHANGE_HEAD",
        "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT",
        "DECAPOD_VALIDATE_SKIP_GIT_GATES",
        "DECAPOD_VERIFYING_TODO",
    ] {
        command.env_remove(name);
    }
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    command(root, args)
        .bounded_output(Duration::from_secs(3600))
        .unwrap_or_else(|error| panic!("real CLI {args:?} did not finish: {error}"))
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("CLI did not return JSON: {error}; {output:?}"))
}

fn task(root: &Path, id: &str) -> (Value, PathBuf) {
    let output = run(root, &["todo", "get", "--id", id, "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    let value = json(&output);
    (
        value["item"].clone(),
        PathBuf::from(value["root"].as_str().expect("local fixture store")),
    )
}

// Read-only observation, never fixture construction or repair. Keep the complete
// raw event fields so rewritten timestamps, actors and payloads cannot pass.
fn history(store: &Path, id: &str) -> BTreeMap<String, Vec<String>> {
    let db = db_connect_for_validate(store.join("decapod.db").to_str().unwrap()).unwrap();
    let mut query = db
        .prepare("SELECT event_id, ts, CAST(seq AS TEXT), stream, COALESCE(subject_kind, ''), COALESCE(subject_id, ''), event_type, payload, actor FROM events WHERE subject_id = ?1 ORDER BY stream, seq")
        .unwrap();
    query
        .query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (1..9)
                    .map(|index| row.get::<_, String>(index))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn assert_preserved(root: &Path, before: &HistoricalTasks) {
    for (id, (original, store, events)) in before {
        assert_eq!(
            task(root, id).0,
            *original,
            "recovery changed task {id}, including its completion timestamp, revision, custody or comments"
        );
        let after = history(store, id);
        for (event_id, event) in events {
            assert_eq!(
                after.get(event_id),
                Some(event),
                "historical event {event_id} changed or disappeared"
            );
        }
        for (event_id, event) in &after {
            if !events.contains_key(event_id) {
                assert!(
                    (event[5].starts_with("task.verify.") && event[5] != "task.verify.prune")
                        || event[5] == "task.journal.checkpoint",
                    "recovery added a lifecycle event: {event:?}"
                );
            }
        }
    }
}

#[test]
fn live_recovery_children_cannot_inherit_scoped_or_ci_local_validation() {
    let child = command(Path::new("fixture"), &["validate", "--format", "json"]);
    for name in [
        "GITHUB_ACTIONS",
        "BUILDKITE",
        "DECAPOD_VERIFYING_TODO",
        "DECAPOD_VALIDATE_SKIP_GIT_GATES",
    ] {
        assert_eq!(
            child
                .get_envs()
                .find(|(key, _)| *key == std::ffi::OsStr::new(name))
                .and_then(|(_, value)| value),
            None
        );
        assert!(
            child
                .get_envs()
                .any(|(key, value)| key == name && value.is_none())
        );
    }
}

#[test]
#[ignore = "requires an explicitly authorized, already governed live fixture and real proof access; see docs/verification-recovery-acceptance.md"]
fn live_recovery_preserves_multiple_historical_completions() {
    recovery_acceptance(true);
}

#[test]
#[ignore = "requires an authorized live fixture left pending by a real failed recovery; see docs/verification-recovery-acceptance.md"]
fn live_recovery_retries_pending_batch_without_rewriting_history() {
    recovery_acceptance(false);
}

fn recovery_acceptance(require_missing_artifacts: bool) {
    let root = PathBuf::from(
        std::env::var_os("DECAPOD_RECOVERY_FIXTURE")
            .expect("set DECAPOD_RECOVERY_FIXTURE to the isolated governed fixture"),
    );
    let inputs: BTreeMap<String, Vec<String>> = serde_json::from_str(
        &std::env::var("DECAPOD_RECOVERY_ARTIFACTS")
            .expect("set DECAPOD_RECOVERY_ARTIFACTS to a JSON task-to-current-files map"),
    )
    .unwrap();
    assert!(
        inputs.len() >= 2,
        "the regression requires multiple historical tasks"
    );
    let mut before = BTreeMap::new();
    let mut args = vec![
        "qa".to_owned(),
        "verify".into(),
        "recover".into(),
        "--json".into(),
    ];
    for (id, paths) in &inputs {
        assert!(
            !paths.is_empty(),
            "each task requires relevant current evidence"
        );
        let (original, store) = task(&root, id);
        assert_eq!(original["status"], "done");
        assert!(
            original["completed_at"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        let events = history(&store, id);
        assert!(
            events.values().any(|event| event[5] == "task.done"),
            "fixture must have historical completion events"
        );
        {
            let db = db_connect_for_validate(store.join("decapod.db").to_str().unwrap()).unwrap();
            let artifact: Option<Option<String>> = db
                .query_row(
                    "SELECT verification_artifacts FROM task_verification WHERE todo_id = ?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()
                .unwrap();
            let missing = artifact
                .flatten()
                .is_none_or(|value| value.trim().is_empty() || value.trim() == "{}");
            if require_missing_artifacts {
                assert!(missing, "fixture task {id} already has evidence");
            } else {
                assert!(
                    !missing,
                    "retry acceptance requires a real captured pending baseline"
                );
                let status: String = db
                    .query_row(
                        "SELECT last_verified_status FROM task_verification WHERE todo_id = ?1",
                        [id],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(status, "CLAIMED", "retry fixture must remain unverified");
                assert!(
                    events
                        .values()
                        .any(|event| event[5] == "task.verify.recover"),
                    "retry fixture must retain evidence of a real recovery attempt"
                );
            }
            let done: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE status = 'done'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                done as usize,
                inputs.len(),
                "recover operates on every done task; explicitly account for the complete fixture batch"
            );
        }
        for path in paths {
            let path = Path::new(path);
            assert!(
                !path.is_absolute()
                    && path
                        .components()
                        .all(|part| matches!(part, std::path::Component::Normal(_))),
                "artifact paths must be fixture-relative"
            );
            assert!(root.join(path).is_file(), "missing current artifact");
            args.extend(["--artifact".into(), format!("{id}={}", path.display())]);
        }
        before.insert(id.clone(), (original, store, events));
    }
    let blocked = run(&root, &["validate", "--format", "json"]);
    assert!(
        !blocked.status.success(),
        "pending historical proof unexpectedly passed"
    );
    assert!(
        String::from_utf8_lossy(&blocked.stdout).contains("PROOF_HOOK_FAILED")
            || String::from_utf8_lossy(&blocked.stderr).contains("PROOF_HOOK_FAILED"),
        "fixture did not reach the expected historical-proof gate: {blocked:?}"
    );
    let recovered = run(&root, &args.iter().map(String::as_str).collect::<Vec<_>>());
    // Check preservation even when another real prerequisite blocks recovery.
    assert_preserved(&root, &before);
    assert!(
        recovered.status.success(),
        "live acceptance NOT established; repair real fixture prerequisites, never weaken gates: {recovered:?}"
    );
    assert_eq!(json(&recovered)["recovered"], inputs.len());
    for id in inputs.keys() {
        let verified = run(&root, &["qa", "verify", "todo", id, "--json"]);
        assert!(verified.status.success(), "{verified:?}");
        assert_eq!(json(&verified)["summary"]["passed"], 1, "{verified:?}");
        assert_eq!(json(&verified)["summary"]["unknown"], 0);
        assert_eq!(json(&verified)["summary"]["failed"], 0);
    }
    let final_validation = run(&root, &["validate", "--format", "json"]);
    assert_preserved(&root, &before);
    assert!(
        final_validation.status.success(),
        "unscoped validation failed: {final_validation:?}"
    );
    let report = json(&final_validation);
    assert_eq!(report["publication_ready"], true, "{report}");
    assert_eq!(report["readiness"]["ready"], true);
    assert_eq!(report["readiness"]["proof_ready"], true);
    assert_ne!(report["scope"], "ci_local");
}
