//! Bounded, text-only Qwen3.5 perception for the vega-0.8B readiness profile.
//!
//! The native implementation uses candle-vllm at
//! `2c2c5293d21961d29dd73c2ff7b246b77d3eaf3f` (MIT), with its pinned Candle
//! fork and attention.rs Metal kernels. It calls the public
//! `Qwen3_5::forward_collecting_layers` API, not an embedding or generation API.
//! Span/chunk semantics follow vegaml/_common.py at
//! `21a17c90624aa85cec60b1092fa72cb8b720f71a` (Apache-2.0).
//!
//! There is no download, tokenizer, Python process, CPU fallback, quantization,
//! cache reuse, batching, or truncation here. The caller must verify the pinned
//! installation before loading it. Source compatibility is not evidence of
//! numerical parity or actual Apple GPU execution; those are separate gates.
//! The verified BF16 checkpoint is upcast to F32 on Metal, preserving upstream
//! `backbone_dtype`'s Apple precision policy. This approximately doubles weight
//! memory versus BF16 inference; kernel-level numerical parity remains unproven.

use super::vega_prompt::{MAX_TOKENS, Prompt, PromptSpans};
use std::ops::Range;
use thiserror::Error;

pub const D_MODEL: usize = 1024;
pub const FEATURE_LAYERS: [usize; 2] = [13, 19];
pub const OPTION_LAYERS: [usize; 1] = [19];
pub const MEMORY_CHUNKS: usize = 16;
pub const VOCAB_SIZE: usize = 248_320;
pub const NATIVE_SOURCE_REVISION: &str = "2c2c5293d21961d29dd73c2ff7b246b77d3eaf3f";
pub const CANDLE_SOURCE_REVISION: &str = "b60efcf926dcfbdcad9e7c82e8a83679c526df20";
const CHUNK_MIN_TOKENS: usize = 128;
const CHUNK_TOKENS: usize = 128;
const BOOLEAN_OPTIONS: usize = 2;

#[derive(Debug, Error)]
pub enum BackboneError {
    #[error("vega native backbone is unavailable: {0}")]
    Unsupported(String),
    #[error("invalid vega backbone checkpoint: {0}")]
    Checkpoint(String),
    #[error("invalid vega backbone input: {0}")]
    Input(String),
    #[error("vega Metal computation failed: {0}")]
    Compute(String),
    #[error("vega recurrent-state cleanup failed; reload the backbone: {0}")]
    Cleanup(String),
    #[error("vega pooled feature contains a non-finite value: {0}")]
    NonFinite(&'static str),
}

pub type Result<T> = std::result::Result<T, BackboneError>;

// All operands and results are exactly representable in F32. These test native
// elementwise dispatch, not Qwen kernels, checkpoint parity, or calibration.
const PROBE_LEFT: [f32; 8] = [1.0, -2.0, 0.5, 4.0, 0.0, 8.0, -0.25, 2.0];
const PROBE_RIGHT: [f32; 8] = [0.5, 4.0, -2.0, 0.25, 3.0, -0.5, 8.0, -1.0];
const PROBE_EXPECTED: [f32; 8] = [1.5, -10.0, -0.5, 5.0, 0.0, 4.0, -2.25, 0.0];

/// Bounded evidence of actual native arithmetic and synchronized host readback.
/// A successful probe is deliberately not a model/parity acceptance receipt.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetalProbe {
    pub schema: &'static str,
    pub status: &'static str,
    pub backend: &'static str,
    pub device: &'static str,
    pub device_name: String,
    pub device_ordinal: usize,
    pub dtype: &'static str,
    pub operation: &'static str,
    pub left: [f32; 8],
    pub right: [f32; 8],
    pub expected: [f32; 8],
    pub actual: [f32; 8],
    pub synchronized: bool,
    pub bit_exact: bool,
    pub wall_clock_ms: f64,
    pub timing_scope: &'static str,
    pub candle_source_revision: &'static str,
    pub parity: &'static str,
}

fn verify_probe_output(actual: &[f32]) -> Result<[f32; 8]> {
    if actual.len() != PROBE_EXPECTED.len()
        || actual
            .iter()
            .zip(PROBE_EXPECTED)
            .any(|(actual, expected)| !actual.is_finite() || actual.to_bits() != expected.to_bits())
    {
        return Err(BackboneError::Compute(
            "native Metal F32 multiply/add capability probe did not match exact output".into(),
        ));
    }
    let mut verified = [0.0; 8];
    verified.copy_from_slice(actual);
    Ok(verified)
}

/// A single boolean prompt, in the exact flat order consumed by FeatureBatch.
/// Only these small pooled vectors cross from Metal to host memory.
#[derive(Debug, Clone)]
pub struct PooledFeatures {
    pub d_model: usize,
    pub option_count: usize,
    /// [1, 2, 3, 1024]: layers 13/19; state/question/last in each layer.
    pub feats: Vec<f32>,
    /// [1, 2, 1, 1024]: supplied no/yes option order, at layer 19.
    pub opts: Vec<f32>,
    /// [1, 16, 1024]: state chunks at layer 19, zero for unused slots.
    pub chunks: Vec<f32>,
    /// [1, 16], with the first k entries true.
    pub chunk_mask: Vec<bool>,
}

fn checked_span(span: &Range<usize>, length: usize, name: &str) -> Result<()> {
    if span.start >= span.end || span.end > length {
        return Err(BackboneError::Input(format!(
            "{name} must be a nonempty in-bounds half-open token span"
        )));
    }
    Ok(())
}

/// Reject overlapping buckets: upstream assigns state, question, options, last
/// in that order, so accepting overlap would silently change its pooling.
fn validate_input(tokens: &[u32], spans: &PromptSpans) -> Result<()> {
    if tokens.is_empty() || tokens.len() > MAX_TOKENS {
        return Err(BackboneError::Input(format!(
            "token count must be in 1..={MAX_TOKENS}; truncation is unsupported"
        )));
    }
    if tokens.iter().any(|&token| token as usize >= VOCAB_SIZE) {
        return Err(BackboneError::Input(
            "token ID exceeds the pinned vocabulary".into(),
        ));
    }
    if spans.last != tokens.len() - 1 {
        return Err(BackboneError::Input(
            "last must identify the final token".into(),
        ));
    }
    checked_span(&spans.state, tokens.len(), "state")?;
    checked_span(&spans.question, tokens.len(), "question")?;
    if spans.options.len() != BOOLEAN_OPTIONS {
        return Err(BackboneError::Input(
            "this profile requires exactly two boolean options".into(),
        ));
    }
    if spans.state.end > spans.question.start {
        return Err(BackboneError::Input(
            "state and question spans overlap or are out of order".into(),
        ));
    }
    let mut previous_end = spans.question.end;
    for option in &spans.options {
        checked_span(option, tokens.len(), "option")?;
        if previous_end > option.start || option.end > spans.last {
            return Err(BackboneError::Input(
                "option spans overlap another pooled bucket".into(),
            ));
        }
        previous_end = option.end;
    }
    Ok(())
}

/// Invert cid=floor(i*k/n) exactly. Its boundaries are ceil(j*n/k), not floor.
/// Ranges are relative to the start of the state span.
fn chunk_ranges(state_tokens: usize) -> Vec<Range<usize>> {
    if state_tokens < CHUNK_MIN_TOKENS.max(2) {
        return Vec::new();
    }
    let k = (state_tokens / CHUNK_TOKENS).clamp(2, MEMORY_CHUNKS);
    (0..k)
        .map(|chunk| (chunk * state_tokens).div_ceil(k)..((chunk + 1) * state_tokens).div_ceil(k))
        .collect()
}

#[cfg(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64"))]
mod native {
    use super::*;
    use candle_core::{DType, Device, Tensor};
    use candle_nn::var_builder::ShardedSafeTensors;
    use candle_vllm::InputMetadata;
    use candle_vllm::backend::progress::ProgressReporter;
    use candle_vllm::openai::distributed::Comm;
    use candle_vllm::openai::models::{Config, qwen3_5::Qwen3_5};
    use parking_lot::RwLock;
    use serde_json::{Value, json};
    use std::fs::File;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use std::sync::Arc;

    const MAX_CONFIG_BYTES: u64 = 64 * 1024;
    const MAX_WEIGHT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    const SEQUENCE_ID: usize = 0;
    const INFERENCE_DTYPE: DType = DType::F32;

    fn compute(error: candle_core::Error) -> BackboneError {
        BackboneError::Compute(error.to_string())
    }

    fn checked_metal_device() -> Result<Device> {
        // Never use a helper that may select CPU as a fallback.
        let device = Device::new_metal(0)
            .map_err(|e| BackboneError::Unsupported(format!("Apple Metal device 0: {e}")))?;
        if !device.is_metal() {
            return Err(BackboneError::Unsupported(
                "a native Metal device is required".into(),
            ));
        }
        Ok(device)
    }

    fn probe_on_device(device: &Device) -> Result<MetalProbe> {
        let start = std::time::Instant::now();
        let metal = device.as_metal_device().map_err(compute)?;
        let device_name = metal.device().name().to_owned();
        if device_name.len() > 256 {
            return Err(BackboneError::Unsupported(
                "Metal device identity exceeds limit".into(),
            ));
        }
        let left = Tensor::from_vec(PROBE_LEFT.to_vec(), (8,), device).map_err(compute)?;
        let right = Tensor::from_vec(PROBE_RIGHT.to_vec(), (8,), device).map_err(compute)?;
        // Candle's Metal backend dispatches both operations; this is not a host
        // computation uploaded to GPU memory or a platform-label-only check.
        let output = left
            .mul(&right)
            .and_then(|product| product.add(&left))
            .map_err(compute)?;
        if [&left, &right, &output].iter().any(|tensor| {
            !tensor.device().is_metal()
                || !tensor.device().same_device(device)
                || tensor.dtype() != DType::F32
                || tensor.dims() != [8]
        }) {
            return Err(BackboneError::Compute(
                "Metal probe device/dtype/shape mismatch".into(),
            ));
        }
        device.synchronize().map_err(compute)?;
        let actual = verify_probe_output(&output.to_vec1::<f32>().map_err(compute)?)?;
        Ok(MetalProbe {
            schema: "decapod.vega-metal-probe.v1",
            status: "passed",
            backend: "candle-metal",
            device: "metal",
            device_name,
            device_ordinal: 0,
            dtype: "f32",
            operation: "left * right + left",
            left: PROBE_LEFT,
            right: PROBE_RIGHT,
            expected: PROBE_EXPECTED,
            actual,
            synchronized: true,
            bit_exact: true,
            wall_clock_ms: start.elapsed().as_secs_f64() * 1000.0,
            timing_scope: "allocation, dispatch, synchronization, host readback and exact comparison; excludes device creation",
            candle_source_revision: CANDLE_SOURCE_REVISION,
            parity: "unverified",
        })
    }

    /// Needs no checkpoint files. Failure is an error, never a CPU retry.
    pub fn probe_metal() -> Result<MetalProbe> {
        probe_on_device(&checked_metal_device()?)
    }

    /// Owned single-request runtime. `pooled` requires &mut self, so requests
    /// cannot share or race recurrent state through this interface.
    pub struct MetalBackbone {
        model: Qwen3_5,
        device: Device,
        probe: MetalProbe,
        poisoned: bool,
    }

    impl MetalBackbone {
        /// Load only previously verified, immutable local checkpoint files.
        /// The installation must stay unchanged while this mmap-backed model
        /// exists. No remote repository or floating revision is accepted here.
        pub fn load(config_path: &Path, weight_paths: &[PathBuf]) -> Result<Self> {
            let cfg = load_profile_config(config_path)?;
            if weight_paths.len() != 1 {
                return Err(BackboneError::Checkpoint(
                    "the pinned 0.8B profile requires exactly one safetensors shard".into(),
                ));
            }
            for path in weight_paths {
                let metadata = std::fs::symlink_metadata(path)
                    .map_err(|e| BackboneError::Checkpoint(format!("weight metadata: {e}")))?;
                if !metadata.file_type().is_file()
                    || metadata.len() == 0
                    || metadata.len() > MAX_WEIGHT_BYTES
                    || path.extension().and_then(|s| s.to_str()) != Some("safetensors")
                {
                    return Err(BackboneError::Checkpoint(
                        "expected a bounded regular local safetensors file".into(),
                    ));
                }
            }
            // This constructor is intentionally used instead of candle-vllm's
            // new_device helper, which can silently choose the CPU.
            let device = checked_metal_device()?;
            // Fail closed before loading or running the model, on the exact
            // device subsequently used by this owned inference runtime.
            let probe = probe_on_device(&device)?;
            // SAFETY: callers supply the verified immutable installation; the
            // mmap-backed checkpoint must not be modified for this model's
            // lifetime. The setup layer owns installation integrity/retention.
            let vb =
                unsafe { ShardedSafeTensors::var_builder(weight_paths, INFERENCE_DTYPE, &device) }
                    .map_err(|e| BackboneError::Checkpoint(format!("safetensors: {e}")))?;
            let model = Qwen3_5::new(
                vb,
                &cfg,
                INFERENCE_DTYPE,
                &device,
                Rc::new(Comm::default()),
                Arc::new(RwLock::new(ProgressReporter::new(0))),
            )
            .map_err(|e| BackboneError::Checkpoint(format!("Qwen3.5 text load: {e}")))?;
            if !model.embed_weight().device().is_metal()
                || model.embed_weight().dtype() != INFERENCE_DTYPE
                || model.dtype() != INFERENCE_DTYPE
            {
                return Err(BackboneError::Unsupported(
                    "the loaded backbone is not F32 on Metal".into(),
                ));
            }
            model.reset_mamba_cache().map_err(compute)?;
            device.synchronize().map_err(compute)?;
            Ok(Self {
                model,
                device,
                probe,
                poisoned: false,
            })
        }

        pub fn metal_probe(&self) -> &MetalProbe {
            &self.probe
        }

        pub fn pooled(&mut self, prompt: &Prompt) -> Result<PooledFeatures> {
            validate_input(&prompt.tokens, &prompt.spans)?;
            if self.poisoned {
                return Err(BackboneError::Cleanup(
                    "previous cleanup did not succeed".into(),
                ));
            }
            if let Err(error) = self.model.reset_mamba_cache() {
                self.poisoned = true;
                return Err(BackboneError::Cleanup(error.to_string()));
            }
            let cleanup = StateCleanup {
                model: &self.model,
                finished: false,
            };
            let result = self.pooled_inner(prompt);
            if let Err(error) = cleanup.finish() {
                self.poisoned = true;
                let detail = match result {
                    Ok(_) => error.to_string(),
                    Err(inference) => format!("{error}; inference also failed: {inference}"),
                };
                return Err(BackboneError::Cleanup(detail));
            }
            // Include queued cleanup in completion and wall-clock measurement.
            if let Err(error) = self.device.synchronize() {
                self.poisoned = true;
                return Err(BackboneError::Cleanup(error.to_string()));
            }
            result
        }

        fn pooled_inner(&self, prompt: &Prompt) -> Result<PooledFeatures> {
            let length = prompt.tokens.len();
            let length_u32 = u32::try_from(length)
                .map_err(|_| BackboneError::Input("token length exceeds u32".into()))?;
            let ids = Tensor::from_vec(prompt.tokens.clone(), (length,), &self.device)
                .map_err(compute)?;
            let positions =
                Tensor::from_vec((0..length_u32).collect::<Vec<_>>(), (length,), &self.device)
                    .map_err(compute)?;
            let cu =
                Tensor::from_vec(vec![0u32, length_u32], (2,), &self.device).map_err(compute)?;
            let metadata = InputMetadata {
                is_prefill: true,
                is_mla: false,
                sequence_ids: Some(vec![SEQUENCE_ID]),
                mamba_slot_mapping: None,
                // No attention KV caches are supplied. The required slot map
                // is therefore never used for a cache write.
                slot_mapping: Tensor::from_vec(vec![-1i64; length], (length,), &self.device)
                    .map_err(compute)?,
                block_tables: None,
                block_tables_host: None,
                context_lens_host: Some(vec![length_u32]),
                context_lens: Some(
                    Tensor::from_vec(vec![length_u32], (1,), &self.device).map_err(compute)?,
                ),
                cu_seqlens_q: Some(cu.clone()),
                cu_seqlens_k: Some(cu),
                max_seqlen_q: length,
                max_seqlen_k: length,
                max_context_len: length,
                seqlens: Some(vec![length_u32]),
                flashinfer_metadata: None,
                is_mtp_verify: false,
            };
            // B1, fresh causal prefill. Native API returns full-token,
            // pre-final-norm layer tensors in increasing layer order.
            let (logits, layers) = self
                .model
                .forward_collecting_layers(&ids, &positions, None, &metadata, &FEATURE_LAYERS)
                .map_err(compute)?;
            // Upstream currently computes final logits even for collection.
            // They do not participate in vega perception and stay on device.
            drop(logits);
            if layers.len() != FEATURE_LAYERS.len() {
                return Err(BackboneError::Compute(
                    "missing requested intermediate layers".into(),
                ));
            }
            for hidden in &layers {
                if !hidden.device().is_metal() || hidden.dims() != [length, D_MODEL] {
                    return Err(BackboneError::Compute(
                        "unexpected hidden-state device or shape".into(),
                    ));
                }
            }
            let layers = layers
                .iter()
                .map(|hidden| hidden.to_dtype(DType::F32).map_err(compute))
                .collect::<Result<Vec<_>>>()?;
            let mut item = Vec::with_capacity(FEATURE_LAYERS.len() * 3);
            for hidden in &layers {
                item.push(span_mean(hidden, &prompt.spans.state)?);
                item.push(span_mean(hidden, &prompt.spans.question)?);
                item.push(
                    hidden
                        .narrow(0, prompt.spans.last, 1)
                        .and_then(|h| h.squeeze(0))
                        .map_err(compute)?,
                );
            }
            let deepest = &layers[1];
            let options = prompt
                .spans
                .options
                .iter()
                .map(|span| span_mean(deepest, span))
                .collect::<Result<Vec<_>>>()?;
            let ranges = chunk_ranges(prompt.spans.state.len());
            let mut chunks = Vec::with_capacity(MEMORY_CHUNKS);
            let mut chunk_mask = vec![false; MEMORY_CHUNKS];
            for (index, range) in ranges.iter().enumerate() {
                let absolute = (prompt.spans.state.start + range.start)
                    ..(prompt.spans.state.start + range.end);
                chunks.push(span_mean(deepest, &absolute)?);
                chunk_mask[index] = true;
            }
            while chunks.len() < MEMORY_CHUNKS {
                chunks.push(Tensor::zeros((D_MODEL,), DType::F32, &self.device).map_err(compute)?);
            }
            Ok(PooledFeatures {
                d_model: D_MODEL,
                option_count: BOOLEAN_OPTIONS,
                feats: small_host_vector(&item, FEATURE_LAYERS.len() * 3 * D_MODEL, "item")?,
                opts: small_host_vector(&options, BOOLEAN_OPTIONS * D_MODEL, "options")?,
                chunks: small_host_vector(&chunks, MEMORY_CHUNKS * D_MODEL, "chunks")?,
                chunk_mask,
            })
        }
    }

    fn span_mean(hidden_f32: &Tensor, span: &Range<usize>) -> Result<Tensor> {
        hidden_f32
            .narrow(0, span.start, span.len())
            .and_then(|rows| rows.mean(0))
            .map_err(compute)
    }

    fn small_host_vector(
        rows: &[Tensor],
        expected: usize,
        label: &'static str,
    ) -> Result<Vec<f32>> {
        let pooled = Tensor::stack(rows, 0)
            .and_then(|tensor| tensor.flatten_all())
            .map_err(compute)?;
        if !pooled.device().is_metal()
            || pooled.dtype() != DType::F32
            || pooled.elem_count() != expected
        {
            return Err(BackboneError::Compute(format!(
                "invalid pooled {label} tensor"
            )));
        }
        pooled.device().synchronize().map_err(compute)?;
        let values = pooled.to_vec1::<f32>().map_err(compute)?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(BackboneError::NonFinite(label));
        }
        Ok(values)
    }

    struct StateCleanup<'a> {
        model: &'a Qwen3_5,
        finished: bool,
    }

    impl StateCleanup<'_> {
        fn finish(mut self) -> candle_core::Result<()> {
            self.model.release_sequence_state(SEQUENCE_ID);
            // free_slot logs errors; reset_all returns them so fail closed.
            let result = self.model.reset_mamba_cache();
            self.finished = true;
            result
        }
    }

    impl Drop for StateCleanup<'_> {
        fn drop(&mut self) {
            if !self.finished {
                self.model.release_sequence_state(SEQUENCE_ID);
                let _ = self.model.reset_mamba_cache();
            }
        }
    }

    fn load_profile_config(path: &Path) -> Result<Config> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|e| BackboneError::Checkpoint(format!("configuration metadata: {e}")))?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
            return Err(BackboneError::Checkpoint(
                "expected a bounded regular configuration file".into(),
            ));
        }
        let mut raw = String::new();
        File::open(path)
            .and_then(|file| file.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut raw))
            .map_err(|e| BackboneError::Checkpoint(format!("configuration read: {e}")))?;
        if raw.len() as u64 > MAX_CONFIG_BYTES {
            return Err(BackboneError::Checkpoint(
                "configuration exceeds its byte limit".into(),
            ));
        }
        let root: Value = serde_json::from_str(&raw)
            .map_err(|e| BackboneError::Checkpoint(format!("configuration JSON: {e}")))?;
        let text = root
            .get("text_config")
            .ok_or_else(|| BackboneError::Checkpoint("missing text_config".into()))?;
        for (field, expected) in [
            ("hidden_size", 1024),
            ("num_hidden_layers", 24),
            ("vocab_size", 248_320),
            ("head_dim", 256),
            ("num_attention_heads", 8),
            ("num_key_value_heads", 2),
            ("intermediate_size", 3584),
            ("linear_conv_kernel_dim", 4),
            ("linear_key_head_dim", 128),
            ("linear_value_head_dim", 128),
            ("linear_num_key_heads", 16),
            ("linear_num_value_heads", 16),
            ("full_attention_interval", 4),
            ("max_position_embeddings", 262_144),
        ] {
            if text.get(field).and_then(Value::as_u64) != Some(expected) {
                return Err(BackboneError::Checkpoint(format!(
                    "unsupported 0.8B profile field: {field}"
                )));
            }
        }
        let expected_layers: Vec<_> = (0..24)
            .map(|index| {
                if (index + 1) % 4 == 0 {
                    "full_attention"
                } else {
                    "linear_attention"
                }
            })
            .collect();
        let rope = text
            .get("rope_parameters")
            .ok_or_else(|| BackboneError::Checkpoint("missing rope_parameters".into()))?;
        if root.get("model_type").and_then(Value::as_str) != Some("qwen3_5")
            || root.get("architectures") != Some(&json!(["Qwen3_5ForConditionalGeneration"]))
            || text.get("model_type").and_then(Value::as_str) != Some("qwen3_5_text")
            || text.get("layer_types") != Some(&json!(expected_layers))
            || text.get("hidden_act").and_then(Value::as_str) != Some("silu")
            || text.get("dtype").and_then(Value::as_str) != Some("bfloat16")
            || text.get("mamba_ssm_dtype").and_then(Value::as_str) != Some("float32")
            || text.get("tie_word_embeddings").and_then(Value::as_bool) != Some(true)
            || text.get("attention_bias").and_then(Value::as_bool) != Some(false)
            || text.get("attn_output_gate").and_then(Value::as_bool) != Some(true)
            || text.get("rms_norm_eps").and_then(Value::as_f64) != Some(1e-6)
            || rope.get("rope_type").and_then(Value::as_str) != Some("default")
            || rope.get("rope_theta").and_then(Value::as_f64) != Some(10_000_000.0)
            || rope.get("partial_rotary_factor").and_then(Value::as_f64) != Some(0.25)
            || rope.get("mrope_interleaved").and_then(Value::as_bool) != Some(true)
            || rope.get("mrope_section") != Some(&json!([11, 11, 10]))
        {
            return Err(BackboneError::Checkpoint(
                "configuration does not match the supported dense 0.8B BF16 profile".into(),
            ));
        }
        if [&root, text].iter().any(|config| {
            config
                .get("quantization_config")
                .is_some_and(|v| !v.is_null())
                || config.get("isq_quant").is_some_and(|v| !v.is_null())
        }) {
            return Err(BackboneError::Unsupported(
                "quantized backbones require separate parity evidence".into(),
            ));
        }
        // Parse the already bounded bytes rather than the upstream file loader,
        // whose missing-file branch panics. Preserve its outer-config handling
        // and public RoPE resolution without adding runtime scaling overrides.
        let mut cfg: Config = serde_json::from_value(text.clone())
            .map_err(|e| BackboneError::Checkpoint(format!("text configuration: {e}")))?;
        cfg.architectures = Some(vec!["Qwen3_5ForConditionalGeneration".into()]);
        cfg.extra_config_json = Some(raw);
        cfg.apply_runtime_rope_overrides(None);
        cfg.head_dim = Some(256);
        cfg.num_key_value_heads = Some(2);
        cfg.attention_bias = Some(false);
        cfg.is_f16_mode = false;
        cfg.isq_quant = None;
        cfg.mtp_enabled = false;
        cfg.mtp_max_verify_tokens = 0;
        if cfg.effective_max_seq_len() < MAX_TOKENS {
            return Err(BackboneError::Checkpoint(
                "backbone context is shorter than the bounded profile".into(),
            ));
        }
        Ok(cfg)
    }
}

#[cfg(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64"))]
pub use native::{MetalBackbone, probe_metal};

#[cfg(not(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64")))]
pub fn probe_metal() -> Result<MetalProbe> {
    Err(BackboneError::Unsupported(
        "requires an Apple Silicon macOS build with vega-native enabled; no fallback".into(),
    ))
}

#[cfg(not(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64")))]
pub struct MetalBackbone;

#[cfg(not(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64")))]
impl MetalBackbone {
    pub fn load(
        _config_path: &std::path::Path,
        _weight_paths: &[std::path::PathBuf],
    ) -> Result<Self> {
        Err(BackboneError::Unsupported(
            "requires an Apple Silicon macOS build with vega-native enabled".into(),
        ))
    }

    pub fn pooled(&mut self, _prompt: &Prompt) -> Result<PooledFeatures> {
        Err(BackboneError::Unsupported(
            "requires an Apple Silicon macOS build with vega-native enabled".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_probe_requires_exact_finite_arithmetic() {
        for ((left, right), expected) in PROBE_LEFT.into_iter().zip(PROBE_RIGHT).zip(PROBE_EXPECTED)
        {
            assert_eq!((left * right + left).to_bits(), expected.to_bits());
        }
        assert_eq!(
            verify_probe_output(&PROBE_EXPECTED).unwrap(),
            PROBE_EXPECTED
        );
        let mut wrong = PROBE_EXPECTED;
        wrong[0] = f32::from_bits(wrong[0].to_bits() + 1);
        assert!(verify_probe_output(&wrong).is_err());
        wrong[0] = f32::NAN;
        assert!(verify_probe_output(&wrong).is_err());
        wrong[0] = f32::INFINITY;
        assert!(verify_probe_output(&wrong).is_err());
        assert!(verify_probe_output(&PROBE_EXPECTED[..7]).is_err());
        let mut negative_zero = PROBE_EXPECTED;
        negative_zero[4] = -0.0;
        assert!(verify_probe_output(&negative_zero).is_err());
    }

    #[cfg(not(all(feature = "vega-native", target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn capability_probe_never_reports_success_without_native_metal() {
        assert!(probe_metal().is_err());
    }

    fn valid_prompt() -> Prompt {
        Prompt {
            tokens: vec![1; 10],
            spans: PromptSpans {
                state: 0..3,
                question: 3..5,
                options: vec![5..7, 7..9],
                last: 9,
            },
        }
    }

    #[test]
    fn rejects_truncation_empty_overlap_and_invalid_token_ids() {
        let mut prompt = valid_prompt();
        assert!(validate_input(&prompt.tokens, &prompt.spans).is_ok());
        prompt.tokens = vec![1; MAX_TOKENS + 1];
        assert!(validate_input(&prompt.tokens, &prompt.spans).is_err());
        prompt = valid_prompt();
        prompt.spans.state = 1..1;
        assert!(validate_input(&prompt.tokens, &prompt.spans).is_err());
        prompt = valid_prompt();
        prompt.spans.options[0] = 4..7;
        assert!(validate_input(&prompt.tokens, &prompt.spans).is_err());
        prompt = valid_prompt();
        prompt.tokens[0] = VOCAB_SIZE as u32;
        assert!(validate_input(&prompt.tokens, &prompt.spans).is_err());
    }

    #[test]
    fn chunks_preserve_floor_token_assignment_including_uneven_lengths() {
        for n in [0, 1, 127] {
            assert!(chunk_ranges(n).is_empty());
        }
        assert_eq!(chunk_ranges(128), vec![0..64, 64..128]);
        assert_eq!(chunk_ranges(129), vec![0..65, 65..129]);
        for n in [128, 129, 255, 256, 383, 384, 2047, 2048, 4096] {
            let ranges = chunk_ranges(n);
            let k = ranges.len();
            assert_eq!(k, (n / CHUNK_TOKENS).clamp(2, MEMORY_CHUNKS));
            assert_eq!(ranges.first().unwrap().start, 0);
            assert_eq!(ranges.last().unwrap().end, n);
            for (chunk, range) in ranges.iter().enumerate() {
                assert!(!range.is_empty());
                for token in range.clone() {
                    assert_eq!(token * k / n, chunk);
                }
            }
        }
    }
}
