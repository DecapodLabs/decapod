//! Text-only CPU prefill for the pinned dense Qwen3.5-0.8B profile.
//!
//! Architecture/weight names follow candle-vllm 2c2c5293, qwen3_5.rs and
//! layers/{attention,deltanet}.rs, and attention.rs 5b4d645e (MIT). This is a
//! separate CPU implementation because those pinned GDN kernels require a GPU.
//! It is NOT an independently reviewed reference or evidence of GPU parity.
//! Only raw decoder blocks 0..=19 are needed: no final norm, LM head or later
//! blocks participate in pooled layers 13/19. All arithmetic is F32; the same
//! verified BF16 checkpoint is upcast. No cached recurrent state survives a call.

use super::{
    BOOLEAN_OPTIONS, BackboneError, D_MODEL, FEATURE_LAYERS, MEMORY_CHUNKS, PooledFeatures, Prompt,
    Result, VOCAB_SIZE, chunk_ranges, read_profile_config, validate_input,
};
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{Embedding, Linear, VarBuilder};
use std::path::{Path, PathBuf};

const EPS: f64 = 1e-6;
const LINEAR_HEADS: usize = 16;
const LINEAR_DIM: usize = 128;
const LINEAR_WIDTH: usize = LINEAR_HEADS * LINEAR_DIM;
const CONV_WIDTH: usize = 3 * LINEAR_WIDTH;

fn compute(error: candle_core::Error) -> BackboneError {
    BackboneError::Compute(format!("CPU: {error}"))
}
fn vector(tensor: &Tensor) -> candle_core::Result<Vec<f32>> {
    tensor.flatten_all()?.to_vec1::<f32>()
}
fn linear(
    vb: VarBuilder<'_>,
    name: &str,
    input: usize,
    output: usize,
) -> candle_core::Result<Linear> {
    candle_nn::linear_no_bias(input, output, vb.pp(name))
}
fn norm(xs: &Tensor, weight: &Tensor) -> candle_core::Result<Tensor> {
    candle_nn::ops::rms_norm(xs, weight, EPS as f32)
}
fn offset_norm(vb: VarBuilder<'_>, name: &str, width: usize) -> candle_core::Result<Tensor> {
    vb.pp(name).get(width, "weight")?.affine(1.0, 1.0)
}

struct FullAttention {
    q: Linear,
    k: Linear,
    v: Linear,
    output: Linear,
    q_norm: Tensor,
    k_norm: Tensor,
}
impl FullAttention {
    fn load(vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        Ok(Self {
            q: linear(vb.clone(), "q_proj", D_MODEL, 4096)?,
            k: linear(vb.clone(), "k_proj", D_MODEL, 512)?,
            v: linear(vb.clone(), "v_proj", D_MODEL, 512)?,
            output: linear(vb.clone(), "o_proj", 2048, D_MODEL)?,
            q_norm: offset_norm(vb.clone(), "q_norm", 256)?,
            k_norm: offset_norm(vb, "k_norm", 256)?,
        })
    }
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let n = xs.dim(0)?;
        let q_gate = self.q.forward(xs)?.reshape((n, 8, 512))?;
        let q = norm(&q_gate.narrow(2, 0, 256)?.contiguous()?, &self.q_norm)?;
        let gate = q_gate.narrow(2, 256, 256)?.reshape((n, 2048))?;
        let k = norm(&self.k.forward(xs)?.reshape((n, 2, 256))?, &self.k_norm)?;
        let v = self.v.forward(xs)?.reshape((n, 2, 256))?;
        let q = rope(&q, n, 8)?.transpose(0, 1)?.contiguous()?;
        let k = rope(&k, n, 2)?.transpose(0, 1)?.contiguous()?;
        let v = v.transpose(0, 1)?.contiguous()?;
        // Grouped query attention: heads [0..4) share KV head 0, [4..8) head 1.
        let mut heads = Vec::with_capacity(8);
        let mask = (0..n)
            .flat_map(|row| (0..n).map(move |col| if col > row { f32::NEG_INFINITY } else { 0.0 }))
            .collect::<Vec<_>>();
        let mask = Tensor::from_vec(mask, (n, n), &Device::Cpu)?;
        for head in 0..8 {
            let scores = q
                .get(head)?
                .matmul(&k.get(head / 4)?.t()?)?
                .affine(1.0 / 16.0, 0.0)?;
            let probabilities = candle_nn::ops::softmax_last_dim(&(&scores + &mask)?)?;
            heads.push(probabilities.matmul(&v.get(head / 4)?)?.unsqueeze(1)?);
        }
        let output = Tensor::cat(&heads, 1)?.reshape((n, 2048))?;
        self.output
            .forward(&output.mul(&candle_nn::ops::sigmoid(&gate)?)?)
    }
}

/// Text-only mRoPE: all three position axes equal the token index. Their
/// interleaved sections therefore reduce to the same partial, half-split RoPE.
fn rope(xs: &Tensor, tokens: usize, heads: usize) -> candle_core::Result<Tensor> {
    let mut rows = vector(xs)?;
    for token in 0..tokens {
        for head in 0..heads {
            let start = (token * heads + head) * 256;
            for j in 0..32 {
                let frequency = 1_f32 / 10_000_000_f64.powf((2 * j) as f64 / 64.0) as f32;
                let angle = token as f32 * frequency;
                let (sin, cos) = angle.sin_cos();
                let (a, b) = (rows[start + j], rows[start + j + 32]);
                rows[start + j] = a * cos - b * sin;
                rows[start + j + 32] = b * cos + a * sin;
            }
        }
    }
    Tensor::from_vec(rows, (tokens, heads, 256), &Device::Cpu)
}

struct DeltaAttention {
    qkv: Linear,
    z: Linear,
    b: Linear,
    a: Linear,
    output: Linear,
    conv: Vec<f32>,
    conv_bias: Vec<f32>,
    a_log: Vec<f32>,
    dt_bias: Vec<f32>,
    norm: Vec<f32>,
    norm_bias: Vec<f32>,
}
impl DeltaAttention {
    fn load(vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        let conv = match vb.get((CONV_WIDTH, 1, 4), "conv1d.weight") {
            Ok(weight) => weight,
            Err(_) => vb
                .get((CONV_WIDTH, 4, 1), "conv1d.weight")?
                .permute((0, 2, 1))?,
        };
        // Missing optional bias is allowed; malformed present bias is an error.
        let optional = |name: &str, width: usize| -> candle_core::Result<Vec<f32>> {
            if vb.contains_tensor(name) {
                vector(&vb.get(width, name)?)
            } else {
                Ok(vec![0.0; width])
            }
        };
        Ok(Self {
            qkv: linear(vb.clone(), "in_proj_qkv", D_MODEL, CONV_WIDTH)?,
            z: linear(vb.clone(), "in_proj_z", D_MODEL, LINEAR_WIDTH)?,
            b: linear(vb.clone(), "in_proj_b", D_MODEL, LINEAR_HEADS)?,
            a: linear(vb.clone(), "in_proj_a", D_MODEL, LINEAR_HEADS)?,
            output: linear(vb.clone(), "out_proj", LINEAR_WIDTH, D_MODEL)?,
            conv: vector(&conv)?,
            conv_bias: optional("conv1d.bias", CONV_WIDTH)?,
            a_log: vector(&vb.get(LINEAR_HEADS, "A_log")?)?,
            dt_bias: vector(&vb.get(LINEAR_HEADS, "dt_bias")?)?,
            norm: vector(&vb.get(LINEAR_DIM, "norm.weight")?)?,
            norm_bias: optional("norm.bias", LINEAR_DIM)?,
        })
    }
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let n = xs.dim(0)?;
        let input = vector(&self.qkv.forward(xs)?)?;
        let z = vector(&self.z.forward(xs)?)?;
        let b = vector(&self.b.forward(xs)?)?;
        let a = vector(&self.a.forward(xs)?)?;
        let mut convolved = vec![0.0; n * CONV_WIDTH];
        for token in 0..n {
            for channel in 0..CONV_WIDTH {
                let mut value = self.conv_bias[channel];
                for tap in 0..4 {
                    if token + tap >= 3 {
                        value += input[(token + tap - 3) * CONV_WIDTH + channel]
                            * self.conv[channel * 4 + tap];
                    }
                }
                convolved[token * CONV_WIDTH + channel] = silu(value);
            }
        }
        let mut state = vec![0.0; LINEAR_HEADS * LINEAR_DIM * LINEAR_DIM];
        let mut output = vec![0.0; n * LINEAR_WIDTH];
        for token in 0..n {
            for head in 0..LINEAR_HEADS {
                let q_start = token * CONV_WIDTH + head * LINEAR_DIM;
                let k_start = q_start + LINEAR_WIDTH;
                let v_start = k_start + LINEAR_WIDTH;
                let q = l2(&convolved[q_start..q_start + LINEAR_DIM]);
                let k = l2(&convolved[k_start..k_start + LINEAR_DIM]);
                let index = token * LINEAR_HEADS + head;
                let delta_t = a[index] + self.dt_bias[head];
                let softplus = if delta_t > 20.0 {
                    delta_t
                } else {
                    delta_t.exp().ln_1p()
                };
                let decay = (-self.a_log[head].exp() * softplus).exp();
                let beta = sigmoid(b[index]);
                let state = &mut state
                    [head * LINEAR_DIM * LINEAR_DIM..(head + 1) * LINEAR_DIM * LINEAR_DIM];
                let result = &mut output[(token * LINEAR_HEADS + head) * LINEAR_DIM
                    ..(token * LINEAR_HEADS + head + 1) * LINEAR_DIM];
                delta_step(
                    &q,
                    &k,
                    &convolved[v_start..v_start + LINEAR_DIM],
                    decay,
                    beta,
                    state,
                    result,
                );
                let inv = (result.iter().map(|x| x * x).sum::<f32>() / LINEAR_DIM as f32
                    + EPS as f32)
                    .sqrt()
                    .recip();
                for j in 0..LINEAR_DIM {
                    result[j] = (result[j] * inv * self.norm[j] + self.norm_bias[j])
                        * silu(z[(token * LINEAR_HEADS + head) * LINEAR_DIM + j]);
                }
            }
        }
        self.output
            .forward(&Tensor::from_vec(output, (n, LINEAR_WIDTH), &Device::Cpu)?)
    }
}
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
fn silu(x: f32) -> f32 {
    x * sigmoid(x)
}
fn l2(xs: &[f32]) -> Vec<f32> {
    let inverse = (xs.iter().map(|x| x * x).sum::<f32>() + EPS as f32)
        .sqrt()
        .recip();
    xs.iter().map(|x| x * inverse).collect()
}
/// Scalar recurrent update. The pinned caller scales normalized q by 1/sqrt(K)
/// before the recurrence; decayed state participates in both delta and output.
fn delta_step(
    q: &[f32],
    k: &[f32],
    v: &[f32],
    decay: f32,
    beta: f32,
    state: &mut [f32],
    output: &mut [f32],
) {
    let (keys, values) = (q.len(), v.len());
    let scale = 1.0 / (keys as f32).sqrt();
    for value in 0..values {
        let mut memory = 0.0;
        for key in 0..keys {
            state[key * values + value] *= decay;
            memory += state[key * values + value] * k[key];
        }
        let delta = (v[value] - memory) * beta;
        let mut result = 0.0;
        for key in 0..keys {
            state[key * values + value] += k[key] * delta;
            result += state[key * values + value] * (q[key] * scale);
        }
        output[value] = result;
    }
}

enum Attention {
    Full(FullAttention),
    Delta(DeltaAttention),
}
struct Layer {
    attention: Attention,
    input_norm: Tensor,
    post_norm: Tensor,
    gate: Linear,
    up: Linear,
    down: Linear,
}
impl Layer {
    fn load(vb: VarBuilder<'_>, index: usize) -> candle_core::Result<Self> {
        Ok(Self {
            attention: if (index + 1) % 4 == 0 {
                Attention::Full(FullAttention::load(vb.pp("self_attn"))?)
            } else {
                Attention::Delta(DeltaAttention::load(vb.pp("linear_attn"))?)
            },
            input_norm: offset_norm(vb.clone(), "input_layernorm", D_MODEL)?,
            post_norm: offset_norm(vb.clone(), "post_attention_layernorm", D_MODEL)?,
            gate: linear(vb.pp("mlp"), "gate_proj", D_MODEL, 3584)?,
            up: linear(vb.pp("mlp"), "up_proj", D_MODEL, 3584)?,
            down: linear(vb.pp("mlp"), "down_proj", 3584, D_MODEL)?,
        })
    }
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let normalized = norm(xs, &self.input_norm)?;
        let attention = match &self.attention {
            Attention::Full(attention) => attention.forward(&normalized)?,
            Attention::Delta(attention) => attention.forward(&normalized)?,
        };
        let residual = (xs + attention)?;
        let normalized = norm(&residual, &self.post_norm)?;
        let gate = candle_nn::ops::silu(&self.gate.forward(&normalized)?)?;
        let hidden = self
            .down
            .forward(&gate.mul(&self.up.forward(&normalized)?)?)?;
        residual + hidden
    }
}

pub struct CpuBackbone {
    embedding: Embedding,
    layers: Vec<Layer>,
}
impl CpuBackbone {
    /// Caller verifies the immutable installation before/after this mmap-backed
    /// load and bounds the worker process's lifetime, memory and output.
    pub fn load(config: &Path, weights: &[PathBuf]) -> Result<Self> {
        read_profile_config(config)?;
        if weights.len() != 1 {
            return Err(BackboneError::Checkpoint(
                "expected one pinned backbone shard".into(),
            ));
        }
        let metadata = std::fs::symlink_metadata(&weights[0])
            .map_err(|e| BackboneError::Checkpoint(e.to_string()))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 * 1024 {
            return Err(BackboneError::Checkpoint(
                "invalid bounded backbone shard".into(),
            ));
        }
        // SAFETY: the parent verifies and freezes these installed files. This
        // worker never writes their bytes and rechecks them before returning.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(weights, DType::F32, &Device::Cpu) }
            .map_err(compute)?;
        let prefixes = [
            "model",
            "model.language_model.model",
            "model.language_model",
            "model.text_model.model",
            "model.text_model",
            "language_model.model",
            "language_model",
            "text_model.model",
            "text_model",
            "",
        ];
        let prefix = prefixes
            .iter()
            .find(|prefix| {
                let key = if prefix.is_empty() {
                    "embed_tokens.weight".to_owned()
                } else {
                    format!("{prefix}.embed_tokens.weight")
                };
                vb.contains_tensor(&key)
            })
            .ok_or_else(|| BackboneError::Checkpoint("missing text backbone embedding".into()))?;
        let vb = if prefix.is_empty() { vb } else { vb.pp(prefix) };
        let embedding =
            candle_nn::embedding(VOCAB_SIZE, D_MODEL, vb.pp("embed_tokens")).map_err(compute)?;
        let layers = (0..=FEATURE_LAYERS[1])
            .map(|i| Layer::load(vb.pp(format!("layers.{i}")), i))
            .collect::<candle_core::Result<Vec<_>>>()
            .map_err(compute)?;
        Ok(Self { embedding, layers })
    }
    pub fn pooled(&mut self, prompt: &Prompt) -> Result<PooledFeatures> {
        validate_input(&prompt.tokens, &prompt.spans)?;
        let ids = Tensor::from_vec(prompt.tokens.clone(), prompt.tokens.len(), &Device::Cpu)
            .map_err(compute)?;
        let mut hidden = self.embedding.forward(&ids).map_err(compute)?;
        let mut captures = Vec::with_capacity(2);
        for (index, layer) in self.layers.iter().enumerate() {
            hidden = layer.forward(&hidden).map_err(compute)?;
            if FEATURE_LAYERS.contains(&index) {
                captures.push(hidden.clone());
            }
        }
        let mean = |hidden: &Tensor, span: &std::ops::Range<usize>| -> Result<Vec<f32>> {
            vector(
                &hidden
                    .narrow(0, span.start, span.len())
                    .and_then(|rows| rows.mean(0))
                    .map_err(compute)?,
            )
            .map_err(compute)
        };
        let mut feats = Vec::with_capacity(6 * D_MODEL);
        for hidden in &captures {
            feats.extend(mean(hidden, &prompt.spans.state)?);
            feats.extend(mean(hidden, &prompt.spans.question)?);
            feats
                .extend(vector(&hidden.get(prompt.spans.last).map_err(compute)?).map_err(compute)?);
        }
        let deepest = &captures[1];
        let mut opts = Vec::with_capacity(BOOLEAN_OPTIONS * D_MODEL);
        for span in &prompt.spans.options {
            opts.extend(mean(deepest, span)?);
        }
        let mut chunks = vec![0.0; MEMORY_CHUNKS * D_MODEL];
        let mut chunk_mask = vec![false; MEMORY_CHUNKS];
        for (index, range) in chunk_ranges(prompt.spans.state.len()).iter().enumerate() {
            let pooled = mean(
                deepest,
                &(prompt.spans.state.start + range.start..prompt.spans.state.start + range.end),
            )?;
            chunks[index * D_MODEL..(index + 1) * D_MODEL].copy_from_slice(&pooled);
            chunk_mask[index] = true;
        }
        if feats
            .iter()
            .chain(&opts)
            .chain(&chunks)
            .any(|x| !x.is_finite())
        {
            return Err(BackboneError::NonFinite("cpu pooled features"));
        }
        Ok(PooledFeatures {
            d_model: D_MODEL,
            option_count: BOOLEAN_OPTIONS,
            feats,
            opts,
            chunks,
            chunk_mask,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurrent_delta_uses_decayed_memory_and_scaled_query() {
        // Hand calculation with K=V=2, q=[sqrt(2),0], k=[1,0].
        // decay(.5)*state [[2,4],[6,8]] = [[1,2],[3,4]].
        // beta(.25)*(v[5,10]-memory[1,2])=[1,2]; updated row0=[2,4].
        let mut state = vec![2., 4., 6., 8.];
        let mut output = [0.; 2];
        delta_step(
            &[2_f32.sqrt(), 0.],
            &[1., 0.],
            &[5., 10.],
            0.5,
            0.25,
            &mut state,
            &mut output,
        );
        assert_eq!(state, [2., 4., 3., 4.]);
        for (actual, expected) in output.into_iter().zip([2., 4.]) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }
    #[test]
    fn partial_rope_preserves_tail_and_zero_position() {
        let data = (0..512).map(|x| x as f32 / 512.).collect::<Vec<_>>();
        let input = Tensor::from_vec(data.clone(), (2, 1, 256), &Device::Cpu).unwrap();
        let actual = vector(&rope(&input, 2, 1).unwrap()).unwrap();
        assert_eq!(&actual[..256], &data[..256]);
        assert_eq!(&actual[320..], &data[320..]);
        // At token1 and pair0 the independently defined rotation angle is1 radian.
        let expected = data[256] * 1_f32.cos() - data[288] * 1_f32.sin();
        assert!((actual[256] - expected).abs() < 1e-7);
    }
}
