//! Bounded conformance for native proof calls actually mediated in this process.
//! Recorded loops and imported JSON are never constructors of ObservedPath.
//! This extends existing custody/runner evidence; it observes no arbitrary shell
//! history and creates no grant, general event store or independent host identity.
use super::ExecutionStateBinding;
use crate::core::{
    custody::IntentRecord,
    readiness_evidence::{self, MaterialIdentity},
    validation_rules::{
        EvaluationPhase, RuleAuthority, RuleRemediation, RuleSeverity, RuleVerdict,
    },
};
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: String,
    /// Only native_proof_calls is supported; no whole-host coverage is implied.
    pub scope: String,
    pub required: bool,
}
impl Policy {
    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.schema_version != "1.0.0" || self.scope != "native_proof_calls" {
            return Err(invalid(
                "OBSERVED_PATH_UNSUPPORTED: only schema1.0.0 native_proof_calls is supported; retain unknown host coverage",
            ));
        }
        Ok(())
    }
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub observation_run_id: String,
    pub proof_plan_digest: String,
    pub subject: MaterialIdentity,
    pub execution: ExecutionStateBinding,
    pub instruction_revision: String,
    pub actor: String,
    pub lease_generation: i64,
    pub producer: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub sequence: u64,
    pub operation_id: String,
    pub previous: Option<String>,
    pub event_id: String,
    pub check: readiness_evidence::CheckKey,
    pub kind: String,
    pub outcome: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Judgment {
    pub schema_version: String,
    pub required: bool,
    pub scope: String,
    pub rule_id: String,
    pub authority: RuleAuthority,
    pub observer_authority: String,
    pub phase: EvaluationPhase,
    pub severity: RuleSeverity,
    pub source: String,
    pub rule_remediation: RuleRemediation,
    pub process_conformance: RuleVerdict,
    pub observation_coverage: RuleVerdict,
    pub full_host_coverage: RuleVerdict,
    pub binding: Option<Binding>,
    pub events: Vec<Event>,
    pub reason: String,
}
impl Judgment {
    fn normalize(mut self) -> Self {
        self.rule_remediation.error_code = match self.process_conformance {
            RuleVerdict::Pass => "OBSERVED_PATH_OBSERVED",
            RuleVerdict::Fail => "OBSERVED_PATH_FAILED",
            RuleVerdict::Stale => "OBSERVED_PATH_STALE",
            RuleVerdict::Unverifiable => "OBSERVED_PATH_NOT_PROVEN",
        }
        .into();
        self.rule_remediation.violation_description =
            if self.process_conformance == RuleVerdict::Pass {
                String::new()
            } else {
                self.reason.clone()
            };
        self.rule_remediation.observed_condition = self.reason.clone();
        self
    }
    pub fn unavailable(required: bool, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            schema_version: "1.0.0".into(),
            required,
            scope: "native_proof_calls".into(),
            rule_id:"project.observed_native_path".into(),
            authority:RuleAuthority::ProjectOverride,
            observer_authority: "kernel_mediated_native_runner".into(),
            phase:EvaluationPhase::Validation,
            severity:if required {RuleSeverity::Error} else {RuleSeverity::Advisory},
            source:".decapod/config.toml#/proof/observed_path".into(),
            rule_remediation:RuleRemediation {
                error_code:"OBSERVED_PATH_NOT_PROVEN".into(),violation_description:reason.clone(),
                expected_condition:"Current owned native authorization and terminal-result observations under the independently accepted policy".into(),
                observed_condition:reason.clone(),affected_paths:vec![".decapod/config.toml#/proof/observed_path".into()],
                required_steps:vec!["Restore the applicable plan and current task lease, retain the accepted path requirement, then rerun native validation through its authorizing adapter".into()],
                autonomous_remediation_permitted:true,human_authorization_required:false,
                evidence_for_revalidation:vec!["Fresh actual native outcomes bound to current source/base/instructions/plan/policy/actor/lease".into()],related_rules:vec![],
            },
            process_conformance: RuleVerdict::Unverifiable,
            observation_coverage: RuleVerdict::Unverifiable,
            full_host_coverage: RuleVerdict::Unverifiable,
            binding: None,
            events: vec![],
            reason,
        }
    }
    pub(crate) fn permits(&self) -> bool {
        !self.required || self.process_conformance == RuleVerdict::Pass
    }
    pub(crate) fn recheck_current(&mut self, current: &MaterialIdentity, accepted_target: &str) {
        if self.binding.as_ref().is_some_and(|binding| {
            binding.subject != *current || binding.subject.accepted_base_revision != accepted_target
        }) {
            self.process_conformance = RuleVerdict::Stale;
            self.reason="OBSERVED_PATH_STALE: native observation is historical after current subject or accepted target movement".into();
            self.rule_remediation.error_code = "OBSERVED_PATH_STALE".into();
            self.rule_remediation.violation_description = self.reason.clone();
            self.rule_remediation.observed_condition = self.reason.clone();
        }
    }
    pub(crate) fn invalidate(&mut self, reason: &str) {
        self.process_conformance = RuleVerdict::Stale;
        self.reason = reason.into();
        self.rule_remediation.error_code = "OBSERVED_PATH_STALE".into();
        self.rule_remediation.violation_description = self.reason.clone();
        self.rule_remediation.observed_condition = self.reason.clone();
    }
    /// Structural verification of retained evidence is not a producer importer.
    pub(crate) fn verify_retained(
        &self,
        subject: &MaterialIdentity,
        suite: &readiness_evidence::SuiteEvidence,
    ) -> io::Result<()> {
        let binding = self.binding.as_ref().ok_or_else(|| {
            invalid("OBSERVED_PATH_NOT_PROVEN: retained observation has no binding")
        })?;
        if self.schema_version != "1.0.0"
            || self.scope != "native_proof_calls"
            || self.authority != RuleAuthority::ProjectOverride
            || self.observer_authority != "kernel_mediated_native_runner"
            || binding.subject != *subject
            || binding.proof_plan_digest != suite.expected_plan_digest
            || self.events.len() > 2048
            || self.events.is_empty()
            || self.full_host_coverage != RuleVerdict::Unverifiable
            || self.process_conformance != RuleVerdict::Pass
            || self.observation_coverage != RuleVerdict::Pass
        {
            return Err(invalid(
                "OBSERVED_PATH_STALE: unsupported, incomplete or different retained subject",
            ));
        }
        let mut previous = None;
        if suite.results.iter().any(|row| {
            matches!(
                row.observation,
                readiness_evidence::Observation::Executed { .. }
            ) && !matches!(
                row.observation,
                readiness_evidence::Observation::Executed {
                    exit_code: Some(0),
                    conclusion: readiness_evidence::Conclusion::Success,
                    ..
                }
            )
        }) {
            return Err(invalid(
                "OBSERVED_PATH_FAILED: witnessed runner outcome contradicts a successful path claim",
            ));
        }
        let mut authorized = std::collections::BTreeSet::new();
        let mut completed = std::collections::BTreeSet::new();
        for (index, event) in self.events.iter().enumerate() {
            let expected = readiness_evidence::digest(&(
                binding,
                event.sequence,
                &event.previous,
                &event.check,
                event.operation_id.as_str(),
                event.kind.as_str(),
                event.outcome.as_str(),
            ))
            .map_err(io::Error::other)?;
            if event.sequence != index as u64 + 1
                || event.previous != previous
                || event.event_id != expected
            {
                return Err(invalid(
                    "OBSERVED_PATH_HISTORY_INVALID: event ordering or payload changed",
                ));
            }
            let key = (event.check.clone(), event.operation_id.clone());
            match (event.kind.as_str(), event.outcome.as_str()) {
                ("authorization", "allowed") if authorized.insert(key.clone()) => (),
                ("result", "success") if authorized.contains(&key) && completed.insert(key) => (),
                _ => {
                    return Err(invalid(
                        "OBSERVED_PATH_NOT_PROVEN: missing, duplicate or non-successful mediated outcome",
                    ));
                }
            }
            previous = Some(event.event_id.clone());
        }
        let expected: std::collections::BTreeSet<_> = suite
            .results
            .iter()
            .filter(|r| {
                matches!(
                    r.observation,
                    readiness_evidence::Observation::Executed { .. }
                )
            })
            .map(|r| (r.key.clone(), r.invocation_digest.clone()))
            .collect();
        if authorized != completed || completed != expected {
            return Err(invalid(
                "OBSERVED_PATH_NOT_PROVEN: incomplete native invocation coverage",
            ));
        }
        Ok(())
    }
}

/// Owned by the actual native adapter; deliberately not Deserialize or Clone.
pub(crate) struct ObservedPath {
    binding: Binding,
    events: Vec<Event>,
}

/// A live runner result. Serialized Judgment is a read model, never this type.
pub struct OwnedObservation {
    judgment: Judgment,
}
impl OwnedObservation {
    pub(crate) fn judgment(&self) -> &Judgment {
        &self.judgment
    }
}

pub(crate) fn capture(
    root: &Path,
    subject: &MaterialIdentity,
    observation_run_id: &str,
    proof_plan_digest: &str,
) -> io::Result<Binding> {
    if observation_run_id.is_empty()
        || observation_run_id.len() > 128
        || !observation_run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(invalid(
            "OBSERVED_PATH_NOT_PROVEN: a fresh bounded runtime observation ID is required",
        ));
    }
    let trajectory = super::load_trajectory_cookie(root)
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            invalid("OBSERVED_PATH_NOT_PROVEN: current task-bound trajectory is missing")
        })?;
    let task = trajectory
        .task_id
        .as_deref()
        .ok_or_else(|| invalid("OBSERVED_PATH_NOT_PROVEN: task identity is missing"))?;
    let execution = super::capture_execution_binding(root, task).map_err(io::Error::other)?;
    if execution.head_revision != subject.head_revision
        || execution.work_branch != subject.work_branch
    {
        return Err(invalid(
            "OBSERVED_PATH_STALE: execution subject differs from current proof subject",
        ));
    }
    let intent: &IntentRecord = trajectory
        .custody
        .intents
        .get(&execution.intent_id)
        .ok_or_else(|| {
            invalid("OBSERVED_PATH_NOT_PROVEN: recorded instruction lineage is missing")
        })?;
    let owner = crate::core::workspace::get_main_repo_root(root).map_err(io::Error::other)?;
    let actor = crate::current_agent_id();
    if actor.trim().is_empty() || actor == "unknown" {
        return Err(invalid(
            "OBSERVED_PATH_NOT_PROVEN: an explicit currently claimed actor is required",
        ));
    }
    let custody =
        crate::core::todo::capture_execution_custody(&owner.join(".decapod/data"), task, &actor)
            .map_err(io::Error::other)?;
    let lease_generation = custody["lease_generation"]
        .as_i64()
        .filter(|g| *g > 0)
        .ok_or_else(|| invalid("OBSERVED_PATH_NOT_PROVEN: live lease generation is missing"))?;
    Ok(Binding {
        observation_run_id: observation_run_id.into(),
        proof_plan_digest: proof_plan_digest.into(),
        subject: subject.clone(),
        execution,
        instruction_revision: intent.instruction_revision().map_err(io::Error::other)?,
        actor,
        lease_generation,
        producer: format!("decapod-native-runner/{}", env!("CARGO_PKG_VERSION")),
    })
}

impl ObservedPath {
    pub(crate) fn new(binding: Binding) -> Self {
        Self {
            binding,
            events: vec![],
        }
    }
    pub(crate) fn lost(self, required: bool, reason: String) -> OwnedObservation {
        OwnedObservation {
            judgment: Judgment {
                process_conformance: RuleVerdict::Stale,
                binding: Some(self.binding),
                events: self.events,
                ..Judgment::unavailable(required, format!("OBSERVED_PATH_STALE: {reason}"))
            }
            .normalize(),
        }
    }
    /// Called only after the existing native authorizer has returned success.
    pub(crate) fn authorized(
        &mut self,
        key: &readiness_evidence::CheckKey,
        invocation: &str,
    ) -> io::Result<()> {
        self.event(key, invocation, "authorization", "allowed")
    }
    fn event(
        &mut self,
        key: &readiness_evidence::CheckKey,
        operation: &str,
        kind: &str,
        outcome: &str,
    ) -> io::Result<()> {
        if self.events.len() >= 2048 {
            return Err(invalid("OBSERVED_PATH_LIMIT: native event bound exceeded"));
        }
        let sequence = self.events.len() as u64 + 1;
        let previous = self.events.last().map(|e| e.event_id.clone());
        let event_id = readiness_evidence::digest(&(
            &self.binding,
            sequence,
            &previous,
            key,
            operation,
            kind,
            outcome,
        ))
        .map_err(io::Error::other)?;
        self.events.push(Event {
            sequence,
            operation_id: operation.into(),
            previous,
            event_id,
            check: key.clone(),
            kind: kind.into(),
            outcome: outcome.into(),
        });
        Ok(())
    }
    /// Called on actual terminal runner observations, never agent loop results.
    pub(crate) fn completed(
        mut self,
        suite: &readiness_evidence::SuiteEvidence,
        current: Binding,
        required: bool,
    ) -> io::Result<OwnedObservation> {
        if current != self.binding
            || suite.expected_plan_digest != self.binding.proof_plan_digest
            || suite.before != self.binding.subject
            || suite.after != current.subject
        {
            return Ok(OwnedObservation {
                judgment: Judgment {
                    process_conformance: RuleVerdict::Stale,
                    binding: Some(self.binding),
                    events: self.events,
                    ..Judgment::unavailable(
                        required,
                        "OBSERVED_PATH_STALE: source, base, instructions, plan, policy, actor or lease changed during execution",
                    )
                }.normalize(),
            });
        }
        for result in &suite.results {
            if matches!(
                result.observation,
                readiness_evidence::Observation::NotApplicable { .. }
            ) {
                continue;
            }
            let operation = result.invocation_digest.as_str();
            if !self.events.iter().any(|e| {
                e.check == result.key && e.operation_id == operation && e.kind == "authorization"
            }) {
                return Err(invalid(
                    "OBSERVED_PATH_NOT_PROVEN: runner outcome has no matching in-process authorization",
                ));
            }
            let outcome = match &result.observation {
                readiness_evidence::Observation::Executed { exit_code, .. } => match exit_code {
                    Some(0) => "success",
                    Some(_) => "failed",
                    None => "unknown",
                },
                _ => "unknown",
            };
            self.event(&result.key, operation, "result", outcome)?;
        }
        let complete = !self.events.is_empty()
            && self
                .events
                .iter()
                .filter(|e| e.kind == "authorization")
                .all(|allowed| {
                    self.events.iter().any(|result| {
                        result.kind == "result"
                            && result.operation_id == allowed.operation_id
                            && result.check == allowed.check
                            && result.outcome != "unknown"
                    })
                });
        let failed = self
            .events
            .iter()
            .any(|e| e.kind == "result" && e.outcome == "failed");
        Ok(OwnedObservation {
            judgment: Judgment {
                process_conformance: if failed {
                    RuleVerdict::Fail
                } else if complete {
                    RuleVerdict::Pass
                } else {
                    RuleVerdict::Unverifiable
                },
                observation_coverage: if complete {
                    RuleVerdict::Pass
                } else {
                    RuleVerdict::Unverifiable
                },
                binding: Some(self.binding),
                events: self.events,
                ..Judgment::unavailable(
                    required,
                    "Native invocation authorization and terminal results only; application proof is separate, unobserved host activity and independent human identity remain unproven",
                )
            }.normalize(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        let subject:MaterialIdentity = serde_json::from_value(serde_json::json!({
            "repository":"owner/repo","work_branch":"agent/fixture/task","head_revision":"a".repeat(40),"base_branch":"main","accepted_base_revision":"b".repeat(40),
            "history_digest":"history","tree_digest":"tree","configuration_digest":"config","workflow_digest":"workflow","toolchain_digest":"tools","candidate_digest":"candidate"
        })).unwrap();
        let execution:ExecutionStateBinding = serde_json::from_value(serde_json::json!({
            "schema_version":"1.0.0","task_id":"task","run_id":"run","intent_id":"intent","head_revision":subject.head_revision,"work_branch":subject.work_branch,
            "material_digest":"material","plan_digest":"plan","trajectory_digest":"trajectory","validation_epoch_digest":"epoch","governance_digest":"governance","policy_digest":"policy","task_custody_digest":"custody","required_approvals":[]
        })).unwrap();
        Binding {
            observation_run_id: "scripted-test-run".into(),
            proof_plan_digest: "plan".into(),
            subject,
            execution,
            instruction_revision: "instruction-revision".into(),
            actor: "actor".into(),
            lease_generation: 1,
            producer: "decapod-native-runner/fixture".into(),
        }
    }
    fn suite(binding: &Binding, code: Option<i32>) -> readiness_evidence::SuiteEvidence {
        let key = readiness_evidence::CheckKey {
            provider: readiness_evidence::Provider::Native,
            workflow: ".decapod/config.toml".into(),
            job: "check".into(),
            matrix: Default::default(),
            profile: "repo.pr-readiness.v1".into(),
        };
        readiness_evidence::SuiteEvidence {
            schema_version: readiness_evidence::SCHEMA_VERSION.into(),
            expected_plan_digest: "plan".into(),
            before: binding.subject.clone(),
            after: binding.subject.clone(),
            facts: readiness_evidence::ApplicabilityFacts {
                event: "pull_request".into(),
                target_branch: "main".into(),
                changed_paths: Default::default(),
            },
            results: vec![readiness_evidence::CheckResult {
                key,
                definition_digest: "definition".into(),
                invocation_digest: "invocation".into(),
                toolchain_digest: "tools".into(),
                executor: readiness_evidence::Provider::Native,
                identity: binding.subject.clone(),
                observation: readiness_evidence::Observation::Executed {
                    execution_id: "actual-test-runner-fixture".into(),
                    conclusion: if code == Some(0) {
                        readiness_evidence::Conclusion::Success
                    } else {
                        readiness_evidence::Conclusion::Failure
                    },
                    exit_code: code,
                    output_digest: "output".into(),
                    artifact_digest: None,
                },
            }],
        }
    }
    fn authorized(binding: Binding, suite: &readiness_evidence::SuiteEvidence) -> ObservedPath {
        let mut path = ObservedPath::new(binding);
        path.authorized(&suite.results[0].key, &suite.results[0].invocation_digest)
            .unwrap();
        path
    }
    #[test]
    fn attempt_unknown_failure_and_fresh_repair_are_distinct() {
        // Scripted trust-seam unit data. Actual execution is covered separately
        // by native fixtures; these values do not claim a host observation.
        let binding = binding();
        let green = suite(&binding, Some(0));
        assert!(
            ObservedPath::new(binding.clone())
                .completed(&green, binding.clone(), true)
                .is_err()
        );
        let unknown = suite(&binding, None);
        let incomplete = authorized(binding.clone(), &unknown)
            .completed(&unknown, binding.clone(), true)
            .unwrap();
        assert_eq!(
            incomplete.judgment().process_conformance,
            RuleVerdict::Unverifiable
        );
        assert!(!incomplete.judgment().permits());
        let red = suite(&binding, Some(1));
        let failure = authorized(binding.clone(), &red)
            .completed(&red, binding.clone(), true)
            .unwrap();
        assert_eq!(failure.judgment().process_conformance, RuleVerdict::Fail);
        let repaired = authorized(binding.clone(), &green)
            .completed(&green, binding.clone(), true)
            .unwrap();
        assert_eq!(repaired.judgment().process_conformance, RuleVerdict::Pass);
        assert_eq!(failure.judgment().process_conformance, RuleVerdict::Fail);
        repaired
            .judgment()
            .verify_retained(&binding.subject, &green)
            .unwrap();
        assert_eq!(
            repaired.judgment().full_host_coverage,
            RuleVerdict::Unverifiable
        );
    }
    #[test]
    fn changed_custody_source_or_producer_and_reordered_events_cannot_replay() {
        let original = binding();
        let suite = suite(&original, Some(0));
        let mut changes = vec![];
        let mut value = original.clone();
        value.observation_run_id = "restarted-harness".into();
        changes.push(value);
        let mut value = original.clone();
        value.proof_plan_digest = "different-required-checks".into();
        changes.push(value);
        let mut value = original.clone();
        value.subject.head_revision = "c".repeat(40);
        changes.push(value);
        let mut value = original.clone();
        value.subject.accepted_base_revision = "d".repeat(40);
        changes.push(value);
        let mut value = original.clone();
        value.execution.plan_digest = "new-plan".into();
        changes.push(value);
        let mut value = original.clone();
        value.execution.policy_digest = "new-policy".into();
        changes.push(value);
        let mut value = original.clone();
        value.execution.task_id = "other-task".into();
        changes.push(value);
        let mut value = original.clone();
        value.instruction_revision = "new-instructions".into();
        changes.push(value);
        let mut value = original.clone();
        value.actor = "other-actor".into();
        changes.push(value);
        let mut value = original.clone();
        value.lease_generation = 2;
        changes.push(value);
        let mut value = original.clone();
        value.producer = "other-host".into();
        changes.push(value);
        for changed in changes {
            let replay = authorized(original.clone(), &suite)
                .completed(&suite, changed, true)
                .unwrap();
            assert_eq!(replay.judgment().process_conformance, RuleVerdict::Stale);
            assert!(!replay.judgment().permits());
        }
        let observed = authorized(original.clone(), &suite)
            .completed(&suite, original.clone(), true)
            .unwrap();
        let mut rewritten = observed.judgment().clone();
        rewritten.events.swap(0, 1);
        assert!(
            rewritten
                .verify_retained(&original.subject, &suite)
                .is_err()
        );
        let mut omitted = observed.judgment().clone();
        omitted.events.remove(0);
        assert!(omitted.verify_retained(&original.subject, &suite).is_err());
        assert!(Judgment::unavailable(false, "optional coverage unknown").permits());
        assert!(!Judgment::unavailable(true, "required coverage missing").permits());
    }
}
