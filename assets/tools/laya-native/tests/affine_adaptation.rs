//! One bounded fitting experiment. Evaluation inputs are a separate file and
//! are never opened by this test. No Python, remote fitting or full-model update.
use decapod_laya_infer::{
    affine::{self, FeatureRow, Features},
    engine::{Engine, Frontend},
    laya_assets,
    protocol::{Backend, RUBRIC},
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::Instant,
};
mod support;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn write_new(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

#[test]
#[ignore = "explicit recovery of fitting diagnostics; never opens evaluation inputs"]
fn recover_fitting_diagnostics_from_captured_features() -> Result<()> {
    let features: Features =
        serde_json::from_slice(&fs::read(std::env::var("DECAPOD_LAYA_CAPTURED_FEATURES")?)?)?;
    let readout = affine::fit(&features).map_err(std::io::Error::other)?;
    write_new(
        Path::new(&std::env::var("DECAPOD_LAYA_RECOVERED_READOUT")?),
        &readout,
    )?;
    eprintln!(
        "parameters={} rows={} errors={} initial_loss={} fitted_loss={} max_abs_weight={} validation={:?}",
        readout.weights.len(),
        readout.fitting_rows,
        readout.training_errors,
        readout.initial_loss,
        readout.fitted_loss,
        readout
            .weights
            .iter()
            .map(|x| x.abs())
            .fold(0.0_f32, f32::max),
        readout.validate(
            &features.base_weights_sha256,
            features.original_weights.len()
        )
    );
    readout
        .validate(
            &features.base_weights_sha256,
            features.original_weights.len(),
        )
        .map_err(std::io::Error::other)?;
    assert_eq!(readout.training_errors, 0);
    Ok(())
}

#[test]
#[ignore = "report already-fixed fitting artifacts only; not fitting, evaluation or qualification"]
fn report_rejected_fitting_artifact() -> Result<()> {
    let raw = fs::read(std::env::var("DECAPOD_LAYA_CAPTURED_FEATURES")?)?;
    let artifact = fs::read(std::env::var("DECAPOD_LAYA_RECOVERED_READOUT")?)?;
    let features: Features = serde_json::from_slice(&raw)?;
    let readout: affine::Readout = serde_json::from_slice(&artifact)?;
    assert_eq!(features.partition, "fit");
    assert_eq!(readout.fit_inputs_sha256, features.inputs_sha256);
    let rows = features
        .rows
        .iter()
        .map(|row| {
            let margin = |weights: &[f32]| -> std::result::Result<f64, String> {
                Ok((affine::reference_logit(&row.true_features, weights, 0.0)?
                    - affine::reference_logit(&row.false_features, weights, 0.0)?)
                    / row.temperature)
            };
            let original = margin(&features.original_weights).map_err(std::io::Error::other)?;
            let fitted = margin(&readout.weights).map_err(std::io::Error::other)?;
            Ok(
                json!({"id":row.id,"family":row.family,"expected_ready":row.expected_ready,
            "input_tokens":row.input_tokens,"state_sha256":row.state_sha256,
            "original_true_probability":1.0/(1.0+(-original).exp()),
            "fitted_true_probability":1.0/(1.0+(-fitted).exp()),
            "fitted_classification_matches":(fitted>0.0)==row.expected_ready}),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let report = json!({"schema":"decapod.laya-rejected-fit-diagnostic.v1",
        "publication_ready":false,"fitting_only":true,"evaluation_accessed":false,
        "features_sha256":laya_assets::hash(&raw),"readout_sha256":laya_assets::hash(&artifact),
        "parameters":readout.weights.len(),"iterations":readout.iterations,
        "regularization":readout.regularization,"initial_loss":readout.initial_loss,
        "fitted_loss":readout.fitted_loss,"training_errors":readout.training_errors,
        "profile_structurally_valid":readout.validate(&features.base_weights_sha256,features.original_weights.len()).is_ok(),
        "rows":rows});
    write_new(
        Path::new(&std::env::var("DECAPOD_LAYA_FIT_DIAGNOSIS")?),
        &report,
    )?;
    eprintln!("{report}");
    Ok(())
}

#[test]
#[ignore = "explicit local fitting stage; requires frozen fitting corpus and pinned original model"]
fn fit_only_final_affine_on_frozen_training_partition() -> Result<()> {
    assert_eq!(
        std::env::var("DECAPOD_LAYA_REFERENCE_PROFILE")?,
        "multilingual"
    );
    let directory =
        std::path::PathBuf::from(std::env::var("DECAPOD_LAYA_REFERENCE_MODEL")?).canonicalize()?;
    let output = std::path::PathBuf::from(std::env::var("DECAPOD_LAYA_FIT_OUTPUT")?);
    if output.exists() {
        return Err(std::io::Error::other("refuse to overwrite an existing fitting run").into());
    }
    fs::create_dir(&output)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
    }
    let output = output.canonicalize()?;
    // The fitting function receives only this fitting file. The frozen
    // evaluation definition is hashed for custody; no evaluation states/scores
    // enter extraction, normalization, optimization or stopping.
    let raw = fs::read(std::env::var("DECAPOD_LAYA_FIT_INPUTS")?)?;
    assert!(raw.len() <= 2 * 1024 * 1024);
    let inputs: Value = serde_json::from_slice(&raw)?;
    assert_eq!(inputs["schema"], "decapod.laya-affine-inputs.v1");
    assert_eq!(inputs["partition"], "fit");
    let expected_definition = laya_assets::hash(include_bytes!(
        "../../../../tests/fixtures/laya-readiness-fit-corpus.json"
    ));
    assert_eq!(inputs["definition_sha256"], expected_definition);
    assert_eq!(inputs["cases"].as_array().unwrap().len(), 32);
    support::profile().verify(&directory)?;
    let frontend = Frontend::open(&directory)?;
    // Validate every fitting input before loading or fitting the scorer. An
    // unsupported row is an error, never dropped from an inconvenient study.
    let mut prepared = Vec::new();
    for case in inputs["cases"].as_array().unwrap() {
        let state = case["state"].as_str().unwrap();
        assert_eq!(case["state_sha256"], laya_assets::hash(state.as_bytes()));
        prepared.push(frontend.prepare(state)?);
    }
    let start = Instant::now();
    let engine = Engine::open_native(&directory, Backend::Metal)?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut original_weights = None;
    let mut rows = Vec::new();
    let start = Instant::now();
    for (case, input) in inputs["cases"].as_array().unwrap().iter().zip(&prepared) {
        let capture = engine.capture_scorer(input)?;
        assert_eq!(capture.backend, Backend::Metal);
        if let Some(weights) = &original_weights {
            assert_eq!(weights, &capture.original_weights);
        } else {
            original_weights = Some(capture.original_weights.clone());
        }
        for (features, actual) in [
            (&capture.false_features, capture.logits[0]),
            (&capture.true_features, capture.logits[1]),
        ] {
            let reference =
                affine::reference_logit(features, &capture.original_weights, capture.original_bias)
                    .map_err(std::io::Error::other)?;
            assert!(
                (actual as f64 - reference).abs() <= 1e-3 + 1e-4 * reference.abs(),
                "independent affine arithmetic mismatch before fitting"
            );
        }
        rows.push(FeatureRow {
            id: case["id"].as_str().unwrap().into(),
            family: case["family"].as_str().unwrap().into(),
            expected_ready: case["expected_ready"].as_bool().unwrap(),
            state_sha256: case["state_sha256"].as_str().unwrap().into(),
            input_tokens: capture.input_tokens,
            false_features: capture.false_features,
            true_features: capture.true_features,
            temperature: capture.temperature,
        });
    }
    let extraction_ms = start.elapsed().as_secs_f64() * 1000.0;
    let features = Features {
        schema: "decapod.laya-affine-features.v1".into(),
        partition: "fit".into(),
        base_weights_sha256: format!("sha256:{}", support::profile().weights_sha256),
        rubric: RUBRIC.into(),
        inputs_sha256: laya_assets::hash(&raw),
        definition_sha256: expected_definition,
        frozen_evaluation_definition_sha256: laya_assets::hash(include_bytes!(
            "../../../../tests/fixtures/laya-readiness-evaluation-corpus.json"
        )),
        cases_code_sha256: inputs["code_sha256"].as_str().unwrap().into(),
        feature_extractor_sha256: laya_assets::hash(include_bytes!("../src/engine.rs")),
        model_code_sha256: laya_assets::hash(include_bytes!("../src/lib.rs")),
        original_weights: original_weights.unwrap(),
        rows,
    };
    write_new(&output.join("fitting-features.json"), &features)?;
    let start = Instant::now();
    let readout = affine::fit(&features).map_err(std::io::Error::other)?;
    let fitting_ms = start.elapsed().as_secs_f64() * 1000.0;
    // Preserve the computed artifact even when its qualification fails.
    write_new(&output.join("readiness-affine.json"), &readout)?;
    readout
        .validate(
            &features.base_weights_sha256,
            features.original_weights.len(),
        )
        .map_err(std::io::Error::other)?;
    let artifact = fs::read(output.join("readiness-affine.json"))?;
    write_new(
        &output.join("fitting-report.json"),
        &json!({"schema":"decapod.laya-affine-fitting-report.v1","publication_ready":false,"scope":"fitting only; evaluation has not run","model_load_ms":load_ms,"synchronized_feature_extraction_ms":extraction_ms,"fitting_ms":fitting_ms,"parameters":readout.weights.len(),"training_rows":readout.fitting_rows,"training_errors":readout.training_errors,"initial_loss":readout.initial_loss,"fitted_loss":readout.fitted_loss,"artifact_sha256":laya_assets::hash(&artifact),"fitting_families":readout.fitting_families,"evaluation_accessed":false}),
    )?;
    support::profile().verify(&directory)?;
    eprintln!(
        "parameters={} rows={} errors={} initial_loss={} fitted_loss={} extraction_ms={} fitting_ms={} artifact={}",
        readout.weights.len(),
        readout.fitting_rows,
        readout.training_errors,
        readout.initial_loss,
        readout.fitted_loss,
        extraction_ms,
        fitting_ms,
        laya_assets::hash(&artifact)
    );
    assert_eq!(
        readout.training_errors, 0,
        "bounded fitting did not satisfy its training contracts; do not open evaluation to tune it"
    );
    Ok(())
}
