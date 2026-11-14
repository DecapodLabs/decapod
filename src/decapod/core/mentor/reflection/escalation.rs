//! Escalation is a proposal to the agent's governor, never a human interview.
//! Literal observations establish source facts, not semantic reasoning quality.
use super::*;
use crate::core::store::{Store, StoreKind};
use crate::plugins::policy;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Uncertainty {
    DiscoverableInformation,
    InferableDecision,
    VerifiableUncertainty,
    IrreducibleAmbiguity,
    MissingAuthority,
    ExceptionalRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRequest {
    pub command: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFact {
    pub source: SourceRef,
    /// Exact observed repository text. Transient: only its digest is retained.
    pub literal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub decision: String,
    pub facts: Vec<SourceFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    /// Must equal the current canonical plan intent, not an inferred substitute.
    pub intent: String,
    pub question: String,
    pub missing: String,
    pub uncertainty: Uncertainty,
    #[serde(default)]
    pub investigation: Vec<String>,
    #[serde(default)]
    pub resolution: Option<Resolution>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub authority: Option<AuthorityRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    ExistingHumanDependency,
    MissingAuthorization,
    ObservedSourcePredicate,
    DiscoverableSource,
    DecisionEvidenceAvailable,
    VerificationRequired,
    IrreducibilityUnproven,
    AuthorityNotEstablished,
    AuthorityAvailable,
    ProposedQuestionDoesNotMatchGate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assessment {
    pub schema_version: String,
    pub reason_code: ReasonCode,
    pub declared_uncertainty: Uncertainty,
    pub human_escalation_permitted: bool,
    /// A byte match, agent justification or policy read cannot prove exhaustion.
    pub semantic_exhaustion_proven: bool,
    pub observed_facts: usize,
    pub subject_digest: String,
    pub sources: Vec<SourceRef>,
    pub limitations: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<AuthorityRequest>,
}

pub(super) fn validate(proposal: &Proposal) -> Result<(), DecapodError> {
    for text in [&proposal.intent, &proposal.question, &proposal.missing] {
        if text.trim().is_empty() || text.len() > 8192 {
            return Err(invalid("ESCALATION_INPUT_LIMIT"));
        }
    }
    for entries in [
        &proposal.investigation,
        &proposal.risks,
        &proposal.constraints,
    ] {
        if entries.len() > 8 || entries.iter().any(|s| s.len() > 512) {
            return Err(invalid("ESCALATION_INPUT_LIMIT"));
        }
    }
    if let Some(resolution) = &proposal.resolution {
        if resolution.decision.trim().is_empty()
            || resolution.decision.len() > 2048
            || resolution.facts.is_empty()
            || resolution.facts.len() > 8
            || resolution.facts.iter().any(|f| {
                f.literal.trim().is_empty()
                    || f.literal.len() > 1024
                    || f.source.ref_path.len() > 256
                    || f.source.hash.len() > 71
            })
        {
            return Err(invalid("ESCALATION_INPUT_LIMIT"));
        }
    }
    if let Some(authority) = &proposal.authority {
        if authority.command.trim().is_empty()
            || authority.command.len() > 128
            || authority.target.as_ref().is_some_and(|p| p.len() > 256)
        {
            return Err(invalid("ESCALATION_INPUT_LIMIT"));
        }
    }
    Ok(())
}

/// Observe the existing policy owner, including its live approval ledger. This
/// is diagnostic only; mutation must still enforce its own transactional gates.
pub(super) fn authority_snapshot(
    root: &Path,
    request: Option<&AuthorityRequest>,
) -> Result<Value, DecapodError> {
    let Some(request) = request else {
        return Ok(Value::Null);
    };
    let store = Store {
        kind: StoreKind::Repo,
        root: find_governance_root(root).join(".decapod/data"),
    };
    let path = store.root.join("RISKMAP.json");
    let map = match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_SOURCE_BYTES {
                return Err(invalid("SOURCE_LIMIT"));
            }
            serde_json::from_slice::<policy::RiskMap>(&bytes)
                .map_err(|_| invalid("POLICY_UNAVAILABLE: malformed risk map"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => policy::RiskMap { zones: vec![] },
        Err(e) => return Err(DecapodError::IoError(e)),
    };
    let (risk, requirements, required) =
        policy::eval_effective_policy(&store, &request.command, request.target.as_deref(), &map)?;
    let approved = if required {
        policy::check_approval(
            &store,
            &request.command,
            request.target.as_deref(),
            "global",
        )?
    } else {
        false
    };
    Ok(json!({"risk":risk, "requirements":requirements,
        "human_required":required && !approved, "approved":approved}))
}

pub(super) fn authority(plan: &GovernedPlan) -> Option<&AuthorityRequest> {
    plan.reflection
        .as_ref()?
        .receipts
        .last()?
        .outcome
        .escalation
        .as_ref()?
        .authority
        .as_ref()
}

pub(super) fn sources(root: &Path, request: &Request) -> Vec<SourceRef> {
    let mut sources = request.evidence.clone();
    if let Some(resolution) = request
        .escalation
        .as_ref()
        .and_then(|p| p.resolution.as_ref())
    {
        sources.extend(resolution.facts.iter().map(|f| f.source.clone()));
    }
    if request.escalation.is_some() && sources.is_empty() {
        let path = request
            .touched_paths
            .iter()
            .find(|p| root.join(p).is_file())
            .map(String::as_str)
            .or_else(|| root.join("README.md").is_file().then_some("README.md"))
            .unwrap_or(PLAN_REF);
        sources.push(SourceRef {
            ref_path: path.into(),
            hash: String::new(),
        });
    }
    sources
}

pub(super) fn assess(
    root: &Path,
    plan: &GovernedPlan,
    request: &Request,
    evidence: &[SourceRef],
    pending: &mut Vec<Challenge>,
) -> Result<Option<Assessment>, DecapodError> {
    let previous = plan
        .reflection
        .as_ref()
        .and_then(|j| j.receipts.last())
        .and_then(|r| r.outcome.escalation.as_ref());
    let Some(proposal) = &request.escalation else {
        if previous.is_some() {
            return Err(invalid("ESCALATION_CONTINUATION_REQUIRED"));
        }
        return Ok(None);
    };
    if proposal.intent != plan.intent {
        return Err(invalid("ESCALATION_INTENT_MISMATCH"));
    }
    let subject = digest(
        &json!({"intent":proposal.intent, "missing":proposal.missing,
        "uncertainty":proposal.uncertainty, "authority":proposal.authority}),
    )?;
    if previous.is_some_and(|p| p.subject_digest != subject) {
        return Err(invalid(
            "ESCALATION_SUBJECT_CHANGED: preserve the original obligation",
        ));
    }
    let policy = authority_snapshot(root, proposal.authority.as_ref())?;
    let hard_authority = policy.get("human_required").and_then(Value::as_bool) == Some(true);
    let authority_available = matches!(
        proposal.uncertainty,
        Uncertainty::MissingAuthority | Uncertainty::ExceptionalRisk
    ) && (policy.get("approved").and_then(Value::as_bool) == Some(true)
        || (proposal.authority.is_some()
            && proposal.uncertainty == Uncertainty::MissingAuthority
            && policy.get("human_required").and_then(Value::as_bool) == Some(false)));
    let existing_human = pending.iter().any(|c| c.trigger.starts_with("human_"));
    let matches_human_question = pending
        .iter()
        .any(|c| c.trigger.starts_with("human_") && c.question == bounded(&proposal.question));
    let mut observed_facts = 0;
    if let Some(resolution) = &proposal.resolution {
        let previous_focus = plan
            .reflection
            .as_ref()
            .and_then(|j| j.receipts.last())
            .and_then(|r| r.outcome.challenge.as_ref());
        for fact in &resolution.facts {
            if fact.source.ref_path == PLAN_REF {
                return Err(invalid("SOURCE_FACT_REQUIRED"));
            }
            check_path(root, &fact.source.ref_path)?;
            let path = root.join(&fact.source.ref_path);
            if !path.is_file() {
                return Err(invalid("SOURCE_FACT_NOT_OBSERVED"));
            }
            let mut text = String::new();
            fs::File::open(path)?
                .take(MAX_SOURCE_BYTES + 1)
                .read_to_string(&mut text)?;
            if text.len() as u64 > MAX_SOURCE_BYTES {
                return Err(invalid("SOURCE_LIMIT"));
            }
            if !text.contains(&fact.literal) {
                return Err(invalid("SOURCE_FACT_NOT_OBSERVED"));
            }
            if previous_focus
                .is_none_or(|c| c.sources.iter().any(|s| s.ref_path == fact.source.ref_path))
            {
                observed_facts += 1;
            }
        }
    }
    if hard_authority {
        let mut gate = challenge("human_policy_authority", "Which explicit authorization permits the proposed operation under the observed policy?".into(),
            SourceRef { ref_path: PLAN_REF.into(), hash:digest(&plan_contract(plan)?)? }, true)?;
        gate.investigation = "An existing policy requires human authority for the exact operation. The agent must obtain the decision through the existing approval surface; do not spend reasoning rounds trying to infer a grant. This response issues no approval.".into();
        pending.insert(0, gate);
    } else if !existing_human {
        let supported_decision = authority_available
            || observed_facts > 0
                && matches!(
                    proposal.uncertainty,
                    Uncertainty::DiscoverableInformation
                        | Uncertainty::InferableDecision
                        | Uncertainty::VerifiableUncertainty
                );
        if !supported_decision {
            let source = evidence
                .iter()
                .find(|s| s.ref_path != PLAN_REF)
                .cloned()
                .or_else(|| {
                    request
                        .touched_paths
                        .iter()
                        .find(|p| root.join(p).is_file())
                        .map(|p| SourceRef {
                            ref_path: p.clone(),
                            hash: String::new(),
                        })
                })
                .unwrap_or_else(|| SourceRef {
                    ref_path: PLAN_REF.into(),
                    hash: String::new(),
                });
            let source = SourceRef {
                hash: if source.ref_path == PLAN_REF {
                    digest(&plan_contract(plan)?)?
                } else {
                    source_hash(root, &source.ref_path)?
                },
                ..source
            };
            let mut gap = challenge("escalation_investigation", "Which observed source fact or admissible governing decision resolves the missing information?".into(), source, false)?;
            if !evidence.is_empty() {
                gap.sources = evidence.to_vec();
            }
            gap.investigation = match proposal.uncertainty {
                Uncertainty::VerifiableUncertainty => format!("Test the competing hypotheses by inspection or the existing proof hooks: {:?}. Exact source predicates can establish only the inspected byte fact. A quoted or declared test pass is never evidence of execution; behavioral verification stays with the native proof owner.", plan.proof_hooks),
                Uncertainty::IrreducibleAmbiguity => "Inspect the cited governing intent and relevant specifications. A self-declared ambiguity is not evidence of an irreducible business requirement. Use the existing plan human-question or spec-review surface to record an actual human dependency, with provenance; do not manufacture a gate for routine engineering work.".into(),
                Uncertainty::MissingAuthority | Uncertainty::ExceptionalRisk => "Inspect existing authority using govern policy eval for the exact command and target. Supply that operation in authority. A concern is not an approval grant; unsupported authority remains unresolved and must never be inferred.".into(),
                _ => "Inspect the cited repository source, conventions and constraints; choose a defensible resolution. Return its concise decision and exact observed source facts with current identities. Descriptions of investigation and reworded answers are not observed evidence. Source facts do not prove semantic correctness or test execution.".into(),
            };
            pending.push(gap);
        }
    }
    Ok(Some(Assessment {
        schema_version:"decapod.escalation.v1".into(), declared_uncertainty:proposal.uncertainty,
        reason_code: if hard_authority { ReasonCode::MissingAuthorization }
            else if existing_human && !matches_human_question { ReasonCode::ProposedQuestionDoesNotMatchGate }
            else if existing_human { ReasonCode::ExistingHumanDependency }
            else if authority_available { ReasonCode::AuthorityAvailable }
            else if observed_facts > 0 { ReasonCode::ObservedSourcePredicate }
            else { match proposal.uncertainty {
                Uncertainty::DiscoverableInformation => ReasonCode::DiscoverableSource,
                Uncertainty::InferableDecision => ReasonCode::DecisionEvidenceAvailable,
                Uncertainty::VerifiableUncertainty => ReasonCode::VerificationRequired,
                Uncertainty::IrreducibleAmbiguity => ReasonCode::IrreducibilityUnproven,
                _ => ReasonCode::AuthorityNotEstablished,
            } },
        human_escalation_permitted:hard_authority || matches_human_question, semantic_exhaustion_proven:false,
        observed_facts, subject_digest:subject, sources:evidence.to_vec(),
        limitations:"Literal source facts and observed policy can be checked deterministically. Relevance, exhaustive search, inferred intent and semantic reasoning quality remain unproven. The invoking agent owns those judgments; existing execution, proof and publication gates remain independent. An external harness must invoke this protocol to be governed.".into(),
        authority:proposal.authority.clone(),
    }))
}
