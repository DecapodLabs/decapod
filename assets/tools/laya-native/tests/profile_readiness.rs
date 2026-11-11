//! Exposed development diagnostics for choosing a supported profile. Never an
//! untouched accuracy holdout or permission to publish. Input comes from the
//! executed production projector, not another model-input builder.
use decapod_laya_infer::{
    engine::{Engine, Frontend},
    laya_assets,
    protocol::*,
};
use serde_json::{Value, json};

use std::{path::PathBuf, time::Instant};
mod support;

#[test]
#[ignore = "requires externally pinned model files and production-projected development inputs"]
fn supported_profile_on_unchanged_production_inputs()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let profile = support::profile();
    let directory = PathBuf::from(std::env::var("DECAPOD_LAYA_REFERENCE_MODEL")?);
    profile.verify(&directory)?;
    let raw = std::fs::read(std::env::var("DECAPOD_LAYA_DIAGNOSTIC_INPUTS")?)?;
    let inputs: Value = serde_json::from_slice(&raw)?;
    assert_eq!(inputs["schema"], "decapod.laya-development-inputs.v1");
    let frontend = Frontend::open(&directory)?;
    let backend = if std::env::var("DECAPOD_LAYA_DIAGNOSTIC_BACKEND").as_deref() == Ok("metal") {
        Backend::Metal
    } else {
        Backend::Cpu
    };
    let start = Instant::now();
    let engine = Engine::open(&directory, backend)?;
    eprintln!(
        "profile={:?} backend={backend:?} load_ms={}",
        std::env::var("DECAPOD_LAYA_REFERENCE_PROFILE"),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let mut results = Vec::new();
    for row in inputs["states"].as_array().unwrap() {
        let state = row["state"].as_str().unwrap();
        assert_eq!(
            laya_assets::hash(state.as_bytes()),
            row["state_sha256"].as_str().unwrap()
        );
        let request = Request {
            schema: WORKER_SCHEMA.into(),
            rubric: RUBRIC.into(),
            request_id: "diagnostic".into(),
            input_digest: laya_assets::hash(state.as_bytes()),
            manifest_digest: "profile-diagnostic-not-registration".into(),
            installation: directory.clone(),
            backend,
            state: state.into(),
        };
        let result = frontend
            .prepare(state)
            .and_then(|prepared| engine.predict(&prepared));
        let mut response = Response {
            schema: WORKER_SCHEMA.into(),
            rubric: RUBRIC.into(),
            request_id: request.request_id.clone(),
            input_digest: request.input_digest.clone(),
            manifest_digest: request.manifest_digest.clone(),
            publication_ready: false,
            judgment: None,
            error_kind: None,
            error: None,
            metrics: None,
        };
        match result {
            Ok(answer) => response.judgment = Some(answer),
            Err(error) => {
                response.error_kind = Some(ErrorKind::InferenceFailed);
                response.error = Some(error.to_string());
            }
        }
        let decision = response.decision(&request, profile.max_len);
        eprintln!(
            "{}",
            json!({"case":row["case"],"response":response,"decision":format!("{decision:?}")})
        );
        results.push((row["case"].as_str().unwrap().to_owned(), decision));
    }
    for (case, decision) in results {
        if case == "ready" {
            assert_eq!(decision, Ok(Decision::Accept));
        } else {
            assert!(
                matches!(decision, Ok(Decision::Reject | Decision::Abstain)),
                "{case}: {decision:?}"
            );
        }
    }
    Ok(())
}
