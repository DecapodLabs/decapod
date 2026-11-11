//! Exercised unsupported CUDA -> fresh CPU IPC. No mock device or affirmative
//! oracle; this test verifies retry custody, not readiness-domain judgment.
use decapod_laya_infer::{
    laya_assets,
    process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    protocol::*,
};
use serde_json::Value;
use std::{fs::OpenOptions, io::Write, path::PathBuf, process::Command, time::Duration};

fn invoke(
    request: &Request,
    path: &std::path::Path,
) -> Result<Response, Box<dyn std::error::Error>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec(request)?)?;
    file.sync_all()?;
    let capture = Command::new(env!("CARGO_BIN_EXE_decapod-laya-infer"))
        .env_clear()
        .arg("--request")
        .arg(path)
        .bounded_capture(
            ProcessLimits {
                timeout: Duration::from_secs(120),
                max_output_bytes: 16384,
            },
            &Cancellation::default(),
        )?;
    assert_eq!(capture.termination, Termination::Exited);
    assert!(
        capture.output.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.output.stderr)
    );
    Ok(serde_json::from_slice(&capture.output.stdout)?)
}

#[cfg(not(feature = "cuda"))]
#[test]
#[ignore = "requires pinned external model and production-projected development input"]
fn unsupported_cuda_retries_actual_cpu_without_forging_a_verdict()
-> Result<(), Box<dyn std::error::Error>> {
    let raw = std::fs::read(std::env::var("DECAPOD_LAYA_DIAGNOSTIC_INPUTS")?)?;
    let cases: Value = serde_json::from_slice(&raw)?;
    let state = cases["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case"] == "ready")
        .unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut request = Request {
        schema: WORKER_SCHEMA.into(),
        rubric: RUBRIC.into(),
        request_id: laya_assets::hash(b"component-cuda-attempt"),
        input_digest: laya_assets::hash(state.as_bytes()),
        manifest_digest: laya_assets::manifest_digest(&laya_assets::baseline())?,
        installation: PathBuf::from(std::env::var("DECAPOD_LAYA_REFERENCE_MODEL")?)
            .canonicalize()?,
        backend: Backend::Cuda,
        state,
    };
    let directory = tempfile::tempdir()?;
    let directory = directory.path().canonicalize()?;
    let unsupported = invoke(&request, &directory.join("gpu.json"))?;
    assert!(unsupported.judgment.is_none());
    assert!(unsupported.permits_cpu_retry(&request, true));
    assert!(!unsupported.permits_cpu_retry(&request, false));
    assert!(unsupported.decision(&request, 1024).is_err());
    request.backend = Backend::Cpu;
    request.request_id = laya_assets::hash(b"component-fresh-cpu-attempt");
    let cpu = invoke(&request, &directory.join("cpu.json"))?;
    assert!(cpu.judgment.is_some());
    cpu.validate_binding(&request)?;
    assert_eq!(cpu.judgment.as_ref().unwrap().backend, Backend::Cpu);
    assert!(!cpu.permits_cpu_retry(&request, true));
    eprintln!(
        "{}",
        serde_json::json!({"scope":"component fallback, not publication proof","unsupported":unsupported,"cpu":cpu})
    );
    Ok(())
}
