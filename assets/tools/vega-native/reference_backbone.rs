//! Separately authored scalar-F64 Qwen3.5 equations from Transformers v5.3.0,
//! aad13b87ed59f2afcfaebc985f403301887a35fc/modeling_qwen3_5.py (Apache-2.0,
//! Qwen/HuggingFace). That source is a specification only: no Python execution.
//! No candidate backbone, CPU model, tensor kernels or pooling calls. Shared
//! JSON/SafeTensors/SHA256 infrastructure and same-agent authorship are limits.
use safetensors::{Dtype, SafeTensors, tensor::TensorView};
use serde_json::{Value, json};
use std::collections::BTreeSet;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const WIDTH: usize = 1024;
const PREFIX: &str = "model.language_model";
pub const MAX_REFERENCE_TOKENS: usize = 512;
const MODEL_HASH: &str = "sha256:04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696";
const CONFIG_HASH: &str = "sha256:b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204";
const ENGINE_CONFIG_HASH: &str =
    "sha256:12f4f73784d58532ef244288c425df24944a8bda9c8414aa7c0981dfd2d88494";
const SPEC_REVISION: &str = "aad13b87ed59f2afcfaebc985f403301887a35fc";
const SPEC_SHA256: &str = "0e7410f5251e5c324a5628c5340ee3551f7a6a505ce398569fcc3b5f3ac07f7a";

fn scalar(view: &TensorView<'_>, index: usize) -> Result<f64> {
    let value = match view.dtype() {
        Dtype::BF16 => {
            let bytes = view
                .data()
                .get(index * 2..index * 2 + 2)
                .ok_or("reference BF16 bounds")?;
            f32::from_bits(u32::from(u16::from_le_bytes(bytes.try_into()?)) << 16) as f64
        }
        Dtype::F32 => {
            let bytes = view
                .data()
                .get(index * 4..index * 4 + 4)
                .ok_or("reference F32 bounds")?;
            f32::from_le_bytes(bytes.try_into()?) as f64
        }
        _ => return Err("reference requires pinned BF16/F32 weights".into()),
    };
    if !value.is_finite() {
        return Err("nonfinite reference weight".into());
    }
    Ok(value)
}
struct Weights<'a>(SafeTensors<'a>);
impl Weights<'_> {
    fn view(&self, name: &str, shape: &[usize]) -> Result<TensorView<'_>> {
        let view = self.0.tensor(&format!("{PREFIX}.{name}"))?;
        if view.shape() != shape {
            return Err(format!("reference weight shape {name}").into());
        }
        Ok(view)
    }
    fn values(&self, name: &str, shape: &[usize]) -> Result<Vec<f64>> {
        let view = self.view(name, shape)?;
        (0..shape.iter().product())
            .map(|i| scalar(&view, i))
            .collect()
    }
    fn linear(&self, name: &str, input: &[f64], cols: usize, rows: usize) -> Result<Vec<f64>> {
        let matrix = self.values(&format!("{name}.weight"), &[rows, cols])?;
        linear(input, &matrix, cols, rows)
    }
    fn norm(&self, name: &str, input: &[f64], width: usize, centered: bool) -> Result<Vec<f64>> {
        rms(
            input,
            &self.values(&format!("{name}.weight"), &[width])?,
            centered,
        )
    }
}
fn finite(values: &[f64]) -> Result<()> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite reference activation".into());
    }
    Ok(())
}
fn linear(input: &[f64], weights: &[f64], cols: usize, rows: usize) -> Result<Vec<f64>> {
    if cols == 0
        || rows == 0
        || input.is_empty()
        || input.len() % cols != 0
        || weights.len() != rows.checked_mul(cols).ok_or("reference matrix overflow")?
    {
        return Err("reference linear shape".into());
    }
    let count = input.len() / cols;
    let mut output = vec![0.0; count.checked_mul(rows).ok_or("reference batch overflow")?];
    // Parallelize independent token rows only; every scalar reduction has the
    // same fixed F64 order. No shared GEMM implementation or detached threads.
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4)
        .min(count);
    let tokens_per_thread = count.div_ceil(threads);
    std::thread::scope(|scope| {
        for (part, out) in output.chunks_mut(tokens_per_thread * rows).enumerate() {
            let first = part * tokens_per_thread;
            scope.spawn(move || {
                for (local, row) in out.chunks_mut(rows).enumerate() {
                    let x = &input[(first + local) * cols..(first + local + 1) * cols];
                    for (target, weight) in row.iter_mut().zip(weights.chunks_exact(cols)) {
                        let mut sum = 0.0_f64;
                        for (&a, &b) in x.iter().zip(weight) {
                            sum = a.mul_add(b, sum);
                        }
                        *target = sum;
                    }
                }
            });
        }
    });
    finite(&output)?;
    Ok(output)
}
fn rms(input: &[f64], weight: &[f64], centered: bool) -> Result<Vec<f64>> {
    if weight.is_empty() || input.len() % weight.len() != 0 {
        return Err("reference RMS shape".into());
    }
    let mut output = input.to_vec();
    for row in output.chunks_exact_mut(weight.len()) {
        let variance = row.iter().map(|x| x * x).sum::<f64>() / weight.len() as f64;
        if !variance.is_finite() {
            return Err("nonfinite reference RMS variance".into());
        }
        let inv = (variance + 1e-6).sqrt().recip();
        for (x, w) in row.iter_mut().zip(weight) {
            *x *= inv * (w + if centered { 1.0 } else { 0.0 });
        }
    }
    finite(&output)?;
    Ok(output)
}
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}
fn silu(x: f64) -> f64 {
    x * sigmoid(x)
}
fn softplus(x: f64) -> f64 {
    x.max(0.0) + (-x.abs()).exp().ln_1p()
}
fn normalize_l2(x: &mut [f64]) -> Result<()> {
    let squared = x.iter().map(|v| v * v).sum::<f64>();
    if !squared.is_finite() {
        return Err("nonfinite reference L2 norm".into());
    }
    let inv = (squared + 1e-6).sqrt().recip();
    for v in x {
        *v *= inv;
    }
    Ok(())
}
fn causal_conv(input: &[f64], weight: &[f64], channels: usize, kernel: usize) -> Result<Vec<f64>> {
    if channels == 0
        || kernel == 0
        || input.len() % channels != 0
        || weight.len() != channels * kernel
    {
        return Err("reference convolution shape".into());
    }
    let mut output = vec![0.0; input.len()];
    for t in 0..input.len() / channels {
        for c in 0..channels {
            for j in 0..kernel {
                if t + j >= kernel - 1 {
                    output[t * channels + c] +=
                        input[(t + j + 1 - kernel) * channels + c] * weight[c * kernel + j];
                }
            }
        }
    }
    finite(&output)?;
    Ok(output)
}
fn delta_step(
    state: &mut [f64],
    q: &[f64],
    k: &[f64],
    v: &[f64],
    decay: f64,
    beta: f64,
) -> Result<Vec<f64>> {
    if q.is_empty()
        || q.len() != k.len()
        || v.is_empty()
        || state.len() != q.len() * v.len()
        || !decay.is_finite()
        || !(0.0..=1.0).contains(&decay)
        || !beta.is_finite()
        || !(0.0..=1.0).contains(&beta)
    {
        return Err("reference delta shape/physical input".into());
    }
    let mut residual = v.to_vec();
    for i in 0..k.len() {
        for j in 0..v.len() {
            state[i * v.len() + j] *= decay;
            residual[j] -= state[i * v.len() + j] * k[i];
        }
    }
    for value in &mut residual {
        *value *= beta;
    }
    let mut out = vec![0.0; v.len()];
    for i in 0..k.len() {
        for j in 0..v.len() {
            state[i * v.len() + j] += k[i] * residual[j];
            out[j] += state[i * v.len() + j] * q[i];
        }
    }
    finite(&out)?;
    Ok(out)
}
fn gated_delta(weights: &Weights<'_>, prefix: &str, x: &[f64]) -> Result<Vec<f64>> {
    let projected = weights.linear(&format!("{prefix}.in_proj_qkv"), x, WIDTH, 6144)?;
    let conv = weights.values(&format!("{prefix}.conv1d.weight"), &[6144, 1, 4])?;
    let mixed = causal_conv(&projected, &conv, 6144, 4)?
        .into_iter()
        .map(silu)
        .collect::<Vec<_>>();
    let z = weights.linear(&format!("{prefix}.in_proj_z"), x, WIDTH, 2048)?;
    let a = weights.linear(&format!("{prefix}.in_proj_a"), x, WIDTH, 16)?;
    let b = weights.linear(&format!("{prefix}.in_proj_b"), x, WIDTH, 16)?;
    let a_log = weights.values(&format!("{prefix}.A_log"), &[16])?;
    let dt = weights.values(&format!("{prefix}.dt_bias"), &[16])?;
    let mut state = vec![0.0; 16 * 128 * 128];
    let mut out = vec![0.0; x.len() / WIDTH * 2048];
    for t in 0..x.len() / WIDTH {
        for head in 0..16 {
            let base = t * 6144;
            let mut q = mixed[base + head * 128..base + (head + 1) * 128].to_vec();
            let mut k = mixed[base + 2048 + head * 128..base + 2048 + (head + 1) * 128].to_vec();
            let value = &mixed[base + 4096 + head * 128..base + 4096 + (head + 1) * 128];
            normalize_l2(&mut q)?;
            normalize_l2(&mut k)?;
            for q in &mut q {
                *q /= 128.0_f64.sqrt();
            }
            let decay = (-a_log[head].exp() * softplus(a[t * 16 + head] + dt[head])).exp();
            let y = delta_step(
                &mut state[head * 128 * 128..(head + 1) * 128 * 128],
                &q,
                &k,
                value,
                decay,
                sigmoid(b[t * 16 + head]),
            )?;
            out[t * 2048 + head * 128..t * 2048 + (head + 1) * 128].copy_from_slice(&y);
        }
    }
    let mut out = weights.norm(&format!("{prefix}.norm"), &out, 128, false)?;
    for (out, gate) in out.iter_mut().zip(z) {
        *out *= silu(gate);
    }
    weights.linear(&format!("{prefix}.out_proj"), &out, 2048, WIDTH)
}
fn split_query_gate(projected: &[f64], heads: usize, dim: usize) -> Result<(Vec<f64>, Vec<f64>)> {
    if heads == 0 || dim == 0 || projected.len() % (heads * dim * 2) != 0 {
        return Err("reference Q/gate layout".into());
    }
    let mut q = Vec::with_capacity(projected.len() / 2);
    let mut gate = Vec::with_capacity(projected.len() / 2);
    for head in projected.chunks_exact(2 * dim) {
        q.extend_from_slice(&head[..dim]);
        gate.extend_from_slice(&head[dim..]);
    }
    Ok((q, gate))
}
fn rope(x: &mut [f64], tokens: usize, heads: usize) -> Result<()> {
    if x.len() != tokens * heads * 256 {
        return Err("reference rotary shape".into());
    }
    // For pure text all three mRoPE position axes equal token position; the
    // [11,11,10] interleaving therefore reduces to this default 64-wide rotation.
    for t in 0..tokens {
        for head in 0..heads {
            for j in 0..32 {
                let angle = t as f64 / 10_000_000.0_f64.powf((2 * j) as f64 / 64.0);
                let (sin, cos) = angle.sin_cos();
                let base = (t * heads + head) * 256;
                let (a, b) = (x[base + j], x[base + j + 32]);
                x[base + j] = a * cos - b * sin;
                x[base + j + 32] = b * cos + a * sin;
            }
        }
    }
    Ok(())
}
fn attention(weights: &Weights<'_>, prefix: &str, x: &[f64]) -> Result<Vec<f64>> {
    let tokens = x.len() / WIDTH;
    let qg = weights.linear(&format!("{prefix}.q_proj"), x, WIDTH, 4096)?;
    let (q, gate) = split_query_gate(&qg, 8, 256)?;
    let mut q = weights.norm(&format!("{prefix}.q_norm"), &q, 256, true)?;
    let k = weights.linear(&format!("{prefix}.k_proj"), x, WIDTH, 512)?;
    let mut k = weights.norm(&format!("{prefix}.k_norm"), &k, 256, true)?;
    let v = weights.linear(&format!("{prefix}.v_proj"), x, WIDTH, 512)?;
    rope(&mut q, tokens, 8)?;
    rope(&mut k, tokens, 2)?;
    let mut out = vec![0.0; tokens * 2048];
    for t in 0..tokens {
        for head in 0..8 {
            let query = &q[(t * 8 + head) * 256..(t * 8 + head + 1) * 256];
            let kv_head = head / 4;
            let mut scores = Vec::with_capacity(t + 1);
            for previous in 0..=t {
                let key = &k[(previous * 2 + kv_head) * 256..(previous * 2 + kv_head + 1) * 256];
                scores.push(query.iter().zip(key).map(|(q, k)| q * k).sum::<f64>() / 16.0);
            }
            let maximum = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            for score in &mut scores {
                *score = (*score - maximum).exp();
            }
            let total = scores.iter().sum::<f64>();
            for (previous, score) in scores.iter().enumerate() {
                for j in 0..256 {
                    out[(t * 8 + head) * 256 + j] +=
                        score / total * v[(previous * 2 + kv_head) * 256 + j];
                }
            }
        }
    }
    for (out, gate) in out.iter_mut().zip(gate) {
        *out *= sigmoid(gate);
    }
    weights.linear(&format!("{prefix}.o_proj"), &out, 2048, WIDTH)
}
fn mlp(weights: &Weights<'_>, prefix: &str, x: &[f64]) -> Result<Vec<f64>> {
    let mut gate = weights.linear(&format!("{prefix}.gate_proj"), x, WIDTH, 3584)?;
    let up = weights.linear(&format!("{prefix}.up_proj"), x, WIDTH, 3584)?;
    for (gate, up) in gate.iter_mut().zip(up) {
        *gate = silu(*gate) * up;
    }
    weights.linear(&format!("{prefix}.down_proj"), &gate, 3584, WIDTH)
}
fn span(value: &Value, length: usize) -> Result<std::ops::Range<usize>> {
    let start = usize::try_from(
        value["start"]
            .as_u64()
            .ok_or("missing reference span start")?,
    )?;
    let end = usize::try_from(value["end"].as_u64().ok_or("missing reference span end")?)?;
    if start >= end || end > length {
        return Err("invalid reference pooling span".into());
    }
    Ok(start..end)
}
fn mean(hidden: &[f64], range: &std::ops::Range<usize>) -> Vec<f64> {
    let mut out = vec![0.0; WIDTH];
    for row in hidden[range.start * WIDTH..range.end * WIDTH].chunks_exact(WIDTH) {
        for (out, value) in out.iter_mut().zip(row) {
            *out += value;
        }
    }
    for out in &mut out {
        *out /= range.len() as f64;
    }
    out
}
// Independently follows vegaml21a17::_chunk_pool: assign each state token
// directly to floor(i*k/n), rather than calling candidate chunk_ranges.
fn memory_pool(
    hidden: &[f64],
    width: usize,
    state: &std::ops::Range<usize>,
    capacity: usize,
    minimum: usize,
    chunk_tokens: usize,
) -> Result<(Vec<f64>, Vec<bool>)> {
    if width == 0
        || capacity < 2
        || capacity > 16
        || chunk_tokens == 0
        || hidden.len() % width != 0
        || state.start >= state.end
        || state.end > hidden.len() / width
    {
        return Err("reference memory shape or profile".into());
    }
    let rows = &hidden[state.start * width..state.end * width];
    finite(rows)?;
    let mut pooled = vec![0.0; capacity * width];
    let mut mask = vec![false; capacity];
    let n = state.len();
    if n < minimum.max(2) {
        return Ok((pooled, mask));
    }
    let k = (n / chunk_tokens).min(capacity).max(2);
    let mut counts = vec![0usize; k];
    for (i, row) in rows.chunks_exact(width).enumerate() {
        let bucket = i.checked_mul(k).ok_or("reference memory index overflow")? / n;
        counts[bucket] += 1;
        for (out, value) in pooled[bucket * width..(bucket + 1) * width]
            .iter_mut()
            .zip(row)
        {
            *out += value;
        }
    }
    for bucket in 0..k {
        if counts[bucket] == 0 {
            return Err("reference empty active memory bucket".into());
        }
        for value in &mut pooled[bucket * width..(bucket + 1) * width] {
            *value /= counts[bucket] as f64;
        }
        mask[bucket] = true;
    }
    finite(&pooled)?;
    Ok((pooled, mask))
}

pub fn compute(
    config: &[u8],
    engine_config: &[u8],
    checkpoint: &[u8],
    prompt: &Value,
) -> Result<Value> {
    // Hashing is shared generic infrastructure, not a numerical oracle.
    if decapod::core::vega_readiness::hash(config) != CONFIG_HASH
        || decapod::core::vega_readiness::hash(engine_config) != ENGINE_CONFIG_HASH
        || checkpoint.len() != 1_746_942_600
        || decapod::core::vega_readiness::hash(checkpoint) != MODEL_HASH
    {
        return Err("reference immutable backbone identity differs".into());
    }
    let config: Value = serde_json::from_slice(config)?;
    let engine: Value = serde_json::from_slice(engine_config)?;
    if engine["memory_chunks"] != 16
        || engine["memory_min_tokens"] != 128
        || engine["memory_chunk_tokens"] != 128
        || engine["feature_layers"] != json!([13, 19])
        || engine["option_layers"] != json!([19])
    {
        return Err("reference immutable pooling profile differs".into());
    }
    let cfg = &config["text_config"];
    if cfg["hidden_size"] != 1024
        || cfg["num_hidden_layers"] != 24
        || cfg["num_attention_heads"] != 8
        || cfg["num_key_value_heads"] != 2
        || cfg["linear_num_key_heads"] != 16
        || cfg["linear_num_value_heads"] != 16
    {
        return Err("reference only supports pinned dense text profile".into());
    }
    let tokens = prompt["tokens"]
        .as_array()
        .ok_or("missing independent tokens")?;
    if tokens.is_empty() || tokens.len() > MAX_REFERENCE_TOKENS {
        return Err("reference text token scope exceeded".into());
    }
    let state = span(&prompt["spans"]["state"], tokens.len())?;
    let question = span(&prompt["spans"]["question"], tokens.len())?;
    let options = prompt["spans"]["options"]
        .as_array()
        .ok_or("missing independent options")?;
    if options.len() != 2 || prompt["spans"]["last"].as_u64() != Some((tokens.len() - 1) as u64) {
        return Err("reference boolean prompt layout".into());
    }
    let no = span(&options[0], tokens.len())?;
    let yes = span(&options[1], tokens.len())?;
    if state.end > question.start
        || question.end > no.start
        || no.end > yes.start
        || yes.end > tokens.len() - 1
    {
        return Err("overlapping independent prompt spans".into());
    }
    let weights = Weights(SafeTensors::deserialize(checkpoint)?);
    let embedding = weights.view("embed_tokens.weight", &[248320, WIDTH])?;
    let mut hidden = Vec::with_capacity(tokens.len() * WIDTH);
    for token in tokens {
        let token = usize::try_from(token.as_u64().ok_or("invalid independent token")?)?;
        if token >= 248320 {
            return Err("independent token outside vocabulary".into());
        }
        for j in 0..WIDTH {
            hidden.push(scalar(&embedding, token * WIDTH + j)?);
        }
    }
    let positions = BTreeSet::from([0, state.start, state.end - 1, tokens.len() - 1])
        .into_iter()
        .collect::<Vec<_>>();
    let mut captures = Vec::new();
    for index in 0..=19 {
        let prefix = format!("layers.{index}");
        let x = weights.norm(&format!("{prefix}.input_layernorm"), &hidden, WIDTH, true)?;
        let mixed = if index % 4 == 3 {
            attention(&weights, &format!("{prefix}.self_attn"), &x)?
        } else {
            gated_delta(&weights, &format!("{prefix}.linear_attn"), &x)?
        };
        for (hidden, mixed) in hidden.iter_mut().zip(mixed) {
            *hidden += mixed;
        }
        let normalized = weights.norm(
            &format!("{prefix}.post_attention_layernorm"),
            &hidden,
            WIDTH,
            true,
        )?;
        let feed_forward = mlp(&weights, &format!("{prefix}.mlp"), &normalized)?;
        for (hidden, value) in hidden.iter_mut().zip(feed_forward) {
            *hidden += value;
        }
        finite(&hidden)?;
        if index == 13 || index == 19 {
            captures.push(hidden.clone());
        }
    }
    let mut feats = Vec::new();
    let mut samples = Vec::new();
    for hidden in &captures {
        feats.extend(mean(hidden, &state));
        feats.extend(mean(hidden, &question));
        feats.extend_from_slice(&hidden[(tokens.len() - 1) * WIDTH..]);
        for &position in &positions {
            samples.extend_from_slice(&hidden[position * WIDTH..(position + 1) * WIDTH]);
        }
    }
    let mut opts = mean(&captures[1], &no);
    opts.extend(mean(&captures[1], &yes));
    let (chunks, chunk_mask) = memory_pool(&captures[1], WIDTH, &state, 16, 128, 128)?;
    Ok(
        json!({"pooled_feats":feats,"pooled_opts":opts,"pooled_chunks":chunks,
        "chunk_mask":chunk_mask,"sample_positions":positions,"sampled_hidden":samples,
        "reference_pooling_spec_revision":"21a17c90624aa85cec60b1092fa72cb8b720f71a",
        "reference_pooling_spec_sha256":"34f6fa580d30828bc4a4663fd0a6a870eee783d250a70a64cfdd1d683bb92cfc",
        "reference_engine_config_sha256":ENGINE_CONFIG_HASH,
        "feature_layers":[13,19],"option_layers":[19],"input_tokens":tokens.len(),
        "reference_spec_revision":SPEC_REVISION,"reference_spec_sha256":SPEC_SHA256,
        "reference_arithmetic":"Scalar F64; scoped token-row threads, no candidate model/tensor kernel calls",
        "scope":"Single pure-text sequence <=512 tokens, no padding/cache/image/adapter/chunked-state equivalence claim; raw block outputs before final norm",
        "independent_human_review":false}),
    )
}

/// Provisional tolerances were frozen before observing candidate model values.
/// This is subset evidence from an independent calculation, not a certificate
/// of all-context equivalence or independent human review of the reference.
pub fn compare(expected: &Value, observed: &Value) -> Result<Value> {
    for key in ["feature_layers", "option_layers", "sample_positions"] {
        if expected[key] != observed[key] {
            return Err(format!("reference {key} selection differs").into());
        }
    }
    let mut metrics = serde_json::Map::new();
    let mut passed = true;
    for key in [
        "pooled_feats",
        "pooled_opts",
        "pooled_chunks",
        "sampled_hidden",
    ] {
        let expected = expected[key].as_array().ok_or("missing reference vector")?;
        let observed = observed[key].as_array().ok_or("missing observed vector")?;
        if expected.is_empty() || expected.len() != observed.len() {
            return Err("backbone comparison shape differs".into());
        }
        let mut absolute = 0.0_f64;
        let mut relative = 0.0_f64;
        let mut squared_error = 0.0_f64;
        let mut squared_reference = 0.0_f64;
        let mut within = true;
        for (reference, value) in expected.iter().zip(observed) {
            let reference = reference
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid reference scalar")?;
            let value = value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or("invalid observed scalar")?;
            let error = (reference - value).abs();
            if !error.is_finite() {
                return Err("nonfinite backbone error".into());
            }
            absolute = absolute.max(error);
            relative = relative.max(error / reference.abs().max(1e-12));
            squared_error += error * error;
            squared_reference += reference * reference;
            within &= error <= 0.005 + 0.001 * reference.abs();
        }
        let normalized_rms = (squared_error / squared_reference.max(1e-30)).sqrt();
        if !normalized_rms.is_finite() {
            return Err("nonfinite backbone normalized error".into());
        }
        within &= normalized_rms <= 0.002;
        passed &= within;
        metrics.insert(key.into(),json!({"count":expected.len(),"maximum_absolute_error":absolute,
            "maximum_relative_error":relative,"normalized_rms_error":normalized_rms,"within_tolerance":within}));
    }
    if expected["chunk_mask"] != observed["chunk_mask"] {
        return Err("reference chunk mask differs".into());
    }
    Ok(
        json!({"status":if passed {"passed_backbone_subset"} else {"failed"},
        "reference_spec_revision":SPEC_REVISION,"reference_spec_sha256":SPEC_SHA256,
        "component_absolute_tolerance":0.005,"component_relative_tolerance":0.001,
        "normalized_rms_tolerance":0.002,"vectors":metrics,"independent_human_review":false,
        "scope":"Scalar-F64 mathematical specification, selected raw rows and pooled features for this short pure-text prompt only; no empirical Python/HF execution or long/chunked/cache/image equivalence claim"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_bucket_means_have_independent_analytic_anchors() {
        // Arithmetic-series means, including floor-bucket asymmetry at odd n.
        for (n, expected) in [
            (128, vec![31.5, 95.5]),
            (129, vec![32.0, 96.5]),
            (209, vec![52.0, 156.5]),
            (384, vec![63.5, 191.5, 319.5]),
            (512, vec![63.5, 191.5, 319.5, 447.5]),
        ] {
            let rows = (0..n).map(|i| i as f64).collect::<Vec<_>>();
            let (pooled, mask) = memory_pool(&rows, 1, &(0..n), 16, 128, 128).unwrap();
            assert_eq!(&pooled[..expected.len()], expected.as_slice());
            assert!(pooled[expected.len()..].iter().all(|v| *v == 0.0));
            assert!(mask[..expected.len()].iter().all(|v| *v));
            assert!(mask[expected.len()..].iter().all(|v| !*v));
        }
        let rows = (0..127).map(|i| i as f64).collect::<Vec<_>>();
        let (pooled, mask) = memory_pool(&rows, 1, &(0..127), 16, 128, 128).unwrap();
        assert_eq!(pooled, vec![0.0; 16]);
        assert_eq!(mask, vec![false; 16]);
    }
    #[test]
    fn memory_uses_only_state_rows_and_rejects_invalid_or_nonfinite_inputs() {
        let mut rows = vec![f64::NAN; 2];
        for i in 0..129 {
            rows.extend([i as f64, -(i as f64)]);
        }
        rows.extend([f64::INFINITY; 2]);
        let (pooled, mask) = memory_pool(&rows, 2, &(1..130), 16, 128, 128).unwrap();
        assert_eq!(&pooled[..4], &[32.0, -32.0, 96.5, -96.5]);
        assert_eq!(mask.iter().filter(|v| **v).count(), 2);
        rows[2] = f64::NAN;
        assert!(memory_pool(&rows, 2, &(1..130), 16, 128, 128).is_err());
        assert!(memory_pool(&[0.0; 128], 0, &(0..128), 16, 128, 128).is_err());
        assert!(memory_pool(&[0.0; 128], 1, &(0..129), 16, 128, 128).is_err());
        assert!(memory_pool(&[0.0; 128], 1, &(0..0), 16, 128, 128).is_err());
        assert!(memory_pool(&[0.0; 128], 1, &(0..128), 1, 128, 128).is_err());
        assert!(memory_pool(&[0.0; 128], 1, &(0..128), 16, 128, 0).is_err());
        assert!(memory_pool(&[f64::MAX; 128], 1, &(0..128), 16, 128, 128).is_err());
    }
    #[test]
    fn hand_anchored_projection_and_causal_convolution() {
        assert_eq!(
            linear(&[1.0, 2.0], &[3.0, 4.0, 5.0, 6.0], 2, 2).unwrap(),
            [11.0, 17.0]
        );
        assert_eq!(
            causal_conv(&[1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, 3.0, 4.0], 1, 4).unwrap(),
            [4.0, 11.0, 20.0, 30.0]
        );
        assert!(linear(&[], &[], 0, 0).is_err());
    }
    #[test]
    fn hand_anchored_zero_state_delta_and_decay() {
        let mut state = vec![0.0; 4];
        assert_eq!(
            delta_step(
                &mut state,
                &[1.0, 2.0],
                &[0.5, 1.0],
                &[2.0, -4.0],
                0.5,
                0.25
            )
            .unwrap(),
            [1.25, -2.5]
        );
        assert_eq!(
            delta_step(&mut state, &[1.0, 2.0], &[0.5, 1.0], &[9.0, 9.0], 0.5, 0.0).unwrap(),
            [0.625, -1.25]
        );
        assert!(
            delta_step(
                &mut state,
                &[1.0, 2.0],
                &[0.5, 1.0],
                &[2.0, -4.0],
                f64::NAN,
                0.25
            )
            .is_err()
        );
    }
    #[test]
    fn query_gate_is_interleaved_per_head_and_rotary_tail_is_untouched() {
        let input = (0..12).map(|v| v as f64).collect::<Vec<_>>();
        let (q, g) = split_query_gate(&input, 2, 3).unwrap();
        assert_eq!(q, [0.0, 1.0, 2.0, 6.0, 7.0, 8.0]);
        assert_eq!(g, [3.0, 4.0, 5.0, 9.0, 10.0, 11.0]);
        let mut x = vec![0.0; 512];
        x[0] = 1.0;
        x[256] = 1.0;
        x[256 + 100] = 7.0;
        rope(&mut x, 2, 1).unwrap();
        assert_eq!(x[0], 1.0);
        assert!((x[256] - 1.0_f64.cos()).abs() < 1e-14);
        assert!((x[256 + 32] - 1.0_f64.sin()).abs() < 1e-14);
        assert_eq!(x[256 + 100], 7.0);
    }
    #[test]
    fn normalization_rejects_nonfinite_and_preserves_zero() {
        let mut zero = [0.0, 0.0];
        normalize_l2(&mut zero).unwrap();
        assert_eq!(zero, [0.0, 0.0]);
        let y = rms(&[3.0, 4.0], &[1.0, -1.0], true).unwrap();
        assert!((y[0] - 6.0 / (12.5_f64 + 1e-6).sqrt()).abs() < 1e-14);
        assert_eq!(y[1], 0.0);
        assert!(rms(&[f64::NAN], &[1.0], false).is_err());
        assert!(rms(&[f64::MAX], &[1.0], false).is_err());
        assert!(normalize_l2(&mut [f64::MAX]).is_err());
    }
    #[test]
    fn comparison_reports_errors_and_rejects_bad_selection_or_nonfinite_values() {
        let expected = json!({"feature_layers":[13,19],"option_layers":[19],"sample_positions":[0],
            "pooled_feats":[1.0,2.0],"pooled_opts":[0.0],"pooled_chunks":[0.0],
            "sampled_hidden":[3.0],"chunk_mask":[false]});
        let mut observed = expected.clone();
        observed["pooled_feats"] = json!([1.0001, 1.9999]);
        assert_eq!(
            compare(&expected, &observed).unwrap()["status"],
            "passed_backbone_subset"
        );
        observed["pooled_feats"] = json!([2.0, 2.0]);
        assert_eq!(compare(&expected, &observed).unwrap()["status"], "failed");
        observed["pooled_feats"] = json!([null, 2.0]);
        assert!(compare(&expected, &observed).is_err());
        observed = expected.clone();
        observed["feature_layers"] = json!([12, 18]);
        assert!(compare(&expected, &observed).is_err());
    }
}
