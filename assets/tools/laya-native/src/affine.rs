//! Bounded final-layer fitting, not full-model training or a readiness oracle.
//! The encoder/head representation stays frozen. Fitting never reads evaluation
//! rows; the resulting external weight vector is separately pinned and tested.
use crate::protocol::RUBRIC;
use serde::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, String>;
pub const SCHEMA: &str = "decapod.laya-affine-readout.v1";
pub const REGULARIZATION: f64 = 0.001;
pub const MAX_ITERATIONS: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureRow {
    pub id: String,
    pub family: String,
    pub expected_ready: bool,
    pub state_sha256: String,
    pub input_tokens: usize,
    pub false_features: Vec<f32>,
    pub true_features: Vec<f32>,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    pub schema: String,
    pub partition: String,
    pub base_weights_sha256: String,
    pub rubric: String,
    pub inputs_sha256: String,
    pub definition_sha256: String,
    pub frozen_evaluation_definition_sha256: String,
    pub cases_code_sha256: String,
    pub feature_extractor_sha256: String,
    pub model_code_sha256: String,
    pub original_weights: Vec<f32>,
    pub rows: Vec<FeatureRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readout {
    pub schema: String,
    pub profile: String,
    pub base_weights_sha256: String,
    pub rubric: String,
    pub weights: Vec<f32>,
    pub preserve_original_bias: bool,
    pub fit_inputs_sha256: String,
    pub fit_definition_sha256: String,
    pub frozen_evaluation_definition_sha256: String,
    pub fitting_cases_code_sha256: String,
    pub feature_extractor_sha256: String,
    pub model_code_sha256: String,
    pub fitting_tool_sha256: String,
    pub fitting_families: Vec<String>,
    pub fitting_rows: usize,
    pub regularization: f64,
    pub iterations: usize,
    pub initial_loss: f64,
    pub fitted_loss: f64,
    pub training_errors: usize,
    pub accuracy_scope: String,
}

impl Readout {
    pub fn validate(&self, base_sha256: &str, width: usize) -> Result<()> {
        if self.schema != SCHEMA
            || self.rubric != RUBRIC
            || self.base_weights_sha256 != base_sha256
            || self.profile != "laya-multilingual-readiness-affine-v1"
            || !self.preserve_original_bias
            || self.weights.len() != width
            || width == 0
            || width > 1024
            || self
                .weights
                .iter()
                .any(|w| !w.is_finite() || w.abs() > 10000.0)
            || self.regularization != REGULARIZATION
            || self.iterations > MAX_ITERATIONS
        {
            return Err("unreviewed or malformed Laya affine profile".into());
        }
        Ok(())
    }
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}
fn loss(margin: f64, label: bool) -> f64 {
    margin.max(0.0) - if label { margin } else { 0.0 } + (-margin.abs()).exp().ln_1p()
}
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn objective_gradient(
    matrix: &[Vec<f64>],
    masses: &[f64],
    labels: &[bool],
    weights: &[f64],
    original: &[f64],
) -> (f64, Vec<f64>, Vec<f64>) {
    let mut objective = 0.0;
    let mut gradient = vec![0.0; weights.len()];
    for ((g, w), prior) in gradient.iter_mut().zip(weights).zip(original) {
        objective += 0.5 * REGULARIZATION * (w - prior).powi(2);
        *g = REGULARIZATION * (w - prior);
    }
    let mut curvature = Vec::with_capacity(matrix.len());
    for ((row, mass), label) in matrix.iter().zip(masses).zip(labels) {
        let margin = dot(row, weights);
        let p = sigmoid(margin);
        objective += mass * loss(margin, *label);
        let residual = mass * (p - f64::from(*label));
        for (g, x) in gradient.iter_mut().zip(row) {
            *g += residual * x;
        }
        curvature.push(mass * p * (1.0 - p));
    }
    (objective, gradient, curvature)
}

/// Cholesky solve of the bounded sample-space positive-definite system.
/// The identity term prevents singular fitting features from fabricating a
/// solution. No model/library inference implements this scalar optimizer.
fn solve_spd(mut a: Vec<Vec<f64>>, rhs: &[f64]) -> Result<Vec<f64>> {
    let n = rhs.len();
    for i in 0..n {
        for j in 0..=i {
            let residual = a[i][j] - (0..j).map(|k| a[i][k] * a[j][k]).sum::<f64>();
            a[i][j] = if i == j {
                if !residual.is_finite() || residual <= 0.0 {
                    return Err("non-positive fitting system".into());
                }
                residual.sqrt()
            } else {
                residual / a[j][j]
            };
        }
    }
    let mut x = vec![0.0; n];
    for i in 0..n {
        x[i] = (rhs[i] - (0..i).map(|j| a[i][j] * x[j]).sum::<f64>()) / a[i][i];
    }
    for i in (0..n).rev() {
        x[i] = (x[i] - (i + 1..n).map(|j| a[j][i] * x[j]).sum::<f64>()) / a[i][i];
    }
    Ok(x)
}

fn optimize(
    matrix: &[Vec<f64>],
    masses: &[f64],
    labels: &[bool],
    original: &[f64],
) -> Result<(Vec<f64>, usize)> {
    let mut weights = original.to_vec();
    let mut iterations = 0;
    for iteration in 0..MAX_ITERATIONS {
        let (objective, gradient, curvature) =
            objective_gradient(matrix, masses, labels, &weights, original);
        if gradient.iter().map(|g| g * g).sum::<f64>().sqrt() < 1e-8 {
            break;
        }
        // Woodbury: invert lambda*I + X' D X in sample space (<=256),
        // preserving the prior on runtime coefficients, not scaled ones.
        let scaled = matrix
            .iter()
            .zip(&curvature)
            .map(|(x, d)| x.iter().map(|v| v * d.sqrt()).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let system = scaled
            .iter()
            .enumerate()
            .map(|(i, row)| {
                scaled
                    .iter()
                    .enumerate()
                    .map(|(j, other)| f64::from(i == j) + dot(row, other) / REGULARIZATION)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let rhs = scaled
            .iter()
            .map(|row| dot(row, &gradient) / REGULARIZATION)
            .collect::<Vec<_>>();
        let correction = solve_spd(system, &rhs)?;
        let direction = gradient
            .iter()
            .enumerate()
            .map(|(j, g)| {
                (g - scaled
                    .iter()
                    .zip(&correction)
                    .map(|(x, c)| x[j] * c)
                    .sum::<f64>())
                    / REGULARIZATION
            })
            .collect::<Vec<_>>();
        let descent = dot(&gradient, &direction);
        if !descent.is_finite() || descent <= 0.0 {
            return Err("non-descent fitting direction".into());
        }
        let mut step = 1.0;
        let mut accepted = None;
        for _ in 0..64 {
            let candidate = weights
                .iter()
                .zip(&direction)
                .map(|(w, d)| w - step * d)
                .collect::<Vec<_>>();
            let (next, _, _) = objective_gradient(matrix, masses, labels, &candidate, original);
            if next.is_finite() && next <= objective - 1e-4 * step * descent {
                accepted = Some(candidate);
                break;
            }
            step *= 0.5;
        }
        weights = accepted.ok_or("bounded fitting line search failed")?;
        iterations = iteration + 1;
    }
    Ok((weights, iterations))
}

/// Independently scalar arithmetic for the affine stage. Inference uses
/// Candle's Linear kernel instead; this function is not its own expected value.
pub fn reference_logit(features: &[f32], weights: &[f32], bias: f32) -> Result<f64> {
    if features.len() != weights.len()
        || features.iter().chain(weights).any(|x| !x.is_finite())
        || !bias.is_finite()
    {
        return Err("invalid scalar affine reference input".into());
    }
    Ok(features
        .iter()
        .zip(weights)
        .map(|(x, w)| *x as f64 * *w as f64)
        .sum::<f64>()
        + bias as f64)
}

pub fn fit(input: &Features) -> Result<Readout> {
    let width = input.original_weights.len();
    if input.schema != "decapod.laya-affine-features.v1"
        || input.partition != "fit"
        || input.rubric != RUBRIC
        || !(1..=1024).contains(&width)
        || !(4..=256).contains(&input.rows.len())
        || input.original_weights.iter().any(|w| !w.is_finite())
    {
        return Err("fitting accepts only bounded fitting features, never evaluation data".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let families = input
        .rows
        .iter()
        .map(|r| r.family.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if families.len() < 2 {
        return Err("fitting needs multiple distinct contract families".into());
    }
    let positives = input.rows.iter().filter(|r| r.expected_ready).count();
    let negatives = input.rows.len() - positives;
    if positives == 0 || negatives == 0 {
        return Err("fitting cannot use a single disposition class".into());
    }
    let mut matrix = Vec::new();
    let mut masses = Vec::new();
    for row in &input.rows {
        if !ids.insert(&row.id)
            || row.false_features.len() != width
            || row.true_features.len() != width
            || row
                .false_features
                .iter()
                .chain(&row.true_features)
                .any(|x| !x.is_finite())
            || !row.temperature.is_finite()
            || row.temperature <= 0.0
            || row.input_tokens == 0
            || row.input_tokens > 1024
        {
            return Err("malformed, duplicate or unsupported fitting row".into());
        }
        matrix.push(
            row.true_features
                .iter()
                .zip(&row.false_features)
                .map(|(t, f)| (*t as f64 - *f as f64) / row.temperature)
                .collect::<Vec<_>>(),
        );
        masses.push(
            0.5 / if row.expected_ready {
                positives as f64
            } else {
                negatives as f64
            },
        );
    }
    let original = input
        .original_weights
        .iter()
        .map(|w| *w as f64)
        .collect::<Vec<_>>();
    let baseline = matrix.iter().map(|x| dot(x, &original)).collect::<Vec<_>>();
    let labels = input
        .rows
        .iter()
        .map(|r| r.expected_ready)
        .collect::<Vec<_>>();
    let (solution, iterations) = optimize(&matrix, &masses, &labels, &original)?;
    // Fixed predeclared decimal quantization makes the published artifact easy
    // to reproduce and inspect. It does not round a verdict toward approval.
    let weights = solution
        .iter()
        .map(|w| ((w * 1e7).round() / 1e7) as f32)
        .collect::<Vec<_>>();
    let fitted = weights.iter().map(|w| *w as f64).collect::<Vec<_>>();
    if weights.iter().any(|w| !w.is_finite()) {
        return Err("fitting produced nonfinite weights".into());
    }
    let initial_loss = input
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| masses[i] * loss(baseline[i], r.expected_ready))
        .sum();
    let fitted_loss = input
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| masses[i] * loss(dot(&matrix[i], &fitted), r.expected_ready))
        .sum();
    let training_errors = input
        .rows
        .iter()
        .enumerate()
        .filter(|(i, r)| (dot(&matrix[*i], &fitted) > 0.0) != r.expected_ready)
        .count();
    Ok(Readout {schema:SCHEMA.into(),profile:"laya-multilingual-readiness-affine-v1".into(),base_weights_sha256:input.base_weights_sha256.clone(),rubric:RUBRIC.into(),weights,preserve_original_bias:true,fit_inputs_sha256:input.inputs_sha256.clone(),fit_definition_sha256:input.definition_sha256.clone(),frozen_evaluation_definition_sha256:input.frozen_evaluation_definition_sha256.clone(),fitting_cases_code_sha256:input.cases_code_sha256.clone(),feature_extractor_sha256:input.feature_extractor_sha256.clone(),model_code_sha256:input.model_code_sha256.clone(),fitting_tool_sha256:crate::laya_assets::hash(include_bytes!("affine.rs")),fitting_families:families.into_iter().collect(),fitting_rows:input.rows.len(),regularization:REGULARIZATION,iterations,initial_loss,fitted_loss,training_errors,accuracy_scope:"Agent-curated bounded functional fitting; evaluation is separate; readiness-domain accuracy and calibrated production risk remain unestablished".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_weight_prior_gradient_and_low_variance_coefficients_are_bounded() {
        let matrix = vec![vec![1.0, 1e-9], vec![-1.0, -1e-9]];
        let masses = [0.5, 0.5];
        let labels = [true, false];
        let original = [0.1, -0.2];
        let point = [0.3, 0.4];
        let (_, gradient, _) = objective_gradient(&matrix, &masses, &labels, &point, &original);
        for j in 0..point.len() {
            let mut plus = point;
            let mut minus = point;
            plus[j] += 1e-5;
            minus[j] -= 1e-5;
            let f = |w: &[f64]| objective_gradient(&matrix, &masses, &labels, w, &original).0;
            assert!((gradient[j] - (f(&plus) - f(&minus)) / 2e-5).abs() < 1e-8);
        }
        let initial = objective_gradient(&matrix, &masses, &labels, &original, &original).0;
        let (weights, _) = optimize(&matrix, &masses, &labels, &original).unwrap();
        let (objective, gradient, _) =
            objective_gradient(&matrix, &masses, &labels, &weights, &original);
        assert!(objective < initial);
        assert!(gradient.iter().all(|g| g.abs() < 1e-8));
        // Nonnegative logistic loss and monotone descent imply this bound.
        assert!(
            weights
                .iter()
                .zip(original)
                .map(|(w, p)| (w - p).powi(2))
                .sum::<f64>()
                <= 2.0 * initial / REGULARIZATION
        );
        assert!((weights[1] - original[1]).abs() < 1e-6);
    }
    #[test]
    fn scalar_spd_solve_matches_known_system() {
        let x = solve_spd(vec![vec![4.0, 1.0], vec![1.0, 3.0]], &[1.0, 2.0]).unwrap();
        assert!((4.0 * x[0] + x[1] - 1.0).abs() < 1e-12);
        assert!((x[0] + 3.0 * x[1] - 2.0).abs() < 1e-12);
        assert!(solve_spd(vec![vec![0.0]], &[1.0]).is_err());
    }
    fn fixture() -> Features {
        let rows = [
            ("a", true, vec![1.0, 0.0]),
            ("a", false, vec![-1.0, 0.0]),
            ("b", true, vec![0.0, 1.0]),
            ("b", false, vec![0.0, -1.0]),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (family, label, v))| FeatureRow {
            id: i.to_string(),
            family: family.into(),
            expected_ready: label,
            state_sha256: "fixture".into(),
            input_tokens: 10,
            false_features: vec![0.0; 2],
            true_features: v,
            temperature: 1.0,
        })
        .collect();
        Features {
            schema: "decapod.laya-affine-features.v1".into(),
            partition: "fit".into(),
            base_weights_sha256: "fixture".into(),
            rubric: RUBRIC.into(),
            inputs_sha256: "fixture".into(),
            definition_sha256: "fixture".into(),
            frozen_evaluation_definition_sha256: "fixture".into(),
            cases_code_sha256: "fixture".into(),
            feature_extractor_sha256: "fixture".into(),
            model_code_sha256: "fixture".into(),
            original_weights: vec![0.0; 2],
            rows,
        }
    }
    #[test]
    fn balanced_fit_is_reproducible_and_does_not_accept_evaluation_rows() {
        let mut input = fixture();
        let a = fit(&input).unwrap();
        let b = fit(&input).unwrap();
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        assert_eq!(a.training_errors, 0);
        assert!(a.fitted_loss < a.initial_loss);
        input.partition = "evaluation".into();
        assert!(fit(&input).is_err());
        input.partition = "fit".into();
        input.rows[0].true_features[0] = f32::NAN;
        assert!(fit(&input).is_err());
    }
    #[test]
    fn stable_loss_derivative_matches_finite_difference() {
        for margin in [-20.0, -1.0, 0.0, 1.0, 20.0] {
            for label in [false, true] {
                let epsilon = 1e-5;
                let numeric = (loss(margin + epsilon, label) - loss(margin - epsilon, label))
                    / (2.0 * epsilon);
                assert!((numeric - (sigmoid(margin) - f64::from(label))).abs() < 1e-8);
            }
        }
    }
}
