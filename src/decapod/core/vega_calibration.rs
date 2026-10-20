//! Framework-free vega calibration. Theta and probability arithmetic remain f64;
//! the four physical features deliberately round to f32 first, as in upstream
//! `thermal_features`, before multiplying the checkpoint's f64 theta.
//!
//! Algorithm ported from Apache-2.0 vegaml `_common.py` and `_api.py` at
//! 21a17c90624aa85cec60b1092fa72cb8b720f71a. This preserves upstream uncertainty
//! signals; no confidence constant is substituted for checkpoint calibration.
//! Fixed-checkpoint numerical parity and domain-level readiness reliability are
//! separate claims. The latter remains unestablished; functional integration
//! acceptance never turns upstream calibration into a domain accuracy guarantee.
//! Adapter calibration metadata can
//! be represented/routed here, but the base engine rejects adapter execution.

use super::vega_engine::{EngineReadout, QuestionType, Result, VegaError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const TYPE_NAMES: [&str; 3] = ["choice", "score", "boolean"];
const FEATURE_NAMES: [&str; 4] = [
    "log1p(kinetic)",
    "log1p(nearest)",
    "steps_used/n_steps",
    "log(k)",
];

fn default_risk() -> String {
    "0.1".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Calibration {
    /// [choice, score, boolean] x [bias, kinetic, nearest, step_fraction, log_k].
    pub theta: [[f64; 5]; 3],
    #[serde(default)]
    pub conformal: BTreeMap<String, BTreeMap<String, f64>>,
    #[serde(default)]
    pub abstain_threshold: BTreeMap<String, f64>,
    #[serde(default = "default_risk")]
    pub default_risk: String,
    #[serde(default)]
    pub unbound_threshold: Option<f64>,
}

/// Null thresholds remain distinguishable from a measured permissive threshold.
/// Callers making safety/readiness decisions must require the calibration they
/// need; an upstream-style absent threshold is not assurance or authorization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibratedAnswer {
    pub question_type: QuestionType,
    /// Probabilities of real options, in their supplied order (padded entries omitted).
    pub probabilities: Vec<f64>,
    /// Indices into the original input row, matching `probabilities`.
    pub option_indices: Vec<usize>,
    pub best_index: usize,
    pub confidence: f64,
    pub temperature: f64,
    pub prediction_set: Option<Vec<usize>>,
    pub conformal_threshold: Option<f64>,
    pub abstain: bool,
    pub abstain_threshold: Option<f64>,
    pub unbound: bool,
    pub unbound_threshold: Option<f64>,
    pub settle_steps: usize,
    pub energy: f64,
    pub boolean_probability: Option<f64>,
    pub expected_score: Option<f64>,
}

impl Calibration {
    /// Require a real calibration block rather than silently substituting zeros.
    /// Teacher calibration is explicit; base teacher requests never fall back.
    pub fn from_config_json(config: &Value, teacher: bool) -> Result<Self> {
        let key = if teacher {
            "calibration_teacher"
        } else {
            "calibration"
        };
        let block = config
            .get(key)
            .filter(|v| v.is_object())
            .ok_or_else(|| VegaError::Calibration(format!("missing {key} block")))?;
        Self::from_json(block)
    }

    pub fn from_json(block: &Value) -> Result<Self> {
        if let Some(names) = block.get("features") {
            let names: Vec<String> = serde_json::from_value(names.clone())
                .map_err(|e| VegaError::Calibration(format!("invalid feature names: {e}")))?;
            if names.iter().map(String::as_str).ne(FEATURE_NAMES) {
                return Err(VegaError::Calibration(
                    "unsupported thermal feature order".into(),
                ));
            }
        }
        let mut calibration: Self = serde_json::from_value(block.clone())
            .map_err(|e| VegaError::Calibration(e.to_string()))?;
        calibration.conformal = normalize_conformal(calibration.conformal);
        calibration.validate()?;
        Ok(calibration)
    }

    pub fn validate(&self) -> Result<()> {
        if self.theta.iter().flatten().any(|x| !x.is_finite()) {
            return Err(VegaError::Calibration(
                "theta must be finite f64 values".into(),
            ));
        }
        validate_level(&self.default_risk, "default risk")?;
        for (question_type, table) in &self.conformal {
            if !TYPE_NAMES.contains(&question_type.as_str()) {
                return Err(VegaError::Calibration(format!(
                    "unknown conformal type {question_type}"
                )));
            }
            for (level, &threshold) in table {
                validate_level(level, "conformal alpha")?;
                validate_probability(threshold, "conformal threshold")?;
            }
        }
        for (risk, &threshold) in &self.abstain_threshold {
            validate_level(risk, "risk")?;
            validate_probability(threshold, "abstain threshold")?;
        }
        if let Some(threshold) = self.unbound_threshold
            && (!threshold.is_finite() || threshold < 0.0)
        {
            return Err(VegaError::Calibration(
                "unbound threshold must be finite and nonnegative".into(),
            ));
        }
        Ok(())
    }

    pub fn temperature(&self, qtype: QuestionType, physical_features: [f64; 4]) -> Result<f64> {
        self.validate()?;
        if physical_features.iter().any(|x| !x.is_finite()) {
            return Err(VegaError::Calibration("nonfinite physical features".into()));
        }
        let theta = &self.theta[qtype.id()];
        let log_tau = theta[0]
            + theta[1..]
                .iter()
                .zip(physical_features)
                .map(|(&a, b)| a * b)
                .sum::<f64>();
        if !log_tau.is_finite() {
            return Err(VegaError::Calibration("nonfinite log temperature".into()));
        }
        Ok(log_tau.clamp(0.2f64.ln(), 20.0f64.ln()).exp())
    }

    /// Calibrate one engine row. All masked logits are checked against the exact
    /// upstream -1e4 sentinel, and the softmax includes those sentinel values
    /// before padded entries are omitted, matching upstream numerical semantics.
    pub fn calibrate(
        &self,
        row: &EngineReadout,
        qtype: QuestionType,
        mask: &[bool],
        max_steps: usize,
        alpha: &str,
        risk: Option<&str>,
    ) -> Result<CalibratedAnswer> {
        self.validate()?;
        validate_level(alpha, "conformal alpha")?;
        let risk = risk.unwrap_or(&self.default_risk);
        validate_level(risk, "risk")?;
        if row.logits.len() != mask.len() || row.energies.len() != mask.len() || mask.len() > 256 {
            return Err(VegaError::Calibration(
                "logit, energy and mask shape mismatch/limit".into(),
            ));
        }
        if row.logits.iter().any(|x| !x.is_finite()) || row.energies.iter().any(|x| !x.is_finite())
        {
            return Err(VegaError::Calibration(
                "nonfinite logits or energies".into(),
            ));
        }
        for ((&logit, &energy), &enabled) in row.logits.iter().zip(&row.energies).zip(mask) {
            if energy != -logit || (!enabled && logit != -1e4) {
                return Err(VegaError::Calibration(
                    "inconsistent energy/logit or masked sentinel".into(),
                ));
            }
        }
        let indices: Vec<_> = mask
            .iter()
            .enumerate()
            .filter_map(|(i, &on)| on.then_some(i))
            .collect();
        let k = indices.len();
        if k < 2 || (qtype == QuestionType::Boolean && k != 2) {
            return Err(VegaError::Calibration(
                "invalid real option count for question type".into(),
            ));
        }
        let features = thermal_features(row, k, max_steps)?;
        let temperature = self.temperature(qtype, features)?;
        let all_probabilities = softmax_f64(&row.logits, temperature)?;
        let probabilities: Vec<_> = indices.iter().map(|&i| all_probabilities[i]).collect();
        let mut best = 0;
        for (i, &probability) in probabilities.iter().enumerate().skip(1) {
            if probability > probabilities[best] {
                best = i;
            }
        }
        let confidence = probabilities[best];
        let conformal_threshold = self
            .conformal
            .get(qtype.as_str())
            .and_then(|table| table.get(alpha))
            .copied();
        let prediction_set = conformal_threshold.map(|threshold| {
            probabilities
                .iter()
                .enumerate()
                .filter_map(|(i, &p)| {
                    (p >= (1.0 - threshold).max(1e-12) || i == best).then_some(indices[i])
                })
                .collect()
        });
        let abstain_threshold = self.abstain_threshold.get(risk).copied();
        let energy = indices
            .iter()
            .map(|&i| row.energies[i] as f64)
            .fold(f64::INFINITY, f64::min);
        Ok(CalibratedAnswer {
            question_type: qtype,
            best_index: indices[best],
            confidence,
            temperature,
            prediction_set,
            conformal_threshold,
            abstain: confidence < abstain_threshold.unwrap_or(0.0),
            abstain_threshold,
            unbound: (row.nearest as f64) > self.unbound_threshold.unwrap_or(f64::INFINITY),
            unbound_threshold: self.unbound_threshold,
            settle_steps: row.steps_used,
            energy,
            boolean_probability: (qtype == QuestionType::Boolean).then(|| probabilities[1]),
            expected_score: (qtype == QuestionType::Score).then(|| {
                probabilities
                    .iter()
                    .enumerate()
                    .map(|(i, &p)| i as f64 * p)
                    .sum()
            }),
            probabilities,
            option_indices: indices,
        })
    }
}

/// Physics features use the same f32 transform then f64 promotion as upstream.
/// Changing these to all-f64 transforms changes the calibration input contract.
pub fn thermal_features(row: &EngineReadout, options: usize, max_steps: usize) -> Result<[f64; 4]> {
    if !(2..=256).contains(&options)
        || max_steps == 0
        || row.steps_used == 0
        || row.steps_used > max_steps
    {
        return Err(VegaError::Calibration("invalid option/step counts".into()));
    }
    if !row.kinetic.is_finite()
        || !row.nearest.is_finite()
        || row.kinetic < 0.0
        || row.nearest < 0.0
    {
        return Err(VegaError::Calibration(
            "kinetic energy/nearest must be finite and nonnegative".into(),
        ));
    }
    let features = [
        row.kinetic.ln_1p(),
        row.nearest.ln_1p(),
        row.steps_used as f32 / max_steps as f32,
        (options as f32).ln(),
    ];
    if features.iter().any(|x| !x.is_finite()) {
        return Err(VegaError::Calibration(
            "nonfinite thermal feature transform".into(),
        ));
    }
    Ok(features.map(f64::from))
}

fn softmax_f64(logits: &[f32], temperature: f64) -> Result<Vec<f64>> {
    let scaled: Vec<_> = logits.iter().map(|&x| x as f64 / temperature).collect();
    let max = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut probabilities: Vec<_> = scaled.iter().map(|&x| (x - max).exp()).collect();
    let sum = probabilities.iter().sum::<f64>();
    if !sum.is_finite() || sum <= 0.0 {
        return Err(VegaError::Calibration(
            "nonfinite softmax denominator".into(),
        ));
    }
    for p in &mut probabilities {
        *p /= sum;
    }
    Ok(probabilities)
}

fn validate_level(level: &str, name: &str) -> Result<()> {
    let parsed = level
        .parse::<f64>()
        .map_err(|_| VegaError::Calibration(format!("invalid {name}: {level}")))?;
    if !parsed.is_finite() || parsed <= 0.0 || parsed >= 1.0 {
        return Err(VegaError::Calibration(format!(
            "{name} must be between zero and one"
        )));
    }
    Ok(())
}
fn validate_probability(p: f64, name: &str) -> Result<()> {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return Err(VegaError::Calibration(format!(
            "{name} must be finite and in [0,1]"
        )));
    }
    Ok(())
}

/// Preserve the upstream's one-unrecognized/one-missing rename rule; never
/// guess an ambiguous mapping, and never supply an invented conformal table.
fn normalize_conformal(
    mut input: BTreeMap<String, BTreeMap<String, f64>>,
) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut out = BTreeMap::new();
    for name in TYPE_NAMES {
        if let Some(table) = input.remove(name) {
            out.insert(name.to_owned(), table);
        }
    }
    let absent: Vec<_> = TYPE_NAMES
        .into_iter()
        .filter(|name| !out.contains_key(*name))
        .collect();
    if input.len() == 1
        && absent.len() == 1
        && let Some((_, table)) = input.into_iter().next()
    {
        out.insert(absent[0].to_owned(), table);
    }
    out
}

/// Adapter-specific calibration is independent from the base calibration.
/// This data/routing API does not imply support for executing LoRA/gate weights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCalibration {
    pub name: String,
    pub gate_threshold: f64,
    pub calibration: Calibration,
}
impl AdapterCalibration {
    pub fn from_metadata(metadata: &Value, teacher: bool) -> Result<Self> {
        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .filter(|x| !x.is_empty())
            .ok_or_else(|| VegaError::Calibration("adapter name is missing".into()))?
            .to_owned();
        let threshold = metadata
            .get("gate_threshold")
            .map(|x| {
                x.as_f64().ok_or_else(|| {
                    VegaError::Calibration("adapter threshold must be numeric".into())
                })
            })
            .transpose()?
            .unwrap_or(0.5);
        validate_probability(threshold, "adapter gate threshold")?;
        // Upstream adapter teacher calibration falls back to that adapter's
        // own default block, never to another adapter or the base engine.
        let block = if teacher {
            metadata
                .get("calibration_teacher")
                .filter(|x| x.as_object().is_some_and(|m| !m.is_empty()))
                .or_else(|| metadata.get("calibration"))
        } else {
            metadata.get("calibration")
        }
        .ok_or_else(|| VegaError::Calibration("adapter calibration is missing".into()))?;
        Ok(Self {
            name,
            gate_threshold: threshold,
            calibration: Calibration::from_json(block)?,
        })
    }
}

/// Return the most open qualifying adapter, with first-index tie breaking.
/// Each adapter has its own threshold; unspecified trailing thresholds retain
/// upstream's 0.5 fallback. None means route to the base calibration.
pub fn route_adapter(gates: &[f64], thresholds: &[f64]) -> Result<Option<usize>> {
    if gates.len() > 256 || thresholds.len() > 256 {
        return Err(VegaError::Calibration(
            "adapter routing count exceeds limit".into(),
        ));
    }
    for &value in gates {
        validate_probability(value, "adapter gate")?;
    }
    for &value in thresholds {
        validate_probability(value, "adapter gate threshold")?;
    }
    let mut best = None;
    for (i, &gate) in gates.iter().enumerate() {
        let threshold = thresholds.get(i).copied().unwrap_or(0.5);
        if gate >= threshold && best.is_none_or(|j| gate > gates[j]) {
            best = Some(i);
        }
    }
    Ok(best)
}

/// Select an already-resolved route without cross-adapter/default substitution.
pub fn calibration_for_route<'a>(
    base: &'a Calibration,
    adapters: &'a [AdapterCalibration],
    route: Option<usize>,
) -> Result<&'a Calibration> {
    match route {
        None => Ok(base),
        Some(index) => adapters
            .get(index)
            .map(|a| &a.calibration)
            .ok_or_else(|| VegaError::Calibration(format!("unknown adapter route {index}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn calibration() -> Calibration {
        Calibration::from_json(&json!({
            "theta": [[0.0,0.0,0.0,0.0,0.0], [0.0,0.0,0.0,0.0,0.0], [0.0,0.0,0.0,0.0,0.0]],
            "conformal": {"boolean": {"0.1": 0.4}},
            "abstain_threshold": {"0.1": 0.75},
            "default_risk": "0.1", "unbound_threshold": 2.0
        }))
        .unwrap()
    }
    fn row(logits: Vec<f32>) -> EngineReadout {
        EngineReadout {
            energies: logits.iter().map(|x| -x).collect(),
            logits,
            z0: vec![0.0],
            z_t: vec![0.0],
            kinetic: 0.5,
            nearest: 1.0,
            grad_norm: 0.1,
            potential: -1.0,
            steps_used: 6,
            settled: false,
        }
    }
    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn f64_theta_preserves_precision_and_checkpoint_type_order() {
        let mut cal = calibration();
        let tiny = 0.123_456_789_012_345_6f64;
        cal.theta[0][0] = tiny;
        cal.theta[1][0] = 2.0f64.ln();
        cal.theta[2][0] = 3.0f64.ln();
        close(
            cal.temperature(QuestionType::Choice, [0.0; 4]).unwrap(),
            tiny.exp(),
        );
        close(cal.temperature(QuestionType::Score, [0.0; 4]).unwrap(), 2.0);
        close(
            cal.temperature(QuestionType::Boolean, [0.0; 4]).unwrap(),
            3.0,
        );
        assert_ne!(cal.theta[0][0], tiny as f32 as f64);
    }

    #[test]
    fn temperature_uses_f32_physics_then_f64_coefficients_and_clamps() {
        let mut cal = calibration();
        cal.theta[2] = [0.1, 0.2, -0.3, 0.4, 0.5];
        let features = thermal_features(&row(vec![0.0, 1.0]), 2, 12).unwrap();
        assert_eq!(
            features,
            [
                0.5f32.ln_1p() as f64,
                1.0f32.ln_1p() as f64,
                0.5,
                2.0f32.ln() as f64
            ]
        );
        let expected =
            (0.1 + 0.2 * features[0] - 0.3 * features[1] + 0.4 * features[2] + 0.5 * features[3])
                .exp();
        close(
            cal.temperature(QuestionType::Boolean, features).unwrap(),
            expected,
        );
        cal.theta[0] = [100.0, 0.0, 0.0, 0.0, 0.0];
        cal.theta[1] = [-100.0, 0.0, 0.0, 0.0, 0.0];
        close(
            cal.temperature(QuestionType::Choice, features).unwrap(),
            20.0,
        );
        close(cal.temperature(QuestionType::Score, features).unwrap(), 0.2);
    }

    #[test]
    fn boolean_probability_conformal_abstention_and_unbound_are_independent() {
        let cal = calibration();
        let mut r = row(vec![0.0, 1.0, -1e4]);
        let answer = cal
            .calibrate(
                &r,
                QuestionType::Boolean,
                &[true, true, false],
                12,
                "0.1",
                None,
            )
            .unwrap();
        let yes = 1.0 / (1.0 + (-1.0f64).exp());
        close(answer.probabilities[1], yes);
        assert_eq!(answer.boolean_probability, Some(yes));
        assert_eq!(answer.best_index, 1);
        assert_eq!(answer.prediction_set, Some(vec![1]));
        assert!(answer.abstain); // .731 < checkpoint .75, no invented .9 rule.
        assert!(!answer.unbound);
        close(answer.energy, -1.0);
        r.nearest = 2.0;
        assert!(
            !cal.calibrate(
                &r,
                QuestionType::Boolean,
                &[true, true, false],
                12,
                "0.1",
                None
            )
            .unwrap()
            .unbound
        );
        r.nearest = 2.001;
        assert!(
            cal.calibrate(
                &r,
                QuestionType::Boolean,
                &[true, true, false],
                12,
                "0.1",
                None
            )
            .unwrap()
            .unbound
        );
    }

    #[test]
    fn conformal_retains_top_option_and_first_tie_and_can_remain_unknown() {
        let mut cal = calibration();
        cal.conformal
            .get_mut("boolean")
            .unwrap()
            .insert("0.1".into(), 0.0);
        let r = row(vec![0.0, 0.0]);
        let answer = cal
            .calibrate(&r, QuestionType::Boolean, &[true, true], 12, "0.1", None)
            .unwrap();
        assert_eq!(answer.best_index, 0);
        assert_eq!(answer.prediction_set, Some(vec![0]));
        let answer = cal
            .calibrate(
                &r,
                QuestionType::Boolean,
                &[true, true],
                12,
                "0.2",
                Some("0.2"),
            )
            .unwrap();
        assert_eq!(answer.prediction_set, None);
        assert_eq!(answer.abstain_threshold, None);
        assert!(!answer.abstain); // upstream missing-risk semantics are explicit.
    }

    #[test]
    fn score_expected_value_and_permuted_option_indices_preserved() {
        let answer = calibration()
            .calibrate(
                &row(vec![0.0, -1e4, 0.0, 0.0]),
                QuestionType::Score,
                &[true, false, true, true],
                12,
                "0.1",
                None,
            )
            .unwrap();
        assert_eq!(answer.option_indices, [0, 2, 3]);
        close(answer.expected_score.unwrap(), 1.0);
        close(answer.confidence, 1.0 / 3.0);
        assert_eq!(answer.boolean_probability, None);
    }

    #[test]
    fn exact_threshold_boundaries_preserve_upstream_inequalities() {
        let mut cal = calibration();
        cal.abstain_threshold.insert("0.1".into(), 0.5);
        cal.conformal
            .get_mut("boolean")
            .unwrap()
            .insert("0.1".into(), 0.5);
        let answer = cal
            .calibrate(
                &row(vec![0.0, 0.0]),
                QuestionType::Boolean,
                &[true, true],
                12,
                "0.1",
                None,
            )
            .unwrap();
        assert!(!answer.abstain);
        assert_eq!(answer.prediction_set, Some(vec![0, 1]));
    }

    #[test]
    fn conformal_renames_only_unambiguous_legacy_type() {
        let mut c = serde_json::to_value(calibration()).unwrap();
        c["conformal"] = json!({"choice":{"0.1":0.1},"score":{"0.1":0.2},"old_bool":{"0.1":0.3}});
        let cal = Calibration::from_json(&c).unwrap();
        assert_eq!(cal.conformal["boolean"]["0.1"], 0.3);
        c["conformal"] = json!({"choice":{"0.1":0.1},"old_bool":{"0.1":0.3}});
        let cal = Calibration::from_json(&c).unwrap();
        assert!(!cal.conformal.contains_key("boolean"));
    }

    #[test]
    fn adapter_routes_use_each_threshold_and_never_share_calibration() {
        assert_eq!(route_adapter(&[0.7, 0.6], &[0.8, 0.5]).unwrap(), Some(1));
        assert_eq!(route_adapter(&[0.8, 0.8], &[0.5, 0.5]).unwrap(), Some(0));
        assert_eq!(route_adapter(&[0.2, 0.4], &[0.5]).unwrap(), None);
        assert_eq!(route_adapter(&[0.1, 0.5], &[0.9]).unwrap(), Some(1));
        let base = calibration();
        let mut other = calibration();
        other.theta[2][0] = 2.0f64.ln();
        let adapters = vec![AdapterCalibration {
            name: "test".into(),
            gate_threshold: 0.7,
            calibration: other,
        }];
        close(
            calibration_for_route(&base, &adapters, None)
                .unwrap()
                .temperature(QuestionType::Boolean, [0.0; 4])
                .unwrap(),
            1.0,
        );
        close(
            calibration_for_route(&base, &adapters, Some(0))
                .unwrap()
                .temperature(QuestionType::Boolean, [0.0; 4])
                .unwrap(),
            2.0,
        );
        assert!(calibration_for_route(&base, &adapters, Some(1)).is_err());
    }

    #[test]
    fn absent_or_malformed_calibration_and_nonfinite_input_fail_closed() {
        assert!(Calibration::from_config_json(&json!({}), false).is_err());
        assert!(Calibration::from_json(&json!({"theta":[[0,0]]})).is_err());
        let mut invalid = calibration();
        invalid.theta[0][0] = f64::NAN;
        assert!(invalid.validate().is_err());
        let mut r = row(vec![0.0, 1.0]);
        r.kinetic = f32::INFINITY;
        assert!(thermal_features(&r, 2, 12).is_err());
        let r = row(vec![0.0, 1.0]);
        assert!(
            calibration()
                .calibrate(&r, QuestionType::Boolean, &[true, false], 12, "0.1", None)
                .is_err()
        );
        assert!(route_adapter(&[f64::NAN], &[]).is_err());
    }
}
