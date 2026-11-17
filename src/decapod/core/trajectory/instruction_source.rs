//! Bounded provider read for recorded instructions, not an approval authority.
//! Only a fresh sealed provider GET can construct ObservedInstruction. Persisted
//! metadata remains a historical observation: local bytes do not authenticate
//! their writer, establish that an account is human, or grant any operation.
use crate::core::{custody::ReportedInstructionSource, snapshot_provider as provider};
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceObservation {
    pub schema_version: String,
    pub provider: String,
    pub repository: String,
    pub object_id: u64,
    pub issue_number: u64,
    pub account_id: u64,
    pub account_type: String,
    pub source_updated_at: String,
    pub original_body_digest: String,
    pub confidence: String,
}

/// No Deserialize, public fields, JSON constructor or caller-set confidence.
#[derive(Debug)]
pub(crate) struct ObservedInstruction {
    instruction: String,
    source: ReportedInstructionSource,
    observation: SourceObservation,
}
impl ObservedInstruction {
    pub(crate) fn into_parts(self) -> (String, ReportedInstructionSource, SourceObservation) {
        (self.instruction, self.source, self.observation)
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Deserialize)]
struct Account {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
struct Comment {
    id: u64,
    issue_url: String,
    html_url: String,
    body: String,
    updated_at: String,
    user: Account,
}

/// Repository and object identities are checked against the provider response.
/// The expected body digest pins a mutable comment before any journal mutation.
/// Runner is the existing official-client or anonymous-HTTPS adapter, never an imported
/// response, alternate URL, shell command or candidate-selected executable.
pub(crate) fn observe_github_comment(
    root: &Path,
    runner: &mut impl provider::Runner,
    repository: &provider::Repository,
    comment_id: u64,
    expected_body_digest: &str,
    budget: &provider::Budget<'_>,
) -> io::Result<ObservedInstruction> {
    if comment_id == 0
        || !expected_body_digest
            .strip_prefix("sha256:")
            .is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
    {
        return Err(invalid(
            "INSTRUCTION_SOURCE_INVALID: positive comment ID and exact lowercase SHA256 body digest required",
        ));
    }
    let endpoint = format!("{}/issues/comments/{comment_id}", repository.api());
    let bytes = provider::checked(runner, provider::provider::gh_spec(root, &endpoint), budget)?;
    decode_comment(repository, comment_id, expected_body_digest, &bytes)
}

fn decode_comment(
    repository: &provider::Repository,
    id: u64,
    expected_digest: &str,
    bytes: &[u8],
) -> io::Result<ObservedInstruction> {
    let source: Comment = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    let prefix = format!("https://api.github.com/{}/issues/", repository.api());
    let issue_number = source
        .issue_url
        .strip_prefix(&prefix)
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            invalid(
                "INSTRUCTION_SOURCE_IDENTITY_MISMATCH: provider returned another repository/issue",
            )
        })?;
    let expected_url = format!(
        "https://github.com/{}/issues/{issue_number}#issuecomment-{id}",
        repository.full_name()
    );
    // PR conversations use /pull/N while the API issue URL retains /issues/N.
    let expected_pr_url = format!(
        "https://github.com/{}/pull/{issue_number}#issuecomment-{id}",
        repository.full_name()
    );
    if source.id != id
        || (source.html_url != expected_url && source.html_url != expected_pr_url)
        || source.user.id == 0
        || !matches!(source.user.kind.as_str(), "User" | "Bot" | "Organization")
        || source.body.trim().is_empty()
        || source.body.len() > 8192
        || source.updated_at.len() > 40
        || !source.updated_at.ends_with('Z')
        || source.updated_at.chars().any(char::is_control)
    {
        return Err(invalid(
            "INSTRUCTION_SOURCE_INVALID: mismatched identity or bounded source contract",
        ));
    }
    let body_digest = provider::hash(source.body.as_bytes());
    if body_digest != expected_digest {
        return Err(invalid(
            "INSTRUCTION_SOURCE_CHANGED: inspect current source and pin its exact body digest before recording a new revision",
        ));
    }
    Ok(ObservedInstruction {
        instruction: source.body,
        source: ReportedInstructionSource {
            source_ref: source.html_url,
            principal_claim: Some(format!("github-account:{}", source.user.id)),
        },
        observation: SourceObservation {
            schema_version: "1.0.0".into(),
            provider: "github.com".into(),
            repository: repository.full_name(),
            object_id: id,
            issue_number,
            account_id: source.user.id,
            account_type: source.user.kind,
            source_updated_at: source.updated_at,
            original_body_digest: body_digest,
            confidence: "provider_observed_record".into(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_observation_pins_source_without_minting_human_authority() {
        let repo = provider::Repository::new("owner", "project").unwrap();
        let mut response = serde_json::json!({"id":42,"issue_url":"https://api.github.com/repos/owner/project/issues/3","html_url":"https://github.com/owner/project/issues/3#issuecomment-42","body":"C: preserve v1","updated_at":"2026-11-17T00:00:00Z","user":{"id":123,"type":"User"}});
        let expected = provider::hash(b"C: preserve v1");
        let encode = |v: &serde_json::Value| serde_json::to_vec(v).unwrap();
        let observed = decode_comment(&repo, 42, &expected, &encode(&response)).unwrap();
        assert_eq!(observed.observation.confidence, "provider_observed_record");
        assert_eq!(
            observed.source.principal_claim.as_deref(),
            Some("github-account:123")
        );
        let mut custody = crate::core::custody::Custody::in_memory();
        custody
            .create_intent("intent:source", "A: preserve v1".into())
            .unwrap();
        custody
            .refine(
                "intent:source",
                crate::core::custody::IntentRefinement {
                    refined_intent: "B: replace v1".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let input = crate::core::custody::InstructionRevisionInput {
            revision_id: "C".into(),
            expected_revision: custody.state().intents["intent:source"]
                .instruction_revision()
                .unwrap(),
            instruction: String::new(),
            source_claim: ReportedInstructionSource {
                source_ref: String::new(),
                principal_claim: None,
            },
            kind: crate::core::custody::RecordedInstructionKind::Contradiction,
            supersedes: vec![],
            derived_intent: None,
        };
        let revision = custody
            .append_observed_instruction("intent:source", input.clone(), observed)
            .unwrap();
        assert_eq!(
            revision.preceding_refinement.as_deref(),
            Some("B: replace v1")
        );
        let retry = decode_comment(&repo, 42, &expected, &encode(&response)).unwrap();
        let before = custody.state().clone();
        custody
            .append_observed_instruction("intent:source", input.clone(), retry)
            .unwrap();
        assert_eq!(*custody.state(), before);
        let lineage = custody.state().intents["intent:source"]
            .instruction_lineage()
            .unwrap();
        assert_eq!(lineage.original_intent, "A: preserve v1");
        assert_eq!(
            lineage.independent_authority,
            crate::core::validation_rules::RuleVerdict::Unverifiable
        );
        let mut forged_input = serde_json::to_value(&input).unwrap();
        forged_input["source_observation"] =
            serde_json::to_value(&revision.source_observation).unwrap();
        assert!(
            serde_json::from_value::<crate::core::custody::InstructionRevisionInput>(forged_input)
                .is_err()
        );
        response["body"] = "C: replace v1".into();
        assert!(
            decode_comment(&repo, 42, &expected, &encode(&response))
                .unwrap_err()
                .to_string()
                .contains("INSTRUCTION_SOURCE_CHANGED")
        );
        response["body"] = "C: preserve v1".into();
        response["issue_url"] = "https://api.github.com/repos/attacker/project/issues/3".into();
        assert!(decode_comment(&repo, 42, &expected, &encode(&response)).is_err());
    }
}
