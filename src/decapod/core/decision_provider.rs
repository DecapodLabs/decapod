//! Optional, provider-neutral probabilistic observations.
//!
//! A decision provider may inform Decapod's governance context, but it never
//! owns policy. In particular, an observation is not an approval, an
//! interlock resolution, or proof of completion.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

pub const TRAJECTORY_SATISFIES_INTENT: &str = "trajectory_satisfies_intent";
const JEV_QUESTION_ID: &str = "trajectory_satisfies_intent";
const JEV_API_URL: &str = "https://api.typesafe.ai/v1/systemone";
const JEV_MODEL: &str = "jev-latest";
const JEV_API_KEY_ENV: &str = "TYPESAFE_API_KEY";

/// The configured provider. `none` is the safe local default.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DecisionProviderKind {
    #[default]
    None,
    Jev,
}

/// The governance and repository context that may be supplied to a provider.
///
/// This is deliberately narrower than Decapod's complete state. It excludes
/// raw operation parameters so arbitrary credentials or payloads are not sent
/// to an external service by this first provider.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DecisionContext {
    pub declared_intent: Option<String>,
    pub trajectory: TrajectoryContext,
    pub governance: GovernanceContext,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TrajectoryContext {
    pub operation: String,
    pub touched_paths: Vec<String>,
    pub diff_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct GovernanceContext {
    pub must_obligations: Vec<GovernanceObligation>,
    pub recommended_obligations: Vec<GovernanceObligation>,
    pub contradictions: Vec<String>,
    pub proof_hooks: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub file_touch_budget: Option<usize>,
    pub workspace: WorkspaceContext,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct GovernanceObligation {
    pub kind: String,
    pub reference: String,
    pub title: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WorkspaceContext {
    pub branch: String,
    pub is_protected: bool,
    pub is_isolated: bool,
    pub can_work: bool,
}

/// One typed observation from a provider. A probability is not a policy
/// decision; Decapod remains responsible for interpreting it, if it ever does.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DecisionObservation {
    pub kind: String,
    pub probability: f64,
    pub provider: String,
    pub model: Option<String>,
}

impl DecisionObservation {
    fn new(kind: &str, probability: f64, provider: &str, model: Option<&str>) -> Option<Self> {
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            return None;
        }
        Some(Self {
            kind: kind.to_string(),
            probability,
            provider: provider.to_string(),
            model: model.map(str::to_string),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoObservationReason {
    Disabled,
    Unconfigured,
    MissingIntent,
    Unavailable,
    MalformedResponse,
    InvalidObservation,
    Configuration,
}

/// The provider result is explicit so provider failure cannot be mistaken for
/// a positive governance decision.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DecisionObservationResult {
    Observed {
        observation: DecisionObservation,
    },
    NoObservation {
        provider: String,
        reason: NoObservationReason,
    },
}

impl DecisionObservationResult {
    pub fn no_observation(provider: impl Into<String>, reason: NoObservationReason) -> Self {
        Self::NoObservation {
            provider: provider.into(),
            reason,
        }
    }
}

impl Default for DecisionObservationResult {
    fn default() -> Self {
        Self::no_observation("none", NoObservationReason::Disabled)
    }
}

/// Decapod-owned provider seam. Implementations only return observations.
pub trait DecisionProvider: Send + Sync {
    fn observe(&self, context: &DecisionContext) -> DecisionObservationResult;
}

struct NoneDecisionProvider;

impl DecisionProvider for NoneDecisionProvider {
    fn observe(&self, _context: &DecisionContext) -> DecisionObservationResult {
        DecisionObservationResult::no_observation("none", NoObservationReason::Disabled)
    }
}

/// Construct the configured provider without exposing provider-specific
/// transport or response types to the rest of Decapod.
pub fn configured(kind: DecisionProviderKind) -> Box<dyn DecisionProvider> {
    match kind {
        DecisionProviderKind::None => Box::new(NoneDecisionProvider),
        DecisionProviderKind::Jev => Box::new(JevDecisionProvider::from_env()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JevHttpResponse {
    status: u16,
    body: Vec<u8>,
}

trait JevTransport: Send + Sync {
    fn request(&self, url: &str, api_key: &str, body: &[u8]) -> Result<JevHttpResponse, String>;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct JevCurlTransport;

impl JevTransport for JevCurlTransport {
    fn request(&self, url: &str, api_key: &str, body: &[u8]) -> Result<JevHttpResponse, String> {
        let mut command = Command::new("curl");
        command.args([
            "--silent",
            "--show-error",
            "--location",
            "--request",
            "POST",
            "--header",
            "Accept: application/json",
            "--header",
            "Content-Type: application/json",
            "--header",
            &format!("Authorization: Bearer {api_key}"),
            "--connect-timeout",
            "2",
            "--max-time",
            "10",
            "--write-out",
            "\n%{http_code}",
            url,
            "--data-binary",
            &String::from_utf8_lossy(body),
        ]);

        let output = command
            .output()
            .map_err(|error| format!("curl transport failed: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "curl exited with status {}",
                output.status.code().unwrap_or(-1)
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "Jev response was not UTF-8".to_string())?;
        let (body, status) = stdout
            .rsplit_once('\n')
            .ok_or_else(|| "Jev response did not include an HTTP status".to_string())?;
        let status = status
            .parse::<u16>()
            .map_err(|_| "Jev returned an invalid HTTP status".to_string())?;
        Ok(JevHttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        })
    }
}

/// Jev adapter. Its HTTP schema is intentionally private to this module.
pub(crate) struct JevDecisionProvider<T = JevCurlTransport> {
    api_key: Option<String>,
    transport: T,
}

impl JevDecisionProvider<JevCurlTransport> {
    fn from_env() -> Self {
        Self {
            api_key: std::env::var(JEV_API_KEY_ENV)
                .ok()
                .filter(|key| !key.trim().is_empty()),
            transport: JevCurlTransport,
        }
    }
}

impl<T> JevDecisionProvider<T> {
    #[cfg(test)]
    fn with_transport(api_key: Option<String>, transport: T) -> Self {
        Self { api_key, transport }
    }
}

#[derive(Debug, Serialize)]
struct JevRequest {
    model: &'static str,
    state: DecisionContext,
    questions: BTreeMap<&'static str, JevQuestion>,
}

#[derive(Debug, Serialize)]
struct JevQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: &'static str,
    criteria: JevNoulCriteria,
}

#[derive(Debug, Serialize)]
struct JevNoulCriteria {
    r#true: &'static str,
    r#false: &'static str,
}

#[derive(Debug, Deserialize)]
struct JevResponse {
    answers: BTreeMap<String, JevAnswer>,
}

#[derive(Debug, Deserialize)]
struct JevAnswer {
    #[serde(rename = "type")]
    answer_type: String,
    noul: Option<f64>,
}

impl<T: JevTransport> DecisionProvider for JevDecisionProvider<T> {
    fn observe(&self, context: &DecisionContext) -> DecisionObservationResult {
        if context
            .declared_intent
            .as_deref()
            .is_none_or(|intent| intent.trim().is_empty())
        {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::MissingIntent,
            );
        }

        let Some(api_key) = self.api_key.as_deref() else {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::Unconfigured,
            );
        };

        let mut questions = BTreeMap::new();
        questions.insert(
            JEV_QUESTION_ID,
            JevQuestion {
                question_type: "noul",
                instructions: "Given the declared intent and governance/repository context, does the current proposed trajectory satisfy the declared intent?",
                criteria: JevNoulCriteria {
                    r#true: "The proposed trajectory is likely to satisfy the declared intent within the supplied governance context.",
                    r#false: "The proposed trajectory is unlikely to satisfy the declared intent within the supplied governance context.",
                },
            },
        );
        let request = JevRequest {
            model: JEV_MODEL,
            state: context.clone(),
            questions,
        };
        let body = match serde_json::to_vec(&request) {
            Ok(body) => body,
            Err(_) => {
                return DecisionObservationResult::no_observation(
                    "jev",
                    NoObservationReason::InvalidObservation,
                );
            }
        };

        let response = match self.transport.request(JEV_API_URL, api_key, &body) {
            Ok(response) if (200..300).contains(&response.status) => response,
            Ok(_) | Err(_) => {
                return DecisionObservationResult::no_observation(
                    "jev",
                    NoObservationReason::Unavailable,
                );
            }
        };

        let response: JevResponse = match serde_json::from_slice(&response.body) {
            Ok(response) => response,
            Err(_) => {
                return DecisionObservationResult::no_observation(
                    "jev",
                    NoObservationReason::MalformedResponse,
                );
            }
        };
        let Some(answer) = response.answers.get(JEV_QUESTION_ID) else {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::InvalidObservation,
            );
        };
        if answer.answer_type != "noul" {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::InvalidObservation,
            );
        }
        let Some(probability) = answer.noul else {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::InvalidObservation,
            );
        };
        let Some(observation) = DecisionObservation::new(
            TRAJECTORY_SATISFIES_INTENT,
            probability,
            "jev",
            Some(JEV_MODEL),
        ) else {
            return DecisionObservationResult::no_observation(
                "jev",
                NoObservationReason::InvalidObservation,
            );
        };
        DecisionObservationResult::Observed { observation }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/decision_provider_tests.rs"]
mod tests;
