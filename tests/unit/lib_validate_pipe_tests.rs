//! Regression tests for #1526: the validation child-process supervisor must
//! drain stdout/stderr concurrently so worker output larger than pipe capacity
//! cannot deadlock, and the timeout path must kill + reap the child and join
//! the reader threads before returning.
//!
//! The child process under test is this same test binary, re-invoked with the
//! `validate_pipe_child_process` test name and a `DECAPOD_TEST_PIPE_CHILD`
//! mode env var. No production code path changes: the supervisor helper is
//! the exact code `run_validation_in_child_process` uses.

use super::*;
use std::io::Write;
use std::process::{Command, Stdio};

const PIPE_NOISE_BYTES: usize = 1 << 20; // 1 MiB, unambiguously above common 64 KiB pipe capacity

/// Child-process entry point: when re-invoked by the tests below with
/// `DECAPOD_TEST_PIPE_CHILD` set, emit controlled output instead of running
/// the real test body.
#[test]
fn validate_pipe_child_process() {
    let mode = match std::env::var("DECAPOD_TEST_PIPE_CHILD") {
        Ok(mode) => mode,
        Err(_) => return, // normal `cargo test` invocation: nothing to do
    };
    match mode.as_str() {
        "noise" => {
            // >1 MiB on stderr (raw bytes; stderr is diagnostic-only).
            let mut stderr = std::io::stderr().lock();
            let chunk = vec![b'x'; 64 * 1024];
            for _ in 0..(PIPE_NOISE_BYTES / chunk.len()) {
                stderr.write_all(&chunk).unwrap();
            }
            stderr.flush().unwrap();
            // >1 MiB on stdout while remaining a single valid JSON document:
            // the report carries a large padding field, and decode must still
            // succeed with the real report intact.
            let pad = "A".repeat(PIPE_NOISE_BYTES);
            let report = serde_json::json!({
                "status": "ok",
                "report": {
                    "status": "ok",
                    "validation_epoch": {
                        "schema_version": "1.0.0",
                        "epoch_id": "epoch-pipe-test",
                        "evaluator_identity": "pipe-test",
                        "evaluator_set_hash": "pipe-test",
                        "constitution_version": "1.0.0",
                        "constitution_hash": "pipe-test",
                        "validation_profile": "pipe-test",
                        "validation_profile_hash": "pipe-test",
                        "proof_rubric": "pipe-test",
                        "proof_rubric_hash": "pipe-test",
                        "generated_specs_manifest_hash": "pipe-test",
                        "generated_specs_fingerprint": "pipe-test",
                        "material_hashes": {}
                    },
                    "elapsed_ms": 1,
                    "pass_count": 1,
                    "fail_count": 0,
                    "warn_count": 0,
                    "skip_count": 0,
                    "note_count": 0,
                    "advisory_count": 0,
                    "failures": [],
                    "warnings": [],
                    "drift_findings": [],
                    "temporary_artifacts_cleaned": 0,
                    "gate_timings": [],
                    "parallelism": 1,
                    "ci_prediction": {
                        "result": "pass",
                        "confidence": "high",
                        "reasons": [],
                        "recommendations": []
                    }
                },
                "test_padding": pad
            });
            // Write stdout explicitly through a locked handle and flush
            // before exiting: no println!, no implicit buffering, no
            // process-exit flush dependence.
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(report.to_string().as_bytes()).unwrap();
            stdout.write_all(b"\n").unwrap();
            stdout.flush().unwrap();
        }
        "hang" => {
            // Emit >1 MiB on both streams so the pipe-reader paths are active,
            // then block forever; only the parent's timeout may end this.
            let chunk = vec![b'y'; 64 * 1024];
            let mut stdout = std::io::stdout().lock();
            for _ in 0..(PIPE_NOISE_BYTES / chunk.len()) {
                stdout.write_all(&chunk).unwrap();
            }
            stdout.flush().unwrap();
            let mut stderr = std::io::stderr().lock();
            for _ in 0..(PIPE_NOISE_BYTES / chunk.len()) {
                stderr.write_all(&chunk).unwrap();
            }
            stderr.flush().unwrap();
            loop {
                thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        other => panic!("unknown DECAPOD_TEST_PIPE_CHILD mode: {other}"),
    }
    // Bypass the test harness's own stdout chatter and exit code so the
    // parent sees exactly the bytes this child mode emits.
    std::process::exit(0);
}

fn spawn_pipe_child(mode: &str) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--quiet", "--nocapture", "validate_pipe_child_process"])
        .env("DECAPOD_TEST_PIPE_CHILD", mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pipe child")
}

/// More than 1 MiB on both stdout and stderr through the validation child-process
/// supervisor must complete (not deadlock) and the full worker report must
/// still decode. Against the pre-#1526 implementation the child blocks on a
/// full pipe and never exits, so this test would hit the timeout instead.
#[test]
fn validate_child_large_output_drains_and_decodes() {
    let child = spawn_pipe_child("noise");
    let started = std::time::Instant::now();
    let output = supervise_piped_child(child, std::time::Duration::from_secs(60))
        .expect("supervisor must not fail on large output");
    let PipedChildOutput::Exited { stdout, stderr } = output else {
        panic!("large-output child must exit normally, not time out");
    };
    assert!(
        stdout.len() >= PIPE_NOISE_BYTES,
        "stdout must carry >1 MiB, got {}",
        stdout.len()
    );
    assert!(
        stderr.len() >= PIPE_NOISE_BYTES,
        "stderr must carry >1 MiB, got {}",
        stderr.len()
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "supervisor must complete without hitting the timeout"
    );
    // The complete worker report must still decode correctly after the
    // large payload. The re-invoked test binary's libtest harness prints a
    // short banner ("\n running 1 test\n") on stdout before the test body;
    // strip only those harness-generated bytes here in the test fixture —
    // production decoding semantics stay unchanged.
    let json_start = stdout
        .iter()
        .position(|&byte| byte == b'{')
        .expect("child stdout must contain the JSON report");
    let report = decode_validation_worker_output(&stdout[json_start..], &stderr)
        .expect("worker report must decode after large output");
    assert_eq!(report.status, "ok");
    assert_eq!(report.fail_count, 0);
    assert_eq!(report.validation_epoch.epoch_id, "epoch-pipe-test");
}

/// A worker that stays alive past the timeout must be killed and reaped, the
/// reader threads joined, and the supervisor must return the timeout outcome
/// without hanging.
#[test]
fn validate_child_timeout_kills_reaps_and_joins_readers() {
    let child = spawn_pipe_child("hang");
    let pid = child.id();
    let started = std::time::Instant::now();
    let output = supervise_piped_child(child, std::time::Duration::from_secs(1))
        .expect("supervisor must return, not propagate a spawn/poll error");
    assert!(
        matches!(output, PipedChildOutput::TimedOut),
        "hanging child must produce the timeout outcome"
    );
    // The timeout is the proof mechanism; the supervisor must return shortly
    // after it rather than hanging on the child or reader threads.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "supervisor hung after timeout: {:?}",
        started.elapsed()
    );
    // The child was reaped: signalling its pid must report no such process.
    #[cfg(unix)]
    {
        let rc = unsafe { libc::kill(pid as i32, 0) };
        assert_eq!(rc, -1, "child process {pid} survived the timeout path");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH),
            "child {pid} must be reaped (ESRCH), got {:?}",
            std::io::Error::last_os_error()
        );
    }
}
