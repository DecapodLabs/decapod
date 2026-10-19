//! One-shot offline native Rust vega worker. Normal stdout is one typed response.
//! `--probe` emits a small standalone capability report without model files.
//! `--proof-output` is an explicit PRIVATE diagnostic export: token IDs can
//! reveal the input. It is never a parity/readiness acceptance certificate.
use decapod::core::{
    vega_prompt,
    vega_readiness::{self, Request, Response},
    vega_setup,
};
#[cfg(all(not(target_os = "macos"), feature = "cuda"))]
extern crate candle_vllm_cuda as candle_vllm;
#[path = "vega-native/decoder.rs"]
mod decoder;
#[path = "vega-native/evaluation_corpus.rs"]
mod evaluation_corpus;
#[path = "vega-native/metal_recurrence_reference.rs"]
mod metal_recurrence_reference;
#[path = "vega-native/readiness_evaluation.rs"]
mod readiness_evaluation;
#[path = "vega-native/reference_backbone.rs"]
mod reference_backbone;
#[path = "vega-native/reference_engine.rs"]
mod reference_engine;
#[path = "vega-native/reference_tokenizer.rs"]
mod reference_tokenizer;
#[path = "vega-native/scientific_collection.rs"]
mod scientific_collection;
#[path = "../../src/decapod/core/vega_backbone.rs"]
mod vega_backbone;
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

const MAX_PROOF_BYTES: usize = 1024 * 1024;

/// Serialize within the byte cap before creating the destination. In
/// particular, never leave a truncated JSON file after exceeding the limit.
fn bounded_proof_bytes(proof: &impl serde::Serialize) -> io::Result<Vec<u8>> {
    struct Buffer(Vec<u8>);
    impl io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_PROOF_BYTES.saturating_sub(self.0.len()) {
                return Err(io::Error::other("private proof exceeds 1 MiB byte limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer(Vec::new());
    serde_json::to_writer(&mut buffer, proof).map_err(io::Error::other)?;
    Ok(buffer.0)
}

#[cfg(unix)]
fn write_private_proof(path: &Path, proof: &impl serde::Serialize) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let bytes = bounded_proof_bytes(proof)?;
    let mut options = fs::OpenOptions::new();
    // O_CREAT|O_EXCL rejects an existing destination atomically, including a
    // dangling symlink. Never follow or overwrite a destination symlink.
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "proof destination is not a private regular file",
        ));
    }
    file.write_all(&bytes)?;
    file.sync_all()
}
#[cfg(not(unix))]
fn write_private_proof(_: &Path, _: &impl serde::Serialize) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "private diagnostic export needs a reviewed platform file-permission implementation",
    ))
}
fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > limit {
        return Err(io::Error::other("not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("file grew beyond limit"));
    }
    Ok(bytes)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("vega worker: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 3 && args[1] == "--proof-output" {
        if args[0] == "--collect-scientific-numerics" {
            return collect_scientific_numerics(Path::new(&args[2]));
        }
        if args[0] == "--verify-scientific-collection" {
            return verify_scientific_collection(Path::new(&args[2]));
        }
    }
    if args.len() == 5
        && args[0] == "--collect-readiness-shard"
        && args[1] == "--index"
        && args[3] == "--proof-output"
    {
        let index = args[2]
            .to_str()
            .ok_or("invalid shard index")?
            .parse::<usize>()?;
        return collect_readiness_shard(index, Path::new(&args[4]));
    }
    if args.len() == 7
        && args[0] == "--evaluate-readiness"
        && args[1] == "--split"
        && args[3] == "--backend"
        && args[5] == "--proof-output"
    {
        return evaluate_readiness(
            args[2].to_str().ok_or("invalid split")?,
            args[4].to_str().ok_or("invalid evaluation backend")?,
            Path::new(&args[6]),
        );
    }
    if args.len() == 3 && args[0] == "--prepare-evaluation" && args[1] == "--proof-output" {
        return prepare_evaluation(Path::new(&args[2]));
    }
    if args.len() == 3 && args[0] == "--diagnose-native" && args[1] == "--proof-output" {
        return diagnose_native(Path::new(&args[2]));
    }
    if args.len() == 3 && args[0] == "--collect-native-verification" && args[1] == "--proof-output"
    {
        return collect_native_verification(Path::new(&args[2]));
    }
    if args.len() == 2 && args[0] == "--backend-check" {
        use vega_readiness::{BACKEND_CHECK_SCHEMA, Backend, BackendCheck, WorkerErrorKind};
        let backend = match args[1].to_str() {
            Some("metal") => Backend::Metal,
            Some("cuda") => Backend::Cuda,
            _ => return Err("backend check requires metal or cuda".into()),
        };
        let (available, error_kind, error) = match vega_backbone::probe_backend(backend) {
            Ok(_) => (true, None, None),
            Err(error) => {
                let kind = if matches!(error, vega_backbone::BackboneError::HardwareUnavailable(_))
                {
                    WorkerErrorKind::HardwareUnavailable
                } else {
                    WorkerErrorKind::InferenceFailed
                };
                (
                    false,
                    Some(kind),
                    Some(error.to_string().chars().take(512).collect()),
                )
            }
        };
        println!(
            "{}",
            serde_json::to_string(&BackendCheck {
                schema: BACKEND_CHECK_SCHEMA.into(),
                backend,
                available,
                error_kind,
                error,
            })?
        );
        return Ok(());
    }
    if matches!(args.len(), 1 | 3)
        && (args[0] == "--probe" || args[0] == "--probe-auto")
        && (args.len() == 1 || args[1] == "--proof-output")
    {
        let proof = if args[0] == "--probe" {
            serde_json::to_value(vega_backbone::probe_metal()?)?
        } else {
            let preferred = vega_readiness::BackendPreference::Auto.resolve();
            let (probe, fallback) = match vega_backbone::probe_backend(preferred) {
                Ok(probe) => (probe, None),
                Err(vega_backbone::BackboneError::HardwareUnavailable(reason)) => {
                    (vega_backbone::probe_cpu()?, Some(reason))
                }
                Err(error) => return Err(error.into()),
            };
            serde_json::json!({
                "schema":"decapod.vega-auto-probe.v1",
                "preferred_backend":preferred,
                "cpu_fallback_reason":fallback,
                "probe":probe,
                "model_inference":"not_executed",
                "parity":"unverified"
            })
        };
        if args.len() == 3 {
            write_private_proof(Path::new(&args[2]), &proof)?;
        }
        println!("{}", serde_json::to_string(&proof)?);
        return Ok(());
    }
    if !matches!(args.len(), 2 | 4)
        || args[0] != "--request"
        || (args.len() == 4 && args[2] != "--proof-output")
    {
        return Err("usage: decapod-vega-infer --probe | --probe-auto | --backend-check <metal|cuda> | --request <private-json> [--proof-output <new-private-json> (PRIVATE, maximum 1 MiB)]".into());
    }
    let request: Request = serde_json::from_slice(&read_bounded(Path::new(&args[1]), 64 * 1024)?)?;
    if request.schema != vega_readiness::WORKER_SCHEMA
        || request.input_digest != vega_readiness::hash(request.state.as_bytes())
        || request.state.len() > 32 * 1024
        || request.request_id.len() > 128
    {
        return Err("invalid request binding".into());
    }
    let proof_path = if args.len() == 4 {
        Some(Path::new(&args[3]))
    } else {
        None
    };
    let response = inference_response(&request, proof_path);
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}

fn inference_response(request: &Request, proof_path: Option<&Path>) -> Response {
    let mut response = Response {
        schema: vega_readiness::WORKER_SCHEMA.into(),
        request_id: request.request_id.clone(),
        input_digest: request.input_digest.clone(),
        manifest_digest: request.manifest_digest.clone(),
        backend: request.backend.runtime().into(),
        device: request.backend.device().into(),
        engine_device: "cpu".into(),
        precision: vega_readiness::Precision::supported(),
        input_tokens: 0,
        outcome: None,
        error: None,
        error_kind: None,
    };
    match infer(request, proof_path) {
        Ok((outcome, tokens)) => {
            response.outcome = Some(outcome);
            response.input_tokens = tokens;
        }
        Err(error) => {
            response.error_kind = Some(
                if matches!(
                    error.downcast_ref::<vega_backbone::BackboneError>(),
                    Some(vega_backbone::BackboneError::HardwareUnavailable(_))
                ) {
                    vega_readiness::WorkerErrorKind::HardwareUnavailable
                } else {
                    vega_readiness::WorkerErrorKind::InferenceFailed
                },
            );
            response.error = Some(error.to_string().chars().take(512).collect());
        }
    }
    response
}

fn evaluation_probe(
    backend: vega_readiness::Backend,
) -> Result<vega_backbone::MetalProbe, vega_backbone::BackboneError> {
    if backend == vega_readiness::Backend::Cpu {
        vega_backbone::probe_cpu()
    } else {
        vega_backbone::probe_backend(backend)
    }
}

/// Execute independent scientific checks before deciding aggregate success.
/// The kernel still owns the original hard worker deadline and final verdict.
fn collect_verification_steps(
    mut step: impl FnMut(&str) -> Result<(), Box<dyn std::error::Error>>,
) -> Vec<serde_json::Value> {
    [
        "native_diagnostics",
        "native_chunk_diagnostics",
        "holdout_auto",
        "holdout_cpu",
    ]
    .into_iter()
    .map(|name| match step(name) {
        Ok(()) => serde_json::json!({"name":name,"status":"passed"}),
        Err(error) => {
            serde_json::json!({"name":name,"status":"failed","error":error.to_string()})
        }
    })
    .collect()
}

fn collect_native_verification(proof_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut outputs = serde_json::Map::new();
    let steps = collect_verification_steps(|name| {
        let path = proof_path.with_extension(format!("{name}.json"));
        let result = match name {
            "native_diagnostics" => diagnose_native(&path),
            "native_chunk_diagnostics" => diagnose_native_state(&path,
                "Required coverage item: preserve deterministic checks and report explicitly missing evidence. ".repeat(16), true),
            "holdout_auto" => evaluate_readiness("holdout", "auto", &path),
            "holdout_cpu" => evaluate_readiness("holdout", "cpu", &path),
            _ => unreachable!(),
        };
        let receipt = match read_bounded(&path, MAX_PROOF_BYTES as u64) {
            Ok(bytes) => {
                let _: serde_json::Value = serde_json::from_slice(&bytes)?;
                serde_json::json!({"file":path,"sha256":vega_readiness::hash(&bytes),"bytes":bytes.len()})
            }
            Err(error) => serde_json::json!({"file":path,"unavailable":error.to_string()}),
        };
        let missing = receipt.get("unavailable").is_some();
        outputs.insert(name.into(), receipt);
        result?;
        if missing {
            return Err("required collection receipt missing".into());
        }
        Ok(())
    });
    let failed = steps.iter().any(|step| step["status"] != "passed");
    write_private_proof(
        proof_path,
        &serde_json::json!({
            "schema":"decapod.vega-verification-collection.v1","publication_ready":false,
            "required_steps":steps,"outputs":outputs,"all_required_steps_passed":!failed,
            "wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
            "scope":"Independent collection; no failed phase is waived. Kernel hard deadline unchanged; scientific acceptance failure remains nonzero."
        }),
    )?;
    if failed {
        return Err(
            "one or more required native verification phases failed; inspect independent receipts"
                .into(),
        );
    }
    Ok(())
}

const SCIENTIFIC_BASELINE: &str =
    "sha256:2b985edf9a5641fabe12b54f5f94bdff2ca888f19551593b8d9f87cd088cf9fc";
const SCIENTIFIC_CORPUS: &str =
    "sha256:09df504b0d50004499172c1cebf9fbf414ed91d72a703ebd0b0a634e3a7e3e88";
const NUMERICAL_PHASES: [&str; 3] = [
    "native_diagnostics",
    "native_chunk_diagnostics",
    "native_limit_diagnostics",
];
const COVERAGE_SENTENCE: &str = "Required coverage item: preserve deterministic checks and report explicitly missing evidence. ";
const SHORT_NUMERICAL_STATE: &str = r#"{"intent":"Document a CLI option","validation":"passed","obligations":[{"requirement":"Describe CPU fallback explicitly","evidence":"missing"}],"unknowns":[]}"#;

/// Frozen before long-model observations: start with306literal sentences and
/// append at most32 literal " x" suffixes. Only the independent tokenizer may
/// select the suffix count, to reach exactly4096 without truncation. No model
/// output or candidate tokenizer participates in fixture selection.
fn independent_limit_state(
    reference: &reference_tokenizer::ReferenceTokenizer,
) -> Result<String, Box<dyn std::error::Error>> {
    let base = COVERAGE_SENTENCE.repeat(306);
    for suffix in 0..=32 {
        let state = format!("{base}{}", " x".repeat(suffix));
        let prompt = reference.prompt(&state, vega_prompt::QUESTION, None)?;
        if prompt["tokens"]
            .as_array()
            .ok_or("missing independent limit tokens")?
            .len()
            == 4096
        {
            return Ok(state);
        }
    }
    Err("frozen independent limit fixture did not reach4096; do not replace it after model observations".into())
}
fn long_numerical_state() -> Result<String, Box<dyn std::error::Error>> {
    let setup = vega_setup::run(&vega_setup::SetupOptions::default())?;
    if setup.manifest_digest.as_deref() != Some(SCIENTIFIC_BASELINE) {
        return Err("scientific model pin changed".into());
    }
    let installation = setup
        .model_directory
        .ok_or("missing verified installation")?;
    long_numerical_state_from(&installation)
}
fn long_numerical_state_from(installation: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = read_bounded(
        &installation.join("backbone/tokenizer.json"),
        16 * 1024 * 1024,
    )?;
    let config = read_bounded(
        &installation.join("backbone/tokenizer_config.json"),
        1024 * 1024,
    )?;
    if vega_readiness::hash(&bytes)
        != "sha256:5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42"
        || vega_readiness::hash(&config)
            != "sha256:49e2b6e395f959f077f1e992b338919c0d4a9732fc6e613995e06557f843500c"
        || !serde_json::from_slice::<serde_json::Value>(&config)?["bos_token"].is_null()
    {
        return Err("independent limit tokenizer/config pin differs".into());
    }
    let reference = reference_tokenizer::ReferenceTokenizer::new(&serde_json::from_slice(&bytes)?)?;
    independent_limit_state(&reference)
}
fn numeric_child_passes(name: &str, report: &serde_json::Value) -> bool {
    if report["schema"] != "decapod.vega-native-diagnostics.v1"
        || report["probe"]["backend"] != "candle-metal"
        || report["probe"]["status"] != "passed"
        || report["probe"]["synchronized"] != true
        || report["probe"]["bit_exact"] != true
        || report["probe"]["actual"].as_array().map(Vec::len) != Some(8)
        || report["probe"]["actual"] != report["probe"]["expected"]
        || report["probe"]["attention_metal_gdn_source_sha256"]
            != vega_backbone::METAL_GDN_SOURCE_SHA256
        || report["metal_recurrence_reference"]["status"] != "passed_scalar_recurrence"
        || report["metal_recurrence_reference"]["repetitions"] != 32
        || report["metal_recurrence_reference"]["all_repetitions_required"] != true
        || report["metal_recurrence_reference"]["synchronized"] != true
        || report["metal_recurrence_reference"]["metal_gdn_source_sha256"]
            != vega_backbone::METAL_GDN_SOURCE_SHA256
        || report["separate_tokenizer_reference"]["status"] != "passed"
        || report["separate_backbone_reference"]["backend_comparisons"] != 2
        || report["required_chunk_evidence_missing"] != false
    {
        return false;
    }
    let Some(attempts) = report["attempts"].as_array() else {
        return false;
    };
    if attempts.len() != 2 {
        return false;
    }
    for (attempt, backend) in attempts.iter().zip(["candle-metal", "candle-cpu"]) {
        if attempt["backend"] != backend
            || attempt["manifest_digest"] != SCIENTIFIC_BASELINE
            || attempt["separate_backbone_reference"]["status"] != "passed_backbone_subset"
            || attempt["combined_reference_engine"]["status"] != "passed_engine_only"
            || attempt["separate_engine_reference"]["status"] != "passed_engine_only"
        {
            return false;
        }
        if name == "native_limit_diagnostics"
            && (report["separate_backbone_reference"]["input_tokens"] != 4096
                || attempt["prompt_spans"]["last"] != 4095
                || attempt["chunk_mask"]
                    .as_array()
                    .is_none_or(|mask| mask.len() != 16 || mask.iter().any(|v| v != true)))
        {
            return false;
        }
    }
    true
}
fn collect_scientific_numerics(proof: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use scientific_collection::{Collection, Phase};
    let context = Collection::create(proof)?;
    let started = std::time::Instant::now();
    let mut steps = Vec::new();
    let mut outputs = serde_json::Map::new();
    for name in NUMERICAL_PHASES {
        let path = proof.with_extension(format!("{name}.json"));
        let result = match name {
            "native_diagnostics" => diagnose_native(&path),
            "native_chunk_diagnostics" => {
                diagnose_native_state(&path, COVERAGE_SENTENCE.repeat(16), true)
            }
            "native_limit_diagnostics" => {
                long_numerical_state().and_then(|state| diagnose_native_state(&path, state, true))
            }
            _ => unreachable!(),
        };
        let reference = context.reference(&path);
        let checked = reference
            .as_ref()
            .ok()
            .and_then(|r| context.read_reference(r).ok())
            .is_some_and(|report| numeric_child_passes(name, &report));
        let passed = result.is_ok() && checked;
        steps.push(serde_json::json!({"name":name,"passed":passed,"error":result.err().map(|e|e.to_string())}));
        outputs.insert(
            name.into(),
            match reference {
                Ok(value) => value,
                Err(error) => serde_json::json!({"unavailable":error.to_string()}),
            },
        );
    }
    let passed = steps.iter().all(|step| step["passed"] == true);
    context.publish(Phase::Numeric,proof,passed,serde_json::json!({
        "status":"collected_acceptance_pending","publication_ready":false,
        "required_steps":steps,"outputs":outputs,"wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
        "scope":"Numerical collection only; failures preserved for required final aggregate, not passing scientific acceptance."
    }))
}
fn collect_readiness_shard(index: usize, proof: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use scientific_collection::{Collection, Phase, SHARDS};
    if index >= SHARDS {
        return Err("invalid frozen shard index".into());
    }
    let context = Collection::load(proof)?;
    let started = std::time::Instant::now();
    // One absolute cutoff covers preparation and both backend phases. The
    // kernel retains1800s/command and7200s/whole validation, with no resets.
    let deadline = started + std::time::Duration::from_secs(1600);
    let mut reports = serde_json::Map::new();
    for backend in ["auto", "cpu"] {
        let path = proof.with_extension(format!("{backend}.json"));
        let result = evaluate_readiness_selected(
            "holdout",
            backend,
            &path,
            Some(readiness_evaluation::Shard {
                index,
                count: SHARDS,
            }),
            Some(deadline),
        );
        let reference = context.reference(&path);
        let receipt = reference
            .as_ref()
            .ok()
            .and_then(|r| context.read_reference(r).ok());
        reports.insert(
            backend.into(),
            serde_json::json!({
                "collection_completed":result.is_ok() && receipt.is_some(),
                "error":result.err().map(|e|e.to_string()),
                "receipt":reference.ok()
            }),
        );
    }
    context.publish(Phase::Shard(index),proof,false,serde_json::json!({
        "status":"collected_acceptance_pending","publication_ready":false,
        "shard":{"index":index,"count":SHARDS},"reports":reports,
        "absolute_soft_dispatch_seconds":1600,"wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
        "scope":"Both backend collections attempted; no per-shard readiness assertion or acceptance waiver."
    }))
}
fn check_evaluation_binding(
    report: &serde_json::Value,
    runtime: &str,
    backend: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if report["manifest_digest"] != SCIENTIFIC_BASELINE
        || report["runtime_sha256"] != runtime
        || report["corpus_sha256"] != SCIENTIFIC_CORPUS
        || report["split"] != "holdout"
        || report["fitting_performed"] != false
        || report["upstream_calibration_modified"] != false
        || report["collection_only"] != true
    {
        return Err("foreign or changed scientific evaluation identity".into());
    }
    let selected = if backend == "cpu" { "cpu" } else { "metal" };
    if report["selected_backend"] != selected {
        return Err("required evaluation backend differs".into());
    }
    for record in report["records"]
        .as_array()
        .ok_or("missing evaluation records")?
    {
        if record["execution"] == "executed" {
            let evidence = &record["execution_evidence"];
            if evidence["manifest_digest"] != SCIENTIFIC_BASELINE
                || evidence["runtime_sha256"] != runtime
                || evidence["backend"]
                    != if backend == "cpu" {
                        "candle-cpu"
                    } else {
                        "candle-metal"
                    }
                || evidence["input_tokens"]
                    .as_u64()
                    .is_none_or(|tokens| tokens == 0 || tokens > 4096)
            {
                return Err("recorded model execution binding differs".into());
            }
        }
    }
    Ok(())
}
fn recheck_numeric_exports(
    context: &scientific_collection::Collection,
    name: &str,
    child: &serde_json::Value,
    reference: &reference_tokenizer::ReferenceTokenizer,
    configuration: &serde_json::Value,
    weights: &[u8],
) -> Result<bool, Box<dyn std::error::Error>> {
    let state = match name {
        "native_diagnostics" => SHORT_NUMERICAL_STATE.to_owned(),
        "native_chunk_diagnostics" => COVERAGE_SENTENCE.repeat(16),
        "native_limit_diagnostics" => independent_limit_state(reference)?,
        _ => return Err("unknown numerical fixture".into()),
    };
    let prompt = reference.prompt(&state, vega_prompt::QUESTION, None)?;
    let expected = context.read_export(&serde_json::json!({
        "file":child["separate_backbone_reference"]["expected_file"],
        "sha256":child["separate_backbone_reference"]["expected_sha256"],
        "max_bytes":child["separate_backbone_reference"]["maximum_export_bytes"]
    }))?;
    if expected["input_tokens"]
        != prompt["tokens"]
            .as_array()
            .ok_or("missing independent tokens")?
            .len()
    {
        return Err("scalar reference input length differs".into());
    }
    let mut passed = true;
    for attempt in child["attempts"]
        .as_array()
        .ok_or("missing numerical attempts")?
    {
        let raw = context.read_export(&attempt["private_numeric_proof"])?;
        if raw["backend"] != attempt["backend"]
            || raw["manifest_digest"] != SCIENTIFIC_BASELINE
            || raw["input_digest"] != vega_readiness::hash(state.as_bytes())
            || raw["prompt_token_ids"] != prompt["tokens"]
            || raw["prompt_spans"] != prompt["spans"]
        {
            return Err("numeric export model/input/backend binding differs".into());
        }
        for (field, count) in [
            ("pooled_feats", 6144),
            ("pooled_opts", 2048),
            ("pooled_chunks", 16384),
            ("sampled_hidden", 8192),
        ] {
            if raw[field].as_array().map(Vec::len) != Some(count)
                || expected[field].as_array().map(Vec::len) != Some(count)
            {
                return Err("numeric export scalar count differs".into());
            }
        }
        passed &=
            reference_backbone::compare(&expected, &raw)?["status"] == "passed_backbone_subset";
        passed &= reference_engine::compare(configuration, weights, &raw)?["status"]
            == "passed_engine_only";
        passed &= reference_engine::compare_with_inputs(configuration, weights, &raw, &expected)?["status"]
            == "passed_engine_only";
    }
    Ok(passed)
}
fn verify_scientific_collection(proof: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let context = scientific_collection::Collection::load(proof)?;
    let diagnostics = context.diagnostics();
    let verified = verify_scientific_contents(&context, proof);
    let (mut report, failure) = match verified {
        Ok(report) => {
            let failure = (report["all_required_scientific_conditions_passed"] != true)
                .then(|| "required scientific acceptance unsatisfied; inspect numerical results and frozen risk/coverage/label limits".to_owned());
            (report, failure)
        }
        Err(error) => (
            serde_json::json!({
                "schema":"decapod.vega-scientific-aggregate.v1","publication_ready":false,
                "all_required_scientific_conditions_passed":false,
                "strict_verification_error":error.to_string()
            }),
            Some(error.to_string()),
        ),
    };
    report["available_phase_diagnostics"] = diagnostics;
    write_private_proof(proof, &report)?;
    if let Some(error) = failure {
        return Err(error.into());
    }
    Ok(())
}
fn verify_scientific_contents(
    context: &scientific_collection::Collection,
    proof: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use scientific_collection::{Phase, SHARDS};
    let numeric = context.read(Phase::Numeric)?;
    let setup = vega_setup::run(&vega_setup::SetupOptions::default())?;
    if setup.manifest_digest.as_deref() != Some(SCIENTIFIC_BASELINE) {
        return Err("final scientific model pin differs".into());
    }
    let installation = setup
        .model_directory
        .ok_or("missing final verified installation")?;
    let tokenizer_bytes = read_bounded(
        &installation.join("backbone/tokenizer.json"),
        16 * 1024 * 1024,
    )?;
    if vega_readiness::hash(&tokenizer_bytes)
        != "sha256:5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42"
    {
        return Err("final independent tokenizer pin differs".into());
    }
    let reference =
        reference_tokenizer::ReferenceTokenizer::new(&serde_json::from_slice(&tokenizer_bytes)?)?;
    let configuration: serde_json::Value = serde_json::from_slice(&read_bounded(
        &installation.join("vega_config.json"),
        1024 * 1024,
    )?)?;
    let weights = read_bounded(&installation.join("engine.safetensors"), 128 * 1024 * 1024)?;
    let mut numeric_passed = numeric.scientific_checks_passed;
    for name in NUMERICAL_PHASES {
        let child = context.read_reference(&numeric.report["outputs"][name])?;
        numeric_passed &= numeric_child_passes(name, &child);
        numeric_passed &=
            recheck_numeric_exports(context, name, &child, &reference, &configuration, &weights)?;
    }
    let inputs = proof.with_extension("evaluation-inputs.json");
    prepare_evaluation(&inputs)?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&read_bounded(&inputs, MAX_PROOF_BYTES as u64)?)?;
    let bytes = read_bounded(&inputs.with_extension("corpus.json"), 8 * 1024 * 1024)?;
    if receipt["corpus_sha256"] != SCIENTIFIC_CORPUS
        || vega_readiness::hash(&bytes) != SCIENTIFIC_CORPUS
    {
        return Err("final frozen corpus identity differs".into());
    }
    let corpus: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut backends = serde_json::Map::new();
    for backend in ["auto", "cpu"] {
        let mut shards = Vec::new();
        for index in 0..SHARDS {
            let phase = context.read(Phase::Shard(index))?;
            if phase.report["shard"] != serde_json::json!({"index":index,"count":SHARDS})
                || phase.report["reports"][backend]["collection_completed"] != true
            {
                return Err("incomplete or mismatched backend collection".into());
            }
            let report = context.read_reference(&phase.report["reports"][backend]["receipt"])?;
            check_evaluation_binding(&report, &context.runtime_sha256, backend)?;
            shards.push(report);
        }
        backends.insert(
            backend.into(),
            readiness_evaluation::aggregate(&corpus, "holdout", &shards)?,
        );
    }
    let accepted = numeric_passed
        && backends.values().all(|report| {
            report["frozen_numeric_criterion_met"] == true
                && report["production_calibration"] == "verified"
        });
    Ok(serde_json::json!({
        "schema":"decapod.vega-scientific-aggregate.v1","publication_ready":false,
        "numeric_checks_passed":numeric_passed,"readiness_evaluation":backends,
        "runtime_sha256":context.runtime_sha256,"manifest_digest":SCIENTIFIC_BASELINE,
        "corpus_sha256":SCIENTIFIC_CORPUS,"all_required_scientific_conditions_passed":accepted,
        "scope":"Strict current-run aggregate; diagnostic collection never waives numerical, coverage, risk or label-authority conditions. Kernel retains final validation authority."
    }))
}

fn evaluate_readiness(
    split: &str,
    backend: &str,
    proof_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    evaluate_readiness_selected(split, backend, proof_path, None, None)
}
fn evaluate_readiness_selected(
    split: &str,
    backend: &str,
    proof_path: &Path,
    shard: Option<readiness_evaluation::Shard>,
    deadline: Option<std::time::Instant>,
) -> Result<(), Box<dyn std::error::Error>> {
    use vega_readiness::{Backend, BackendPreference};
    if !matches!(split, "development" | "holdout") || !matches!(backend, "auto" | "cpu") {
        return Err("evaluation requires a frozen split and auto or explicit cpu backend".into());
    }
    let inputs_path = proof_path.with_extension("evaluation-inputs.json");
    prepare_evaluation(&inputs_path)?;
    let receipt: serde_json::Value =
        serde_json::from_slice(&read_bounded(&inputs_path, MAX_PROOF_BYTES as u64)?)?;
    let corpus_path = inputs_path.with_extension("corpus.json");
    let bytes = read_bounded(&corpus_path, 8 * 1024 * 1024)?;
    if receipt["corpus_sha256"] != vega_readiness::hash(&bytes) {
        return Err("frozen evaluation input digest changed".into());
    }
    let corpus: serde_json::Value = serde_json::from_slice(&bytes)?;
    let setup = vega_setup::run(&vega_setup::SetupOptions::default())?;
    let installation = setup
        .model_directory
        .ok_or("evaluation installation missing")?;
    let manifest = setup.manifest.ok_or("evaluation manifest missing")?;
    let manifest_digest = setup
        .manifest_digest
        .ok_or("evaluation manifest digest missing")?;
    if manifest.revision != vega_setup::BASELINE_REVISION
        || manifest.backbone_revision != vega_setup::BASELINE_BACKBONE_REVISION
        || manifest.source_revision != vega_setup::BASELINE_SOURCE_REVISION
    {
        return Err("evaluation cannot silently change the pinned baseline".into());
    }
    let preferred = if backend == "cpu" {
        Backend::Cpu
    } else {
        BackendPreference::Auto.resolve()
    };
    let selection_probe = evaluation_probe(preferred);
    let (selected, fallback) = match selection_probe {
        Ok(_) => (preferred, None),
        Err(vega_backbone::BackboneError::HardwareUnavailable(reason)) if backend == "auto" => {
            vega_backbone::probe_cpu()?;
            (Backend::Cpu, Some(reason))
        }
        Err(error) => return Err(error.into()),
    };
    let executable = std::env::current_exe()?;
    let runtime_sha256 = vega_readiness::hash(&read_bounded(&executable, 256 * 1024 * 1024)?);
    // Unsupported inputs need no model load. This immutable, hash-verified
    // tokenizer snapshot only rejects out-of-scope cases; every accepted input
    // still executes infer's complete before/after model-integrity checks.
    let pre_tokenizer = if shard.is_some() {
        Some(load_pinned_tokenizer(&installation, &manifest)?)
    } else {
        None
    };
    let mut ordinal = 0;
    let assess = |state: &str,
                  deadline: std::time::Instant|
     -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        if let Some((tokenizer, bos)) = &pre_tokenizer {
            if let Err(error) = tokenize(state, tokenizer, *bos) {
                if error == "vega_prompt_tokens_exceeded_no_truncation" {
                    return Err(readiness_evaluation::UnsupportedTokens(error).into());
                }
                return Err(io::Error::other(error).into());
            }
        }
        ordinal += 1;
        let request = Request {
            schema: vega_readiness::WORKER_SCHEMA.into(),
            request_id: format!("frozen-evaluation-{split}-{ordinal}"),
            input_digest: vega_readiness::hash(state.as_bytes()),
            installation: installation.clone(),
            manifest_digest: manifest_digest.clone(),
            state: state.to_owned(),
            alpha: "0.1".into(),
            risk: "0.1".into(),
            backend: selected,
        };
        let temporary = tempfile::tempdir()?;
        let case_proof = temporary.path().join("proof.json");
        if std::time::Instant::now() >= deadline {
            return Err("evaluation dispatch window expired before inference".into());
        }
        // Stay inside the kernel-owned worker process and its original hard
        // deadline. Nested independent process groups could escape a parent
        // SIGKILL on macOS. Every call loads fresh recurrent state; no new
        // per-case execution budget is granted. The 600s window stops new
        // dispatches, while the kernel enforces the hard whole-worker limit.
        let (outcome, tokens) = infer(&request, Some(&case_proof))?;
        let diagnostic: serde_json::Value =
            serde_json::from_slice(&read_bounded(&case_proof, MAX_PROOF_BYTES as u64)?)?;
        if diagnostic["request_id"] != request.request_id
            || diagnostic["input_digest"] != request.input_digest
            || diagnostic["manifest_digest"] != manifest_digest
            || diagnostic["outcome"] != serde_json::to_value(&outcome)?
        {
            return Err("evaluation proof/outcome binding differs".into());
        }
        Ok(serde_json::json!({"outcome":outcome,
            "execution_evidence":{"backend":selected.runtime(),"device":selected.device(),
                "precision":vega_readiness::Precision::supported(),"input_tokens":tokens,
                "manifest_digest":manifest_digest,"runtime_sha256":runtime_sha256,
                "wall_clock_ms":diagnostic["wall_clock_ms"],"peak_resident_bytes":diagnostic["peak_resident_bytes"],
                "memory_scope":diagnostic["memory_scope"]}}))
    };
    let mut report = if let Some(shard) = shard {
        readiness_evaluation::run_shard_until(
            &corpus,
            split,
            shard,
            deadline.ok_or("missing absolute shard deadline")?,
            assess,
        )?
    } else {
        readiness_evaluation::run(&corpus, split, assess)?
    };
    report["manifest"] = serde_json::to_value(manifest)?;
    report["manifest_digest"] = manifest_digest.into();
    report["runtime_sha256"] = runtime_sha256.into();
    report["corpus_sha256"] = receipt["corpus_sha256"].clone();
    report["corpus_schema"] = evaluation_corpus::CORPUS_SCHEMA.into();
    report["corpus_file"] = serde_json::to_value(corpus_path)?;
    report["selected_backend"] = serde_json::to_value(selected)?;
    report["evaluation_cpu_selection_reason"] = serde_json::to_value(fallback)?;
    report["cpu_fallback_scope"] = "This evaluation's selected backend; does not establish the separate production runtime-binding/fallback protocol".into();
    report["collection_only"] = shard.is_some().into();
    write_private_proof(proof_path, &report)?;
    if shard.is_some() {
        return Ok(());
    }
    if report["frozen_numeric_criterion_met"] != true
        || report["production_calibration"] != "verified"
    {
        return Err("readiness evaluation does not satisfy final acceptance; inspect measured coverage, risk and label-scope limits".into());
    }
    Ok(())
}

fn prepare_evaluation(proof_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use decapod::core::bounded_process::{
        BoundedCommand, Cancellation, ProcessLimits, Termination,
    };
    use std::io::Write;
    let directory = tempfile::tempdir()?;
    let dataset = directory.path().join("dataset.parquet");
    let capture = std::process::Command::new("/usr/bin/curl")
        .env_clear()
        .args([
            "--disable",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            "60",
            "--max-filesize",
            "1875846",
            "--output",
        ])
        .arg(&dataset)
        .arg(evaluation_corpus::DATASET_URL)
        .bounded_capture(
            ProcessLimits {
                timeout: std::time::Duration::from_secs(65),
                max_output_bytes: 16 * 1024,
            },
            &Cancellation::default(),
        )?;
    if capture.termination != Termination::Exited || !capture.output.status.success() {
        return Err("pinned public evaluation download failed".into());
    }
    let corpus = evaluation_corpus::build(&dataset)?;
    let bytes = serde_json::to_vec(&corpus)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("reviewable corpus exceeds separate 8 MiB limit".into());
    }
    let parent = proof_path.parent().ok_or("proof has no parent")?;
    let destination = proof_path.with_extension("corpus.json");
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&destination)?;
    write_private_proof(
        proof_path,
        &serde_json::json!({
            "schema":"decapod.vega-evaluation-input-receipt.v2","publication_ready":false,
            "dataset_revision":evaluation_corpus::DATASET_REVISION,"dataset_sha256":evaluation_corpus::DATASET_SHA256,
            "corpus_sha256":vega_readiness::hash(&bytes),"corpus_file":destination,"corpus_bytes":bytes.len(),
            "corpus_schema":evaluation_corpus::CORPUS_SCHEMA,
            "benchmark_units":corpus["benchmark_units"],"judgment_cases":corpus["judgment_cases"],
            "policy_cases_excluded_from_judgment":corpus["policy_cases_excluded_from_judgment"],
            "calibration_fit":"not_performed","evaluation":"not_executed","production_calibration":"unsatisfied",
            "label_author":"agent-authored adaptation of public benchmark oracles, not human-reviewed PR-readiness labels"
        }),
    )?;
    Ok(())
}

/// Collection, not acceptance: attempt each independent backend even when
/// another fails. Neither missing reference data nor a model's negative answer
/// prevents dispatch diagnostics. The caller still fails if execution failed.
fn diagnose_native(proof_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Fixed numerical diagnostic; not a labelled evaluation or passing proof.
    diagnose_native_state(proof_path, SHORT_NUMERICAL_STATE.into(), false)
}

fn diagnose_native_state(
    proof_path: &Path,
    state: String,
    require_chunks: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use vega_readiness::Backend;
    let started = std::time::Instant::now();
    let preferred = vega_readiness::BackendPreference::Auto.resolve();
    let probe = vega_backbone::probe_backend(preferred);
    let mut report = serde_json::json!({
        "schema":"decapod.vega-native-diagnostics.v1",
        "publication_ready":false,"parity":"unverified","readiness_calibration":"unverified",
        "probe":match &probe {
            Ok(value) => serde_json::to_value(value)?,
            Err(error) => serde_json::json!({"error":error.to_string()})
        },
        "scope":"Pinned model execution diagnostics only. CPU/accelerator agreement is not independent model parity. No fitted or held-out readiness coverage is established."
    });
    report["metal_recurrence_reference"] = match metal_recurrence_reference::run() {
        Ok(value) => value,
        Err(error) => serde_json::json!({"status":"failed","error":error.to_string()}),
    };
    let evaluation_path = proof_path.with_extension("evaluation.json");
    report["evaluation_input_collection"] = match prepare_evaluation(&evaluation_path) {
        Ok(()) => serde_json::from_slice(&read_bounded(&evaluation_path, MAX_PROOF_BYTES as u64)?)?,
        Err(error) => {
            serde_json::json!({"status":"failed","error":error.to_string(),"evaluation":"not_executed"})
        }
    };
    let setup = match vega_setup::run(&vega_setup::SetupOptions::default()) {
        Ok(setup) => setup,
        Err(error) => {
            report["installation_error"] = error.to_string().into();
            write_private_proof(proof_path, &report)?;
            return Err(error.into());
        }
    };
    let installation = setup.model_directory.ok_or("installation missing")?;
    let manifest_digest = setup.manifest_digest.ok_or("manifest missing")?;
    let manifest = setup.manifest.ok_or("manifest identity missing")?;
    if manifest.revision != vega_setup::BASELINE_REVISION
        || manifest.backbone_revision != vega_setup::BASELINE_BACKBONE_REVISION
        || manifest.source_revision != vega_setup::BASELINE_SOURCE_REVISION
    {
        return Err(
            "diagnostics require the compiled immutable baseline, never a floating activation"
                .into(),
        );
    }
    let (mut attempts, inference_failed) = collect_diagnostics(
        [preferred, Backend::Cpu],
        |backend| {
            let request = Request {
                schema: vega_readiness::WORKER_SCHEMA.into(),
                request_id: format!("native-diagnostic-{}", backend.device()),
                input_digest: vega_readiness::hash(state.as_bytes()),
                installation: installation.clone(),
                manifest_digest: manifest_digest.clone(),
                state: state.clone(),
                alpha: "0.1".into(),
                risk: "0.1".into(),
                backend,
            };
            let component_fallback = if backend == Backend::Cpu {
                // Exercise a genuinely unsupported alternate GPU API on this
                // host, not a mocked failure of the available preferred GPU.
                let unavailable_backend = if cfg!(target_os = "macos") {
                    Backend::Cuda
                } else {
                    Backend::Metal
                };
                let unavailable_request = Request {
                    request_id: format!("native-unavailable-{}", unavailable_backend.device()),
                    backend: unavailable_backend,
                    schema: request.schema.clone(),
                    input_digest: request.input_digest.clone(),
                    installation: request.installation.clone(),
                    manifest_digest: request.manifest_digest.clone(),
                    state: request.state.clone(),
                    alpha: request.alpha.clone(),
                    risk: request.risk.clone(),
                };
                let failure = inference_response(&unavailable_request, None);
                if !failure.permits_cpu_retry(unavailable_backend, true)
                    || failure.permits_cpu_retry(unavailable_backend, false)
                {
                    return Err("actual unsupported alternate GPU API did not satisfy the strict CPU retry classifier".into());
                }
                Some(serde_json::json!({"requested_backend":unavailable_backend,
                    "actual_failure_response":failure,"strict_cpu_retry_permitted":true,
                    "disabled_retry_rejected":true,
                    "scope":"Actual unsupported alternate GPU API followed by this fresh CPU inference using the production retry classifier. Does not simulate failure of available Metal or establish the kernel custody/loader-companion publication protocol."}))
            } else {
                None
            };
            let temporary = tempfile::tempdir()?;
            let path = temporary.path().join("inference.json");
            infer(&request, Some(&path))?;
            let mut value = serde_json::from_slice::<serde_json::Value>(&read_bounded(
                &path,
                MAX_PROOF_BYTES as u64,
            )?)?;
            value["component_cpu_fallback"] = serde_json::to_value(component_fallback)?;
            let configuration: serde_json::Value = serde_json::from_slice(&read_bounded(
                &installation.join("vega_config.json"),
                1024 * 1024,
            )?)?;
            // Reference outputs come from independent equations, not the candidate
            // engine's outputs. Pooled tensors are explicitly reference inputs.
            value["separate_engine_reference"] = match reference_engine::compare(
                &configuration,
                &read_bounded(&installation.join("engine.safetensors"), 128 * 1024 * 1024)?,
                &value,
            ) {
                Ok(comparison) => comparison,
                Err(error) => {
                    serde_json::json!({"status":"failed","error":error.to_string(),"backbone_parity":"unverified"})
                }
            };
            Ok(value)
        },
    );
    // Both backend execution attempts happen before the independent oracle.
    // Missing/unsupported reference inputs cannot suppress device diagnostics.
    report["separate_backbone_reference"] = match verify_backbone_reference(
        &installation,
        &state,
        &mut attempts,
        proof_path,
    ) {
        Ok(value) => value,
        Err(error) => {
            serde_json::json!({"status":"failed","error":error.to_string(),"backbone_parity":"unverified"})
        }
    };
    let missing_chunk_evidence = require_chunks
        && (attempts
            .iter()
            .filter(|attempt| !attempt["outcome"].is_null())
            .count()
            != 2
            || attempts.iter().any(|attempt| {
                !attempt["outcome"].is_null()
                    && !attempt["chunk_mask"].as_array().is_some_and(|mask| {
                        mask.iter().any(|active| active.as_bool() == Some(true))
                    })
            }));
    report["requires_nonempty_memory_chunks"] = require_chunks.into();
    report["required_chunk_evidence_missing"] = missing_chunk_evidence.into();
    let reference_failed = report["separate_backbone_reference"]["status"] == "failed"
        || attempts.iter().any(|attempt| {
            attempt["separate_engine_reference"]["status"] == "failed"
                || attempt["separate_backbone_reference"]["status"] == "failed"
                || attempt["combined_reference_engine"]["status"] == "failed"
        });
    // Each full numeric export retains the original1MiB cap. The combined
    // receipt binds their private paths/hashes instead of duplicating vectors
    // and accidentally increasing its own export limit.
    for attempt in &mut attempts {
        if attempt["outcome"].is_null() {
            continue;
        }
        let device = attempt["device"].as_str().ok_or("missing proof device")?;
        if !matches!(device, "metal" | "cuda" | "cpu") {
            return Err("invalid proof device".into());
        }
        let numeric_path = proof_path.with_extension(format!("{device}.json"));
        write_private_proof(&numeric_path, attempt)?;
        let digest = vega_readiness::hash(&read_bounded(&numeric_path, MAX_PROOF_BYTES as u64)?);
        let object = attempt.as_object_mut().ok_or("invalid numeric attempt")?;
        for key in [
            "pooled_feats",
            "pooled_opts",
            "pooled_chunks",
            "sampled_hidden",
            "prompt_token_ids",
        ] {
            object.remove(key);
        }
        object.insert(
            "private_numeric_proof".into(),
            serde_json::json!({"file":numeric_path,"sha256":digest,"max_bytes":MAX_PROOF_BYTES}),
        );
    }
    report["attempts"] = attempts.into();
    report["separate_tokenizer_reference"] = match verify_tokenizer_reference(&installation) {
        Ok(comparison) => comparison,
        Err(error) => serde_json::json!({"status":"failed","error":error.to_string()}),
    };
    report["wall_clock_ms"] = (started.elapsed().as_secs_f64() * 1000.0).into();
    report["cpu_execution_scope"] = "CPU model execution after a real unsupported alternate GPU API using the production retry classifier; available preferred-GPU failure and kernel custody/loader-companion routing remain separate obligations".into();
    write_private_proof(proof_path, &report)?;
    if probe.is_err()
        || missing_chunk_evidence
        || inference_failed
        || reference_failed
        || report["metal_recurrence_reference"]["status"] == "failed"
        || report["separate_tokenizer_reference"]["status"] == "failed"
    {
        return Err("one or more native diagnostics failed; inspect private proof".into());
    }
    Ok(())
}

fn verify_backbone_reference(
    installation: &Path,
    state: &str,
    attempts: &mut [serde_json::Value],
    proof_path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let tokenizer_bytes = read_bounded(
        &installation.join("backbone/tokenizer.json"),
        16 * 1024 * 1024,
    )?;
    if vega_readiness::hash(&tokenizer_bytes)
        != "sha256:5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42"
    {
        return Err("independent tokenizer artifact pin differs".into());
    }
    let tokenizer =
        reference_tokenizer::ReferenceTokenizer::new(&serde_json::from_slice(&tokenizer_bytes)?)?;
    let prompt = tokenizer.prompt(state, vega_prompt::QUESTION, None)?;
    let config = read_bounded(&installation.join("backbone/config.json"), 1024 * 1024)?;
    let engine_config = read_bounded(&installation.join("vega_config.json"), 1024 * 1024)?;
    let weights = read_bounded(
        &installation.join("backbone/model.safetensors-00001-of-00001.safetensors"),
        2 * 1024 * 1024 * 1024,
    )?;
    let expected = reference_backbone::compute(&config, &engine_config, &weights, &prompt)?;
    drop(weights);
    let reference_path = proof_path.with_extension("reference.json");
    write_private_proof(&reference_path, &expected)?;
    let reference_digest =
        vega_readiness::hash(&read_bounded(&reference_path, MAX_PROOF_BYTES as u64)?);
    let configuration: serde_json::Value = serde_json::from_slice(&engine_config)?;
    let engine_weights = read_bounded(&installation.join("engine.safetensors"), 128 * 1024 * 1024)?;
    let mut compared = 0;
    for attempt in attempts {
        if attempt["outcome"].is_null() {
            continue;
        }
        if attempt["prompt_token_ids"] != prompt["tokens"]
            || attempt["prompt_spans"] != prompt["spans"]
        {
            attempt["separate_backbone_reference"] = serde_json::json!({"status":"failed","error":"independent token/span input differs"});
            continue;
        }
        attempt["separate_backbone_reference"] =
            match reference_backbone::compare(&expected, attempt) {
                Ok(value) => value,
                Err(error) => serde_json::json!({"status":"failed","error":error.to_string()}),
            };
        attempt["combined_reference_engine"] = match reference_engine::compare_with_inputs(
            &configuration,
            &engine_weights,
            attempt,
            &expected,
        ) {
            Ok(value) => value,
            Err(error) => serde_json::json!({"status":"failed","error":error.to_string()}),
        };
        compared += 1;
    }
    Ok(
        serde_json::json!({"status":"reference_calculated","prompt_cases":1,"backend_comparisons":compared,
        "expected_file":reference_path,"expected_sha256":reference_digest,"maximum_export_bytes":MAX_PROOF_BYTES,
        "reference_spec_revision":expected["reference_spec_revision"],"reference_spec_sha256":expected["reference_spec_sha256"],
        "input_tokens":expected["input_tokens"],"max_reference_tokens":reference_backbone::MAX_REFERENCE_TOKENS,
        "wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
        "independent_human_review":false,"scope":expected["scope"],
        "limitations":"Same-agent independent mathematical implementation, shared JSON/SafeTensors/hash infrastructure and pinned weights; not empirical HF execution, independent human review or all-input/backbone equivalence. Only recorded fixed fixtures have execution evidence; streamed-state, vision/adapters and full production fallback routing remain unverified."}),
    )
}

fn verify_tokenizer_reference(
    installation: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let path = installation.join("backbone/tokenizer.json");
    let metadata: serde_json::Value = serde_json::from_slice(&read_bounded(
        &installation.join("backbone/tokenizer_config.json"),
        1024 * 1024,
    )?)?;
    if !metadata["bos_token"].is_null() {
        return Err("reference fixture expects the pinned absent BOS token".into());
    }
    let json = serde_json::from_slice(&read_bounded(&path, 16 * 1024 * 1024)?)?;
    let reference = reference_tokenizer::ReferenceTokenizer::new(&json)?;
    let actual =
        tokenizers::Tokenizer::from_file(path).map_err(|e| io::Error::other(e.to_string()))?;
    let fixtures = [
        "",
        "All required checks passed.",
        "Checks passed; explicit requirement remains missing.",
        "line one\n\nline two\t  ",
        "cafe\u{301} café 中文",
        "<|im_start|> quoted evidence <|im_end|>",
    ];
    for state in fixtures {
        let expected = reference.prompt(state, vega_prompt::QUESTION, None)?;
        let observed = tokenize(state, &actual, None).map_err(io::Error::other)?;
        if expected["tokens"] != serde_json::to_value(observed.tokens)?
            || expected["spans"] != serde_json::to_value(observed.spans)?
        {
            return Err(format!("independent token/span mismatch in fixture {:?}", state).into());
        }
    }
    let oversized = " a".repeat(4200);
    if reference
        .prompt(&oversized, vega_prompt::QUESTION, None)
        .is_ok()
        || tokenize(&oversized, &actual, None).is_ok()
    {
        return Err("oversized input was not rejected by both independent paths".into());
    }
    let limit = independent_limit_state(&reference)?;
    let expected = reference.prompt(&limit, vega_prompt::QUESTION, None)?;
    let observed = tokenize(&limit, &actual, None).map_err(io::Error::other)?;
    if expected["tokens"] != serde_json::to_value(observed.tokens)?
        || expected["spans"] != serde_json::to_value(observed.spans)?
        || expected["tokens"].as_array().map(Vec::len) != Some(4096)
    {
        return Err("independent4096token boundary differs".into());
    }
    let beyond = format!("{limit} x");
    if reference.encode(&beyond)?.len() != reference.encode(&limit)?.len() + 1
        || reference
            .prompt(&beyond, vega_prompt::QUESTION, None)
            .is_ok()
        || tokenize(&beyond, &actual, None).err().as_deref()
            != Some("vega_prompt_tokens_exceeded_no_truncation")
    {
        return Err("independent4097token boundary was not rejected without truncation".into());
    }
    Ok(
        serde_json::json!({"status":"passed","fixed_prompt_fixtures":fixtures.len(),"boundary_prompt_fixtures":1,"exact_boundary_tokens":4096,"oversized_rejections":2,
        "tolerance":"exact token IDs and spans","reference":"separate byte-level BPE; shared JSON, regex and Unicode infrastructure",
        "backbone_parity":"unverified","independent_human_review":false}),
    )
}

fn collect_diagnostics(
    backends: impl IntoIterator<Item = vega_readiness::Backend>,
    mut execute: impl FnMut(
        vega_readiness::Backend,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>>,
) -> (Vec<serde_json::Value>, bool) {
    let mut failed = false;
    let attempts = backends.into_iter().map(|backend| match execute(backend) {
        Ok(value) => value,
        Err(error) => {
            failed = true;
            serde_json::json!({"backend":backend.device(),"execution":"failed","error":error.to_string()})
        }
    }).collect();
    (attempts, failed)
}

#[cfg(unix)]
fn peak_resident_bytes() -> io::Result<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage initializes this platform-defined structure on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let peak = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).map_err(io::Error::other)?;
    Ok(if cfg!(target_os = "macos") {
        peak
    } else {
        peak.saturating_mul(1024)
    })
}
#[cfg(not(unix))]
fn peak_resident_bytes() -> io::Result<u64> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "RSS measurement unavailable",
    ))
}
fn tokenize(
    state: &str,
    tokenizer: &tokenizers::Tokenizer,
    bos: Option<u32>,
) -> Result<vega_prompt::Prompt, String> {
    // Tokenizer ownership belongs to the native worker. Generic prompt/span
    // construction and its fail-closed bounds remain in the root library.
    // No caller-controlled truncation or padding can mutate span alignment.
    let mut tokenizer = tokenizer.clone();
    tokenizer.with_truncation(None).map_err(|e| e.to_string())?;
    tokenizer.with_padding(None);
    vega_prompt::boolean_prompt(state, vega_prompt::QUESTION, bos, |s| {
        tokenizer
            .encode(s, false)
            .map(|e| e.get_ids().to_vec())
            .map_err(|e| e.to_string())
    })
}
fn load_pinned_tokenizer(
    installation: &Path,
    manifest: &vega_setup::InstallManifest,
) -> Result<(tokenizers::Tokenizer, Option<u32>), Box<dyn std::error::Error>> {
    let read = |name: &str, limit: u64| -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let bytes = read_bounded(&installation.join(name), limit)?;
        let pin = manifest
            .files
            .get(name)
            .ok_or("missing tokenizer file pin")?;
        if pin.bytes != bytes.len() as u64 || pin.sha256 != vega_readiness::hash(&bytes) {
            return Err("tokenizer snapshot differs from verified model manifest".into());
        }
        Ok(bytes)
    };
    let bytes = read("backbone/tokenizer.json", 16 * 1024 * 1024)?;
    let tokenizer = tokenizers::Tokenizer::from_bytes(&bytes)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let config: serde_json::Value =
        serde_json::from_slice(&read("backbone/tokenizer_config.json", 1024 * 1024)?)?;
    let bos = match config.get("bos_token") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let token = value
                .as_str()
                .or_else(|| value.get("content").and_then(serde_json::Value::as_str))
                .ok_or("unsupported BOS token metadata")?;
            Some(
                tokenizer
                    .token_to_id(token)
                    .ok_or("BOS token missing from pinned tokenizer")?,
            )
        }
    };
    Ok((tokenizer, bos))
}
fn infer(
    request: &Request,
    proof_path: Option<&Path>,
) -> Result<(vega_readiness::Outcome, usize), Box<dyn std::error::Error>> {
    use decapod::core::{
        vega_calibration::Calibration,
        vega_engine::{ChunkBatch, EngineConfig, FeatureBatch, QuestionType},
    };
    use std::time::Instant;
    let total_start = Instant::now();
    let stage = Instant::now();
    let manifest =
        vega_setup::verify_installation(&request.installation, &request.manifest_digest)?;
    let installation_verify_ms = stage.elapsed().as_secs_f64() * 1000.0;
    if manifest.adapters != "off" {
        return Err("unsupported adapters: this profile is explicitly base-only".into());
    }
    let stage = Instant::now();
    let config: serde_json::Value = serde_json::from_slice(&read_bounded(
        &request.installation.join("vega_config.json"),
        1024 * 1024,
    )?)?;
    let (tokenizer, bos) = load_pinned_tokenizer(&request.installation, &manifest)?;
    let prompt = tokenize(&request.state, &tokenizer, bos).map_err(io::Error::other)?;
    let metadata_and_tokenization_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let weights = manifest
        .files
        .iter()
        .filter(|(_, p)| p.role == "backbone_weights")
        .map(|(path, _)| request.installation.join(path))
        .collect::<Vec<_>>();
    if weights.len() != 1 {
        return Err("unsupported backbone shard layout".into());
    }
    let stage = Instant::now();
    let mut backbone = vega_backbone::NativeBackbone::load(
        &request.installation.join("backbone/config.json"),
        &weights,
        request.backend,
    )?;
    let backbone_load_including_probe_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let stage = Instant::now();
    let pooled = backbone.pooled(&prompt, proof_path.is_some())?;
    if pooled.d_model != vega_backbone::D_MODEL || pooled.option_count != 2 {
        return Err("backbone pooling metadata differs from the pinned profile".into());
    }
    let backbone_inference_pooling_readback_cleanup_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let one_shot_reuse_rejected =
        if proof_path.is_some() && request.backend != vega_readiness::Backend::Cpu {
            if !matches!(
                backbone.pooled(&prompt, false),
                Err(vega_backbone::BackboneError::Cleanup(_))
            ) {
                return Err("consumed one-shot GPU model did not reject reuse".into());
            }
            Some(true)
        } else {
            None
        };
    let stage = Instant::now();
    let engine_config = EngineConfig::from_json(&config)?;
    let steps = engine_config.n_steps;
    let engine = decoder::from_safetensors(
        engine_config,
        &read_bounded(
            &request.installation.join("engine.safetensors"),
            128 * 1024 * 1024,
        )?,
    )?;
    let batch = FeatureBatch {
        batch_size: 1,
        max_options: 2,
        feats: pooled.feats,
        opts: pooled.opts,
        opt_mask: vec![true, true],
        qtype: vec![QuestionType::Boolean],
        chunks: Some(ChunkBatch {
            count: 16,
            values: pooled.chunks,
            mask: pooled.chunk_mask,
        }),
    };
    let engine_load_and_batch_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let stage = Instant::now();
    let output = engine.forward(&batch, true)?;
    let engine_forward_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let row = output.rows.first().ok_or("missing engine row")?;
    // serde_json converts non-finite floats to null. Reject them before any
    // proof serialization, including latent vectors not present in Outcome.
    if row
        .logits
        .iter()
        .chain(&row.energies)
        .chain(&row.z0)
        .chain(&row.z_t)
        .chain([&row.kinetic, &row.grad_norm, &row.nearest, &row.potential])
        .any(|value| !value.is_finite())
    {
        return Err("non-finite engine diagnostic".into());
    }
    let stage = Instant::now();
    let answer = Calibration::from_config_json(&config, false)?.calibrate(
        row,
        QuestionType::Boolean,
        &[true, true],
        steps,
        &request.alpha,
        Some(&request.risk),
    )?;
    let calibration_ms = stage.elapsed().as_secs_f64() * 1000.0;
    if answer
        .probabilities
        .iter()
        .copied()
        .chain([answer.confidence, answer.temperature, answer.energy])
        .chain(
            [
                answer.conformal_threshold,
                answer.abstain_threshold,
                answer.unbound_threshold,
                answer.boolean_probability,
                answer.expected_score,
            ]
            .into_iter()
            .flatten(),
        )
        .any(|value| !value.is_finite())
    {
        return Err("non-finite calibration diagnostic".into());
    }
    if answer.conformal_threshold.is_none()
        || answer.abstain_threshold.is_none()
        || answer.unbound_threshold.is_none()
        || answer.probabilities.len() != 2
        || answer.option_indices != [0, 1]
    {
        return Err("required calibration coverage missing".into());
    }
    let outcome = vega_readiness::Outcome {
        probabilities: [answer.probabilities[0], answer.probabilities[1]],
        prediction_set: answer
            .prediction_set
            .as_ref()
            .map(|set| set.iter().map(|&i| i == 1).collect()),
        abstain: answer.abstain,
        unbound: answer.unbound,
        settle_steps: answer.settle_steps,
        energy: answer.energy,
        kinetic: row.kinetic as f64,
        nearest: row.nearest as f64,
        confidence: answer.confidence,
    };
    outcome.decision().map_err(io::Error::other)?;
    let stage = Instant::now();
    vega_setup::verify_installation(&request.installation, &request.manifest_digest)?;
    let installation_reverify_ms = stage.elapsed().as_secs_f64() * 1000.0;
    let inference_total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    if let Some(path) = proof_path {
        let chunks = batch.chunks.as_ref().ok_or("missing pooled chunks")?;
        if batch
            .feats
            .iter()
            .chain(&batch.opts)
            .chain(&chunks.values)
            .chain(&pooled.sampled_hidden)
            .any(|value| !value.is_finite())
        {
            return Err("non-finite pooled diagnostic".into());
        }
        let vector_hash = |values: &[f32]| {
            let bytes = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            vega_readiness::hash(&bytes)
        };
        // Keep sections small rather than raising the macro recursion limit.
        let mut proof = serde_json::Map::new();
        for section in [
            serde_json::json!({
                "schema":"decapod.vega-native-proof.v1", "request_id":request.request_id,
                "privacy":"private; token IDs and numeric features may disclose input",
                "export_max_bytes":MAX_PROOF_BYTES,
                "input_digest":request.input_digest,"manifest_digest":request.manifest_digest,
                "manifest":manifest,
                "backbone_source_revision":vega_backbone::NATIVE_SOURCE_REVISION,
                "candle_source_revision":vega_backbone::CANDLE_SOURCE_REVISION,
                "backend":request.backend.runtime(),"device":request.backend.device(),"engine_device":"cpu",
                "precision":vega_readiness::Precision::supported(),
                "device_probe":backbone.device_probe(),
                "one_shot_reuse_rejected":one_shot_reuse_rejected,
                "cpu_source":"assets/tools/vega-native/cpu_backbone.rs; separately implemented, not independently reviewed"
            }),
            serde_json::json!({
                "prompt_token_ids":prompt.tokens,"prompt_spans":prompt.spans,
                "prompt_tokens_sha256":vega_readiness::hash(&serde_json::to_vec(&prompt.tokens)?),
                "prompt_spans_sha256":vega_readiness::hash(&serde_json::to_vec(&prompt.spans)?),
                "token_limit":vega_prompt::MAX_TOKENS,
                "rubric":vega_prompt::RUBRIC,"boolean_option_order":[false,true],
                "feature_layers":vega_backbone::FEATURE_LAYERS,"option_layers":vega_backbone::OPTION_LAYERS,
                "feats_shape":[1,2,3,1024],"opts_shape":[1,2,1,1024],"chunks_shape":[1,16,1024],
                "pooled_feats":batch.feats,"pooled_opts":batch.opts,"pooled_chunks":chunks.values,
                "sample_positions":pooled.sample_positions,"sampled_hidden":pooled.sampled_hidden,
                "sampled_hidden_scope":"Private proof only; selected raw blocks in layer-major then sorted token-position then hidden-feature order",
                "pooled_feats_sha256":vector_hash(&batch.feats),"pooled_opts_sha256":vector_hash(&batch.opts),
                "pooled_chunks_sha256":vector_hash(&chunks.values),"chunk_mask":chunks.mask
                ,"sampled_hidden_sha256":vector_hash(&pooled.sampled_hidden)
            }),
            serde_json::json!({
                "pooled_hash_encoding":"concatenated little-endian F32 bytes in flat tensor order",
                "structured_hash_encoding":"compact serde_json of original typed data; struct field order follows Rust declarations, not the reordered export object",
                "engine_readout":row,"calibrated_answer":answer,"outcome":outcome,
                "engine_readout_sha256":vega_readiness::hash(&serde_json::to_vec(row)?),
                "calibrated_answer_sha256":vega_readiness::hash(&serde_json::to_vec(&answer)?),
                "outcome_sha256":vega_readiness::hash(&serde_json::to_vec(&outcome)?),
                "alpha":request.alpha,"risk":request.risk,"engine_steps":steps,
                "adapters":"off","calibration_scope":"upstream-general-decisions","parity":"unverified",
                "independent_reference":"missing; diagnostics do not establish model parity or readiness-domain calibration"
            }),
            serde_json::json!({
                "wall_clock_ms":{
                    "installation_verify":installation_verify_ms,
                    "metadata_and_tokenization":metadata_and_tokenization_ms,
                    "backbone_load_including_probe":backbone_load_including_probe_ms,
                    "backbone_inference_pooling_readback_cleanup":backbone_inference_pooling_readback_cleanup_ms,
                    "engine_load_and_batch":engine_load_and_batch_ms,
                    "engine_forward":engine_forward_ms,"calibration":calibration_ms,
                    "installation_reverify":installation_reverify_ms,"inference_total":inference_total_ms
                },
                "timing_scope":"single request, monotonic wall clock; inference_total excludes request parsing, diagnostic export and stdout; native stages synchronize queued GPU work; no performance threshold",
                "peak_resident_bytes":peak_resident_bytes().ok(),
                "memory_scope":"getrusage(RUSAGE_SELF) lifetime process high-water resident bytes, includes prior diagnostic attempts; not isolated model allocation or total GPU memory"
            }),
        ] {
            let serde_json::Value::Object(fields) = section else {
                return Err("invalid diagnostic section".into());
            };
            proof.extend(fields);
        }
        write_private_proof(path, &proof)?;
    }
    Ok((outcome, prompt.tokens.len()))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn strict_missing_numeric_phase_writes_failure_and_preserves_later_phase() {
        let directory = tempfile::tempdir().unwrap();
        let proof = directory.path().join("aggregate.json");
        let context = scientific_collection::Collection::create(&proof).unwrap();
        context
            .publish(
                scientific_collection::Phase::Shard(5),
                &directory.path().join("later.json"),
                false,
                serde_json::json!({"available_receipt":"later_cpu_diagnostic"}),
            )
            .unwrap();
        assert!(super::verify_scientific_collection(&proof).is_err());
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&proof).unwrap()).unwrap();
        assert_eq!(report["publication_ready"], false);
        assert_eq!(report["all_required_scientific_conditions_passed"], false);
        assert!(report["strict_verification_error"].is_string());
        assert_eq!(
            report["available_phase_diagnostics"]["phases"][6]["report"]["available_receipt"],
            "later_cpu_diagnostic"
        );
        assert_eq!(
            report["available_phase_diagnostics"]["phases"][6]["scientific_contents_reverified"],
            false
        );
        // Retrying cannot clobber the failure report or confer readiness.
        let original = std::fs::read(&proof).unwrap();
        assert!(super::verify_scientific_collection(&proof).is_err());
        assert_eq!(std::fs::read(&proof).unwrap(), original);
    }
    use super::*;

    #[test]
    fn tokenizer_snapshot_requires_exact_verified_bytes_before_parsing() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("backbone")).unwrap();
        let tokenizer = tokenizers::Tokenizer::new(tokenizers::models::bpe::BPE::default());
        let bytes = tokenizer.to_string(false).unwrap().into_bytes();
        let config = br#"{"bos_token":null}"#;
        fs::write(dir.path().join("backbone/tokenizer.json"), &bytes).unwrap();
        fs::write(dir.path().join("backbone/tokenizer_config.json"), config).unwrap();
        let file = |data: &[u8]| vega_setup::FilePin {
            role: "synthetic unit fixture".into(),
            repository: "unit/fixture".into(),
            revision: "unit".into(),
            upstream_path: "unit".into(),
            bytes: data.len() as u64,
            sha256: vega_readiness::hash(data),
        };
        let manifest = vega_setup::InstallManifest {
            schema: "synthetic unit fixture".into(),
            profile: "unit".into(),
            model: "unit".into(),
            revision: "unit".into(),
            backbone: "unit".into(),
            backbone_revision: "unit".into(),
            source: "unit".into(),
            source_revision: "unit".into(),
            license: "unit".into(),
            adapters: "off".into(),
            calibration_sha256: vega_readiness::hash(b"fixture"),
            files: std::collections::BTreeMap::from([
                ("backbone/tokenizer.json".into(), file(&bytes)),
                ("backbone/tokenizer_config.json".into(), file(config)),
            ]),
        };
        assert!(load_pinned_tokenizer(dir.path(), &manifest).is_ok());
        let mut changed = bytes.clone();
        changed.push(b' '); // Valid JSON, different immutable artifact.
        fs::write(dir.path().join("backbone/tokenizer.json"), changed).unwrap();
        assert!(load_pinned_tokenizer(dir.path(), &manifest).is_err());
        fs::write(dir.path().join("backbone/tokenizer.json"), bytes).unwrap();
        fs::write(dir.path().join("backbone/tokenizer_config.json"), b"{}").unwrap();
        assert!(load_pinned_tokenizer(dir.path(), &manifest).is_err());
    }

    #[test]
    fn verification_failures_do_not_suppress_other_required_phases() {
        let mut called = Vec::new();
        let results = collect_verification_steps(|name| {
            called.push(name.to_owned());
            match name {
                "native_diagnostics" => Err("independent parity failed".into()),
                "native_chunk_diagnostics" => Ok(()),
                "holdout_auto" => Err("calibration authority unsatisfied".into()),
                "holdout_cpu" => Ok(()),
                _ => unreachable!(),
            }
        });
        assert_eq!(
            called,
            [
                "native_diagnostics",
                "native_chunk_diagnostics",
                "holdout_auto",
                "holdout_cpu"
            ]
        );
        assert_eq!(results.len(), 4);
        assert_eq!(results[0]["status"], "failed");
        assert_eq!(results[1]["status"], "passed");
        assert_eq!(results[2]["status"], "failed");
        assert_eq!(results[3]["status"], "passed");
        assert!(results.iter().any(|step| step["status"] != "passed"));
    }
    #[test]
    fn explicit_cpu_evaluation_probe_dispatches_cpu_arithmetic() {
        let probe = evaluation_probe(vega_readiness::Backend::Cpu).unwrap();
        assert_eq!(probe.device, "cpu");
        assert_eq!(probe.actual, probe.expected);
        assert!(probe.bit_exact && probe.synchronized);
        // A real model-free CPU tensor operation, not checkpoint inference or
        // evidence of the production fallback protocol.
        assert_eq!(probe.parity, "unverified");
    }

    #[test]
    fn failed_accelerator_diagnostic_does_not_skip_independent_cpu_collection() {
        use vega_readiness::Backend;
        let mut executed = Vec::new();
        let (attempts, failed) = collect_diagnostics([Backend::Metal, Backend::Cpu], |backend| {
            executed.push(backend);
            if backend == Backend::Metal {
                Err("accelerator inference failed".into())
            } else {
                Ok(serde_json::json!({"backend":"cpu","publication_ready":false}))
            }
        });
        assert_eq!(executed, [Backend::Metal, Backend::Cpu]);
        assert!(failed);
        assert_eq!(attempts[0]["execution"], "failed");
        assert_eq!(attempts[1]["backend"], "cpu");
    }

    #[test]
    fn diagnostic_serialization_is_bounded() {
        assert_eq!(
            bounded_proof_bytes(&serde_json::json!({"parity":"unverified"})).unwrap(),
            br#"{"parity":"unverified"}"#
        );
        assert!(bounded_proof_bytes(&"x".repeat(MAX_PROOF_BYTES)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_export_is_private_and_never_follows_destination_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = std::env::temp_dir().join(format!(
            "vega-proof-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("proof.json");
        let proof = serde_json::json!({"parity":"unverified"});
        write_private_proof(&path, &proof).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        let original = fs::read(&path).unwrap();
        assert!(write_private_proof(&path, &proof).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let link = directory.join("symlink.json");
        symlink(&path, &link).unwrap();
        assert!(write_private_proof(&link, &proof).is_err());
        let missing = directory.join("missing.json");
        let dangling = directory.join("dangling.json");
        symlink(&missing, &dangling).unwrap();
        assert!(write_private_proof(&dangling, &proof).is_err());
        assert!(!missing.exists());
        let oversize = directory.join("oversize.json");
        assert!(write_private_proof(&oversize, &"x".repeat(MAX_PROOF_BYTES)).is_err());
        assert!(!oversize.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
