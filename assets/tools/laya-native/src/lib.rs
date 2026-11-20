//! Native Laya numerical implementation, not a readiness authorization.
//!
//! Decision-head equations follow NandhaKishorM/laya `laya/common.py` at
//! 7a77439b0923d6efe00052d50a7f787840bfce98 (Apache-2.0). Evaluation has no
//! dropout, training, Python, ONNX export, or autoregressive generation.
use candle::{D, DType, Device, IndexOp, Result, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, VarBuilder, embedding, layer_norm, linear};

pub mod affine;
pub mod engine;
#[path = "../../../../src/decapod/core/laya_assets.rs"]
pub mod laya_assets;
pub mod modernbert;
#[path = "../../../../src/decapod/core/bounded_process.rs"]
pub mod process;
#[path = "../../../../src/decapod/core/laya_protocol.rs"]
pub mod protocol;
// The shared manifest module uses the same sibling name in the controller.
pub use protocol as laya_protocol;

/// Translate the pinned Transformers-5 RoPE keys without relying on defaults
/// from another encoder revision. Unsupported architecture modes are rejected.
pub fn backbone_config(bytes: &[u8]) -> Result<modernbert::Config> {
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(candle::Error::wrap)?;
    if value["model_type"] != "modernbert"
        || value["hidden_activation"] != "gelu"
        || ["attention_bias", "mlp_bias", "norm_bias"]
            .iter()
            .any(|field| value[*field] != false)
    {
        candle::bail!("unsupported Laya encoder architecture")
    }
    for (source, target) in [
        ("full_attention", "global_rope_theta"),
        ("sliding_attention", "local_rope_theta"),
    ] {
        let theta = value["rope_parameters"][source]["rope_theta"]
            .as_f64()
            .filter(|theta| theta.is_finite() && *theta > 0.0)
            .ok_or_else(|| candle::Error::Msg("missing pinned RoPE configuration".into()))?;
        value[target] = serde_json::json!(theta);
    }
    serde_json::from_value(value).map_err(candle::Error::wrap)
}

struct HeadLayer {
    norm1: LayerNorm,
    norm2: LayerNorm,
    qkv: Linear,
    output: Linear,
    linear1: Linear,
    linear2: Linear,
    heads: usize,
}

impl HeadLayer {
    fn load(vb: VarBuilder<'_>, width: usize) -> Result<Self> {
        let attention = vb.pp("self_attn");
        Ok(Self {
            norm1: layer_norm(width, 1e-5, vb.pp("norm1"))?,
            norm2: layer_norm(width, 1e-5, vb.pp("norm2"))?,
            qkv: Linear::new(
                attention.get((3 * width, width), "in_proj_weight")?,
                Some(attention.get(3 * width, "in_proj_bias")?),
            ),
            output: linear(width, width, attention.pp("out_proj"))?,
            linear1: linear(width, 4 * width, vb.pp("linear1"))?,
            linear2: linear(4 * width, width, vb.pp("linear2"))?,
            heads: width / 64,
        })
    }

    fn forward(&self, input: &Tensor, padding_bias: &Tensor) -> Result<Tensor> {
        let (batch, length, width) = input.dims3()?;
        let qkv = input
            .apply(&self.norm1)?
            .apply(&self.qkv)?
            .reshape((batch, length, 3, self.heads, 64))?
            .permute((2, 0, 3, 1, 4))?;
        let query = qkv.get(0)?.contiguous()?;
        let key = qkv.get(1)?.contiguous()?;
        let value = qkv.get(2)?.contiguous()?;
        let attention = (query.matmul(&key.t()?)? / 8.0)?.broadcast_add(padding_bias)?;
        let attention = candle_nn::ops::softmax_last_dim(&attention)?;
        let output = attention
            .matmul(&value)?
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch, length, width))?
            .apply(&self.output)?;
        let residual = (input + output)?;
        let feed_forward = residual
            .apply(&self.norm2)?
            .apply(&self.linear1)?
            .relu()?
            .apply(&self.linear2)?;
        residual + feed_forward
    }
}

/// Raw option and action logits; calibration and the Decapod gate remain separate.
pub struct Output {
    pub logits: Tensor,
    pub action_logits: Tensor,
    /// Frozen representation immediately before the final shared affine
    /// scorer. Explicit fitting diagnostics only; never publication authority.
    pub scoring_features: Tensor,
}

pub struct Model {
    backbone: modernbert::ModernBert,
    type_embedding: Embedding,
    head: Vec<HeadLayer>,
    scorer_norm: LayerNorm,
    scorer_hidden: Linear,
    scorer_output: Linear,
    action_hidden: Linear,
    action_output: Linear,
    width: usize,
    max_length: usize,
}

impl Model {
    pub fn scorer_weights(&self) -> Result<Vec<f32>> {
        Ok(self.scorer_output.weight().to_vec2::<f32>()?.remove(0))
    }
    pub fn scorer_bias(&self) -> Result<f32> {
        match self.scorer_output.bias() {
            Some(bias) => Ok(bias.to_vec1::<f32>()?[0]),
            None => Ok(0.0),
        }
    }

    /// A reviewed profile can replace only the existing final affine weight
    /// vector. Bias, encoder, type embedding, Transformer head and action head
    /// remain frozen. Caller verifies the externally pinned profile first.
    pub fn replace_scorer(&mut self, weights: &[f32]) -> Result<()> {
        if weights.len() != self.width || weights.iter().any(|w| !w.is_finite()) {
            candle::bail!("invalid fitted Laya scorer dimensions or values")
        }
        self.scorer_output = Linear::new(
            Tensor::from_vec(
                weights.to_vec(),
                (1, self.width),
                self.scorer_output.weight().device(),
            )?,
            self.scorer_output.bias().cloned(),
        );
        Ok(())
    }

    pub fn load(
        vb: VarBuilder<'_>,
        config: &modernbert::Config,
        head_layers: usize,
        max_length: usize,
    ) -> Result<Self> {
        if config.hidden_size == 0
            || config.hidden_size % 64 != 0
            || head_layers > 4
            || !matches!(max_length, 512 | 1024)
            || max_length > config.max_position_embeddings
        {
            candle::bail!("unsupported Laya decision-head dimensions")
        }
        let width = config.hidden_size;
        // Candle's standalone ModernBERT loader expects a `model.` prefix;
        // Laya stores the same backbone directly beneath `encoder.`.
        let encoder = vb.clone().rename_f(|name| {
            name.strip_prefix("model.")
                .map(|suffix| format!("encoder.{suffix}"))
                .unwrap_or_else(|| name.to_string())
        });
        Ok(Self {
            backbone: modernbert::ModernBert::load(encoder, config)?,
            type_embedding: embedding(3, width, vb.pp("type_emb"))?,
            head: (0..head_layers)
                .map(|index| HeadLayer::load(vb.pp(format!("head.layers.{index}")), width))
                .collect::<Result<_>>()?,
            scorer_norm: layer_norm(width, 1e-5, vb.pp("scorer.0"))?,
            scorer_hidden: linear(width, width, vb.pp("scorer.1"))?,
            scorer_output: linear(width, 1, vb.pp("scorer.3"))?,
            action_hidden: linear(width + 4, 256, vb.pp("act_head.0"))?,
            action_output: linear(256, 2, vb.pp("act_head.2"))?,
            width,
            max_length,
        })
    }

    pub fn forward(
        &self,
        ids: &Tensor,
        attention_mask: &Tensor,
        marker_positions: &Tensor,
        marker_mask: &Tensor,
        question_types: &Tensor,
    ) -> Result<Output> {
        let (batch, length) = ids.dims2()?;
        let (marker_batch, markers) = marker_positions.dims2()?;
        if batch == 0
            || length == 0
            || length > self.max_length
            || marker_batch != batch
            || markers == 0
        {
            candle::bail!("unsupported Laya input dimensions; no truncation is permitted")
        }
        let padding_bias = ((1.0 - attention_mask.to_dtype(DType::F32)?)? * f32::MIN as f64)?
            .reshape((batch, 1, 1, length))?;
        let types = question_types.apply(&self.type_embedding)?.unsqueeze(1)?;
        let mut hidden = self
            .backbone
            .forward(ids, attention_mask)?
            .broadcast_add(&types)?;
        for layer in &self.head {
            hidden = layer.forward(&hidden, &padding_bias)?;
        }
        let index = marker_positions
            .unsqueeze(2)?
            .broadcast_as((batch, markers, self.width))?
            .contiguous()?;
        let selected = hidden.gather(&index, 1)?;
        let scoring_features = selected
            .apply(&self.scorer_norm)?
            .apply(&self.scorer_hidden)?
            .gelu_erf()?;
        let raw = scoring_features
            .apply(&self.scorer_output)?
            .squeeze(D::Minus1)?;
        let valid = marker_mask.to_dtype(DType::F32)?;
        let logits = (raw.mul(&valid)? + ((1.0 - &valid)? * -10000.0)?)?;
        let probabilities = candle_nn::ops::softmax_last_dim(&logits)?.to_vec2::<f32>()?;
        let masks = valid.to_vec2::<f32>()?;
        let mut features = Vec::with_capacity(batch * 4);
        for (row, mask) in probabilities.iter().zip(&masks) {
            let mut ordered = row.clone();
            ordered.sort_by(|left, right| right.total_cmp(left));
            let top1 = ordered[0];
            let top2 = ordered.get(1).copied().unwrap_or(0.0);
            let count = mask.iter().sum::<f32>().max(2.0);
            let entropy = -row.iter().map(|p| p * p.max(1e-9).ln()).sum::<f32>() / count.ln();
            features.extend([top1, top1 - top2, entropy, count / 255.0]);
        }
        let features = Tensor::from_vec(features, (batch, 4), hidden.device())?;
        let pooled = hidden.i((.., 0, ..))?;
        let action_logits = Tensor::cat(&[pooled, features], 1)?
            .apply(&self.action_hidden)?
            .gelu_erf()?
            .apply(&self.action_output)?;
        Ok(Output {
            logits,
            action_logits,
            scoring_features,
        })
    }
}

/// Explicit device selection; a label is never a claim of model execution.
pub fn device(metal: bool) -> Result<Device> {
    if metal {
        // The pinned Candle constructor indexes its inventory before returning
        // a Result. An empty inventory (including sandbox-denied access) must
        // remain a typed unavailable backend, not a worker panic.
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if metal::Device::all().is_empty() {
            candle::bail!("Metal hardware unavailable: no accessible device")
        }
        Device::new_metal(0)
    } else {
        Ok(Device::Cpu)
    }
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod device_tests {
    #[test]
    fn actual_empty_metal_inventory_returns_error_instead_of_panicking() {
        // This exercises the actual environment. A successful enumeration is
        // not dispatch proof; published-logit tests exercise real kernels.
        if metal::Device::all().is_empty() {
            assert!(super::device(true).is_err());
        }
        assert!(super::device(false).unwrap().is_cpu());
    }
}
