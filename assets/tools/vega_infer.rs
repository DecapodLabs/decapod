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
    if args.len() == 1 && args[0] == "--probe" {
        println!("{}", serde_json::to_string(&vega_backbone::probe_metal()?)?);
        return Ok(());
    }
    if !matches!(args.len(), 2 | 4)
        || args[0] != "--request"
        || (args.len() == 4 && args[2] != "--proof-output")
    {
        return Err("usage: decapod-vega-infer --probe | --backend-check <metal|cuda> | --request <private-json> [--proof-output <new-private-json> (PRIVATE, maximum 1 MiB)]".into());
    }
    let request: Request = serde_json::from_slice(&read_bounded(Path::new(&args[1]), 64 * 1024)?)?;
    if request.schema != vega_readiness::WORKER_SCHEMA
        || request.input_digest != vega_readiness::hash(request.state.as_bytes())
        || request.state.len() > 32 * 1024
        || request.request_id.len() > 128
    {
        return Err("invalid request binding".into());
    }
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
    let proof_path = if args.len() == 4 {
        Some(Path::new(&args[3]))
    } else {
        None
    };
    match infer(&request, proof_path) {
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
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
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
    let tokenizer_path = request.installation.join("backbone/tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| io::Error::other(e.to_string()))?;
    let tokenizer_config: serde_json::Value = serde_json::from_slice(&read_bounded(
        &request.installation.join("backbone/tokenizer_config.json"),
        1024 * 1024,
    )?)?;
    let bos = match tokenizer_config.get("bos_token") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let token = v
                .as_str()
                .or_else(|| v.get("content").and_then(serde_json::Value::as_str))
                .ok_or("unsupported BOS token metadata")?;
            Some(
                tokenizer
                    .token_to_id(token)
                    .ok_or("BOS token missing from pinned tokenizer")?,
            )
        }
    };
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
    let pooled = backbone.pooled(&prompt)?;
    let backbone_inference_pooling_readback_cleanup_ms = stage.elapsed().as_secs_f64() * 1000.0;
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
                "pooled_feats_sha256":vector_hash(&batch.feats),"pooled_opts_sha256":vector_hash(&batch.opts),
                "pooled_chunks_sha256":vector_hash(&chunks.values),"chunk_mask":chunks.mask
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
                "timing_scope":"single request, monotonic wall clock; inference_total excludes request parsing, diagnostic export and stdout; native stages synchronize queued GPU work; no performance threshold or memory measurement"
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
    use super::*;

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
