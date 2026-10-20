//! Model-free Metal synchronization regression, not backbone/calibration proof.
//! Independent scalar F64 delta equations; no candidate CPU/tensor math oracle.
use serde_json::{Value, json};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn verify_source_identity() -> Result<()> {
    let source = include_bytes!("vendor/metal-kernels/src/gdn.metal");
    if decapod::core::vega_readiness::hash(source) != super::vega_backbone::METAL_GDN_SOURCE_SHA256
    {
        return Err("vendored Metal recurrence source/provenance differs".into());
    }
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn run() -> Result<Value> {
    use candle_core::{DType, Device, Tensor};
    verify_source_identity()?;
    // Fixed before patched-kernel observations: two SIMD groups per 64-thread
    // group on this Apple GPU; both value tiles participate at width128.
    const WIDTH: usize = 128;
    const TOKENS: usize = 257;
    const REPEATS: usize = 32;
    const ABS: f64 = 1e-6;
    const REL: f64 = 1e-5;
    let started = std::time::Instant::now();
    let device = Device::new_metal(0)?;
    let mut q = vec![0f32; TOKENS * WIDTH];
    let mut k = q.clone();
    let mut v = q.clone();
    let mut expected = vec![0f64; TOKENS * WIDTH];
    let mut state = vec![0f64; WIDTH * WIDTH];
    for t in 0..TOKENS {
        let key = (t * 7 + 3) % WIDTH;
        q[t * WIDTH + key] = 0.25;
        k[t * WIDTH + key] = 0.5;
        for lane in 0..WIDTH {
            v[t * WIDTH + lane] = ((t * 3 + lane) % 17) as f32 / 32.0 - 0.25;
        }
        // D=decay*S; delta=(v-D*k)*beta; S=D+k*delta; y=S*q.
        // This sparse, dyadic fixture permits an independent indexed scalar
        // calculation rather than calling either candidate/reference model.
        for lane in 0..WIDTH {
            for row in 0..WIDTH {
                state[row * WIDTH + lane] *= 0.5;
            }
            let delta = (f64::from(v[t * WIDTH + lane]) - state[key * WIDTH + lane] * 0.5) * 0.5;
            state[key * WIDTH + lane] += 0.5 * delta;
            expected[t * WIDTH + lane] = state[key * WIDTH + lane] * 0.25;
        }
    }
    let q = Tensor::from_vec(q, (TOKENS, 1, WIDTH), &device)?;
    let k = Tensor::from_vec(k, (TOKENS, 1, WIDTH), &device)?;
    let v = Tensor::from_vec(v, (TOKENS, 1, WIDTH), &device)?;
    let g = Tensor::from_vec(vec![0.5f32.ln(); TOKENS], (TOKENS, 1), &device)?;
    let beta = Tensor::from_vec(vec![0.5f32; TOKENS], (TOKENS, 1), &device)?;
    let slots = Tensor::from_vec(vec![0i64], (1,), &device)?;
    let boundaries = Tensor::from_vec(vec![0u32, TOKENS as u32], (2,), &device)?;
    let mut output_error = 0f64;
    let mut state_error = 0f64;
    for repeat in 0..REPEATS {
        let mut recurrent = Tensor::zeros((1, 1, WIDTH, WIDTH), DType::F32, &device)?;
        let output = attention_rs::gdn::gated_delta_rule_recurrence_varlen(
            &q,
            &k,
            &v,
            &g,
            &beta,
            &mut recurrent,
            &slots,
            &boundaries,
            None,
        )?;
        device.synchronize()?;
        let output = output.flatten_all()?.to_vec1::<f32>()?;
        let actual_state = recurrent.flatten_all()?.to_vec1::<f32>()?;
        if output.len() != expected.len() || actual_state.len() != state.len() {
            return Err("Metal recurrence output/state shape differs".into());
        }
        for (name, expected, actual, maximum) in [
            (
                "output",
                expected.as_slice(),
                output.as_slice(),
                &mut output_error,
            ),
            (
                "state",
                state.as_slice(),
                actual_state.as_slice(),
                &mut state_error,
            ),
        ] {
            for (index, (&reference, &actual)) in expected.iter().zip(actual).enumerate() {
                let error = (reference - f64::from(actual)).abs();
                *maximum = maximum.max(error);
                if !reference.is_finite()
                    || !actual.is_finite()
                    || !error.is_finite()
                    || error > ABS + REL * reference.abs()
                {
                    return Err(format!(
                        "Metal recurrence repeat{repeat} {name}[{index}] error{error} exceeds fixed bound"
                    ).into());
                }
            }
        }
    }
    Ok(
        json!({"status":"passed_scalar_recurrence","backend":"candle-metal",
        "tokens":TOKENS,"key_width":WIDTH,"value_width":WIDTH,"heads":1,
        "repetitions":REPEATS,"all_repetitions_required":true,"synchronized":true,
        "output_scalars_per_repetition":expected.len(),"final_state_scalars_per_repetition":state.len(),
        "maximum_output_absolute_error":output_error,"maximum_state_absolute_error":state_error,
        "absolute_tolerance":ABS,"relative_tolerance":REL,
        "metal_gdn_source_sha256":super::vega_backbone::METAL_GDN_SOURCE_SHA256,
        "wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
        "scope":"Sparse dyadic primitive and shared-memory synchronization; no model/backbone/calibration equivalence",
        "independent_human_review":false}),
    )
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub fn run() -> Result<Value> {
    verify_source_identity()?;
    Ok(
        json!({"status":"not_applicable","scope":"Metal-specific primitive; no hardware execution claimed"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patched_source_matches_reported_runtime_identity() {
        verify_source_identity().unwrap();
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn every_real_metal_repetition_matches_independent_scalar_delta() {
        assert_eq!(run().unwrap()["status"], "passed_scalar_recurrence");
    }
}
