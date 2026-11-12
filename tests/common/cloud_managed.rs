//! Machine-private fixture seeding in an isolated child process. No test
//! changes the parent test runner's process-global credential environment.
#![allow(dead_code)]

use decapod::core::cloud_session::{Phase, Session, SessionState, SessionStore};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub const ACCESS: &str = "synthetic-bearer-secret";
pub const REFRESH: &str = "synthetic-refresh-secret";

pub fn seed_ready(
    project: &Path,
    machine: &Path,
    origin: &str,
    repository: &str,
    session: Session,
) {
    // The fixture can be included under a required boundary target. Rust's
    // test harness omits the crate prefix but retains nested module names.
    let module = module_path!()
        .split_once("::")
        .expect("private seed fixture is a test module")
        .1;
    let seed_test = format!("{module}::seed_private_session_fixture");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &seed_test, "--nocapture", "--test-threads=1"])
        .current_dir(project)
        .env("HOME", machine)
        .env("XDG_DATA_HOME", machine.join("data"))
        .env(
            "DECAPOD_MANAGED_TEST_SEED",
            serde_json::to_string(&session).unwrap(),
        )
        .env("DECAPOD_MANAGED_TEST_ORIGIN", origin)
        .env("DECAPOD_MANAGED_TEST_REPOSITORY", repository)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private fixture seeding failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn seed_private_session_fixture() {
    let Ok(encoded) = std::env::var("DECAPOD_MANAGED_TEST_SEED") else {
        return;
    };
    let store = SessionStore::for_project(
        &std::env::var("DECAPOD_MANAGED_TEST_ORIGIN").unwrap(),
        &std::env::var("DECAPOD_MANAGED_TEST_REPOSITORY").unwrap(),
        &std::env::current_dir().unwrap(),
    )
    .unwrap();
    store
        .lock(Duration::from_secs(1))
        .unwrap()
        .replace(SessionState {
            phase: Phase::Ready,
            session: Some(serde_json::from_str(&encoded).unwrap()),
            bootstrap: None,
        })
        .unwrap();
}
