//! Portable, scalar-f32 reference implementation of the vega base decision engine.
//!
//! Algorithm ported from `vegaml/_common.py` and `vegaml/mlx_engine.py` at
//! officialNandhaKishorM/vegaml@21a17c90624aa85cec60b1092fa72cb8b720f71a
//! (Apache-2.0). See the third-party license/notice shipped with this integration.
//! This is an inference-only correctness baseline, not a Metal implementation or
//! evidence of checkpoint parity. No Python, model download, training, dropout,
//! hidden-state synthesis, or adapter fallback occurs here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

// Resource limits belong to this reference runtime, not to the learned model.
// They bound both untrusted checkpoint allocation and caller-supplied batches.
/// Shared checkpoint byte bound for setup and the native worker decoder.
pub const MAX_CHECKPOINT_BYTES: usize = 512 * 1024 * 1024;
const MAX_WEIGHT_ELEMENTS: usize = 64 * 1024 * 1024;
const MAX_INPUT_ELEMENTS: usize = 16 * 1024 * 1024;
const MAX_BATCH: usize = 256;
const MAX_OPTIONS: usize = 256;
const MEMORY_HEADS: usize = 4;

/// Mixed feature-slot rows followed by mixed option rows, each with d_model columns.
type MixedSlotsAndOptions = (Vec<Vec<f32>>, Vec<Vec<f32>>);

#[derive(Debug, Error)]
pub enum VegaError {
    #[error("invalid vega configuration: {0}")]
    Config(String),
    #[error("invalid vega checkpoint: {0}")]
    Checkpoint(String),
    #[error("invalid vega feature input: {0}")]
    Input(String),
    #[error("non-finite vega computation at {0}")]
    NonFinite(String),
    #[error("vega adapter execution is unsupported: {0}; select adapters=off")]
    UnsupportedAdapter(String),
    #[error("invalid vega calibration: {0}")]
    Calibration(String),
}

pub type Result<T> = std::result::Result<T, VegaError>;

/// Checkpoint IDs are fixed: choice=0, score=1, boolean=2.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QuestionType {
    Choice,
    Score,
    Boolean,
}

impl QuestionType {
    pub fn id(self) -> usize {
        match self {
            Self::Choice => 0,
            Self::Score => 1,
            Self::Boolean => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Score => "score",
            Self::Boolean => "boolean",
        }
    }
}

fn default_force_max() -> f32 {
    20.0
}
fn default_ssm_state() -> usize {
    32
}

/// Engine fields from vega_config.json. Unrelated metadata is left to the
/// enclosing model/provenance loader; dimensions are checked before allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub d_model: usize,
    pub feature_layers: Vec<usize>,
    pub option_layers: Vec<usize>,
    pub latent_dim: usize,
    pub world_dim: usize,
    pub proj_hidden: usize,
    pub n_steps: usize,
    pub dt_max: f32,
    pub settle_eps: f32,
    #[serde(default = "default_force_max")]
    pub force_max: f32,
    #[serde(default = "default_ssm_state")]
    pub ssm_state: usize,
    #[serde(default)]
    pub memory_chunks: usize,
}

impl EngineConfig {
    pub fn from_json(value: &serde_json::Value) -> Result<Self> {
        let config: Self =
            serde_json::from_value(value.clone()).map_err(|e| VegaError::Config(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        for (name, value, max) in [
            ("d_model", self.d_model, 4096),
            ("feature_layers", self.feature_layers.len(), 128),
            ("option_layers", self.option_layers.len(), 128),
            ("latent_dim", self.latent_dim, 512),
            ("world_dim", self.world_dim, 1024),
            ("proj_hidden", self.proj_hidden, 4096),
            ("n_steps", self.n_steps, 128),
            ("ssm_state", self.ssm_state, 512),
        ] {
            if value == 0 || value > max {
                return Err(VegaError::Config(format!("{name} must be in 1..={max}")));
            }
        }
        if self.proj_hidden < 4 || self.memory_chunks > 128 {
            return Err(VegaError::Config(
                "proj_hidden must be >=4 and memory_chunks <=128".into(),
            ));
        }
        if self.memory_chunks != 0 && !self.world_dim.is_multiple_of(MEMORY_HEADS) {
            return Err(VegaError::Config(
                "memory world_dim must be divisible by 4".into(),
            ));
        }
        for (name, value) in [
            ("dt_max", self.dt_max),
            ("settle_eps", self.settle_eps),
            ("force_max", self.force_max),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(VegaError::Config(format!(
                    "{name} must be finite and positive"
                )));
            }
        }
        for (name, layers) in [
            ("feature_layers", &self.feature_layers),
            ("option_layers", &self.option_layers),
        ] {
            if layers.windows(2).any(|w| w[0] >= w[1]) {
                return Err(VegaError::Config(format!(
                    "{name} must be strictly increasing"
                )));
            }
        }
        let mut total = 0usize;
        for shape in self.expected_shapes().values() {
            total = total
                .checked_add(element_count(shape)?)
                .ok_or_else(|| VegaError::Config("checkpoint element count overflow".into()))?;
        }
        if total > MAX_WEIGHT_ELEMENTS {
            return Err(VegaError::Config(
                "checkpoint exceeds reference runtime weight limit".into(),
            ));
        }
        Ok(())
    }

    /// Checked state-dict schema for bounded external checkpoint decoders.
    pub fn weight_shapes(&self) -> Result<BTreeMap<String, Vec<usize>>> {
        self.validate()?;
        Ok(self.expected_shapes())
    }

    fn expected_shapes(&self) -> BTreeMap<String, Vec<usize>> {
        let (d, l, o, z, s, h, m) = (
            self.d_model,
            self.feature_layers.len(),
            self.option_layers.len(),
            self.latent_dim,
            self.world_dim,
            self.proj_hidden,
            self.ssm_state,
        );
        let mut shapes = BTreeMap::new();
        for (name, shape) in [
            ("mix", vec![3, l]),
            ("opt_mix", vec![o]),
            ("feat_mu", vec![l, 3, d]),
            ("feat_sd", vec![l, 3, d]),
            ("opt_mu", vec![o, d]),
            ("opt_sd", vec![o, d]),
            ("qtype_emb.weight", vec![3, z]),
            ("log_kappa", vec![]),
            ("force_scale", vec![]),
            ("A_log", vec![m]),
        ] {
            shapes.insert(name.to_owned(), shape);
        }
        for (name, input, hidden, output) in [
            ("world", d, h, s),
            ("probe", d, h, z),
            ("impulse", d, h, z),
            ("to_z0", 2 * s + 2 * z, h / 2, z),
            ("to_p0", 2 * s + 2 * z, h / 2, z),
            ("well", d + z, h, z + 3),
            ("rail_dir", z, h / 4, z),
        ] {
            mlp_shapes(&mut shapes, name, input, hidden, output);
        }
        norm_shapes(&mut shapes, "foresight.norm", s);
        linear_shapes(&mut shapes, "foresight.inp", s, 4 * s);
        linear_shapes(&mut shapes, "foresight.out", 2 * s, s);
        linear_shapes(&mut shapes, "ssm_in", 3 * z + 1, m + 2);
        linear_shapes(&mut shapes, "ssm_out", m, z);
        if self.memory_chunks > 0 {
            mlp_shapes(&mut shapes, "memory.q", 2 * d, h / 2, s);
            norm_shapes(&mut shapes, "memory.kv.0", d);
            linear_shapes(&mut shapes, "memory.kv.1", d, 2 * s);
            shapes.insert("memory.pos".into(), vec![self.memory_chunks, 2 * s]);
            linear_shapes(&mut shapes, "memory.out", s, s);
        }
        shapes
    }
}

/// Inspect a bounded, parsed safetensors header before activation. This checks
/// exact names, dimensions, supported floating-point dtypes and a contiguous,
/// nonoverlapping data region. It does not attest finiteness of payload values;
/// The native worker decoder and `VegaEngine::from_tensors` perform full-value validation.
pub fn validate_weight_header(
    config: &EngineConfig,
    header: &serde_json::Value,
    data_bytes: u64,
) -> Result<()> {
    config.validate()?;
    if data_bytes > MAX_CHECKPOINT_BYTES as u64 {
        return Err(VegaError::Checkpoint(
            "checkpoint byte limit exceeded".into(),
        ));
    }
    let object = header
        .as_object()
        .ok_or_else(|| VegaError::Checkpoint("header must be an object".into()))?;
    let expected = config.expected_shapes();
    let mut intervals = Vec::with_capacity(expected.len());
    let mut count = 0;
    for (name, value) in object {
        if name == "__metadata__" {
            if !value
                .as_object()
                .is_some_and(|m| m.values().all(serde_json::Value::is_string))
            {
                return Err(VegaError::Checkpoint(
                    "metadata must map strings to strings".into(),
                ));
            }
            continue;
        }
        if name.contains("lora_") || name.starts_with("gate") || name.contains(".base.") {
            return Err(VegaError::UnsupportedAdapter(name.clone()));
        }
        let shape = expected
            .get(name)
            .ok_or_else(|| VegaError::Checkpoint(format!("unexpected tensor {name}")))?;
        let tensor = value
            .as_object()
            .ok_or_else(|| VegaError::Checkpoint(format!("invalid tensor header {name}")))?;
        if tensor.len() != 3 {
            return Err(VegaError::Checkpoint(format!(
                "{name}: expected dtype, shape and data_offsets only"
            )));
        }
        let actual: Vec<usize> =
            serde_json::from_value(tensor.get("shape").cloned().unwrap_or_default())
                .map_err(|e| VegaError::Checkpoint(format!("{name}: invalid shape: {e}")))?;
        if &actual != shape {
            return Err(VegaError::Checkpoint(format!(
                "{name}: shape differs from config"
            )));
        }
        let width = match tensor.get("dtype").and_then(serde_json::Value::as_str) {
            Some("F32") => 4u64,
            Some("F16" | "BF16") => 2u64,
            _ => return Err(VegaError::Checkpoint(format!("{name}: unsupported dtype"))),
        };
        let offsets: [u64; 2] =
            serde_json::from_value(tensor.get("data_offsets").cloned().unwrap_or_default())
                .map_err(|e| VegaError::Checkpoint(format!("{name}: invalid offsets: {e}")))?;
        let bytes = (element_count(shape)? as u64)
            .checked_mul(width)
            .ok_or_else(|| VegaError::Checkpoint("tensor byte count overflow".into()))?;
        if offsets[1] > data_bytes || offsets[1].checked_sub(offsets[0]) != Some(bytes) {
            return Err(VegaError::Checkpoint(format!(
                "{name}: tensor data length/out-of-bounds mismatch"
            )));
        }
        intervals.push((offsets[0], offsets[1]));
        count += 1;
    }
    if count != expected.len() {
        return Err(VegaError::Checkpoint("missing expected tensors".into()));
    }
    intervals.sort_unstable();
    let mut end = 0;
    for (start, next) in intervals {
        if start != end {
            return Err(VegaError::Checkpoint(
                "overlapping or noncontiguous tensor data".into(),
            ));
        }
        end = next;
    }
    if end != data_bytes {
        return Err(VegaError::Checkpoint(
            "unreferenced trailing checkpoint bytes".into(),
        ));
    }
    Ok(())
}

fn norm_shapes(shapes: &mut BTreeMap<String, Vec<usize>>, prefix: &str, input: usize) {
    shapes.insert(format!("{prefix}.weight"), vec![input]);
    shapes.insert(format!("{prefix}.bias"), vec![input]);
}
fn linear_shapes(
    shapes: &mut BTreeMap<String, Vec<usize>>,
    prefix: &str,
    input: usize,
    output: usize,
) {
    shapes.insert(format!("{prefix}.weight"), vec![output, input]);
    shapes.insert(format!("{prefix}.bias"), vec![output]);
}
fn mlp_shapes(
    shapes: &mut BTreeMap<String, Vec<usize>>,
    prefix: &str,
    input: usize,
    hidden: usize,
    output: usize,
) {
    norm_shapes(shapes, &format!("{prefix}.net.0"), input);
    linear_shapes(shapes, &format!("{prefix}.net.1"), input, hidden);
    linear_shapes(shapes, &format!("{prefix}.net.4"), hidden, output);
}
fn element_count(shape: &[usize]) -> Result<usize> {
    shape
        .iter()
        .try_fold(1usize, |n, &d| n.checked_mul(d))
        .ok_or_else(|| VegaError::Checkpoint("tensor element count overflow".into()))
}
fn finite(name: &str, values: &[f32]) -> Result<()> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(VegaError::NonFinite(name.to_owned()))
    }
}

/// An explicitly shaped, row-major tensor. Primarily useful for deterministic
/// fixtures; production checkpoints are decoded by the native worker before `from_tensors`.
#[derive(Debug, Clone)]
pub struct TensorData {
    pub shape: Vec<usize>,
    pub values: Vec<f32>,
}

/// Raw (not prestandardized) backbone features, in checkpoint layer order.
#[derive(Debug, Clone)]
pub struct FeatureBatch {
    pub batch_size: usize,
    pub max_options: usize,
    /// [B, number of feature layers, 3, d_model]; slots are state/question/last.
    pub feats: Vec<f32>,
    /// [B, K, number of option layers, d_model].
    pub opts: Vec<f32>,
    /// [B, K], with at least two real options per item.
    pub opt_mask: Vec<bool>,
    pub qtype: Vec<QuestionType>,
    pub chunks: Option<ChunkBatch>,
}

#[derive(Debug, Clone)]
pub struct ChunkBatch {
    pub count: usize,
    /// [B, C, d_model], pooled at the deepest feature layer.
    pub values: Vec<f32>,
    pub mask: Vec<bool>,
}

/// Per-question physical readouts, all in original supplied option order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineReadout {
    pub logits: Vec<f32>,
    pub energies: Vec<f32>,
    pub z0: Vec<f32>,
    pub z_t: Vec<f32>,
    pub kinetic: f32,
    pub grad_norm: f32,
    pub nearest: f32,
    pub potential: f32,
    pub steps_used: usize,
    pub settled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineOutput {
    pub rows: Vec<EngineReadout>,
}

#[derive(Debug)]
struct Norm {
    weight: Vec<f32>,
    bias: Vec<f32>,
}
impl Norm {
    fn take(tensors: &mut BTreeMap<String, TensorData>, prefix: &str) -> Result<Self> {
        Ok(Self {
            weight: take(tensors, &format!("{prefix}.weight"))?,
            bias: take(tensors, &format!("{prefix}.bias"))?,
        })
    }
    fn forward(&self, x: &[f32]) -> Result<Vec<f32>> {
        let mean = x.iter().sum::<f32>() / x.len() as f32;
        let variance = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / x.len() as f32;
        let denominator = (variance + 1e-5).sqrt();
        finite("layer norm statistics", &[mean, variance, denominator])?;
        let out: Vec<_> = x
            .iter()
            .zip(&self.weight)
            .zip(&self.bias)
            .map(|((&x, &w), &b)| (x - mean) / denominator * w + b)
            .collect();
        finite("layer norm", &out)?;
        Ok(out)
    }
}

#[derive(Debug)]
struct Linear {
    input: usize,
    weight: Vec<f32>,
    bias: Vec<f32>,
}
impl Linear {
    fn take(tensors: &mut BTreeMap<String, TensorData>, prefix: &str) -> Result<Self> {
        let weight = take(tensors, &format!("{prefix}.weight"))?;
        let bias = take(tensors, &format!("{prefix}.bias"))?;
        Ok(Self {
            input: weight.len() / bias.len(),
            weight,
            bias,
        })
    }
    fn forward(&self, x: &[f32]) -> Result<Vec<f32>> {
        if x.len() != self.input {
            return Err(VegaError::Input(
                "internal linear dimension mismatch".into(),
            ));
        }
        let out: Vec<_> = self
            .weight
            .chunks_exact(self.input)
            .zip(&self.bias)
            .map(|(row, &b)| row.iter().zip(x).map(|(&w, &v)| w * v).sum::<f32>() + b)
            .collect();
        finite("linear", &out)?;
        Ok(out)
    }
}

#[derive(Debug)]
struct Mlp {
    norm: Norm,
    first: Linear,
    last: Linear,
}
impl Mlp {
    fn take(tensors: &mut BTreeMap<String, TensorData>, prefix: &str) -> Result<Self> {
        Ok(Self {
            norm: Norm::take(tensors, &format!("{prefix}.net.0"))?,
            first: Linear::take(tensors, &format!("{prefix}.net.1"))?,
            last: Linear::take(tensors, &format!("{prefix}.net.4"))?,
        })
    }
    fn forward(&self, x: &[f32]) -> Result<Vec<f32>> {
        let mut hidden = self.first.forward(&self.norm.forward(x)?)?;
        for value in &mut hidden {
            *value = gelu(*value);
        }
        self.last.forward(&hidden)
    }
}

#[derive(Debug)]
struct Foresight {
    norm: Norm,
    input: Linear,
    output: Linear,
}
impl Foresight {
    fn take(tensors: &mut BTreeMap<String, TensorData>) -> Result<Self> {
        Ok(Self {
            norm: Norm::take(tensors, "foresight.norm")?,
            input: Linear::take(tensors, "foresight.inp")?,
            output: Linear::take(tensors, "foresight.out")?,
        })
    }
    fn forward(&self, state: &[f32]) -> Result<Vec<f32>> {
        let u = self.input.forward(&self.norm.forward(state)?)?;
        let (x, gate) = u.split_at(u.len() / 2);
        let gated: Vec<_> = x
            .iter()
            .zip(gate)
            .map(|(&x, &g)| gelu(x) * sigmoid(g))
            .collect();
        let mut out = self.output.forward(&gated)?;
        for (v, &s) in out.iter_mut().zip(state) {
            *v += s;
        }
        finite("world dynamics", &out)?;
        Ok(out)
    }
}

#[derive(Debug)]
struct Memory {
    query: Mlp,
    kv_norm: Norm,
    kv: Linear,
    positions: Vec<f32>,
    position_count: usize,
    output: Linear,
}
impl Memory {
    fn take(tensors: &mut BTreeMap<String, TensorData>, positions: usize) -> Result<Self> {
        Ok(Self {
            query: Mlp::take(tensors, "memory.q")?,
            kv_norm: Norm::take(tensors, "memory.kv.0")?,
            kv: Linear::take(tensors, "memory.kv.1")?,
            positions: take(tensors, "memory.pos")?,
            position_count: positions,
            output: Linear::take(tensors, "memory.out")?,
        })
    }
    fn forward(
        &self,
        query: &[f32],
        chunks: &[f32],
        mask: &[bool],
        d_model: usize,
    ) -> Result<Vec<f32>> {
        let s = self.output.input;
        let count = mask.len();
        // Upstream's final multiplication by any(mask) is exactly zero here.
        if !mask.iter().any(|&v| v) {
            return Ok(vec![0.0; s]);
        }
        let q = self.query.forward(query)?;
        let n = mask.iter().filter(|&&v| v).count() as f32;
        let mut kvs = Vec::with_capacity(count * 2 * s);
        for (i, ch) in chunks.chunks_exact(d_model).enumerate() {
            let mut kv = self.kv.forward(&self.kv_norm.forward(ch)?)?;
            let position = (((i as f32 + 0.5) / n * self.position_count as f32) as usize)
                .min(self.position_count - 1);
            for (j, value) in kv.iter_mut().enumerate() {
                *value += self.positions[position * 2 * s + j];
            }
            finite("memory key/value", &kv)?;
            kvs.extend(kv);
        }
        let head_dim = s / MEMORY_HEADS;
        let mut attended = vec![0.0; s];
        for head in 0..MEMORY_HEADS {
            let offset = head * head_dim;
            let scores: Vec<_> = (0..count)
                .map(|i| {
                    if mask[i] {
                        (0..head_dim)
                            .map(|j| q[offset + j] * kvs[i * 2 * s + offset + j])
                            .sum::<f32>()
                            / (head_dim as f32).sqrt()
                    } else {
                        -1e4
                    }
                })
                .collect();
            let weights = softmax(&scores)?;
            for (i, &weight) in weights.iter().enumerate() {
                for (j, value) in attended[offset..offset + head_dim].iter_mut().enumerate() {
                    *value += weight * kvs[i * 2 * s + s + offset + j];
                }
            }
        }
        finite("memory attended values", &attended)?;
        self.output.forward(&attended)
    }
}

fn take(tensors: &mut BTreeMap<String, TensorData>, name: &str) -> Result<Vec<f32>> {
    tensors
        .remove(name)
        .map(|t| t.values)
        .ok_or_else(|| VegaError::Checkpoint(format!("missing tensor {name}")))
}
fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + libm::erff(x / std::f32::consts::SQRT_2))
}
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
fn softplus(x: f32) -> f32 {
    if x > 20.0 { x } else { x.exp().ln_1p() }
}
fn norm(x: &[f32]) -> f32 {
    x.iter().map(|v| v * v).sum::<f32>().sqrt()
}
fn softmax(x: &[f32]) -> Result<Vec<f32>> {
    finite("softmax input", x)?;
    let max = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut exp: Vec<_> = x.iter().map(|v| (v - max).exp()).collect();
    let sum = exp.iter().sum::<f32>();
    if !sum.is_finite() || sum <= 0.0 {
        return Err(VegaError::NonFinite("softmax denominator".into()));
    }
    for value in &mut exp {
        *value /= sum;
    }
    Ok(exp)
}

#[derive(Debug)]
struct Landscape {
    centers: Vec<Vec<f32>>,
    log_a: Vec<f32>,
    sigma: Vec<f32>,
}

/// A loaded base-only reference engine. Construct once and reuse for batches.
#[derive(Debug)]
pub struct VegaEngine {
    config: EngineConfig,
    mix: Vec<f32>,
    opt_mix: Vec<f32>,
    feat_mu: Vec<f32>,
    feat_sd: Vec<f32>,
    opt_mu: Vec<f32>,
    opt_sd: Vec<f32>,
    world: Mlp,
    foresight: Foresight,
    probe: Mlp,
    impulse: Mlp,
    qtype_emb: Vec<f32>,
    to_z0: Mlp,
    to_p0: Mlp,
    well: Mlp,
    rail_dir: Mlp,
    kappa: f32,
    ssm_in: Linear,
    a: Vec<f32>,
    ssm_out: Linear,
    force_scale: f32,
    memory: Option<Memory>,
}

impl VegaEngine {
    /// A request for auto/on must fail explicitly until gated adapters are ported.
    pub fn require_base_adapters(mode: &str) -> Result<()> {
        if mode == "off" {
            Ok(())
        } else {
            Err(VegaError::UnsupportedAdapter(mode.to_owned()))
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn from_tensors(
        config: EngineConfig,
        mut tensors: BTreeMap<String, TensorData>,
    ) -> Result<Self> {
        config.validate()?;
        let expected = config.expected_shapes();
        for name in tensors.keys() {
            if !expected.contains_key(name) {
                if name.contains("lora_") || name.starts_with("gate") || name.contains(".base.") {
                    return Err(VegaError::UnsupportedAdapter(name.clone()));
                }
                return Err(VegaError::Checkpoint(format!("unexpected tensor {name}")));
            }
        }
        for (name, shape) in expected {
            let tensor = tensors
                .get(&name)
                .ok_or_else(|| VegaError::Checkpoint(format!("missing tensor {name}")))?;
            if tensor.shape != shape || tensor.values.len() != element_count(&shape)? {
                return Err(VegaError::Checkpoint(format!(
                    "{name}: expected shape {shape:?} with matching element count"
                )));
            }
            finite(&format!("checkpoint {name}"), &tensor.values)?;
        }
        let feat_sd = take(&mut tensors, "feat_sd")?;
        let opt_sd = take(&mut tensors, "opt_sd")?;
        if feat_sd.iter().chain(&opt_sd).any(|v| *v <= 0.0) {
            return Err(VegaError::Checkpoint(
                "standard deviations must be positive".into(),
            ));
        }
        let kappa = take(&mut tensors, "log_kappa")?[0].exp();
        let a: Vec<_> = take(&mut tensors, "A_log")?
            .into_iter()
            .map(f32::exp)
            .collect();
        finite("checkpoint exp(log_kappa)", &[kappa])?;
        finite("checkpoint exp(A_log)", &a)?;
        let memory = if config.memory_chunks > 0 {
            Some(Memory::take(&mut tensors, config.memory_chunks)?)
        } else {
            None
        };
        Ok(Self {
            mix: take(&mut tensors, "mix")?,
            opt_mix: take(&mut tensors, "opt_mix")?,
            feat_mu: take(&mut tensors, "feat_mu")?,
            feat_sd,
            opt_mu: take(&mut tensors, "opt_mu")?,
            opt_sd,
            world: Mlp::take(&mut tensors, "world")?,
            foresight: Foresight::take(&mut tensors)?,
            probe: Mlp::take(&mut tensors, "probe")?,
            impulse: Mlp::take(&mut tensors, "impulse")?,
            qtype_emb: take(&mut tensors, "qtype_emb.weight")?,
            to_z0: Mlp::take(&mut tensors, "to_z0")?,
            to_p0: Mlp::take(&mut tensors, "to_p0")?,
            well: Mlp::take(&mut tensors, "well")?,
            rail_dir: Mlp::take(&mut tensors, "rail_dir")?,
            kappa,
            ssm_in: Linear::take(&mut tensors, "ssm_in")?,
            a,
            ssm_out: Linear::take(&mut tensors, "ssm_out")?,
            force_scale: take(&mut tensors, "force_scale")?[0],
            memory,
            config,
        })
    }

    pub fn forward(&self, input: &FeatureBatch, halt: bool) -> Result<EngineOutput> {
        self.validate_input(input)?;
        let d = self.config.d_model;
        let lf = self.config.feature_layers.len();
        let lo = self.config.option_layers.len();
        let k = input.max_options;
        let mut rows = Vec::with_capacity(input.batch_size);
        for b in 0..input.batch_size {
            let feats = &input.feats[b * lf * 3 * d..(b + 1) * lf * 3 * d];
            let opts = &input.opts[b * k * lo * d..(b + 1) * k * lo * d];
            let mask = &input.opt_mask[b * k..(b + 1) * k];
            let chunks = input.chunks.as_ref().map(|ch| {
                (
                    &ch.values[b * ch.count * d..(b + 1) * ch.count * d],
                    &ch.mask[b * ch.count..(b + 1) * ch.count],
                )
            });
            rows.push(self.forward_row(feats, opts, mask, input.qtype[b], chunks, halt)?);
        }
        Ok(EngineOutput { rows })
    }

    fn validate_input(&self, input: &FeatureBatch) -> Result<()> {
        let b = input.batch_size;
        let k = input.max_options;
        if b == 0 || b > MAX_BATCH || !(2..=MAX_OPTIONS).contains(&k) {
            return Err(VegaError::Input(
                "batch/options outside reference runtime limits".into(),
            ));
        }
        let d = self.config.d_model;
        let nf = element_count(&[b, self.config.feature_layers.len(), 3, d])?;
        let no = element_count(&[b, k, self.config.option_layers.len(), d])?;
        let mut total = nf
            .checked_add(no)
            .ok_or_else(|| VegaError::Input("input size overflow".into()))?;
        if input.feats.len() != nf
            || input.opts.len() != no
            || input.opt_mask.len() != b * k
            || input.qtype.len() != b
        {
            return Err(VegaError::Input(
                "feature, option, mask or question type shape mismatch".into(),
            ));
        }
        for (mask, qtype) in input.opt_mask.chunks_exact(k).zip(&input.qtype) {
            let n = mask.iter().filter(|&&x| x).count();
            if n < 2 || (*qtype == QuestionType::Boolean && n != 2) {
                return Err(VegaError::Input(
                    "each question needs >=2 options; boolean needs exactly 2 in [no, yes] order"
                        .into(),
                ));
            }
        }
        if let Some(ch) = &input.chunks {
            if self.memory.is_none() || ch.count == 0 || ch.count > self.config.memory_chunks {
                return Err(VegaError::Input(
                    "chunks require a compatible checkpoint memory reader".into(),
                ));
            }
            let nc = element_count(&[b, ch.count, d])?;
            if ch.values.len() != nc || ch.mask.len() != b * ch.count {
                return Err(VegaError::Input(
                    "chunk values or mask shape mismatch".into(),
                ));
            }
            total = total
                .checked_add(nc)
                .ok_or_else(|| VegaError::Input("input size overflow".into()))?;
            finite("chunk features", &ch.values)?;
        }
        if total > MAX_INPUT_ELEMENTS {
            return Err(VegaError::Input("input element limit exceeded".into()));
        }
        finite("pooled features", &input.feats)?;
        finite("option features", &input.opts)
    }

    fn mixed(&self, feats: &[f32], opts: &[f32], k: usize) -> Result<MixedSlotsAndOptions> {
        let d = self.config.d_model;
        let lf = self.config.feature_layers.len();
        let lo = self.config.option_layers.len();
        let mut slots = vec![vec![0.0; d]; 3];
        for (s, slot) in slots.iter_mut().enumerate() {
            let weights = softmax(&self.mix[s * lf..(s + 1) * lf])?;
            for (layer, &weight) in weights.iter().enumerate() {
                for (j, value) in slot.iter_mut().enumerate() {
                    let i = (layer * 3 + s) * d + j;
                    *value += (feats[i] - self.feat_mu[i]) / self.feat_sd[i] * weight;
                }
            }
            finite("mixed feature slot", slot)?;
        }
        let weights = softmax(&self.opt_mix)?;
        let mut options = vec![vec![0.0; d]; k];
        for (option, row) in options.iter_mut().enumerate() {
            for (layer, &weight) in weights.iter().enumerate() {
                for (j, value) in row.iter_mut().enumerate() {
                    let i = layer * d + j;
                    *value +=
                        (opts[option * lo * d + i] - self.opt_mu[i]) / self.opt_sd[i] * weight;
                }
            }
            finite("mixed option", row)?;
        }
        Ok((slots, options))
    }

    fn landscape(
        &self,
        options: &[Vec<f32>],
        mask: &[bool],
        qtype: QuestionType,
        probe: &[f32],
    ) -> Result<Landscape> {
        let d = self.config.latent_dim;
        let emb = &self.qtype_emb[qtype.id() * d..(qtype.id() + 1) * d];
        let mut centers = Vec::with_capacity(options.len());
        let mut log_a = Vec::with_capacity(options.len());
        let mut sigma = Vec::with_capacity(options.len());
        let mut gaps = Vec::with_capacity(options.len());
        for option in options {
            let x: Vec<_> = option.iter().chain(probe).copied().collect();
            let e = self.well.forward(&x)?;
            let center: Vec<_> = e[..d].iter().zip(emb).map(|(&v, &b)| v + b).collect();
            finite("well centers", &center)?;
            centers.push(center);
            log_a.push(e[d].clamp(-4.0, 4.0));
            sigma.push(0.3 + softplus(e[d + 1]));
            gaps.push(softplus(e[d + 2]));
        }
        if qtype == QuestionType::Score {
            let count = mask.iter().filter(|&&x| x).count() as f32;
            let mut r0 = vec![0.0; d];
            for (center, &enabled) in centers.iter().zip(mask) {
                for (r, &v) in r0.iter_mut().zip(center) {
                    *r += v * if enabled { 1.0 } else { 0.0 };
                }
            }
            for r in &mut r0 {
                *r /= count;
            }
            let mut direction = self.rail_dir.forward(probe)?;
            let length = norm(&direction).max(1e-12);
            finite("score direction norm", &[length])?;
            for v in &mut direction {
                *v /= length;
            }
            let mut cumulative = 0.0;
            let t: Vec<_> = gaps
                .iter()
                .zip(mask)
                .map(|(&gap, &enabled)| {
                    cumulative += gap * if enabled { 1.0 } else { 0.0 };
                    cumulative
                })
                .collect();
            let mean = t
                .iter()
                .zip(mask)
                .map(|(&v, &enabled)| v * if enabled { 1.0 } else { 0.0 })
                .sum::<f32>()
                / count;
            for (center, &position) in centers.iter_mut().zip(&t) {
                for ((value, &origin), &axis) in center.iter_mut().zip(&r0).zip(&direction) {
                    *value = origin + (position - mean) * axis;
                }
                finite("score rail", center)?;
            }
        }
        finite("well widths", &sigma)?;
        Ok(Landscape {
            centers,
            log_a,
            sigma,
        })
    }

    fn potential(
        &self,
        z: &[f32],
        landscape: &Landscape,
        mask: &[bool],
    ) -> Result<(f32, Vec<f32>, Vec<f32>)> {
        let mut potential = 0.5 * self.kappa * z.iter().map(|x| x * x).sum::<f32>();
        let mut gradient: Vec<_> = z.iter().map(|x| self.kappa * x).collect();
        let mut exponents = Vec::with_capacity(mask.len());
        for (k, &enabled) in mask.iter().enumerate() {
            let diff: Vec<_> = z
                .iter()
                .zip(&landscape.centers[k])
                .map(|(&z, &c)| z - c)
                .collect();
            let variance = landscape.sigma[k] * landscape.sigma[k];
            let q = diff.iter().map(|x| x * x).sum::<f32>() / (2.0 * variance);
            let g = (landscape.log_a[k] - q).exp() * if enabled { 1.0 } else { 0.0 };
            finite("well exponent", &[variance, q, g])?;
            potential -= g;
            for (grad, delta) in gradient.iter_mut().zip(diff) {
                *grad += g * delta / variance;
            }
            exponents.push(q);
        }
        finite("potential", &[potential])?;
        finite("potential gradient", &gradient)?;
        Ok((potential, gradient, exponents))
    }

    fn forward_row(
        &self,
        feats: &[f32],
        opts: &[f32],
        mask: &[bool],
        qtype: QuestionType,
        chunks: Option<(&[f32], &[bool])>,
        halt: bool,
    ) -> Result<EngineReadout> {
        let (slots, options) = self.mixed(feats, opts, mask.len())?;
        let mut state = self.world.forward(&slots[0])?;
        if let (Some(memory), Some((chunks, cmask))) = (&self.memory, chunks)
            && cmask.iter().any(|&x| x)
        {
            let d = self.config.d_model;
            let deepest = (self.config.feature_layers.len() - 1) * 3 * d;
            let standardized: Vec<_> = chunks
                .iter()
                .enumerate()
                .map(|(i, &x)| (x - self.feat_mu[deepest + i % d]) / self.feat_sd[deepest + i % d])
                .collect();
            finite("standardized memory chunks", &standardized)?;
            let query: Vec<_> = slots[1].iter().chain(&slots[2]).copied().collect();
            let read = memory.forward(&query, &standardized, cmask, d)?;
            for (s, r) in state.iter_mut().zip(read) {
                *s += r;
            }
            finite("memory-augmented world", &state)?;
        }
        let next = self.foresight.forward(&state)?;
        let mut probe = self.probe.forward(&slots[1])?;
        let d = self.config.latent_dim;
        for (j, p) in probe.iter_mut().enumerate() {
            *p += self.qtype_emb[qtype.id() * d + j];
        }
        finite("probe with type embedding", &probe)?;
        let impulse = self.impulse.forward(&slots[2])?;
        let context: Vec<_> = state
            .iter()
            .chain(&next)
            .chain(&probe)
            .chain(&impulse)
            .copied()
            .collect();
        let mut z = self.to_z0.forward(&context)?;
        let z0 = z.clone();
        let mut p = self.to_p0.forward(&context)?;
        let landscape = self.landscape(&options, mask, qtype, &probe)?;
        let mut h = vec![0.0; self.config.ssm_state];
        let mut settled = false;
        let mut steps_used = self.config.n_steps;
        let (mut potential, mut grad, mut q) = self.potential(&z, &landscape, mask)?;
        let mut gn = norm(&grad);
        finite("initial gradient norm", &[gn])?;
        for step in 0..self.config.n_steps {
            let x: Vec<_> = z
                .iter()
                .chain(&p)
                .chain(&grad)
                .copied()
                .chain(std::iter::once(gn))
                .collect();
            let u = self.ssm_in.forward(&x)?;
            let m = h.len();
            let dt = self.config.dt_max * sigmoid(u[m]);
            let gamma = softplus(u[m + 1]);
            for (i, value) in h.iter_mut().enumerate() {
                *value = (-self.a[i] * dt).exp() * *value + dt * u[i];
            }
            finite("SSM hidden state", &h)?;
            let mut force = self.ssm_out.forward(&h)?;
            for f in &mut force {
                *f = self.force_scale * f.tanh();
            }
            finite("SSM bounded force", &force)?;
            // Preserve upstream semantics: h keeps evolving after a halt, while
            // only position and momentum freeze. Settling is observed even when
            // halt=false, but only halt=true applies the freeze on later steps.
            if !halt || !settled {
                let decay = (-gamma * dt).exp();
                let soft_cap = 1.0 + gn / self.config.force_max;
                finite("integrator coefficients", &[dt, gamma, decay, soft_cap])?;
                for (j, momentum) in p.iter_mut().enumerate() {
                    *momentum = *momentum * decay - dt * (grad[j] / soft_cap) + dt * force[j];
                    z[j] += dt * *momentum;
                }
                finite("particle momentum", &p)?;
                finite("particle position", &z)?;
            }
            (potential, grad, q) = self.potential(&z, &landscape, mask)?;
            gn = norm(&grad);
            let kinetic = 0.5 * p.iter().map(|v| v * v).sum::<f32>();
            finite("settling readouts", &[gn, kinetic])?;
            if kinetic < self.config.settle_eps && gn < 10.0 * self.config.settle_eps && !settled {
                settled = true;
                steps_used = step + 1;
            }
        }
        let energies: Vec<_> = q
            .iter()
            .zip(&landscape.log_a)
            .zip(mask)
            .map(|((&q, &a), &enabled)| if enabled { q - a } else { 1e4 })
            .collect();
        finite("well energies", &energies)?;
        let nearest = q
            .iter()
            .zip(mask)
            .map(|(&q, &enabled)| if enabled { q } else { 1e4 })
            .fold(f32::INFINITY, f32::min);
        Ok(EngineReadout {
            logits: energies.iter().map(|e| -e).collect(),
            energies,
            z0,
            z_t: z,
            kinetic: 0.5 * p.iter().map(|v| v * v).sum::<f32>(),
            grad_norm: gn,
            nearest,
            potential,
            steps_used,
            settled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EngineConfig {
        EngineConfig {
            d_model: 2,
            feature_layers: vec![1, 3],
            option_layers: vec![3],
            latent_dim: 2,
            world_dim: 4,
            proj_hidden: 4,
            n_steps: 12,
            dt_max: 0.5,
            settle_eps: 0.01,
            force_max: 20.0,
            ssm_state: 2,
            memory_chunks: 0,
        }
    }

    fn zero_state(cfg: &EngineConfig) -> BTreeMap<String, TensorData> {
        cfg.expected_shapes()
            .into_iter()
            .map(|(name, shape)| {
                let count = element_count(&shape).unwrap();
                let value = if name.ends_with("net.0.weight")
                    || name == "foresight.norm.weight"
                    || name == "memory.kv.0.weight"
                    || name == "feat_sd"
                    || name == "opt_sd"
                {
                    1.0
                } else {
                    0.0
                };
                (
                    name,
                    TensorData {
                        shape,
                        values: vec![value; count],
                    },
                )
            })
            .collect()
    }

    fn input(cfg: &EngineConfig) -> FeatureBatch {
        FeatureBatch {
            batch_size: 1,
            max_options: 3,
            feats: vec![0.0; cfg.feature_layers.len() * 3 * cfg.d_model],
            opts: vec![0.0; 3 * cfg.option_layers.len() * cfg.d_model],
            opt_mask: vec![true, true, false],
            qtype: vec![QuestionType::Boolean],
            chunks: None,
        }
    }

    fn near(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn zero_checkpoint_has_exact_stationary_solution_and_masked_well() {
        let cfg = config();
        let engine = VegaEngine::from_tensors(cfg.clone(), zero_state(&cfg)).unwrap();
        let out = engine.forward(&input(&cfg), true).unwrap();
        let row = &out.rows[0];
        assert_eq!(row.z_t, [0.0, 0.0]);
        assert_eq!(row.logits, [0.0, 0.0, -1e4]);
        assert_eq!(row.energies, [0.0, 0.0, 1e4]);
        assert_eq!(row.kinetic, 0.0);
        assert_eq!(row.nearest, 0.0);
        assert_eq!(row.potential, -2.0);
        assert_eq!(row.steps_used, 1);
        assert!(row.settled);
    }

    #[test]
    fn mixing_uses_per_slot_layers_and_raw_standardization() {
        let cfg = config();
        let mut weights = zero_state(&cfg);
        weights.get_mut("mix").unwrap().values = vec![0.0, 3.0f32.ln(), 0.0, 0.0, 3.0f32.ln(), 0.0];
        weights.get_mut("feat_mu").unwrap().values.fill(1.0);
        weights.get_mut("feat_sd").unwrap().values.fill(2.0);
        weights.get_mut("opt_mu").unwrap().values.fill(2.0);
        weights.get_mut("opt_sd").unwrap().values.fill(4.0);
        let engine = VegaEngine::from_tensors(cfg, weights).unwrap();
        let (slots, options) = engine
            .mixed(
                &[
                    3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0, 17.0, 19.0, 21.0, 23.0, 25.0,
                ],
                &[6.0, 10.0, 14.0, 18.0],
                2,
            )
            .unwrap();
        for (got, expected) in slots.iter().flatten().zip([5.5, 6.5, 6.0, 7.0, 6.5, 7.5]) {
            near(*got, expected, 1e-6);
        }
        assert_eq!(options, [vec![1.0, 2.0], vec![3.0, 4.0]]);
    }

    #[test]
    fn one_step_matches_independent_f64_physics() {
        let mut cfg = config();
        cfg.n_steps = 1;
        let mut weights = zero_state(&cfg);
        weights.get_mut("to_z0.net.4.bias").unwrap().values = vec![0.4, -0.2];
        weights.get_mut("to_p0.net.4.bias").unwrap().values = vec![0.1, 0.3];
        weights.get_mut("log_kappa").unwrap().values[0] = 0.05f32.ln();
        let engine = VegaEngine::from_tensors(cfg.clone(), weights).unwrap();
        let row = engine.forward(&input(&cfg), true).unwrap().rows.remove(0);
        let z = [0.4f64, -0.2];
        let p = [0.1f64, 0.3];
        let variance = (0.3 + 2.0f64.ln()).powi(2);
        let q = (z[0] * z[0] + z[1] * z[1]) / (2.0 * variance);
        let gradient = z.map(|z| 0.05 * z + 2.0 * (-q).exp() * z / variance);
        let gn = (gradient[0].powi(2) + gradient[1].powi(2)).sqrt();
        let dt = 0.25;
        let next_p =
            [0, 1].map(|j| p[j] * (-2.0f64.ln() * dt).exp() - dt * gradient[j] / (1.0 + gn / 20.0));
        for j in 0..2 {
            near(row.z_t[j], (z[j] + dt * next_p[j]) as f32, 2e-7);
        }
        near(
            row.kinetic,
            (0.5 * (next_p[0].powi(2) + next_p[1].powi(2))) as f32,
            2e-7,
        );
    }

    #[test]
    fn halt_freezes_after_first_settle_but_nonhalt_keeps_integrating() {
        let mut cfg = config();
        cfg.settle_eps = 10.0;
        let mut weights = zero_state(&cfg);
        weights.get_mut("to_p0.net.4.bias").unwrap().values = vec![0.1, 0.0];
        let engine = VegaEngine::from_tensors(cfg.clone(), weights.clone()).unwrap();
        let halted = engine.forward(&input(&cfg), true).unwrap();
        let moving = engine.forward(&input(&cfg), false).unwrap();
        cfg.n_steps = 1;
        let first = VegaEngine::from_tensors(cfg.clone(), weights)
            .unwrap()
            .forward(&input(&cfg), true)
            .unwrap();
        assert_eq!(halted.rows[0].z_t, first.rows[0].z_t);
        assert_eq!(halted.rows[0].steps_used, 1);
        assert_eq!(moving.rows[0].steps_used, 1);
        assert_ne!(moving.rows[0].z_t, halted.rows[0].z_t);
    }

    #[test]
    fn score_rail_centers_use_cumulative_gaps_and_ignore_masked_centers() {
        let cfg = config();
        let mut weights = zero_state(&cfg);
        weights.get_mut("rail_dir.net.4.bias").unwrap().values = vec![3.0, 4.0];
        let engine = VegaEngine::from_tensors(cfg, weights).unwrap();
        let land = engine
            .landscape(
                &[vec![0.0; 2], vec![0.0; 2], vec![0.0; 2]],
                &[true, true, false],
                QuestionType::Score,
                &[0.0; 2],
            )
            .unwrap();
        let half_gap = 2.0f32.ln() / 2.0;
        near(land.centers[0][0], -half_gap * 0.6, 1e-7);
        near(land.centers[0][1], -half_gap * 0.8, 1e-7);
        near(land.centers[1][0], half_gap * 0.6, 1e-7);
        near(land.centers[1][1], half_gap * 0.8, 1e-7);
    }

    #[test]
    fn memory_attention_respects_relative_position_and_all_masked_zero() {
        let mut cfg = config();
        cfg.memory_chunks = 2;
        let mut weights = zero_state(&cfg);
        // q=k=0 gives equal attention. Value halves are [1,2,3,4]
        // and [5,6,7,8], so the independent expected mean is [3,4,5,6].
        weights.get_mut("memory.pos").unwrap().values = vec![
            0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0,
        ];
        let identity = weights.get_mut("memory.out.weight").unwrap();
        for j in 0..4 {
            identity.values[j * 4 + j] = 1.0;
        }
        let engine = VegaEngine::from_tensors(cfg, weights).unwrap();
        let mem = engine.memory.as_ref().unwrap();
        assert_eq!(
            mem.forward(&[0.0; 4], &[0.0; 4], &[true, true], 2).unwrap(),
            [3.0, 4.0, 5.0, 6.0]
        );
        assert_eq!(
            mem.forward(&[0.0; 4], &[0.0; 4], &[false, false], 2)
                .unwrap(),
            [0.0; 4]
        );
        // With n=1, the first chunk maps to position floor(0.5*2)=1.
        assert_eq!(
            mem.forward(&[0.0; 4], &[0.0; 4], &[true, false], 2)
                .unwrap(),
            [5.0, 6.0, 7.0, 8.0]
        );
    }

    #[test]
    fn loader_rejects_unknown_missing_shape_nonfinite_and_bad_standardization() {
        let cfg = config();
        let mut weights = zero_state(&cfg);
        weights.insert(
            "gate_nets.0.weight".into(),
            TensorData {
                shape: vec![1],
                values: vec![0.0],
            },
        );
        assert!(matches!(
            VegaEngine::from_tensors(cfg.clone(), weights),
            Err(VegaError::UnsupportedAdapter(_))
        ));
        for damage in 0..4 {
            let mut weights = zero_state(&cfg);
            match damage {
                0 => {
                    weights.remove("mix");
                }
                1 => {
                    weights.get_mut("mix").unwrap().shape = vec![6];
                }
                2 => {
                    weights.get_mut("mix").unwrap().values[0] = f32::NAN;
                }
                _ => {
                    weights.get_mut("feat_sd").unwrap().values[0] = 0.0;
                }
            }
            assert!(VegaEngine::from_tensors(cfg.clone(), weights).is_err());
        }
        assert!(VegaEngine::require_base_adapters("auto").is_err());
        assert!(VegaEngine::require_base_adapters("on").is_err());
        assert!(VegaEngine::require_base_adapters("off").is_ok());
    }

    #[test]
    fn malformed_input_and_resource_overflows_fail_closed() {
        let cfg = config();
        let engine = VegaEngine::from_tensors(cfg.clone(), zero_state(&cfg)).unwrap();
        let mut batch = input(&cfg);
        batch.opts.pop();
        assert!(engine.forward(&batch, true).is_err());
        batch = input(&cfg);
        batch.feats[0] = f32::INFINITY;
        assert!(engine.forward(&batch, true).is_err());
        batch = input(&cfg);
        batch.opt_mask.fill(false);
        assert!(engine.forward(&batch, true).is_err());
        batch = input(&cfg);
        batch.batch_size = usize::MAX;
        assert!(engine.forward(&batch, true).is_err());
        let mut invalid = cfg;
        invalid.d_model = usize::MAX;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn exact_gelu_reference_values() {
        near(gelu(1.0), 0.8413447, 1e-7);
        near(gelu(-1.0), -0.15865526, 1e-7);
        assert_eq!(gelu(0.0), 0.0);
    }

    #[test]
    fn projector_matches_independently_expanded_f64_equations() {
        let mlp = Mlp {
            norm: Norm {
                weight: vec![2.0, 0.5],
                bias: vec![0.1, -0.2],
            },
            first: Linear {
                input: 2,
                weight: vec![1.0, 2.0, -3.0, 0.5],
                bias: vec![0.2, -0.4],
            },
            last: Linear {
                input: 2,
                weight: vec![1.0, -0.25],
                bias: vec![0.3],
            },
        };
        let n = 1.0 / 1.00001f64.sqrt();
        let normalized = [-2.0 * n + 0.1, 0.5 * n - 0.2];
        let hidden = [
            normalized[0] + 2.0 * normalized[1] + 0.2,
            -3.0 * normalized[0] + 0.5 * normalized[1] - 0.4,
        ];
        let gaussian = |x: f64| x * 0.5 * (1.0 + libm::erf(x / 2.0f64.sqrt()));
        let expected = gaussian(hidden[0]) - 0.25 * gaussian(hidden[1]) + 0.3;
        near(mlp.forward(&[1.0, 3.0]).unwrap()[0], expected as f32, 5e-7);
    }

    #[test]
    fn twelve_steps_with_nonzero_ssm_memory_match_independent_f64_integrator() {
        let cfg = config();
        let mut weights = zero_state(&cfg);
        weights.get_mut("to_z0.net.4.bias").unwrap().values = vec![0.4, -0.2];
        weights.get_mut("to_p0.net.4.bias").unwrap().values = vec![0.1, 0.3];
        weights.get_mut("log_kappa").unwrap().values[0] = 0.05f32.ln();
        weights.get_mut("A_log").unwrap().values = vec![0.7f32.ln(), 1.2f32.ln()];
        weights.get_mut("ssm_in.bias").unwrap().values = vec![0.2, -0.3, 0.4, -0.5];
        weights.get_mut("ssm_out.weight").unwrap().values = vec![0.5, -0.2, 0.1, 0.4];
        weights.get_mut("ssm_out.bias").unwrap().values = vec![0.05, -0.08];
        weights.get_mut("force_scale").unwrap().values[0] = 0.2;
        let engine = VegaEngine::from_tensors(cfg.clone(), weights).unwrap();
        let row = engine.forward(&input(&cfg), false).unwrap().rows.remove(0);
        let mut z = [0.4f64, -0.2];
        let mut p = [0.1f64, 0.3];
        let mut h = [0.0f64; 2];
        let width_squared = (0.3 + 2.0f64.ln()).powi(2);
        let dt = 0.5 / (1.0 + (-0.4f64).exp());
        let decay = (-(-0.5f64).exp().ln_1p() * dt).exp();
        for _ in 0..12 {
            let q = z.iter().map(|v| v * v).sum::<f64>() / (2.0 * width_squared);
            let gradient = z.map(|v| 0.05 * v + 2.0 * (-q).exp() * v / width_squared);
            let length = gradient.iter().map(|v| v * v).sum::<f64>().sqrt();
            h[0] = (-0.7 * dt).exp() * h[0] + dt * 0.2;
            h[1] = (-1.2 * dt).exp() * h[1] - dt * 0.3;
            let force = [
                0.2 * (0.5 * h[0] - 0.2 * h[1] + 0.05).tanh(),
                0.2 * (0.1 * h[0] + 0.4 * h[1] - 0.08).tanh(),
            ];
            for j in 0..2 {
                p[j] = p[j] * decay - dt * gradient[j] / (1.0 + length / 20.0) + dt * force[j];
                z[j] += dt * p[j];
            }
        }
        for (actual, expected) in row.z_t.iter().zip(z) {
            near(*actual, expected as f32, 2e-6);
        }
        near(
            row.kinetic,
            (0.5 * p.iter().map(|v| v * v).sum::<f64>()) as f32,
            2e-6,
        );
        let nearest = z.iter().map(|v| v * v).sum::<f64>() / (2.0 * width_squared);
        near(row.nearest, nearest as f32, 2e-6);
    }

    fn archive_fixture(cfg: &EngineConfig) -> (serde_json::Value, Vec<u8>) {
        let mut header = serde_json::Map::new();
        let mut payload = Vec::new();
        for (name, tensor) in zero_state(cfg) {
            let start = payload.len();
            for value in tensor.values {
                payload.extend(value.to_le_bytes());
            }
            header.insert(
                name,
                serde_json::json!({
                    "dtype": "F32", "shape": tensor.shape, "data_offsets": [start, payload.len()]
                }),
            );
        }
        (serde_json::Value::Object(header), payload)
    }

    #[test]
    fn strict_header_accepts_complete_fixture_without_real_weights() {
        let cfg = config();
        let (header, payload) = archive_fixture(&cfg);
        validate_weight_header(&cfg, &header, payload.len() as u64).unwrap();
    }

    #[test]
    fn header_rejects_missing_extra_shape_dtype_offsets_and_trailing_bytes() {
        let cfg = config();
        let (header, payload) = archive_fixture(&cfg);
        for damage in 0..6 {
            let mut broken = header.clone();
            match damage {
                0 => {
                    broken.as_object_mut().unwrap().remove("mix");
                }
                1 => {
                    broken["gate.extra"] = serde_json::json!({});
                }
                2 => {
                    broken["mix"]["shape"] = serde_json::json!([6]);
                }
                3 => {
                    broken["mix"]["dtype"] = serde_json::json!("I32");
                }
                4 => {
                    broken["mix"]["data_offsets"] = serde_json::json!([0, 24]);
                }
                _ => {
                    broken["mix"]["data_offsets"] = serde_json::json!([0, u64::MAX]);
                }
            }
            assert!(validate_weight_header(&cfg, &broken, payload.len() as u64).is_err());
        }
        assert!(validate_weight_header(&cfg, &header, payload.len() as u64 + 1).is_err());
    }
}
