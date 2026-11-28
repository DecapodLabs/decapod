use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use decapod::CloudRuntimeConfig;
use decapod::core::cloud_proof::{
    AdmitOrPublishProofRequest, CloudControlDenialKind, CloudControlError, CustodyGetRequest,
    PROOF_ACCEPTANCE_CONTROL_OP, PROOF_ACCEPTANCE_READ_OP, PROOF_CAPTURE_CONTRACT_DIGEST,
    PROOF_CAPTURE_CONTRACT_ID, ProofCaptureRequest, ProofControlTransport, ProofEvidenceReference,
    ProofGitIdentity, ProofLifecycleState, ProofSubject, ResourceCheckRequest,
    VerifyProofTransitionRequest, check_shared_resources, custody_get, new_uuid_v4,
    proof_capture_fingerprint, reconstruct_attestation, submit_admit_or_publish, submit_capture,
    submit_retire_historical, submit_verify,
};
use decapod::core::cloud_workspace::{
    CLOUD_CUSTODY_DIR_ENV, CloudCustodyBindParams, CloudCustodyStore, CloudLeaseMetadata,
    custody_bind_or_reconcile, custody_handoff_or_reconcile, custody_release_or_reconcile,
    custody_renew_or_reconcile,
};
use decapod::core::error::DecapodError;
use decapod::core::repo_identity::RepositoryIdentity;
use decapod::core::storage::{Task, TodoStore};
use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{CloudTodoStoreFactory, TodoCli, run_todo_cli_with_cloud_factory};
use serde_json::{Value as JsonValue, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tempfile::TempDir;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn future_rfc3339(offset_secs: i64) -> String {
    let ts = time::OffsetDateTime::now_utc() + time::Duration::seconds(offset_secs);
    ts.format(&time::format_description::well_known::Rfc3339)
        .expect("rfc3339")
}

#[derive(Debug, Clone)]
struct ServerTaskFence {
    workspace_id: String,
    custody_id: String,
    owner_principal: String,
    display_agent: String,
    task_revision: i64,
    fencing_generation: i64,
    authority_generation: i64,
    expires_at_unix: i64,
    expires_at_rfc3339: String,
    revoked: bool,
    released: bool,
    resources: Vec<String>,
}

#[derive(Debug, Clone)]
struct ServerAttestation {
    attestation_id: String,
    project_id: String,
    task_id: String,
    task_revision: i64,
    fencing_generation: i64,
    fingerprint: String,
    candidate_commit: String,
    candidate_tree: String,
    accepted_commit: String,
    policy_digest: String,
    verified_evidence_digest: Option<String>,
    state: String,
    last_operation_id: String,
}

#[derive(Debug, Default)]
struct MockServerState {
    now_unix: i64,
    fences: BTreeMap<(String, String), ServerTaskFence>,
    attestations: BTreeMap<String, ServerAttestation>,
    operations: BTreeMap<String, JsonValue>,
    write_counts: BTreeMap<String, usize>,
    drop_next_ack_for_action: Option<String>,
    fail_next_get_operation: bool,
}

#[derive(Clone)]
struct MockProofServer {
    principal: String,
    display_agent: String,
    state: Arc<Mutex<MockServerState>>,
}

impl MockProofServer {
    fn new_cluster(principal: &str, display_agent: &str) -> Self {
        let s = MockServerState {
            now_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
            ..Default::default()
        };
        Self {
            principal: principal.to_string(),
            display_agent: display_agent.to_string(),
            state: Arc::new(Mutex::new(s)),
        }
    }

    fn as_principal(&self, principal: &str, display_agent: &str) -> Self {
        Self {
            principal: principal.to_string(),
            display_agent: display_agent.to_string(),
            state: Arc::clone(&self.state),
        }
    }

    fn advance_clock(&self, secs: i64) {
        self.state.lock().unwrap().now_unix += secs;
    }

    fn write_count(&self, action: &str) -> usize {
        *self
            .state
            .lock()
            .unwrap()
            .write_counts
            .get(action)
            .unwrap_or(&0)
    }

    fn expires_iso(state: &MockServerState, ttl_seconds: i64) -> (i64, String) {
        let real_now = time::OffsetDateTime::now_utc().unix_timestamp();
        let target_unix = state.now_unix.max(real_now) + ttl_seconds;
        let dt = time::OffsetDateTime::from_unix_timestamp(target_unix).expect("timestamp");
        let iso = dt
            .format(&time::format_description::well_known::Rfc3339)
            .expect("rfc3339");
        (target_unix, iso)
    }
}

impl ProofControlTransport for MockProofServer {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> std::result::Result<JsonValue, CloudControlError> {
        if op_name != PROOF_ACCEPTANCE_CONTROL_OP {
            return Err(CloudControlError {
                http_status: Some(400),
                code: "unsupported_control_op".into(),
                detail: op_name.into(),
                kind: CloudControlDenialKind::InvalidRequest,
            });
        }
        let action = payload["action"].as_str().unwrap_or_default().to_string();
        let op_id = payload["operation_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let project_id = payload["project_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let mut st = self.state.lock().unwrap();
        *st.write_counts.entry(action.clone()).or_insert(0) += 1;

        if let Some(existing) = st.operations.get(&op_id).cloned() {
            return Ok(existing);
        }

        let response = match action.as_str() {
            "custody_bind" => {
                let task_id = payload["task_id"].as_str().unwrap().to_string();
                let workspace_id = payload["workspace_id"].as_str().unwrap().to_string();
                let custody_id = payload["custody_id"].as_str().unwrap().to_string();
                let ttl = payload["ttl_seconds"].as_i64().unwrap_or(1800);
                let key = (project_id.clone(), task_id.clone());
                let (task_rev, fence_gen, auth_gen, resources) =
                    if let Some(prev) = st.fences.get(&key) {
                        if prev.owner_principal == self.principal
                            && prev.workspace_id == workspace_id
                            && prev.custody_id == custody_id
                            && !prev.revoked
                            && !prev.released
                            && prev.expires_at_unix > st.now_unix
                        {
                            (
                                prev.task_revision,
                                prev.fencing_generation,
                                prev.authority_generation,
                                prev.resources.clone(),
                            )
                        } else {
                            (
                                prev.task_revision + 1,
                                prev.fencing_generation + 1,
                                prev.authority_generation + 1,
                                prev.resources.clone(),
                            )
                        }
                    } else {
                        (1, 1, 1, Vec::new())
                    };
                let (exp_unix, exp_iso) = Self::expires_iso(&st, ttl);
                st.fences.insert(
                    key,
                    ServerTaskFence {
                        workspace_id: workspace_id.clone(),
                        custody_id: custody_id.clone(),
                        owner_principal: self.principal.clone(),
                        display_agent: self.display_agent.clone(),
                        task_revision: task_rev,
                        fencing_generation: fence_gen,
                        authority_generation: auth_gen,
                        expires_at_unix: exp_unix,
                        expires_at_rfc3339: exp_iso.clone(),
                        revoked: false,
                        released: false,
                        resources,
                    },
                );
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_bound",
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_id": workspace_id,
                    "custody_id": custody_id,
                    "task_revision": task_rev,
                    "fencing_generation": fence_gen,
                    "authority_generation": auth_gen,
                    "expires_at": exp_iso,
                })
            }
            "custody_renew" => {
                let task_id = payload["task_id"].as_str().unwrap().to_string();
                let workspace_id = payload["workspace_id"].as_str().unwrap().to_string();
                let custody_id = payload["custody_id"].as_str().unwrap().to_string();
                let exp_rev = payload["expected_task_revision"].as_i64().unwrap_or(0);
                let exp_gen = payload["expected_fencing_generation"].as_i64().unwrap_or(0);
                let ttl = payload["ttl_seconds"].as_i64().unwrap_or(1800);
                let heartbeat = payload["heartbeat"].as_bool().unwrap_or(false);
                let key = (project_id.clone(), task_id.clone());
                let now_unix = st.now_unix;
                let (exp_unix, exp_iso) = Self::expires_iso(&st, ttl);
                let fence = st.fences.get_mut(&key).ok_or_else(|| CloudControlError {
                    http_status: Some(409),
                    code: "custody_not_found".into(),
                    detail: "no active custody".into(),
                    kind: CloudControlDenialKind::Conflict,
                })?;
                if fence.owner_principal != self.principal {
                    return Err(CloudControlError {
                        http_status: Some(403),
                        code: "custody_owner_required".into(),
                        detail: "caller is not active principal".into(),
                        kind: CloudControlDenialKind::EntitlementForbidden,
                    });
                }
                if fence.revoked
                    || fence.released
                    || fence.expires_at_unix <= now_unix
                    || fence.workspace_id != workspace_id
                    || fence.custody_id != custody_id
                    || fence.task_revision != exp_rev
                    || fence.fencing_generation != exp_gen
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "stale_fencing_generation".into(),
                        detail: "custody fence is expired, revoked, or stale".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                fence.expires_at_unix = exp_unix;
                fence.expires_at_rfc3339 = exp_iso.clone();
                if !heartbeat {
                    // Explicit renew keeps same revision/gen if same authority unless rotated,
                    // keeping active candidate evidence valid (#1809 criterion 4).
                }
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_renewed",
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_id": workspace_id,
                    "custody_id": custody_id,
                    "task_revision": fence.task_revision,
                    "fencing_generation": fence.fencing_generation,
                    "authority_generation": fence.authority_generation,
                    "expires_at": fence.expires_at_rfc3339,
                })
            }
            "custody_handoff" => {
                let task_id = payload["task_id"].as_str().unwrap().to_string();
                let to_actor = payload["to_actor"].as_str().unwrap().to_string();
                let exp_gen = payload["expected_fencing_generation"].as_i64();
                let ttl = payload["ttl_seconds"].as_i64().unwrap_or(1800);
                let key = (project_id.clone(), task_id.clone());
                let now_unix = st.now_unix;
                let (exp_unix, exp_iso) = Self::expires_iso(&st, ttl);
                let fence = st.fences.get_mut(&key).ok_or_else(|| CloudControlError {
                    http_status: Some(409),
                    code: "custody_not_found".into(),
                    detail: "missing task fence".into(),
                    kind: CloudControlDenialKind::Conflict,
                })?;
                if fence.owner_principal != self.principal {
                    return Err(CloudControlError {
                        http_status: Some(403),
                        code: "custody_owner_required".into(),
                        detail: "handoff requires active principal".into(),
                        kind: CloudControlDenialKind::EntitlementForbidden,
                    });
                }
                if fence.revoked
                    || fence.released
                    || fence.expires_at_unix <= now_unix
                    || exp_gen.is_some_and(|g| g != fence.fencing_generation)
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "stale_fencing_generation".into(),
                        detail: "cannot hand off expired or stale lease".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                fence.owner_principal = to_actor.clone();
                fence.display_agent = to_actor.clone();
                fence.task_revision += 1;
                fence.fencing_generation += 1;
                fence.authority_generation += 1;
                fence.expires_at_unix = exp_unix;
                fence.expires_at_rfc3339 = exp_iso.clone();
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_handed_off",
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_id": fence.workspace_id,
                    "custody_id": fence.custody_id,
                    "assigned_to": to_actor,
                    "task_revision": fence.task_revision,
                    "fencing_generation": fence.fencing_generation,
                    "expires_at": fence.expires_at_rfc3339,
                })
            }
            "custody_release" => {
                let task_id = payload["task_id"].as_str().unwrap().to_string();
                let exp_gen = payload["expected_fencing_generation"].as_i64();
                let yield_lease = payload["yield_lease"].as_bool().unwrap_or(false);
                let key = (project_id.clone(), task_id.clone());
                let fence = st.fences.get_mut(&key).ok_or_else(|| CloudControlError {
                    http_status: Some(409),
                    code: "custody_not_found".into(),
                    detail: "missing task fence".into(),
                    kind: CloudControlDenialKind::Conflict,
                })?;
                if fence.owner_principal != self.principal {
                    return Err(CloudControlError {
                        http_status: Some(403),
                        code: "custody_owner_required".into(),
                        detail: "release requires active principal".into(),
                        kind: CloudControlDenialKind::EntitlementForbidden,
                    });
                }
                if exp_gen.is_some_and(|g| g != fence.fencing_generation) {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "stale_fencing_generation".into(),
                        detail: "stale fence on release".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                fence.released = true;
                fence.resources.clear();
                fence.task_revision += 1;
                fence.fencing_generation += 1;
                let status = if yield_lease {
                    "custody_yielded"
                } else {
                    "custody_released"
                };
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": status,
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_id": fence.workspace_id,
                    "custody_id": fence.custody_id,
                    "task_revision": fence.task_revision,
                    "fencing_generation": fence.fencing_generation,
                })
            }
            "custody_revoke" => {
                let task_id = payload["task_id"].as_str().unwrap().to_string();
                let key = (project_id.clone(), task_id.clone());
                let fence = st.fences.get_mut(&key).ok_or_else(|| CloudControlError {
                    http_status: Some(409),
                    code: "custody_not_found".into(),
                    detail: "missing fence".into(),
                    kind: CloudControlDenialKind::Conflict,
                })?;
                fence.revoked = true;
                fence.resources.clear();
                fence.fencing_generation += 1;
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_revoked",
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": task_id,
                    "task_revision": fence.task_revision,
                    "fencing_generation": fence.fencing_generation,
                })
            }
            "capture" => {
                let sub_raw = payload["submission"]
                    .as_str()
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(400),
                        code: "invalid_submission".into(),
                        detail: "missing submission string".into(),
                        kind: CloudControlDenialKind::InvalidRequest,
                    })?;
                let sub: ProofCaptureRequest =
                    serde_json::from_str(sub_raw).map_err(|e| CloudControlError {
                        http_status: Some(400),
                        code: "invalid_submission".into(),
                        detail: e.to_string(),
                        kind: CloudControlDenialKind::InvalidRequest,
                    })?;
                let key = (project_id.clone(), sub.subject.task_id.clone());
                let fence = st.fences.get(&key).ok_or_else(|| CloudControlError {
                    http_status: Some(409),
                    code: "custody_unbound".into(),
                    detail: "fabricated or unbound custody_id rejected".into(),
                    kind: CloudControlDenialKind::Conflict,
                })?;
                if fence.revoked
                    || fence.released
                    || fence.expires_at_unix <= st.now_unix
                    || fence.workspace_id != sub.subject.workspace_id
                    || fence.custody_id != sub.subject.custody_id
                    || fence.task_revision != sub.subject.task_revision
                    || fence.fencing_generation != sub.subject.fencing_generation
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "custody_fence_mismatch".into(),
                        detail: "fabricated, stale, or expired custody fence rejected".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                let fp = proof_capture_fingerprint(&sub).map_err(|e| CloudControlError {
                    http_status: Some(400),
                    code: "invalid_fingerprint".into(),
                    detail: e.to_string(),
                    kind: CloudControlDenialKind::InvalidRequest,
                })?;
                let att_id = format!("att_{}", st.attestations.len() + 1);
                let att = ServerAttestation {
                    attestation_id: att_id.clone(),
                    project_id: project_id.clone(),
                    task_id: sub.subject.task_id.clone(),
                    task_revision: sub.subject.task_revision,
                    fencing_generation: sub.subject.fencing_generation,
                    fingerprint: fp.clone(),
                    candidate_commit: sub.subject.candidate.commit.clone(),
                    candidate_tree: sub.subject.candidate.tree.clone(),
                    accepted_commit: sub.subject.accepted_target.commit.clone(),
                    policy_digest: sub.subject.policy_digest.clone(),
                    verified_evidence_digest: None,
                    state: ProofLifecycleState::Captured.as_str().to_string(),
                    last_operation_id: op_id.clone(),
                };
                st.attestations.insert(att_id.clone(), att);
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "attestation_id": att_id,
                    "operation_id": op_id,
                    "project_id": project_id,
                    "task_id": sub.subject.task_id,
                    "workspace_id": sub.subject.workspace_id,
                    "custody_id": sub.subject.custody_id,
                    "task_revision": sub.subject.task_revision,
                    "fencing_generation": sub.subject.fencing_generation,
                    "fingerprint": fp,
                    "state": "captured",
                    "logical_custody_verified": false,
                    "acceptance_supported": false,
                    "publication_eligible": false,
                    "forge_merge_authority": false,
                    "candidate": {
                        "commit": sub.subject.candidate.commit,
                        "tree": sub.subject.candidate.tree,
                    },
                    "accepted_target": {
                        "commit": sub.subject.accepted_target.commit,
                        "tree": sub.subject.accepted_target.tree,
                    },
                })
            }
            "verify" => {
                let att_id = payload["attestation_id"].as_str().unwrap().to_string();
                let exp_fp = payload["expected_fingerprint"].as_str().unwrap();
                let exp_commit = payload["expected_candidate_commit"].as_str().unwrap();
                let exp_tree = payload["expected_candidate_tree"].as_str().unwrap();
                let ved = payload["verified_evidence_digest"]
                    .as_str()
                    .unwrap()
                    .to_string();
                let att = st
                    .attestations
                    .get_mut(&att_id)
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(404),
                        code: "proof_acceptance_not_found".into(),
                        detail: "missing attestation".into(),
                        kind: CloudControlDenialKind::InvalidRequest,
                    })?;
                if att.state != "captured"
                    || att.fingerprint != exp_fp
                    || att.candidate_commit != exp_commit
                    || att.candidate_tree != exp_tree
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "proof_verify_mismatch".into(),
                        detail: "candidate commit, tree, fingerprint, or state mismatch".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                att.state = ProofLifecycleState::IndependentlyVerified.as_str().into();
                att.verified_evidence_digest = Some(ved.clone());
                att.last_operation_id = op_id.clone();
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "attestation_id": att.attestation_id,
                    "operation_id": op_id,
                    "project_id": att.project_id,
                    "fingerprint": att.fingerprint,
                    "state": "independently_verified",
                    "candidate": { "commit": att.candidate_commit, "tree": att.candidate_tree },
                    "verified_evidence_digest": ved,
                    "references_verified": true,
                    "logical_custody_verified": false,
                    "acceptance_supported": false,
                    "publication_eligible": false,
                })
            }
            "admit" | "publish" => {
                let att_id = payload["attestation_id"].as_str().unwrap().to_string();
                let exp_fp = payload["expected_fingerprint"].as_str().unwrap();
                let exp_rev = payload["expected_task_revision"].as_i64().unwrap_or(0);
                let exp_gen = payload["expected_fencing_generation"].as_i64().unwrap_or(0);
                let exp_cand = payload["expected_candidate_commit"].as_str().unwrap();
                let exp_target = payload["expected_accepted_target_commit"].as_str().unwrap();
                let exp_policy = payload["expected_policy_digest"].as_str().unwrap();
                let att =
                    st.attestations
                        .get(&att_id)
                        .cloned()
                        .ok_or_else(|| CloudControlError {
                            http_status: Some(404),
                            code: "proof_acceptance_not_found".into(),
                            detail: "missing attestation".into(),
                            kind: CloudControlDenialKind::InvalidRequest,
                        })?;
                let required_prev = if action == "admit" {
                    "independently_verified"
                } else {
                    "currently_admissible"
                };
                if att.state != required_prev
                    || att.fingerprint != exp_fp
                    || att.task_revision != exp_rev
                    || att.fencing_generation != exp_gen
                    || att.candidate_commit != exp_cand
                    || att.accepted_commit != exp_target
                    || att.policy_digest != exp_policy
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "proof_transition_rejected".into(),
                        detail: format!(
                            "invalid state '{}' or identity drift for {action}",
                            att.state
                        ),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                let fence = st
                    .fences
                    .get(&(project_id.clone(), att.task_id.clone()))
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(409),
                        code: "custody_missing".into(),
                        detail: "no active custody".into(),
                        kind: CloudControlDenialKind::Conflict,
                    })?;
                if fence.revoked
                    || fence.released
                    || fence.expires_at_unix <= st.now_unix
                    || fence.task_revision != exp_rev
                    || fence.fencing_generation != exp_gen
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "stale_fencing_generation".into(),
                        detail: "custody fence is expired, revoked, or superseded".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                let next_state = if action == "admit" {
                    ProofLifecycleState::CurrentlyAdmissible.as_str()
                } else {
                    ProofLifecycleState::Published.as_str()
                };
                st.attestations.get_mut(&att_id).unwrap().state = next_state.into();
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "attestation_id": att.attestation_id,
                    "operation_id": op_id,
                    "project_id": att.project_id,
                    "fingerprint": att.fingerprint,
                    "state": next_state,
                    "task_revision": exp_rev,
                    "fencing_generation": exp_gen,
                    "candidate": { "commit": att.candidate_commit },
                    "accepted_target": { "commit": att.accepted_commit },
                    "policy_digest": att.policy_digest,
                    "references_verified": true,
                    "logical_custody_verified": true,
                    "acceptance_supported": true,
                    "publication_eligible": action == "publish",
                })
            }
            "retire_historical" => {
                let att_id = payload["attestation_id"].as_str().unwrap().to_string();
                let exp_fp = payload["expected_fingerprint"].as_str().unwrap();
                let att = st
                    .attestations
                    .get_mut(&att_id)
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(404),
                        code: "proof_acceptance_not_found".into(),
                        detail: "missing attestation".into(),
                        kind: CloudControlDenialKind::InvalidRequest,
                    })?;
                if att.fingerprint != exp_fp {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "fingerprint_mismatch".into(),
                        detail: "fingerprint mismatch".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                att.state = ProofLifecycleState::HistoricallyAccepted.as_str().into();
                json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "attestation_id": att.attestation_id,
                    "operation_id": op_id,
                    "project_id": att.project_id,
                    "fingerprint": att.fingerprint,
                    "state": "historically_accepted",
                    "publication_eligible": false,
                })
            }
            other => {
                return Err(CloudControlError {
                    http_status: Some(400),
                    code: "unknown_action".into(),
                    detail: other.to_string(),
                    kind: CloudControlDenialKind::InvalidRequest,
                });
            }
        };

        st.operations.insert(op_id, response.clone());
        if st.drop_next_ack_for_action.as_deref() == Some(action.as_str()) {
            st.drop_next_ack_for_action = None;
            return Err(CloudControlError {
                http_status: None,
                code: "outcome_unknown".into(),
                detail: "simulated commit-before-ack connection reset".into(),
                kind: CloudControlDenialKind::OutcomeUnknown,
            });
        }
        Ok(response)
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> std::result::Result<JsonValue, CloudControlError> {
        if op_name != PROOF_ACCEPTANCE_READ_OP {
            return Err(CloudControlError {
                http_status: Some(400),
                code: "unsupported_read_op".into(),
                detail: op_name.into(),
                kind: CloudControlDenialKind::InvalidRequest,
            });
        }
        let action = payload["action"].as_str().unwrap_or_default();
        let project_id = payload["project_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let mut st = self.state.lock().unwrap();
        match action {
            "get_operation" => {
                if st.fail_next_get_operation {
                    st.fail_next_get_operation = false;
                    return Err(CloudControlError {
                        http_status: Some(503),
                        code: "control_unavailable".into(),
                        detail: "simulated process death during reconciliation".into(),
                        kind: CloudControlDenialKind::Unavailable,
                    });
                }
                let op_id = payload["operation_id"].as_str().unwrap_or_default();
                if let Some(receipt) = st.operations.get(op_id).cloned() {
                    Ok(json!({ "found": true, "receipt": receipt }))
                } else {
                    Ok(json!({ "found": false }))
                }
            }
            "get_custody" => {
                let task_id = payload["task_id"].as_str().unwrap_or_default().to_string();
                let workspace_id = payload["workspace_id"].as_str().unwrap_or_default();
                let custody_id = payload["custody_id"].as_str().unwrap_or_default();
                let task_revision = payload["task_revision"].as_i64().unwrap_or(0);
                let fencing_gen = payload["fencing_generation"].as_i64().unwrap_or(0);
                let fence = st
                    .fences
                    .get(&(project_id.clone(), task_id.clone()))
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(409),
                        code: "custody_not_found".into(),
                        detail: "no active custody".into(),
                        kind: CloudControlDenialKind::Conflict,
                    })?;
                if fence.revoked
                    || fence.released
                    || fence.expires_at_unix <= st.now_unix
                    || fence.workspace_id != workspace_id
                    || fence.custody_id != custody_id
                    || fence.task_revision != task_revision
                    || fence.fencing_generation != fencing_gen
                {
                    return Err(CloudControlError {
                        http_status: Some(409),
                        code: "stale_fencing_generation".into(),
                        detail: "custody is expired, revoked, released, or superseded".into(),
                        kind: CloudControlDenialKind::Conflict,
                    });
                }
                Ok(json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_current",
                    "project_id": project_id,
                    "task_id": task_id,
                    "workspace_id": fence.workspace_id,
                    "custody_id": fence.custody_id,
                    "task_revision": fence.task_revision,
                    "fencing_generation": fence.fencing_generation,
                    "authority_generation": fence.authority_generation,
                    "expires_at": fence.expires_at_rfc3339,
                }))
            }
            "resource_check" => {
                let resources: BTreeSet<String> = payload["resources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect();
                let exclude = payload["exclude_task_id"].as_str();
                for ((pid, tid), fence) in &st.fences {
                    if pid != &project_id
                        || fence.revoked
                        || fence.released
                        || fence.expires_at_unix <= st.now_unix
                        || exclude.is_some_and(|ex| ex == tid.as_str())
                    {
                        continue;
                    }
                    if let Some(res) = fence.resources.iter().find(|r| resources.contains(*r)) {
                        return Ok(json!({
                            "occupied": true,
                            "blocking_resource": res,
                            "blocking_task_id": tid,
                            "blocking_holder": fence.owner_principal,
                            "expires_at": fence.expires_at_rfc3339,
                        }));
                    }
                }
                Ok(json!({ "occupied": false }))
            }
            "reconstruct" => {
                let att_id = payload["attestation_id"].as_str().unwrap_or_default();
                let att = st
                    .attestations
                    .get(att_id)
                    .ok_or_else(|| CloudControlError {
                        http_status: Some(404),
                        code: "proof_acceptance_not_found".into(),
                        detail: "missing attestation".into(),
                        kind: CloudControlDenialKind::InvalidRequest,
                    })?;
                Ok(json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "attestation_id": att.attestation_id,
                    "project_id": att.project_id,
                    "task_id": att.task_id,
                    "fingerprint": att.fingerprint,
                    "state": att.state,
                    "task_revision": att.task_revision,
                    "fencing_generation": att.fencing_generation,
                }))
            }
            other => Err(CloudControlError {
                http_status: Some(400),
                code: "unknown_read_action".into(),
                detail: other.to_string(),
                kind: CloudControlDenialKind::InvalidRequest,
            }),
        }
    }
}

fn setup_cloud_repo(root: &Path, origin_url: &str) {
    std::fs::create_dir_all(root.join(".decapod/data")).unwrap();
    std::fs::write(
        root.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@decapodlabs.ai"]);
    git(&["config", "user.name", "Test"]);
    git(&["remote", "add", "origin", origin_url]);
    std::fs::write(root.join("README.md"), b"cloud repo\n").unwrap();
    git(&["add", "README.md"]);
    git(&["commit", "-m", "initial"]);
}

fn parse_todo(args: &[&str]) -> TodoCli {
    TodoCli::try_parse_from(std::iter::once("todo").chain(args.iter().copied()))
        .expect("valid todo args")
}

#[derive(Clone, Default)]
struct DummyCloudFactory;

#[async_trait]
impl TodoStore for DummyCloudFactory {
    async fn list_tasks(&self) -> Result<Vec<Task>> {
        Ok(vec![])
    }
    async fn add_task(&self, task: Task, _actor: String, _intent: String) -> Result<Task> {
        Ok(task)
    }
    async fn claim_task(&self, _id: &str, _actor: String) -> Result<Task> {
        anyhow::bail!("unexpected claim_task call after preflight")
    }
    async fn complete_task(&self, _id: &str, _actor: String, _res: String) -> Result<Task> {
        anyhow::bail!("unexpected complete_task")
    }
}

impl CloudTodoStoreFactory for DummyCloudFactory {
    fn build(
        &self,
        _config: &CloudRuntimeConfig,
        _identity: &RepositoryIdentity,
    ) -> Result<Box<dyn TodoStore>, DecapodError> {
        Ok(Box::new(Self))
    }
}

#[test]
fn cli_preflight_rejects_unsupported_shared_mode_and_invalid_lease_bounds() {
    let _guard = lock_env();
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    setup_cloud_repo(&repo, "https://github.com/DecapodLabs/decapod.git");
    let store = Store {
        kind: StoreKind::Repo,
        root: repo.join(".decapod/data"),
    };
    let factory = DummyCloudFactory;

    // 1. `--mode shared` MUST fail closed explicitly (never silently dropped).
    let shared_err = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&["claim", "--id", "t1", "--mode", "shared"]),
        &factory,
    )
    .expect_err("shared claim mode must fail closed in cloud");
    assert!(
        shared_err
            .to_string()
            .contains("CLOUD_CLAIM_MODE_UNSUPPORTED"),
        "unexpected error: {shared_err}"
    );

    // 2. Out-of-bounds `--lease-seconds` on claim and renew MUST fail closed.
    let low_ttl = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&["claim", "--id", "t1", "--lease-seconds", "15"]),
        &factory,
    )
    .expect_err("low TTL must be rejected");
    assert!(
        low_ttl.to_string().contains("CLOUD_LEASE_TTL_INVALID"),
        "unexpected error: {low_ttl}"
    );

    let high_ttl = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&["renew", "--id", "t1", "--lease-seconds", "999999"]),
        &factory,
    )
    .expect_err("excessive TTL must be rejected");
    assert!(
        high_ttl.to_string().contains("CLOUD_LEASE_TTL_INVALID"),
        "unexpected error: {high_ttl}"
    );

    // 3. Empty `resource:` scope on `add` MUST fail closed, as must same-agent / invalid-TTL `handoff`.
    let empty_res = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&["add", "Resource test", "--scope", "resource:"]),
        &factory,
    )
    .expect_err("empty resource: scope must be rejected");
    assert!(
        empty_res
            .to_string()
            .contains("CLOUD_RESOURCE_SCOPE_INVALID"),
        "unexpected error: {empty_res}"
    );

    let same_agent_handoff = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&[
            "handoff",
            "--id",
            "t1",
            "--from",
            "alice",
            "--to",
            "alice",
            "--summary",
            "self handoff",
        ]),
        &factory,
    )
    .expect_err("self-handoff must fail closed");
    assert!(
        same_agent_handoff
            .to_string()
            .contains("HANDOFF_SAME_AGENT"),
        "unexpected error: {same_agent_handoff}"
    );

    let bad_handoff_ttl = run_todo_cli_with_cloud_factory(
        &store,
        parse_todo(&[
            "handoff",
            "--id",
            "t1",
            "--from",
            "alice",
            "--to",
            "bob",
            "--lease-seconds",
            "10",
            "--summary",
            "short ttl",
        ]),
        &factory,
    )
    .expect_err("out-of-range handoff TTL must fail closed");
    assert!(
        bad_handoff_ttl
            .to_string()
            .contains("CLOUD_LEASE_TTL_INVALID"),
        "unexpected error: {bad_handoff_ttl}"
    );

    // Never create local SQLite decapod.db on any failure.
    assert!(!store.root.join("decapod.db").exists());
}

#[test]
fn hosted_custody_heartbeat_handoff_expiry_release_and_identical_agent_label_isolation() {
    let _guard = lock_env();
    let tmp = TempDir::new().unwrap();
    let custody_dir_a = tmp.path().join("custody_alice");
    let custody_dir_b = tmp.path().join("custody_bob");
    let main_repo = tmp.path().join("main_repo");
    let wt_a = tmp.path().join("wt_alice");
    let wt_b = tmp.path().join("wt_bob");
    std::fs::create_dir_all(&wt_a).unwrap();
    std::fs::create_dir_all(&wt_b).unwrap();
    setup_cloud_repo(&main_repo, "https://github.com/DecapodLabs/decapod.git");

    // Two distinct humans/devices share the identical DECAPOD_AGENT_ID display label `"shared-dev"`,
    // authenticated as distinct principals (`alice@dev1` vs `bob@dev2`).
    let server_alice = MockProofServer::new_cluster("principal:alice@dev1", "shared-dev");
    let server_bob = server_alice.as_principal("principal:bob@dev2", "shared-dev");

    let store_a = CloudCustodyStore::at(&custody_dir_a);
    let _store_b = CloudCustodyStore::at(&custody_dir_b);

    let params_a = CloudCustodyBindParams {
        origin: "https://api.dactyl.example".into(),
        project_id: "DecapodLabs/decapod".into(),
        task_id: "task-db-1".into(),
        task_title: "Migrate schema".into(),
        native_task_version: 1,
        workspace_id: "ws_alice_1".into(),
        worktree_path: wt_a.clone(),
        branch: "agent/shared-dev/task-db-1".into(),
        actor: "shared-dev".into(),
        ttl_seconds: 600,
        invocation_id: None,
        container_expected: false,
    };
    let (mut rec_a, receipt_a) =
        custody_bind_or_reconcile(&server_alice, &store_a, &main_repo, &params_a).unwrap();
    assert_eq!(receipt_a.task_revision, 1);
    assert_eq!(receipt_a.fencing_generation, 1);

    // Attach shared resource reservation `resource:prod-db` on server for Alice's task.
    server_alice
        .state
        .lock()
        .unwrap()
        .fences
        .get_mut(&("DecapodLabs/decapod".into(), "task-db-1".into()))
        .unwrap()
        .resources
        .push("resource:prod-db".into());

    // Device B (identical display label `shared-dev`, distinct task `task-db-2`) checks `resource:prod-db`:
    // MUST detect conflict (`<= 1` winner), while disjoint `resource:analytics` passes (`Ok(None)`).
    let conflict = check_shared_resources(
        &server_bob,
        &ResourceCheckRequest {
            project_id: "DecapodLabs/decapod".into(),
            resources: vec!["resource:prod-db".into()],
            exclude_task_id: Some("task-db-2".into()),
        },
    )
    .unwrap()
    .expect("shared resource conflict must block second holder even with identical agent label");
    assert_eq!(conflict.resource, "resource:prod-db");
    assert_eq!(conflict.task_id, "task-db-1");
    assert_eq!(conflict.holder, "principal:alice@dev1");

    let disjoint = check_shared_resources(
        &server_bob,
        &ResourceCheckRequest {
            project_id: "DecapodLabs/decapod".into(),
            resources: vec!["resource:analytics".into()],
            exclude_task_id: Some("task-db-2".into()),
        },
    )
    .unwrap();
    assert!(disjoint.is_none(), "disjoint resource must progress");

    // Same-authority heartbeat across simulated CLI restart preserves `task_revision` & `fencing_generation`
    // and keeps captured + independently verified evidence admissible!
    let store_a_restarted = CloudCustodyStore::at(&custody_dir_a);
    let mut rec_reloaded = store_a_restarted
        .load(&main_repo, &wt_a)
        .unwrap()
        .expect("custody record durable across restart");
    let sub = ProofCaptureRequest {
        contract: PROOF_CAPTURE_CONTRACT_ID.into(),
        contract_digest: PROOF_CAPTURE_CONTRACT_DIGEST.into(),
        operation_id: new_uuid_v4().unwrap(),
        subject: ProofSubject {
            repository_id: "DecapodLabs/decapod".into(),
            task_id: "task-db-1".into(),
            workspace_id: rec_reloaded.workspace_id.clone(),
            custody_id: rec_reloaded.custody_id.clone(),
            task_revision: rec_reloaded.task_revision.unwrap(),
            fencing_generation: rec_reloaded.fencing_generation.unwrap(),
            candidate: ProofGitIdentity {
                commit: "1111111111111111111111111111111111111111".into(),
                tree: "2222222222222222222222222222222222222222".into(),
            },
            accepted_target: ProofGitIdentity {
                commit: "3333333333333333333333333333333333333333".into(),
                tree: "4444444444444444444444444444444444444444".into(),
            },
            integration: None,
            policy_digest: "sha256:aaaa".into(),
            requirements_digest: "sha256:aaaa".into(),
            material_digest: "sha256:bbbb".into(),
            prerequisites_digest: "sha256:aaaa".into(),
        },
        evidence: vec![ProofEvidenceReference {
            kind: "suite".into(),
            digest: "sha256:cccc".into(),
            bytes: 128,
            provenance_digest: "sha256:dddd".into(),
        }],
    };
    let fp = proof_capture_fingerprint(&sub).unwrap();
    let cap = submit_capture(&server_alice, "DecapodLabs/decapod", 1, &sub).unwrap();
    assert_eq!(cap["state"], "captured");
    assert_eq!(cap["acceptance_supported"], false);
    assert_eq!(cap["publication_eligible"], false);
    let att_id = cap["attestation_id"].as_str().unwrap().to_string();

    let ver = submit_verify(
        &server_alice,
        &VerifyProofTransitionRequest {
            project_id: "DecapodLabs/decapod".into(),
            operation_id: new_uuid_v4().unwrap(),
            attestation_id: att_id.clone(),
            expected_fingerprint: fp.clone(),
            expected_candidate_commit: sub.subject.candidate.commit.clone(),
            expected_candidate_tree: sub.subject.candidate.tree.clone(),
            verified_evidence_digest: "sha256:eeee".into(),
            verifier_scope: "ci".into(),
        },
    )
    .unwrap();
    assert_eq!(ver["state"], "independently_verified");
    assert_eq!(ver["acceptance_supported"], false);

    // Heartbeat preserves task_revision=1 & fencing_generation=1!
    let hb = custody_renew_or_reconcile(
        &server_alice,
        &store_a_restarted,
        &main_repo,
        &mut rec_reloaded,
        1200,
        true,
    )
    .unwrap();
    assert_eq!(hb.task_revision, 1);
    assert_eq!(hb.fencing_generation, 1);

    // Admit -> Publish -> Retire Historical all succeed after same-authority heartbeat!
    let adm = submit_admit_or_publish(
        &server_alice,
        "admit",
        &AdmitOrPublishProofRequest {
            project_id: "DecapodLabs/decapod".into(),
            operation_id: new_uuid_v4().unwrap(),
            attestation_id: att_id.clone(),
            expected_fingerprint: fp.clone(),
            expected_task_revision: 1,
            expected_fencing_generation: 1,
            expected_candidate_commit: sub.subject.candidate.commit.clone(),
            expected_accepted_target_commit: sub.subject.accepted_target.commit.clone(),
            expected_policy_digest: "sha256:aaaa".into(),
        },
    )
    .unwrap();
    assert_eq!(adm["state"], "currently_admissible");
    assert_eq!(adm["acceptance_supported"], true);
    assert_eq!(adm["publication_eligible"], false);

    let pub_r = submit_admit_or_publish(
        &server_alice,
        "publish",
        &AdmitOrPublishProofRequest {
            project_id: "DecapodLabs/decapod".into(),
            operation_id: new_uuid_v4().unwrap(),
            attestation_id: att_id.clone(),
            expected_fingerprint: fp.clone(),
            expected_task_revision: 1,
            expected_fencing_generation: 1,
            expected_candidate_commit: sub.subject.candidate.commit.clone(),
            expected_accepted_target_commit: sub.subject.accepted_target.commit.clone(),
            expected_policy_digest: "sha256:aaaa".into(),
        },
    )
    .unwrap();
    assert_eq!(pub_r["state"], "published");
    assert_eq!(pub_r["publication_eligible"], true);

    let ret_r = submit_retire_historical(
        &server_alice,
        "DecapodLabs/decapod",
        &new_uuid_v4().unwrap(),
        &att_id,
        &fp,
        "merged",
    )
    .unwrap();
    assert_eq!(ret_r["state"], "historically_accepted");
    assert_eq!(ret_r["publication_eligible"], false);

    let reconstructed =
        reconstruct_attestation(&server_alice, "DecapodLabs/decapod", Some(&att_id), None).unwrap();
    assert_eq!(reconstructed["state"], "historically_accepted");

    // Authorized handoff to Bob advances fencing_generation=2, task_revision=2, and removes Alice's local custody.
    let handoff = custody_handoff_or_reconcile(
        &server_alice,
        &store_a_restarted,
        &main_repo,
        "DecapodLabs/decapod",
        "task-db-1",
        "shared-dev",
        "principal:bob@dev2",
        Some("principal:alice@dev1"),
        900,
        "handoff after publish",
        Some(&rec_reloaded),
    )
    .unwrap();
    assert_eq!(handoff.task_revision, 2);
    assert_eq!(handoff.fencing_generation, 2);
    assert_eq!(handoff.assigned_to, "principal:bob@dev2");
    assert!(store_a_restarted.load(&main_repo, &wt_a).unwrap().is_none());

    // Stale old holder Alice MUST fail closed on custody_get & custody_renew after handoff!
    let stale_get = custody_get(
        &server_alice,
        &CustodyGetRequest {
            project_id: "DecapodLabs/decapod".into(),
            task_id: "task-db-1".into(),
            workspace_id: "ws_alice_1".into(),
            custody_id: rec_a.custody_id.clone(),
            task_revision: 1,
            fencing_generation: 1,
        },
    )
    .expect_err("stale custody after handoff must fail closed");
    assert!(
        stale_get.to_string().contains("CLOUD_PROOF_CONFLICT")
            || stale_get.to_string().contains("CLOUD_ENTITLEMENT_DENIED"),
        "unexpected error: {stale_get}"
    );

    let stale_renew = custody_renew_or_reconcile(
        &server_alice,
        &store_a_restarted,
        &main_repo,
        &mut rec_a,
        900,
        true,
    )
    .expect_err("old holder must fail closed on renew after handoff");
    assert!(
        stale_renew.to_string().contains("CLOUD_ENTITLEMENT_DENIED")
            || stale_renew.to_string().contains("CLOUD_PROOF_CONFLICT"),
        "unexpected error: {stale_renew}"
    );

    // Bob binds custody for task-db-1 and then deterministically releases/yields: frees `resource:prod-db`.
    let (rec_b, receipt_b) = custody_bind_or_reconcile(
        &server_bob,
        &store_a_restarted,
        &main_repo,
        &CloudCustodyBindParams {
            origin: "https://api.dactyl.example".into(),
            project_id: "DecapodLabs/decapod".into(),
            task_id: "task-db-1".into(),
            task_title: "Migrate schema".into(),
            native_task_version: 2,
            workspace_id: "ws_bob_1".into(),
            worktree_path: wt_b.clone(),
            branch: "agent/shared-dev/task-db-1-bob".into(),
            actor: "principal:bob@dev2".into(),
            ttl_seconds: 900,
            invocation_id: None,
            container_expected: false,
        },
    )
    .unwrap();
    assert!(receipt_b.fencing_generation >= 3);

    // Test server-authoritative expiry invalidates lease without local clock reliance.
    server_bob.advance_clock(3600);
    let expired_err = custody_get(
        &server_bob,
        &CustodyGetRequest {
            project_id: "DecapodLabs/decapod".into(),
            task_id: "task-db-1".into(),
            workspace_id: "ws_bob_1".into(),
            custody_id: rec_b.custody_id.clone(),
            task_revision: receipt_b.task_revision,
            fencing_generation: receipt_b.fencing_generation,
        },
    )
    .expect_err("expired custody must fail closed");
    assert!(expired_err.to_string().contains("CLOUD_PROOF_CONFLICT"));

    // Deterministic release frees `resource:prod-db` cleanly.
    let rel = custody_release_or_reconcile(
        &server_bob,
        &store_a_restarted,
        &main_repo,
        "DecapodLabs/decapod",
        "task-db-1",
        "principal:bob@dev2",
        "work complete",
        false,
        Some(&rec_b),
    )
    .unwrap();
    assert_eq!(rel.status, "custody_released");
    let post_release_check = check_shared_resources(
        &server_alice,
        &ResourceCheckRequest {
            project_id: "DecapodLabs/decapod".into(),
            resources: vec!["resource:prod-db".into()],
            exclude_task_id: Some("task-db-2".into()),
        },
    )
    .unwrap();
    assert!(post_release_check.is_none());
    let _ = rec_b.expires_at;
    let _ = future_rfc3339(600);
}

#[test]
fn commit_before_ack_loss_and_process_death_reconcile_without_double_replay_or_sqlite() {
    let _guard = lock_env();
    let tmp = TempDir::new().unwrap();
    let custody_dir = tmp.path().join("custody");
    let main_repo = tmp.path().join("main_repo");
    let wt = tmp.path().join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    setup_cloud_repo(&main_repo, "https://github.com/DecapodLabs/decapod.git");

    let server = MockProofServer::new_cluster("principal:alice@dev1", "alice");
    let store = CloudCustodyStore::at(&custody_dir);
    let (mut rec, _) = custody_bind_or_reconcile(
        &server,
        &store,
        &main_repo,
        &CloudCustodyBindParams {
            origin: "https://api.dactyl.example".into(),
            project_id: "DecapodLabs/decapod".into(),
            task_id: "task-crash-1".into(),
            task_title: "Crash recovery".into(),
            native_task_version: 1,
            workspace_id: "ws_crash_1".into(),
            worktree_path: wt.clone(),
            branch: "agent/alice/task-crash-1".into(),
            actor: "alice".into(),
            ttl_seconds: 600,
            invocation_id: None,
            container_expected: false,
        },
    )
    .unwrap();

    // Inject BOTH commit-before-ack loss on `custody_renew` AND immediate failure on `get_operation`
    // simulating client process crash before it could reconcile!
    {
        let mut st = server.state.lock().unwrap();
        st.drop_next_ack_for_action = Some("custody_renew".into());
        st.fail_next_get_operation = true;
    }

    let crash_err = custody_renew_or_reconcile(&server, &store, &main_repo, &mut rec, 1200, true)
        .expect_err("process crash during ack loss must surface error without dropping pending op");
    assert!(crash_err.to_string().contains("CLOUD_CONTROL_UNAVAILABLE"));
    assert_eq!(server.write_count("custody_renew"), 1);

    // Pending `operation_id` MUST remain persisted in machine-local `CloudLeaseMetadata`, with NO SQLite db!
    let saved_meta: CloudLeaseMetadata = store
        .load_lease_metadata("DecapodLabs/decapod", "task-crash-1")
        .unwrap()
        .expect("pending lease metadata must persist on crash");
    let saved_op_id = saved_meta
        .pending_operation_id
        .expect("pending_operation_id must be preserved after crash");
    assert_eq!(saved_meta.pending_action.as_deref(), Some("custody_renew"));
    assert!(!main_repo.join(".decapod/data/decapod.db").exists());

    // Simulate NEW CLI process restart (`CloudCustodyStore::at` reloaded cleanly from disk):
    // Calling `custody_renew_or_reconcile` MUST reconcile `saved_op_id` via `get_operation`
    // with ZERO second `custody_renew` write RPC!
    let restarted_store = CloudCustodyStore::at(&custody_dir);
    let mut reloaded_rec = restarted_store.load(&main_repo, &wt).unwrap().unwrap();
    let reconciled = custody_renew_or_reconcile(
        &server,
        &restarted_store,
        &main_repo,
        &mut reloaded_rec,
        1200,
        true,
    )
    .expect("restart must reconcile committed pending operation via get_operation");
    assert_eq!(
        server.write_count("custody_renew"),
        1,
        "must NEVER double-replay custody_renew write after commit-before-ack crash"
    );
    assert_eq!(reconciled.task_revision, 1);
    assert_eq!(reconciled.fencing_generation, 1);
    let cleared = restarted_store
        .load_lease_metadata("DecapodLabs/decapod", "task-crash-1")
        .unwrap()
        .unwrap();
    assert!(cleared.pending_operation_id.is_none());
    let _ = (saved_op_id, CLOUD_CUSTODY_DIR_ENV);
}

#[test]
fn proof_capture_rejects_fabricated_custody_and_unverified_or_skipped_transitions() {
    let _guard = lock_env();
    let tmp = TempDir::new().unwrap();
    let main_repo = tmp.path().join("main_repo");
    let custody_dir = tmp.path().join("custody");
    let wt = tmp.path().join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    setup_cloud_repo(&main_repo, "https://github.com/DecapodLabs/decapod.git");

    let server = MockProofServer::new_cluster("principal:alice@dev1", "alice");
    let store = CloudCustodyStore::at(&custody_dir);
    let (_rec, _receipt) = custody_bind_or_reconcile(
        &server,
        &store,
        &main_repo,
        &CloudCustodyBindParams {
            origin: "https://api.dactyl.example".into(),
            project_id: "DecapodLabs/decapod".into(),
            task_id: "task-proof-1".into(),
            task_title: "Proof gate".into(),
            native_task_version: 1,
            workspace_id: "ws_bound_1".into(),
            worktree_path: wt,
            branch: "agent/alice/task-proof-1".into(),
            actor: "alice".into(),
            ttl_seconds: 600,
            invocation_id: None,
            container_expected: false,
        },
    )
    .unwrap();

    // 1. Fabricated custody_id rejected fail-closed.
    let fabricated_sub = ProofCaptureRequest {
        contract: PROOF_CAPTURE_CONTRACT_ID.into(),
        contract_digest: PROOF_CAPTURE_CONTRACT_DIGEST.into(),
        operation_id: new_uuid_v4().unwrap(),
        subject: ProofSubject {
            repository_id: "DecapodLabs/decapod".into(),
            task_id: "task-proof-1".into(),
            workspace_id: "ws_fabricated".into(),
            custody_id: "custody_fabricated".into(),
            task_revision: 1,
            fencing_generation: 1,
            candidate: ProofGitIdentity {
                commit: "aaaa".into(),
                tree: "bbbb".into(),
            },
            accepted_target: ProofGitIdentity {
                commit: "cccc".into(),
                tree: "dddd".into(),
            },
            integration: None,
            policy_digest: "sha256:p1".into(),
            requirements_digest: "sha256:p1".into(),
            material_digest: "sha256:m1".into(),
            prerequisites_digest: "sha256:p1".into(),
        },
        evidence: vec![ProofEvidenceReference {
            kind: "suite".into(),
            digest: "sha256:e1".into(),
            bytes: 64,
            provenance_digest: "sha256:pe1".into(),
        }],
    };
    let fab_err = submit_capture(&server, "DecapodLabs/decapod", 1, &fabricated_sub)
        .expect_err("fabricated custody must be rejected");
    assert!(fab_err.to_string().contains("CLOUD_PROOF_CONFLICT"));

    // 2. Skipping `verify` (`capture -> admit`) rejected fail-closed.
    let valid_sub = ProofCaptureRequest {
        subject: ProofSubject {
            workspace_id: "ws_bound_1".into(),
            custody_id: _rec.custody_id.clone(),
            ..fabricated_sub.subject.clone()
        },
        operation_id: new_uuid_v4().unwrap(),
        ..fabricated_sub
    };
    let fp = proof_capture_fingerprint(&valid_sub).unwrap();
    let cap = submit_capture(&server, "DecapodLabs/decapod", 1, &valid_sub).unwrap();
    let att_id = cap["attestation_id"].as_str().unwrap().to_string();

    let skip_err = submit_admit_or_publish(
        &server,
        "admit",
        &AdmitOrPublishProofRequest {
            project_id: "DecapodLabs/decapod".into(),
            operation_id: new_uuid_v4().unwrap(),
            attestation_id: att_id,
            expected_fingerprint: fp,
            expected_task_revision: 1,
            expected_fencing_generation: 1,
            expected_candidate_commit: "aaaa".into(),
            expected_accepted_target_commit: "cccc".into(),
            expected_policy_digest: "sha256:p1".into(),
        },
    )
    .expect_err("skipping independent verify must fail closed");
    assert!(skip_err.to_string().contains("CLOUD_PROOF_CONFLICT"));
}
