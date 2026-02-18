use crate::core::error::DecapodError;
use crate::core::mentor::{MentorEngine, Obligation, ObligationKind, ObligationsContext};
use crate::core::rpc::{
    Advisory, Attestation, IntentExecutionPlan, Interlock, LoopSignal, OneShotTodo,
    ReconciliationPointer, StrategyOutline, TechDecisionCheck,
};
use crate::core::workspace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const INTERLOCK_WORKSPACE_REQUIRED: &str = "workspace_required";
pub const INTERLOCK_VERIFICATION_REQUIRED: &str = "verification_required";
pub const INTERLOCK_STORE_BOUNDARY_VIOLATION: &str = "store_boundary_violation";
pub const INTERLOCK_DECISION_REQUIRED: &str = "decision_required";
pub const INTERLOCK_INTENT_REQUIRED: &str = "intent_required";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssurancePhase {
    Plan,
    Build,
    Verify,
    Complete,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssuranceEvaluateInput {
    pub op: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub touched_paths: Vec<String>,
    #[serde(default)]
    pub diff_summary: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub phase: Option<AssurancePhase>,
    #[serde(default)]
    pub time_budget_s: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssuranceEvaluateResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interlock: Option<Interlock>,
    pub advisory: Advisory,
    pub attestation: Attestation,
}

pub struct AssuranceEngine {
    repo_root: PathBuf,
}

impl AssuranceEngine {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }

    pub fn evaluate(
        &self,
        input: &AssuranceEvaluateInput,
    ) -> Result<AssuranceEvaluateResult, DecapodError> {
        let mentor = MentorEngine::new(&self.repo_root);
        let high_risk = mentor.is_high_risk_op(&input.op, &input.touched_paths);
        let obligations_ctx = ObligationsContext {
            op: input.op.clone(),
            params: input.params.clone(),
            touched_paths: input.touched_paths.clone(),
            diff_summary: input.diff_summary.clone(),
            project_profile_id: None,
            session_id: input.session_id.clone(),
            high_risk,
        };
        let obligations = mentor.compute_obligations(&obligations_ctx)?;
        let workspace_status = workspace::get_workspace_status(&self.repo_root)?;

        let mut must = obligations
            .must
            .iter()
            .map(Self::obligation_to_pointer)
            .collect::<Vec<_>>();
        let mut recommended = obligations
            .recommended
            .iter()
            .map(Self::obligation_to_pointer)
            .collect::<Vec<_>>();

        must.insert(
            0,
            ReconciliationPointer {
                kind: "workspace".to_string(),
                r#ref: workspace_status
                    .git
                    .worktree_path
                    .as_ref()
                    .unwrap_or(&self.repo_root)
                    .to_string_lossy()
                    .to_string(),
                title: format!("Workspace branch: {}", workspace_status.git.current_branch),
                why_short: "Workspace and protected branch state always govern execution"
                    .to_string(),
                evidence: crate::core::rpc::EvidenceRef {
                    source: "workspace".to_string(),
                    id: workspace_status.git.current_branch.clone(),
                    hash: None,
                },
            },
        );

        Self::dedupe_and_cap(&mut must);
        Self::dedupe_and_cap(&mut recommended);

        let verification_plan = crate::core::rpc::VerificationPlan {
            required: vec![
                "decapod validate".to_string(),
                "cargo test --locked".to_string(),
                "Compare observed outputs against docs/spec.md expectations".to_string(),
            ],
            checklist: vec![
                "Run each required proof command in order".to_string(),
                "Read full command output, not just exit status".to_string(),
                "Confirm failures are resolved and rerun until clean".to_string(),
                "Only mark complete after validate passes".to_string(),
            ],
        };

        let interlock = self.resolve_interlock(input, &obligations, &workspace_status);
        let loop_signal = self.detect_loop_signal()?;
        let env_notes = vec![
            format!("repo_root={}", self.repo_root.display()),
            format!(
                "workspace_path={}",
                workspace_status
                    .git
                    .worktree_path
                    .as_ref()
                    .unwrap_or(&self.repo_root)
                    .display()
            ),
            format!(
                "tool_summary=docker_available:{} in_container:{}",
                workspace_status.container.docker_available,
                workspace_status.container.in_container
            ),
            "done_means=decapod validate passes".to_string(),
        ];

        let advisory = Advisory {
            reconciliations: crate::core::rpc::ReconciliationSets { must, recommended },
            verification_plan,
            intent_prompts: self.build_intent_prompts(
                input,
                interlock.as_ref(),
                &obligations,
                loop_signal.as_ref(),
            ),
            intent_execution_plan: Some(self.build_intent_execution_plan(input)),
            strategy_outline: Some(self.build_strategy_outline(input)),
            loop_signal,
            notes: Some(env_notes),
        };

        let attestation = self.write_attestation(input, interlock.as_ref())?;
        Ok(AssuranceEvaluateResult {
            interlock,
            advisory,
            attestation,
        })
    }

    fn resolve_interlock(
        &self,
        input: &AssuranceEvaluateInput,
        obligations: &crate::core::mentor::Obligations,
        status: &workspace::WorkspaceStatus,
    ) -> Option<Interlock> {
        if self.has_store_boundary_violation(input) {
            return Some(Interlock {
                code: INTERLOCK_STORE_BOUNDARY_VIOLATION.to_string(),
                message:
                    "Direct .decapod/data mutation requested outside allowed control-plane ops"
                        .to_string(),
                unblock_ops: vec![
                    "todo.add".to_string(),
                    "todo.claim".to_string(),
                    "todo.done".to_string(),
                    "assurance.evaluate".to_string(),
                ],
                evidence: Some(serde_json::json!({ "touched_paths": input.touched_paths })),
            });
        }

        if self.requires_completion_proof(input) && !self.has_completion_proofs(input) {
            return Some(Interlock {
                code: INTERLOCK_VERIFICATION_REQUIRED.to_string(),
                message: "Completion is blocked until required proofs have run".to_string(),
                unblock_ops: vec![
                    "qa.check".to_string(),
                    "validate".to_string(),
                    "assurance.evaluate".to_string(),
                ],
                evidence: Some(serde_json::json!({ "phase": input.phase })),
            });
        }

        if self.requires_mandatory_decision(input, obligations) {
            return Some(Interlock {
                code: INTERLOCK_DECISION_REQUIRED.to_string(),
                message: "Mandatory decision must be reconciled before proceeding".to_string(),
                unblock_ops: vec![
                    "scaffold.next_question".to_string(),
                    "mentor.obligations".to_string(),
                    "assurance.evaluate".to_string(),
                ],
                evidence: Some(serde_json::json!({
                    "contradictions": obligations.contradictions,
                    "touched_paths": input.touched_paths
                })),
            });
        }

        if self.requires_workspace_interlock(input) && (!status.can_work || status.git.is_protected)
        {
            return Some(Interlock {
                code: INTERLOCK_WORKSPACE_REQUIRED.to_string(),
                message: format!(
                    "Meaningful op '{}' is blocked outside a valid isolated workspace",
                    input.op
                ),
                unblock_ops: vec![
                    "workspace.ensure".to_string(),
                    "workspace.status".to_string(),
                ],
                evidence: Some(serde_json::json!({
                    "branch": status.git.current_branch,
                    "is_protected": status.git.is_protected,
                    "in_container": status.container.in_container,
                    "docker_available": status.container.docker_available,
                })),
            });
        }

        if self.requires_user_intent(input) && !self.has_user_intent(input) {
            return Some(Interlock {
                code: INTERLOCK_INTENT_REQUIRED.to_string(),
                message: "User intent is missing for plan/build work. Ask the user: 1) What pain are we solving first? 2) What desired outcome proves success? 3) What constraints are non-negotiable? 4) What quality bar defines industry-grade output?"
                    .to_string(),
                unblock_ops: vec![
                    "mentor.obligations".to_string(),
                    "scaffold.next_question".to_string(),
                    "assurance.evaluate".to_string(),
                ],
                evidence: Some(serde_json::json!({
                    "required_fields_any_of": ["user_intent", "mission", "goal", "idea"],
                    "suggested_questions": [
                        "What user pain is highest priority?",
                        "What concrete outcome will make this successful?",
                        "What constraints are non-negotiable?",
                        "What quality bar should this meet?"
                    ],
                    "op": input.op
                })),
            });
        }

        None
    }

    fn has_store_boundary_violation(&self, input: &AssuranceEvaluateInput) -> bool {
        let allowed_control_ops = ["todo.", "federation.", "data.", "agent.", "assurance."];
        let op_allowed = allowed_control_ops
            .iter()
            .any(|prefix| input.op.starts_with(prefix));
        if op_allowed {
            return false;
        }
        input
            .touched_paths
            .iter()
            .any(|p| p.starts_with(".decapod/data/") || p.contains("/.decapod/data/"))
    }

    fn requires_workspace_interlock(&self, input: &AssuranceEvaluateInput) -> bool {
        !matches!(
            input.op.as_str(),
            "agent.init" | "workspace.status" | "assurance.evaluate" | "mentor.obligations"
        )
    }

    fn requires_completion_proof(&self, input: &AssuranceEvaluateInput) -> bool {
        matches!(input.phase, Some(AssurancePhase::Complete))
            || input.op.contains("complete")
            || input.op == "todo.done"
    }

    fn requires_user_intent(&self, input: &AssuranceEvaluateInput) -> bool {
        if matches!(
            input.phase,
            Some(AssurancePhase::Plan) | Some(AssurancePhase::Build)
        ) {
            return true;
        }
        matches!(input.op.as_str(), "plan" | "build")
    }

    fn has_user_intent(&self, input: &AssuranceEvaluateInput) -> bool {
        ["user_intent", "mission", "goal", "idea"].iter().any(|k| {
            input
                .params
                .get(k)
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        })
    }

    fn has_completion_proofs(&self, input: &AssuranceEvaluateInput) -> bool {
        if input
            .params
            .get("proofs_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return true;
        }
        input
            .params
            .get("proofs")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    }

    fn requires_mandatory_decision(
        &self,
        input: &AssuranceEvaluateInput,
        obligations: &crate::core::mentor::Obligations,
    ) -> bool {
        let touches_auth = input
            .touched_paths
            .iter()
            .any(|p| p.to_lowercase().contains("auth"));
        let missing_auth_provider = input.params.get("auth_provider").is_none();
        (touches_auth && missing_auth_provider) || !obligations.contradictions.is_empty()
    }

    fn detect_loop_signal(&self) -> Result<Option<LoopSignal>, DecapodError> {
        let attestation_path = self
            .repo_root
            .join(".decapod")
            .join("generated")
            .join("assurance_attestations.jsonl");
        if !attestation_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&attestation_path).map_err(DecapodError::IoError)?;
        let mut file_counts: HashMap<String, usize> = HashMap::new();
        let mut interlock_counts: HashMap<String, usize> = HashMap::new();

        for line in content.lines().rev().take(40) {
            let parsed = serde_json::from_str::<Attestation>(line);
            if let Ok(att) = parsed {
                for path in &att.touched_paths {
                    *file_counts.entry(path.clone()).or_default() += 1;
                }
                if let Some(code) = &att.interlock_code {
                    *interlock_counts.entry(code.clone()).or_default() += 1;
                }
            }
        }

        let repeated_file = file_counts.into_iter().max_by_key(|(_, c)| *c);
        let repeated_gate = interlock_counts.into_iter().max_by_key(|(_, c)| *c);

        if let Some((path, count)) = repeated_file {
            if count >= 3 {
                return Ok(Some(LoopSignal {
                    code: "file_edit_loop".to_string(),
                    message: format!("Detected repeated edits on '{}'", path),
                    suggested_redirect_ops: vec![
                        "assurance.evaluate".to_string(),
                        "scaffold.next_question".to_string(),
                    ],
                }));
            }
        }

        if let Some((code, count)) = repeated_gate {
            if count >= 3 {
                return Ok(Some(LoopSignal {
                    code: "failing_gate_loop".to_string(),
                    message: format!("Detected repeated interlock '{}'", code),
                    suggested_redirect_ops: vec![
                        "mentor.obligations".to_string(),
                        "assurance.evaluate".to_string(),
                    ],
                }));
            }
        }

        Ok(None)
    }

    fn write_attestation(
        &self,
        input: &AssuranceEvaluateInput,
        interlock: Option<&Interlock>,
    ) -> Result<Attestation, DecapodError> {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let canonical_input = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
        let mut hasher = Sha256::new();
        hasher.update(canonical_input.as_bytes());
        let input_hash = format!("{:x}", hasher.finalize());

        let attestation = Attestation {
            id: ulid::Ulid::new().to_string(),
            op: "assurance.evaluate".to_string(),
            timestamp,
            input_hash,
            touched_paths: input.touched_paths.clone(),
            interlock_code: interlock.map(|i| i.code.clone()),
            outcome: if interlock.is_some() {
                "interlocked".to_string()
            } else {
                "ok".to_string()
            },
            trace_path: ".decapod/generated/assurance_attestations.jsonl".to_string(),
        };

        let attestation_path = self
            .repo_root
            .join(".decapod")
            .join("generated")
            .join("assurance_attestations.jsonl");
        if let Some(parent) = attestation_path.parent() {
            fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&attestation_path)
            .map_err(DecapodError::IoError)?;
        let line = serde_json::to_string(&attestation).unwrap_or_else(|_| "{}".to_string());
        file.write_all(line.as_bytes())
            .map_err(DecapodError::IoError)?;
        file.write_all(b"\n").map_err(DecapodError::IoError)?;

        Ok(attestation)
    }

    fn build_intent_prompts(
        &self,
        input: &AssuranceEvaluateInput,
        interlock: Option<&Interlock>,
        obligations: &crate::core::mentor::Obligations,
        loop_signal: Option<&LoopSignal>,
    ) -> Vec<String> {
        let mut prompts = vec![
            format!(
                "What is the user's desired outcome for '{}' in one sentence, and what evidence will prove it?",
                input.op
            ),
            "What is the smallest next action that advances user intent without increasing risk?"
                .to_string(),
        ];

        if !obligations.must.is_empty() {
            prompts.push(format!(
                "Which MUST obligation will you satisfy first, and how does that directly support the user's goal? (must_count={})",
                obligations.must.len()
            ));
        }

        match input.phase {
            Some(AssurancePhase::Plan) => prompts.push(
                "Before implementation, restate the user's intent and name the acceptance criteria in concrete terms."
                    .to_string(),
            ),
            Some(AssurancePhase::Build) => prompts.push(
                "While implementing, check that each file change maps to user intent; remove work that does not move the outcome."
                    .to_string(),
            ),
            Some(AssurancePhase::Verify) => prompts.push(
                "During verification, compare observed behavior against the user's requested outcome, not just command exit status."
                    .to_string(),
            ),
            Some(AssurancePhase::Complete) => prompts.push(
                "At completion, provide proof that the final state matches user intent and identify any remaining gaps."
                    .to_string(),
            ),
            None => {}
        }

        if let Some(interlock) = interlock {
            prompts.push(format!(
                "Interlock '{}' is active. What exact unblock action should run next to restore progress toward user intent?",
                interlock.code
            ));
        }

        if let Some(loop_signal) = loop_signal {
            prompts.push(format!(
                "Loop signal '{}' detected. What alternative approach better serves the same user outcome?",
                loop_signal.code
            ));
        }

        prompts.truncate(6);
        prompts
    }

    fn build_strategy_outline(&self, input: &AssuranceEvaluateInput) -> StrategyOutline {
        let mut required_facts = vec![
            "What user pain is highest priority to solve first?".to_string(),
            "What concrete user outcome defines success for this iteration?".to_string(),
            "What constraints are non-negotiable (time, budget, compliance, deployment)?"
                .to_string(),
            "What performance/reliability/security targets are required?".to_string(),
        ];

        if input.params.get("database").is_some() || input.params.get("db").is_some() {
            required_facts.push(
                "What workload characteristics justify this database choice (consistency, query shape, scale, ops)?"
                    .to_string(),
            );
        }
        if input.params.get("framework").is_some() {
            required_facts.push(
                "Does the requested framework maximize delivery speed and maintainability for this mission?"
                    .to_string(),
            );
        }

        let current_db = input
            .params
            .get("database")
            .or_else(|| input.params.get("db"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let current_framework = input
            .params
            .get("framework")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let decision_checks = vec![
            TechDecisionCheck {
                area: "database".to_string(),
                current_suggestion: current_db,
                alternatives_to_evaluate: vec![
                    "postgres".to_string(),
                    "sqlite".to_string(),
                    "mysql".to_string(),
                    "document-db".to_string(),
                ],
                decide_by: "Select the option that best satisfies data model, consistency, operations, and cost constraints from user intent.".to_string(),
            },
            TechDecisionCheck {
                area: "framework".to_string(),
                current_suggestion: current_framework,
                alternatives_to_evaluate: vec![
                    "minimal-framework".to_string(),
                    "full-stack-framework".to_string(),
                    "no-framework".to_string(),
                ],
                decide_by: "Choose the framework that minimizes risk and maximizes speed-to-correctness for the user's mission.".to_string(),
            },
            TechDecisionCheck {
                area: "architecture-shape".to_string(),
                current_suggestion: None,
                alternatives_to_evaluate: vec![
                    "monolith".to_string(),
                    "modular-monolith".to_string(),
                    "service-oriented".to_string(),
                ],
                decide_by: "Pick the simplest architecture that still meets the mission and scaling constraints.".to_string(),
            },
        ];

        let quality_bar = vec![
            "Output must directly map every major design decision to user pain/intent.".to_string(),
            "Delivery artifacts must be production-grade: clear docs, tests, and verification evidence."
                .to_string(),
            "No unchecked assumptions: unknowns are surfaced as explicit questions or bounded decisions."
                .to_string(),
            "Final recommendation includes tradeoffs, rejected alternatives, and rationale.".to_string(),
        ];

        StrategyOutline {
            required_facts,
            decision_checks,
            quality_bar,
        }
    }

    fn build_intent_execution_plan(&self, input: &AssuranceEvaluateInput) -> IntentExecutionPlan {
        let mission = self.extract_user_mission(input);
        let assigned_task_id = input
            .params
            .get("assigned_task_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                input
                    .params
                    .get("todo_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                input
                    .params
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        let mut one_shot_todos = vec![];

        let analyze_id = "oneshot.define-outcome".to_string();
        one_shot_todos.push(OneShotTodo {
            id: analyze_id.clone(),
            title: "Define expected outcome and proof criteria from user intent".to_string(),
            depends_on: vec![],
            done_when: "Success criteria and observable proof are written in one concise checklist"
                .to_string(),
        });
        let mut previous_id = analyze_id;

        let implementation_id = "oneshot.implement-minimal-change".to_string();
        one_shot_todos.push(OneShotTodo {
            id: implementation_id.clone(),
            title: "Implement the smallest change that moves the mission forward".to_string(),
            depends_on: vec![previous_id.clone()],
            done_when:
                "Code/config changes are complete and each touched file maps directly to mission"
                    .to_string(),
        });
        previous_id = implementation_id;

        if !input.touched_paths.is_empty() {
            let touched_preview = input
                .touched_paths
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            let id = "oneshot.verify-behavior".to_string();
            one_shot_todos.push(OneShotTodo {
                id: id.clone(),
                title: format!("Verify behavior on key touched paths: {}", touched_preview),
                depends_on: vec![previous_id.clone()],
                done_when: "Observed behavior matches user-requested outcome, not only test pass"
                    .to_string(),
            });
            previous_id = id;
        }

        let proof_id = "oneshot.run-proof-gates".to_string();
        one_shot_todos.push(OneShotTodo {
            id: proof_id.clone(),
            title: "Run validation/proof gates and capture evidence".to_string(),
            depends_on: vec![previous_id.clone()],
            done_when: "decapod validate and required proofs are green with evidence attached"
                .to_string(),
        });
        previous_id = proof_id;

        if let Some(task_id) = &assigned_task_id {
            one_shot_todos.push(OneShotTodo {
                id: format!("oneshot.complete-{}", task_id),
                title: format!(
                    "Complete assigned task {} with dependency-safe closure",
                    task_id
                ),
                depends_on: vec![previous_id],
                done_when: format!(
                    "Assigned task {} can be closed with objective evidence chain",
                    task_id
                ),
            });
        }

        IntentExecutionPlan {
            mission,
            assigned_task_id,
            one_shot_todos,
        }
    }

    fn extract_user_mission(&self, input: &AssuranceEvaluateInput) -> String {
        for key in ["user_intent", "intent", "mission", "goal", "idea"] {
            if let Some(val) = input.params.get(key).and_then(|v| v.as_str()) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        if let Some(summary) = input.diff_summary.as_ref() {
            let trimmed = summary.trim();
            if !trimmed.is_empty() {
                return format!(
                    "Deliver intended outcome for operation '{}': {}",
                    input.op, trimmed
                );
            }
        }
        format!("Deliver intended outcome for operation '{}'", input.op)
    }

    fn obligation_to_pointer(obligation: &Obligation) -> ReconciliationPointer {
        let kind = match obligation.kind {
            ObligationKind::DocAnchor => "doc_anchor",
            ObligationKind::Adr => "adr",
            ObligationKind::KgNode => "kg_node",
            ObligationKind::Todo => "todo",
            ObligationKind::Gate => "gate",
            ObligationKind::Container => "workspace",
        };
        ReconciliationPointer {
            kind: kind.to_string(),
            r#ref: obligation.ref_path.clone(),
            title: obligation.title.clone(),
            why_short: obligation.why_short.clone(),
            evidence: crate::core::rpc::EvidenceRef {
                source: obligation.evidence.source.clone(),
                id: obligation.evidence.id.clone(),
                hash: obligation.evidence.hash.clone(),
            },
        }
    }

    fn dedupe_and_cap(items: &mut Vec<ReconciliationPointer>) {
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| seen.insert(format!("{}::{}", item.kind, item.r#ref)));
        items.truncate(5);
    }
}
