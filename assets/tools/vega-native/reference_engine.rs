//! Separately authored scalar-f64 boolean engine reference from Apache-2.0
//! vegaml/_common.py@21a17c90624aa85cec60b1092fa72cb8b720f71a.
//! No candidate engine/decoder/backbone/calibration calls. Same-agent authorship
//! is not independent human review. Shared SafeTensors parsing is disclosed.
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Weights(BTreeMap<String, (Vec<usize>, Vec<f64>)>);
impl Weights {
    fn load(bytes: &[u8]) -> Result<Self> {
        let archive = safetensors::SafeTensors::deserialize(bytes)?;
        let mut values = BTreeMap::new();
        for (name, tensor) in archive.tensors() {
            let row: Vec<f64> = match tensor.dtype() {
                safetensors::Dtype::F32 => tensor
                    .data()
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
                    .collect(),
                safetensors::Dtype::BF16 => tensor
                    .data()
                    .chunks_exact(2)
                    .map(|b| {
                        f32::from_bits(u32::from(u16::from_le_bytes(b.try_into().unwrap())) << 16)
                            as f64
                    })
                    .collect(),
                _ => return Err("reference supports pinned F32/BF16 weights only".into()),
            };
            if row.iter().any(|v| !v.is_finite()) {
                return Err("nonfinite reference weight".into());
            }
            values.insert(name, (tensor.shape().to_vec(), row));
        }
        Ok(Self(values))
    }
    fn get(&self, name: &str) -> Result<&[f64]> {
        Ok(&self
            .0
            .get(name)
            .ok_or_else(|| format!("missing reference weight {name}"))?
            .1)
    }
    fn affine(&self, name: &str, input: &[f64]) -> Result<Vec<f64>> {
        let key = format!("{name}.weight");
        let (shape, weight) = self.0.get(&key).ok_or("missing affine weight")?;
        if shape.len() != 2 || shape[1] != input.len() {
            return Err("reference affine shape mismatch".into());
        }
        let bias = self.get(&format!("{name}.bias"))?;
        if bias.len() != shape[0] {
            return Err("reference bias shape mismatch".into());
        }
        Ok(weight
            .chunks_exact(input.len())
            .zip(bias)
            .map(|(row, b)| row.iter().zip(input).map(|(w, x)| w * x).sum::<f64>() + b)
            .collect())
    }
    fn normalize(&self, name: &str, input: &[f64]) -> Result<Vec<f64>> {
        if input.is_empty() {
            return Err("empty normalization".into());
        }
        let mean = input.iter().sum::<f64>() / input.len() as f64;
        let variance = input.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / input.len() as f64;
        let weight = self.get(&format!("{name}.weight"))?;
        let bias = self.get(&format!("{name}.bias"))?;
        if weight.len() != input.len() || bias.len() != input.len() {
            return Err("norm shape mismatch".into());
        }
        Ok(input
            .iter()
            .zip(weight)
            .zip(bias)
            .map(|((x, w), b)| (x - mean) / (variance + 1e-5).sqrt() * w + b)
            .collect())
    }
    fn mlp(&self, name: &str, input: &[f64]) -> Result<Vec<f64>> {
        let norm = self.normalize(&format!("{name}.net.0"), input)?;
        let hidden = self.affine(&format!("{name}.net.1"), &norm)?;
        self.affine(
            &format!("{name}.net.4"),
            &hidden.into_iter().map(gelu).collect::<Vec<_>>(),
        )
    }
}
fn gelu(x: f64) -> f64 {
    x * 0.5 * (1.0 + libm::erf(x / std::f64::consts::SQRT_2))
}
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}
fn softplus(x: f64) -> f64 {
    x.max(0.0) + (-x.abs()).exp().ln_1p()
}
fn softmax(xs: &[f64]) -> Vec<f64> {
    let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exp = xs.iter().map(|x| (x - max).exp()).collect::<Vec<_>>();
    let sum = exp.iter().sum::<f64>();
    exp.into_iter().map(|x| x / sum).collect()
}
fn array(value: &Value) -> Result<Vec<f64>> {
    value
        .as_array()
        .ok_or("reference input must be an array")?
        .iter()
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| "nonfinite reference input".into())
        })
        .collect()
}
fn usize_field(value: &Value, name: &str) -> Result<usize> {
    Ok(usize::try_from(
        value[name].as_u64().ok_or("missing reference integer")?,
    )?)
}

/// The caller supplies pooled tensors as INPUTS, not expected outputs. This
/// establishes only engine correspondence conditional on those input features.
pub fn boolean_engine(config: &Value, checkpoint: &[u8], proof: &Value) -> Result<Value> {
    let w = Weights::load(checkpoint)?;
    let d = usize_field(config, "d_model")?;
    let zdim = usize_field(config, "latent_dim")?;
    let sdim = usize_field(config, "world_dim")?;
    let steps = usize_field(config, "n_steps")?;
    let layers = config["feature_layers"]
        .as_array()
        .ok_or("missing layer list")?
        .len();
    let option_layers = config["option_layers"]
        .as_array()
        .ok_or("missing option layer list")?
        .len();
    if layers != 2 || option_layers != 1 || d != 1024 || zdim != 64 || sdim != 256 || steps != 12 {
        return Err("reference is deliberately restricted to the pinned boolean engine".into());
    }
    let feats = array(&proof["pooled_feats"])?;
    let opts = array(&proof["pooled_opts"])?;
    if feats.len() != layers * 3 * d || opts.len() != 2 * option_layers * d {
        return Err("pooled shape mismatch".into());
    }
    let fm = w.get("feat_mu")?;
    let fs = w.get("feat_sd")?;
    let om = w.get("opt_mu")?;
    let os = w.get("opt_sd")?;
    let mix = w.get("mix")?;
    let mut slots = vec![vec![0.0; d]; 3];
    for slot in 0..3 {
        let weights = softmax(&mix[slot * layers..(slot + 1) * layers]);
        for (layer, weight) in weights.iter().enumerate() {
            for feature in 0..d {
                let idx = (layer * 3 + slot) * d + feature;
                slots[slot][feature] += weight * (feats[idx] - fm[idx]) / fs[idx];
            }
        }
    }
    let o = (0..2)
        .map(|option| {
            (0..d)
                .map(|j| (opts[option * d + j] - om[j]) / os[j])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut world = w.mlp("world", &slots[0])?;
    let mask = proof["chunk_mask"].as_array().ok_or("missing chunk mask")?;
    if mask.iter().any(|v| v == true) {
        let chunks = array(&proof["pooled_chunks"])?;
        if mask.len() != 16 || chunks.len() != 16 * d {
            return Err("chunk shape mismatch".into());
        }
        let question = [slots[1].clone(), slots[2].clone()].concat();
        let query = w.mlp("memory.q", &question)?;
        let count = mask.iter().filter(|v| **v == true).count();
        let positions = w.get("memory.pos")?;
        let mut kv = Vec::new();
        for i in 0..16 {
            let x = (0..d)
                .map(|j| (chunks[i * d + j] - fm[3 * d + j]) / fs[3 * d + j])
                .collect::<Vec<_>>();
            let mut value = w.affine("memory.kv.1", &w.normalize("memory.kv.0", &x)?)?;
            let position = (((i as f64 + 0.5) / count.max(1) as f64 * 16.0) as usize).min(15);
            for j in 0..2 * sdim {
                value[j] += positions[position * 2 * sdim + j];
            }
            kv.push(value);
        }
        let dh = sdim / 4;
        let mut read = vec![0.0; sdim];
        for head in 0..4 {
            let logits = (0..16)
                .map(|i| {
                    if mask[i] != true {
                        -1e4
                    } else {
                        (0..dh)
                            .map(|j| query[head * dh + j] * kv[i][head * dh + j])
                            .sum::<f64>()
                            / (dh as f64).sqrt()
                    }
                })
                .collect::<Vec<_>>();
            let att = softmax(&logits);
            for j in 0..dh {
                read[head * dh + j] = (0..16).map(|i| att[i] * kv[i][sdim + head * dh + j]).sum();
            }
        }
        let memory = w.affine("memory.out", &read)?;
        for j in 0..sdim {
            world[j] += memory[j];
        }
    }
    let foresight = w.affine("foresight.inp", &w.normalize("foresight.norm", &world)?)?;
    let hidden = (0..2 * sdim)
        .map(|i| gelu(foresight[i]) * sigmoid(foresight[2 * sdim + i]))
        .collect::<Vec<_>>();
    let delta = w.affine("foresight.out", &hidden)?;
    let next = world
        .iter()
        .zip(delta)
        .map(|(s, d)| s + d)
        .collect::<Vec<_>>();
    let embedding = &w.get("qtype_emb.weight")?[2 * zdim..3 * zdim];
    let probe = w
        .mlp("probe", &slots[1])?
        .iter()
        .zip(embedding)
        .map(|(a, b)| a + b)
        .collect::<Vec<_>>();
    let impulse = w.mlp("impulse", &slots[2])?;
    let context = [world, next, probe.clone(), impulse].concat();
    let initial = w.mlp("to_z0", &context)?;
    let mut position = initial.clone();
    let mut momentum = w.mlp("to_p0", &context)?;
    let mut wells = Vec::new();
    for option in o {
        let params = w.mlp("well", &[option, probe.clone()].concat())?;
        wells.push((
            params[..zdim]
                .iter()
                .zip(embedding)
                .map(|(a, b)| a + b)
                .collect::<Vec<_>>(),
            params[zdim].clamp(-4.0, 4.0),
            0.3 + softplus(params[zdim + 1]),
        ));
    }
    let kappa = w.get("log_kappa")?[0].exp();
    let potential = |position: &[f64]| {
        let mut grad = position.iter().map(|x| kappa * x).collect::<Vec<_>>();
        let mut energy = kappa * 0.5 * position.iter().map(|x| x * x).sum::<f64>();
        let mut exponents = Vec::new();
        for (center, depth, width) in &wells {
            let q = position
                .iter()
                .zip(center)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                / (2.0 * width.powi(2));
            let gaussian = (depth - q).exp();
            energy -= gaussian;
            for j in 0..zdim {
                grad[j] += gaussian * (position[j] - center[j]) / width.powi(2);
            }
            exponents.push(q);
        }
        (energy, grad, exponents)
    };
    let mut memory = vec![0.0; w.get("A_log")?.len()];
    let mut settled = false;
    let mut used = steps;
    let dt_max = config["dt_max"].as_f64().ok_or("missing dt_max")?;
    let eps = config["settle_eps"].as_f64().ok_or("missing settle_eps")?;
    let force_max = config["force_max"].as_f64().unwrap_or(20.0);
    for step in 0..steps {
        let (_, grad, _) = potential(&position);
        let gn = grad.iter().map(|x| x * x).sum::<f64>().sqrt();
        let input = [position.clone(), momentum.clone(), grad.clone(), vec![gn]].concat();
        let control = w.affine("ssm_in", &input)?;
        let m = memory.len();
        let dt = dt_max * sigmoid(control[m]);
        let friction = softplus(control[m + 1]);
        for j in 0..m {
            memory[j] = (-w.get("A_log")?[j].exp() * dt).exp() * memory[j] + dt * control[j];
        }
        let force = w.affine("ssm_out", &memory)?;
        if !settled {
            for j in 0..zdim {
                momentum[j] = momentum[j] * (-friction * dt).exp()
                    - dt * grad[j] / (1.0 + gn / force_max)
                    + dt * w.get("force_scale")?[0] * force[j].tanh();
                position[j] += dt * momentum[j];
            }
        }
        let kin = 0.5 * momentum.iter().map(|x| x * x).sum::<f64>();
        let (_, gradient, _) = potential(&position);
        if !settled && kin < eps && gradient.iter().map(|x| x * x).sum::<f64>().sqrt() < 10.0 * eps
        {
            settled = true;
            used = step + 1;
        }
    }
    let (energy, gradient, exponents) = potential(&position);
    let logits = exponents
        .iter()
        .zip(&wells)
        .map(|(q, (_, a, _))| a - q)
        .collect::<Vec<_>>();
    Ok(json!({"logits":logits,"z0":initial,"z_t":position,
        "potential":energy,"grad_norm":gradient.iter().map(|x|x*x).sum::<f64>().sqrt(),
        "kinetic":0.5*momentum.iter().map(|x|x*x).sum::<f64>(),
        "nearest":exponents.into_iter().fold(f64::INFINITY,f64::min),"steps_used":used,"settled":settled}))
}

/// Calibration reference consumes a supplied engine readout, so it isolates
/// calibration arithmetic from backbone/engine differences.
pub fn calibrate(config: &Value, row: &Value, steps: usize) -> Result<Value> {
    let logits = array(&row["logits"])?
        .into_iter()
        .map(|x| x as f32 as f64)
        .collect::<Vec<_>>();
    if logits.len() != 2 || steps == 0 {
        return Err("reference requires boolean logits and steps".into());
    }
    let kinetic = row["kinetic"].as_f64().ok_or("missing kinetic")? as f32;
    let nearest = row["nearest"].as_f64().ok_or("missing nearest")? as f32;
    let used = usize_field(row, "steps_used")?;
    let features = [
        kinetic.ln_1p() as f64,
        nearest.ln_1p() as f64,
        (used as f32 / steps as f32) as f64,
        2_f32.ln() as f64,
    ];
    let theta = array(&config["calibration"]["theta"][2])?;
    let temperature = (theta[0]
        + theta[1..]
            .iter()
            .zip(features)
            .map(|(a, b)| a * b)
            .sum::<f64>())
    .clamp(0.2_f64.ln(), 20_f64.ln())
    .exp();
    let probabilities = softmax(&logits.iter().map(|v| v / temperature).collect::<Vec<_>>());
    let best = usize::from(probabilities[1] > probabilities[0]);
    let threshold = config["calibration"]["conformal"]["boolean"]["0.1"]
        .as_f64()
        .ok_or("missing conformal threshold")?;
    let abstain = config["calibration"]["abstain_threshold"]["0.1"]
        .as_f64()
        .ok_or("missing abstain threshold")?;
    let unbound = config["calibration"]["unbound_threshold"]
        .as_f64()
        .ok_or("missing unbound threshold")?;
    Ok(
        json!({"probabilities":probabilities,"temperature":temperature,
        "prediction_set":(0..2).filter(|i|*i==best || probabilities[*i]>=(1.0-threshold).max(1e-12)).collect::<Vec<_>>(),
        "abstain":probabilities[best]<abstain,"unbound":nearest as f64>unbound}),
    )
}

/// Fixed tolerances are documented before execution in the reference contract.
pub fn compare(config: &Value, checkpoint: &[u8], proof: &Value) -> Result<Value> {
    let reference = boolean_engine(config, checkpoint, proof)?;
    let observed = &proof["engine_readout"];
    let mut maximum = 0_f64;
    for name in ["logits", "z0", "z_t"] {
        let expected = array(&reference[name])?;
        let actual = array(&observed[name])?;
        if expected.len() != actual.len() {
            return Err("reference comparison shape mismatch".into());
        }
        for (expected, actual) in expected.iter().zip(actual) {
            let error = (expected - actual).abs();
            maximum = maximum.max(error);
            if error > 0.001 + 0.0001 * expected.abs() {
                return Err(format!(
                    "reference engine {name} error {error} exceeds frozen tolerance"
                )
                .into());
            }
        }
    }
    for name in ["potential", "grad_norm", "kinetic", "nearest"] {
        let expected = reference[name]
            .as_f64()
            .ok_or("missing reference readout")?;
        let actual = observed[name].as_f64().ok_or("missing observed readout")?;
        let error = (expected - actual).abs();
        maximum = maximum.max(error);
        if error > 0.001 + 0.0001 * expected.abs() {
            return Err(
                format!("reference engine {name} error {error} exceeds frozen tolerance").into(),
            );
        }
    }
    if reference["steps_used"] != observed["steps_used"]
        || reference["settled"] != observed["settled"]
    {
        return Err("reference settle/step mismatch".into());
    }
    let answer = calibrate(config, observed, 12)?;
    let actual = &proof["calibrated_answer"];
    for name in ["abstain", "unbound", "prediction_set"] {
        if answer[name] != actual[name] {
            return Err(format!("reference calibration {name} mismatch").into());
        }
    }
    let expected = array(&answer["probabilities"])?;
    let observed = array(&actual["probabilities"])?;
    if expected
        .iter()
        .zip(observed)
        .any(|(a, b)| (a - b).abs() > 1e-10)
        || (answer["temperature"]
            .as_f64()
            .ok_or("missing temperature")?
            - actual["temperature"]
                .as_f64()
                .ok_or("missing observed temperature")?)
        .abs()
            > 1e-10
    {
        return Err("reference calibrated probability/temperature mismatch".into());
    }
    Ok(
        json!({"status":"passed_engine_only","engine_max_absolute_error":maximum,"engine_absolute_tolerance":0.001,
        "engine_relative_tolerance":0.0001,"calibration_absolute_tolerance":1e-10,
        "scope":"Separate scalar F64 engine and calibration; pooled features are inputs, not verified backbone outputs",
        "backbone_parity":"unverified","independent_human_review":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_calibration_tie_and_unbound_boundary() {
        let config = json!({"calibration":{"theta":[[0.,0.,0.,0.,0.],[0.,0.,0.,0.,0.],[0.,0.,0.,0.,0.]],
            "conformal":{"boolean":{"0.1":0.25}},"abstain_threshold":{"0.1":0.6},"unbound_threshold":1.0}});
        let row = json!({"logits":[0.,0.],"kinetic":0.,"nearest":1.,"steps_used":12});
        let result = calibrate(&config, &row, 12).unwrap();
        assert_eq!(result["probabilities"], json!([0.5, 0.5]));
        assert_eq!(result["temperature"], 1.0);
        assert_eq!(result["prediction_set"], json!([0]));
        assert_eq!(result["abstain"], true);
        assert_eq!(result["unbound"], false);
        assert!(calibrate(&config, &row, 0).is_err());
        assert!(calibrate(&config, &json!({"logits":[null,0.]}), 12).is_err());
    }
}
