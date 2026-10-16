//! Strict checkpoint decoding owned by the native worker. Portable tests use
//! synthetic fixtures; they do not establish model or Apple Metal parity.
use decapod::core::vega_engine::{
    EngineConfig, MAX_CHECKPOINT_BYTES, Result, TensorData, VegaEngine, VegaError,
};
use std::collections::BTreeMap;

/// Strict state-dict loading: every expected tensor exactly once, exact
/// dimensions, no unknown names (including adapter weights), finite floats.
/// Safetensors validates offsets and byte lengths before any tensor decoding.
pub fn from_safetensors(config: EngineConfig, bytes: &[u8]) -> Result<VegaEngine> {
    use safetensors::{Dtype, SafeTensors};
    config.validate()?;
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(VegaError::Checkpoint(
            "checkpoint byte limit exceeded".into(),
        ));
    }
    let archive =
        SafeTensors::deserialize(bytes).map_err(|e| VegaError::Checkpoint(e.to_string()))?;
    let expected = config.weight_shapes()?;
    if archive.len() != expected.len() {
        return Err(VegaError::Checkpoint(format!(
            "expected {} tensor names, received {}",
            expected.len(),
            archive.len()
        )));
    }
    let mut tensors = BTreeMap::new();
    for (name, shape) in expected {
        let tensor = archive
            .tensor(&name)
            .map_err(|e| VegaError::Checkpoint(format!("{name}: {e}")))?;
        if tensor.shape() != shape.as_slice() {
            return Err(VegaError::Checkpoint(format!(
                "{name}: expected shape {shape:?}, received {:?}",
                tensor.shape()
            )));
        }
        let values = match tensor.dtype() {
            Dtype::F32 => tensor
                .data()
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
            Dtype::BF16 => tensor
                .data()
                .chunks_exact(2)
                .map(|b| f32::from_bits((u16::from_le_bytes([b[0], b[1]]) as u32) << 16))
                .collect(),
            Dtype::F16 => tensor
                .data()
                .chunks_exact(2)
                .map(|b| half_to_f32(u16::from_le_bytes([b[0], b[1]])))
                .collect(),
            other => {
                return Err(VegaError::Checkpoint(format!(
                    "{name}: unsupported dtype {other:?}; expected F32/F16/BF16"
                )));
            }
        };
        tensors.insert(name, TensorData { shape, values });
    }
    VegaEngine::from_tensors(config, tensors)
}

// IEEE binary16 -> f32, exact (including subnormals); nonfinite values are
// rejected by state-dict validation rather than silently saturated.
fn half_to_f32(bits: u16) -> f32 {
    let sign = ((bits & 0x8000) as u32) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = (bits & 0x03ff) as u32;
    let out = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let shift = mantissa.leading_zeros() - 21;
            let normalized = (mantissa << shift) & 0x3ff;
            sign | ((113 - shift) << 23) | (normalized << 13)
        }
        31 => sign | 0x7f800000 | (mantissa << 13),
        _ => sign | (((exponent as u32) + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use decapod::core::vega_engine::{FeatureBatch, QuestionType, validate_weight_header};

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
        cfg.weight_shapes()
            .unwrap()
            .into_iter()
            .map(|(name, shape)| {
                let count = shape.iter().product();
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
    fn strict_header_and_safetensors_payload_load_without_real_weights() {
        let cfg = config();
        let (header, payload) = archive_fixture(&cfg);
        validate_weight_header(&cfg, &header, payload.len() as u64).unwrap();
        let mut json = serde_json::to_vec(&header).unwrap();
        while !json.len().is_multiple_of(8) {
            json.push(b' ');
        }
        let mut bytes = (json.len() as u64).to_le_bytes().to_vec();
        bytes.extend(&json);
        bytes.extend(&payload);
        let engine = from_safetensors(cfg.clone(), &bytes).unwrap();
        assert_eq!(
            engine.forward(&input(&cfg), true).unwrap().rows[0].potential,
            -2.0
        );
        // First payload tensor is A_log. Nonfinite bytes cannot become a model.
        bytes[8 + json.len()..12 + json.len()].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(from_safetensors(cfg, &bytes).is_err());
    }

    #[test]
    fn ieee_half_conversion_is_exact() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x0001), 2.0f32.powi(-24));
        assert_eq!(half_to_f32(0x03ff), 1023.0 * 2.0f32.powi(-24));
        assert_eq!(half_to_f32(0x0400), 2.0f32.powi(-14));
        assert!(half_to_f32(0x7c00).is_infinite());
        assert!(half_to_f32(0x7e00).is_nan());
    }
}
