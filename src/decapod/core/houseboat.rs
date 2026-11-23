//! Decapod Houseboat Core Engine (#852-#859), Knowledge Write-Time Attestation
//! (#657), Cross-Crash Unknown-Outcome Reconciliation (#1367), Local Storage
//! Compaction Benchmarking (#1630), and Darkmatter Read-Only Snapshot Contract
//! Verification (#1711).
//!
//! This module stays strictly decoupled from forbidden/shared core files while
//! reusing Decapod's existing primitives (`gatekeeper`, `storage_compaction`,
//! `db`, `todo`, `mooring`).

use crate::core::db;
use crate::core::error::DecapodError;
use crate::core::gatekeeper;
use crate::core::mooring::{
    ArtifactProvenance, CollaborationContract, MooringEnforcementRequest, MooringOperation,
    MooringSubjectKind,
};
use crate::core::schemas;
use crate::core::storage_compaction;
use crate::core::todo::{self, ClaimMode, TodoCommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;
use std::time::Instant;

pub const HOUSEBOAT_SCHEMA_VERSION: &str = "decapod.houseboat.v1";
pub const DARKMATTER_SNAPSHOT_SCHEMA_VERSION: &str = "decapod.darkmatter-readonly-snapshot.v1";
pub const KNOWLEDGE_ATTESTATION_SCHEMA_VERSION: &str = "decapod.knowledge-attestation.v1";
pub const STORAGE_BENCHMARK_SCHEMA_VERSION: &str = "decapod.storage-compaction-benchmark.v1";

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

// ============================================================================
// #852: Houseboat 2 — Typed Fleet Coordination Graph & Multi-Workunit Merge Gate
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FleetEdgeKind {
    DependsOn,
    ConflictsWith,
    Supersedes,
    DerivesFrom,
    IntegrationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetEdge {
    pub from_workunit: String,
    pub to_workunit: String,
    pub kind: FleetEdgeKind,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_proof_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkunitNode {
    pub workunit_id: String,
    pub task_id: String,
    pub branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub touched_paths: BTreeSet<String>,
    pub trajectory_verified: bool,
    pub proof_refs: Vec<String>,
    pub capacity_units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombinedReconciliationProof {
    pub proof_id: String,
    pub accepted_target_sha: String,
    pub combined_tree_sha: String,
    pub participating_heads: BTreeMap<String, String>,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetAdmissionDecision {
    pub workunit_id: String,
    pub admitted: bool,
    pub blockers: Vec<String>,
    pub required_integrations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombinedMergeGateVerdict {
    pub allowed: bool,
    pub code: String,
    pub participating_workunits: Vec<String>,
    pub overlapping_paths: BTreeMap<String, Vec<String>>,
    pub explicit_conflicts: Vec<(String, String)>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetCoordGraph {
    nodes: BTreeMap<String, WorkunitNode>,
    edges: Vec<FleetEdge>,
}

impl FleetCoordGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: WorkunitNode) -> Result<(), String> {
        if node.workunit_id.trim().is_empty() {
            return Err("workunit_id must not be empty".to_string());
        }
        self.nodes.insert(node.workunit_id.clone(), node);
        Ok(())
    }

    pub fn add_edge(&mut self, edge: FleetEdge) -> Result<(), String> {
        if edge.from_workunit == edge.to_workunit {
            return Err(format!(
                "self-referential fleet edge rejected for '{}'",
                edge.from_workunit
            ));
        }
        if !self.nodes.contains_key(&edge.from_workunit)
            || !self.nodes.contains_key(&edge.to_workunit)
        {
            return Err(format!(
                "fleet edge endpoints ('{}' -> '{}') must exist in graph",
                edge.from_workunit, edge.to_workunit
            ));
        }
        self.edges.push(edge);
        if self.has_dependency_cycle() {
            self.edges.pop();
            return Err(
                "FLEET_DEPENDENCY_CYCLE: cyclic depends-on/derives-from edge rejected".to_string(),
            );
        }
        Ok(())
    }

    pub fn edges(&self) -> &[FleetEdge] {
        &self.edges
    }

    fn has_dependency_cycle(&self) -> bool {
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut indegree: BTreeMap<&str, usize> =
            self.nodes.keys().map(|k| (k.as_str(), 0)).collect();
        for edge in &self.edges {
            if matches!(
                edge.kind,
                FleetEdgeKind::DependsOn | FleetEdgeKind::DerivesFrom
            ) {
                adj.entry(edge.from_workunit.as_str())
                    .or_default()
                    .push(edge.to_workunit.as_str());
                *indegree.entry(edge.to_workunit.as_str()).or_insert(0) += 1;
            }
        }
        let mut queue: VecDeque<&str> = indegree
            .iter()
            .filter_map(|(k, &deg)| (deg == 0).then_some(*k))
            .collect();
        let mut visited = 0usize;
        while let Some(node) = queue.pop_front() {
            visited += 1;
            if let Some(neighbors) = adj.get(node) {
                for &next in neighbors {
                    if let Some(deg) = indegree.get_mut(next) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }
        visited != self.nodes.len()
    }

    pub fn evaluate_admission(&self, workunit_id: &str) -> Result<FleetAdmissionDecision, String> {
        let node = self
            .nodes
            .get(workunit_id)
            .ok_or_else(|| format!("unknown workunit '{workunit_id}'"))?;
        let mut blockers = Vec::new();
        let mut required_integrations = Vec::new();

        if !node.trajectory_verified || node.proof_refs.is_empty() {
            blockers.push(format!(
                "UNVERIFIED_TRAJECTORY: workunit '{}' lacks verified trajectory proof",
                node.workunit_id
            ));
        }

        // Check if another workunit supersedes this workunit.
        for edge in &self.edges {
            if edge.kind == FleetEdgeKind::Supersedes && edge.to_workunit == workunit_id {
                blockers.push(format!(
                    "WORKUNIT_SUPERSEDED: '{}' is superseded by '{}' ({})",
                    workunit_id, edge.from_workunit, edge.reason
                ));
            }
            if edge.from_workunit == workunit_id {
                match edge.kind {
                    FleetEdgeKind::DependsOn | FleetEdgeKind::DerivesFrom => {
                        if let Some(target) = self.nodes.get(&edge.to_workunit)
                            && (!target.trajectory_verified || target.proof_refs.is_empty())
                        {
                            blockers.push(format!(
                                "PREREQUISITE_UNVERIFIED: '{}' {:?} unverified '{}'",
                                workunit_id, edge.kind, edge.to_workunit
                            ));
                        }
                    }
                    FleetEdgeKind::ConflictsWith => {
                        blockers.push(format!(
                            "FLEET_CONFLICT_UNRESOLVED: '{}' conflicts-with '{}' ({})",
                            workunit_id, edge.to_workunit, edge.reason
                        ));
                    }
                    FleetEdgeKind::IntegrationRequired => {
                        required_integrations.push(edge.to_workunit.clone());
                    }
                    FleetEdgeKind::Supersedes => {}
                }
            } else if edge.to_workunit == workunit_id && edge.kind == FleetEdgeKind::ConflictsWith {
                blockers.push(format!(
                    "FLEET_CONFLICT_UNRESOLVED: '{}' conflicts-with '{}' ({})",
                    edge.from_workunit, workunit_id, edge.reason
                ));
            }
        }

        Ok(FleetAdmissionDecision {
            workunit_id: workunit_id.to_string(),
            admitted: blockers.is_empty(),
            blockers,
            required_integrations,
        })
    }

    pub fn evaluate_combined_merge_gate(
        &self,
        workunit_ids: &[&str],
        accepted_target_sha: &str,
        reconciliation_proof: Option<&CombinedReconciliationProof>,
    ) -> CombinedMergeGateVerdict {
        let mut blockers = Vec::new();
        let mut participating = Vec::new();
        let mut path_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut expected_heads: BTreeMap<String, String> = BTreeMap::new();

        for &id in workunit_ids {
            participating.push(id.to_string());
            let Some(node) = self.nodes.get(id) else {
                blockers.push(format!(
                    "UNKNOWN_WORKUNIT: '{id}' not registered in fleet graph"
                ));
                continue;
            };
            expected_heads.insert(node.workunit_id.clone(), node.head_sha.clone());
            if node.base_sha != accepted_target_sha {
                blockers.push(format!(
                    "COMBINED_TARGET_DRIFT: workunit '{}' base '{}' != accepted target '{}'",
                    node.workunit_id, node.base_sha, accepted_target_sha
                ));
            }
            if !node.trajectory_verified || node.proof_refs.is_empty() {
                blockers.push(format!(
                    "WORKUNIT_PROOF_MISSING: workunit '{}' has no verified trajectory proof",
                    node.workunit_id
                ));
            }
            for path in &node.touched_paths {
                path_owners
                    .entry(path.clone())
                    .or_default()
                    .push(node.workunit_id.clone());
            }
        }

        let overlapping_paths: BTreeMap<String, Vec<String>> = path_owners
            .into_iter()
            .filter(|(_, owners)| owners.len() > 1)
            .collect();

        let id_set: BTreeSet<&str> = workunit_ids.iter().copied().collect();
        let mut explicit_conflicts = Vec::new();
        let mut has_integration_edge = false;
        for edge in &self.edges {
            if id_set.contains(edge.from_workunit.as_str())
                && id_set.contains(edge.to_workunit.as_str())
            {
                match edge.kind {
                    FleetEdgeKind::ConflictsWith => {
                        explicit_conflicts
                            .push((edge.from_workunit.clone(), edge.to_workunit.clone()));
                    }
                    FleetEdgeKind::IntegrationRequired => {
                        has_integration_edge = true;
                    }
                    FleetEdgeKind::Supersedes => {
                        blockers.push(format!(
                            "SUPERSEDED_IN_COMBINED_SET: '{}' supersedes '{}'",
                            edge.from_workunit, edge.to_workunit
                        ));
                    }
                    FleetEdgeKind::DependsOn | FleetEdgeKind::DerivesFrom => {}
                }
            }
        }

        let requires_combined_proof =
            !overlapping_paths.is_empty() || !explicit_conflicts.is_empty() || has_integration_edge;

        if requires_combined_proof {
            match reconciliation_proof {
                None => {
                    blockers.push(
                        "COMBINED_CANDIDATE_RECONCILIATION_REQUIRED: overlapping/conflicting multi-workunit set requires verified CombinedReconciliationProof"
                            .to_string(),
                    );
                }
                Some(proof) => {
                    if !proof.passed {
                        blockers.push(format!(
                            "COMBINED_RECONCILIATION_FAILED: proof '{}' did not pass",
                            proof.proof_id
                        ));
                    }
                    if proof.accepted_target_sha != accepted_target_sha {
                        blockers.push(format!(
                            "COMBINED_PROOF_TARGET_MISMATCH: proof target '{}' != accepted '{}'",
                            proof.accepted_target_sha, accepted_target_sha
                        ));
                    }
                    if proof.participating_heads != expected_heads {
                        blockers.push(
                            "COMBINED_PROOF_HEAD_MISMATCH: participating workunit head SHAs changed since reconciliation"
                                .to_string(),
                        );
                    }
                }
            }
        }

        let allowed = blockers.is_empty();
        let code = if allowed {
            "COMBINED_MERGE_ADMITTED".to_string()
        } else if blockers
            .iter()
            .any(|b| b.starts_with("COMBINED_CANDIDATE_RECONCILIATION_REQUIRED"))
        {
            "COMBINED_CANDIDATE_RECONCILIATION_REQUIRED".to_string()
        } else {
            "COMBINED_MERGE_BLOCKED".to_string()
        };

        CombinedMergeGateVerdict {
            allowed,
            code,
            participating_workunits: participating,
            overlapping_paths,
            explicit_conflicts,
            blockers,
        }
    }
}

// ============================================================================
// #853: Houseboat 3 — Workspace Lifecycle State Machine, Habitat Manifest & Leak Detector
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceLifecycleState {
    Provisioning,
    Active,
    Canceling,
    Abandoned,
    Teardown,
    Recovered,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTransitionRecord {
    pub from: WorkspaceLifecycleState,
    pub to: WorkspaceLifecycleState,
    pub reason: String,
    pub actor: String,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitatManifest {
    pub schema_version: String,
    pub base_image: String,
    pub image_digest: String,
    pub toolchain_pins: BTreeMap<String, String>,
    pub env_keys_declared: BTreeSet<String>,
    pub mount_specs: Vec<String>,
    pub input_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
}

impl HabitatManifest {
    pub fn build(
        base_image: impl Into<String>,
        image_digest: impl Into<String>,
        toolchain_pins: BTreeMap<String, String>,
        env_keys_declared: BTreeSet<String>,
        mount_specs: Vec<String>,
    ) -> Self {
        let mut manifest = Self {
            schema_version: "decapod.habitat-manifest.v1".to_string(),
            base_image: base_image.into(),
            image_digest: image_digest.into(),
            toolchain_pins,
            env_keys_declared,
            mount_specs,
            input_hash: String::new(),
            output_hash: None,
        };
        manifest.input_hash = manifest.compute_input_hash();
        manifest
    }

    pub fn compute_input_hash(&self) -> String {
        let bytes = serde_json::to_vec(&(
            &self.schema_version,
            &self.base_image,
            &self.image_digest,
            &self.toolchain_pins,
            &self.env_keys_declared,
            &self.mount_specs,
        ))
        .unwrap_or_default();
        sha256_hex(&bytes)
    }

    pub fn verify_reproducibility(&self) -> bool {
        !self.image_digest.trim().is_empty() && self.input_hash == self.compute_input_hash()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLifecycleMachine {
    pub workspace_id: String,
    pub workunit_id: String,
    pub state: WorkspaceLifecycleState,
    pub habitat: HabitatManifest,
    pub dirty_worktree: bool,
    pub history: Vec<WorkspaceTransitionRecord>,
}

impl WorkspaceLifecycleMachine {
    pub fn new(
        workspace_id: impl Into<String>,
        workunit_id: impl Into<String>,
        habitat: HabitatManifest,
    ) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            workunit_id: workunit_id.into(),
            state: WorkspaceLifecycleState::Provisioning,
            habitat,
            dirty_worktree: false,
            history: Vec::new(),
        }
    }

    pub fn transition(
        &mut self,
        target: WorkspaceLifecycleState,
        actor: impl Into<String>,
        reason: impl Into<String>,
        epoch: u64,
    ) -> Result<(), String> {
        use WorkspaceLifecycleState::*;
        let valid = matches!(
            (self.state, target),
            (Provisioning, Active | Canceling | Abandoned | Teardown)
                | (Active, Canceling | Abandoned | Teardown)
                | (Canceling, Teardown | Recovered)
                | (Abandoned, Teardown | Recovered)
                | (Teardown, Terminated | Recovered)
                | (Recovered, Active | Teardown)
        );
        if self.state == target && target == Terminated {
            return Ok(());
        }
        if !valid {
            return Err(format!(
                "INVALID_WORKSPACE_TRANSITION: cannot transition workspace '{}' from {:?} to {:?}",
                self.workspace_id, self.state, target
            ));
        }
        if target == Terminated && self.dirty_worktree {
            return Err(format!(
                "DIRTY_WORKTREE_PRESERVED: refusing non-force termination of dirty workspace '{}'; transition to Recovered first",
                self.workspace_id
            ));
        }
        let prev = self.state;
        self.state = target;
        self.history.push(WorkspaceTransitionRecord {
            from: prev,
            to: target,
            reason: reason.into(),
            actor: actor.into(),
            epoch,
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakedResourceKind {
    Container,
    Mount,
    TempCredential,
    BackgroundService,
    OrphanWorktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedWorkspaceResource {
    pub resource_id: String,
    pub kind: LeakedResourceKind,
    pub owner_workunit: Option<String>,
    pub created_at_epoch: u64,
    pub ttl_secs: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeakedResourceFinding {
    pub resource_id: String,
    pub kind: LeakedResourceKind,
    pub reason: String,
    pub safe_remediation: String,
    pub requires_human_review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeakedResourceReport {
    pub scanned_count: usize,
    pub leaked_count: usize,
    pub findings: Vec<LeakedResourceFinding>,
}

pub fn detect_leaked_resources(
    active_workunits: &BTreeSet<String>,
    observed: &[ObservedWorkspaceResource],
    now_epoch: u64,
) -> LeakedResourceReport {
    let mut findings = Vec::new();
    for res in observed {
        let expired = now_epoch >= res.created_at_epoch.saturating_add(res.ttl_secs);
        let orphaned = match &res.owner_workunit {
            None => true,
            Some(owner) => !active_workunits.contains(owner),
        };
        if !expired && !orphaned {
            continue;
        }
        let reason = match (orphaned, expired) {
            (true, true) => "orphaned_and_expired".to_string(),
            (true, false) => "owner_workunit_inactive".to_string(),
            (false, true) => "ttl_exceeded".to_string(),
            (false, false) => unreachable!(),
        };
        let (safe_remediation, requires_human_review) = match (res.kind, res.dirty) {
            (LeakedResourceKind::OrphanWorktree, true) => (
                "preserve dirty worktree and surface for explicit operator review".to_string(),
                true,
            ),
            (LeakedResourceKind::TempCredential, _) => (
                "revoke and zeroize expired temporary credential immediately".to_string(),
                false,
            ),
            (LeakedResourceKind::Container, _) => (
                "stop and remove orphaned container after flushing logs".to_string(),
                false,
            ),
            (LeakedResourceKind::Mount, _) => (
                "unmount stale workspace bind mount after verifying zero open handles".to_string(),
                false,
            ),
            (LeakedResourceKind::BackgroundService, _) => (
                "terminate leaked helper process to restore INV-DAEMONLESS".to_string(),
                false,
            ),
            (LeakedResourceKind::OrphanWorktree, false) => (
                "prune clean orphaned worktree via decapod workspace prune".to_string(),
                false,
            ),
        };
        findings.push(LeakedResourceFinding {
            resource_id: res.resource_id.clone(),
            kind: res.kind,
            reason,
            safe_remediation,
            requires_human_review,
        });
    }
    LeakedResourceReport {
        scanned_count: observed.len(),
        leaked_count: findings.len(),
        findings,
    }
}

// ============================================================================
// #854: Houseboat 4 — Configurable Project Drift Policy, Abandoned API/Toolchain
// Detector & Bounded Seaworthiness Remediation Loop
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeaworthinessSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftPolicyConfig {
    pub projection_drift_severity: SeaworthinessSeverity,
    pub unsupported_toolchain_severity: SeaworthinessSeverity,
    pub deprecated_api_severity: SeaworthinessSeverity,
    pub max_warning_threshold: usize,
    pub max_remediation_attempts: u32,
    pub allow_uncertain_auto_apply: bool,
}

impl Default for DriftPolicyConfig {
    fn default() -> Self {
        Self {
            projection_drift_severity: SeaworthinessSeverity::Warning,
            unsupported_toolchain_severity: SeaworthinessSeverity::Blocking,
            deprecated_api_severity: SeaworthinessSeverity::Warning,
            max_warning_threshold: 5,
            max_remediation_attempts: 3,
            allow_uncertain_auto_apply: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainCheckInput {
    pub tool: String,
    pub observed_version: String,
    pub min_supported_version: String,
    pub sunset_epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeprecatedApiUsage {
    pub symbol: String,
    pub file_path: String,
    pub replacement: Option<String>,
    pub sunset_epoch: Option<u64>,
    pub confidence_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeaworthinessFinding {
    pub category: String,
    pub surface: String,
    pub severity: SeaworthinessSeverity,
    pub message: String,
    pub confidence_pct: u8,
    pub uncertainty_note: Option<String>,
    pub proposal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeaworthinessReport {
    pub seaworthy: bool,
    pub blocking_count: usize,
    pub warning_count: usize,
    pub threshold_exceeded: bool,
    pub findings: Vec<SeaworthinessFinding>,
}

fn parse_semver_tuple(raw: &str) -> Option<(u64, u64, u64)> {
    let clean = raw.trim().trim_start_matches('v');
    let core = clean.split('-').next()?;
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

pub fn evaluate_seaworthiness(
    policy: &DriftPolicyConfig,
    projection_drifts: &[String],
    toolchains: &[ToolchainCheckInput],
    api_usages: &[DeprecatedApiUsage],
    now_epoch: u64,
) -> SeaworthinessReport {
    let mut findings = Vec::new();

    for surface in projection_drifts {
        findings.push(SeaworthinessFinding {
            category: "projection_drift".to_string(),
            surface: surface.clone(),
            severity: policy.projection_drift_severity,
            message: format!("projection surface '{surface}' drifted from accepted inputs"),
            confidence_pct: 100,
            uncertainty_note: None,
            proposal: format!("refresh managed projection for '{surface}' and revalidate"),
        });
    }

    for tc in toolchains {
        let unsupported_ver = match (
            parse_semver_tuple(&tc.observed_version),
            parse_semver_tuple(&tc.min_supported_version),
        ) {
            (Some(obs), Some(min)) => obs < min,
            _ => true,
        };
        let past_sunset = tc.sunset_epoch.is_some_and(|s| now_epoch >= s);
        if unsupported_ver || past_sunset {
            let unparsed = parse_semver_tuple(&tc.observed_version).is_none();
            findings.push(SeaworthinessFinding {
                category: "unsupported_toolchain".to_string(),
                surface: tc.tool.clone(),
                severity: policy.unsupported_toolchain_severity,
                message: format!(
                    "toolchain '{}' version '{}' fails floor '{}' (past_sunset={})",
                    tc.tool, tc.observed_version, tc.min_supported_version, past_sunset
                ),
                confidence_pct: if unparsed { 60 } else { 100 },
                uncertainty_note: unparsed.then(|| {
                    format!(
                        "could not parse semver for observed version '{}'",
                        tc.observed_version
                    )
                }),
                proposal: format!(
                    "upgrade '{}' to >= '{}' and record proof",
                    tc.tool, tc.min_supported_version
                ),
            });
        }
    }

    for usage in api_usages {
        let past_sunset = usage.sunset_epoch.is_some_and(|s| now_epoch >= s);
        let severity = if past_sunset {
            SeaworthinessSeverity::Blocking
        } else {
            policy.deprecated_api_severity
        };
        let uncertainty_note = if usage.confidence_pct < 100 {
            Some(format!(
                "static usage detection confidence is {}%; manual confirmation required before applying",
                usage.confidence_pct
            ))
        } else {
            None
        };
        let proposal = match &usage.replacement {
            Some(rep) => format!(
                "migrate '{}' in '{}' to '{}'",
                usage.symbol, usage.file_path, rep
            ),
            None => format!(
                "audit abandoned API '{}' in '{}' (no drop-in replacement declared)",
                usage.symbol, usage.file_path
            ),
        };
        findings.push(SeaworthinessFinding {
            category: "abandoned_or_deprecated_api".to_string(),
            surface: format!("{}::{}", usage.file_path, usage.symbol),
            severity,
            message: format!(
                "deprecated/abandoned API '{}' referenced at '{}' (past_sunset={})",
                usage.symbol, usage.file_path, past_sunset
            ),
            confidence_pct: usage.confidence_pct,
            uncertainty_note,
            proposal,
        });
    }

    let blocking_count = findings
        .iter()
        .filter(|f| f.severity == SeaworthinessSeverity::Blocking)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity == SeaworthinessSeverity::Warning)
        .count();
    let threshold_exceeded = warning_count > policy.max_warning_threshold;

    SeaworthinessReport {
        seaworthy: blocking_count == 0 && !threshold_exceeded,
        blocking_count,
        warning_count,
        threshold_exceeded,
        findings,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemediationAttempt {
    pub attempt: u32,
    pub proposal: String,
    pub confidence_pct: u8,
    pub uncertainty_preserved: Option<String>,
    pub auto_apply_permitted: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeaworthinessRemediationLoop {
    pub policy: DriftPolicyConfig,
    pub attempts: Vec<RemediationAttempt>,
}

impl SeaworthinessRemediationLoop {
    pub fn new(policy: DriftPolicyConfig) -> Self {
        Self {
            policy,
            attempts: Vec::new(),
        }
    }

    pub fn propose_next(
        &mut self,
        finding: &SeaworthinessFinding,
    ) -> Result<RemediationAttempt, String> {
        let next_attempt = (self.attempts.len() as u32) + 1;
        if next_attempt > self.policy.max_remediation_attempts {
            return Err(format!(
                "REMEDIATION_BUDGET_EXHAUSTED: reached max_remediation_attempts={}",
                self.policy.max_remediation_attempts
            ));
        }
        let is_uncertain = finding.confidence_pct < 100 || finding.uncertainty_note.is_some();
        let auto_apply_permitted = !is_uncertain && self.policy.allow_uncertain_auto_apply;
        let record = RemediationAttempt {
            attempt: next_attempt,
            proposal: finding.proposal.clone(),
            confidence_pct: finding.confidence_pct,
            uncertainty_preserved: finding.uncertainty_note.clone(),
            auto_apply_permitted,
            status: if is_uncertain {
                "proposal_requires_human_review_due_to_uncertainty".to_string()
            } else {
                "proposal_ready_for_verified_application".to_string()
            },
        };
        self.attempts.push(record.clone());
        Ok(record)
    }
}

// ============================================================================
// #856: Houseboat 6 — Unified Durable Journal, Replay & Multi-Machine Conflict Sync
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalDomain {
    Todo,
    Workunit,
    Capsule,
    Lease,
    Proof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub entry_id: String,
    pub machine_id: String,
    pub domain: JournalDomain,
    pub entity_id: String,
    pub transition: String,
    pub payload_sha256: String,
    pub prev_hash: String,
    pub entry_hash: String,
    pub timestamp_epoch: u64,
    pub committed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalCheckpoint {
    pub checkpoint_id: String,
    pub last_committed_seq: u64,
    pub chain_head_hash: String,
    pub entity_states_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalReplaySummary {
    pub committed_count: usize,
    pub duplicate_entries_ignored: usize,
    pub incomplete_transitions: Vec<String>,
    pub entity_states: BTreeMap<(JournalDomain, String), String>,
    pub chain_head_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub domain: JournalDomain,
    pub entity_id: String,
    pub local_entry_id: String,
    pub local_transition: String,
    pub remote_entry_id: String,
    pub remote_transition: String,
    pub resolution_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiMachineSyncResult {
    pub clean: bool,
    pub merged_entries: Vec<JournalEntry>,
    pub conflicts: Vec<SyncConflict>,
    pub preserved_evidence_hashes: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DurableJournal {
    pub machine_id: String,
    pub entries: Vec<JournalEntry>,
    pub checkpoints: Vec<JournalCheckpoint>,
}

impl DurableJournal {
    pub fn new(machine_id: impl Into<String>) -> Self {
        Self {
            machine_id: machine_id.into(),
            entries: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append(
        &mut self,
        entry_id: impl Into<String>,
        domain: JournalDomain,
        entity_id: impl Into<String>,
        transition: impl Into<String>,
        payload: &[u8],
        timestamp_epoch: u64,
        committed: bool,
    ) -> JournalEntry {
        let entry_id = entry_id.into();
        if let Some(existing) = self.entries.iter().find(|e| e.entry_id == entry_id) {
            return existing.clone();
        }
        let seq = self.entries.last().map_or(1, |e| e.seq + 1);
        let prev_hash = self
            .entries
            .last()
            .map_or_else(|| "genesis".to_string(), |e| e.entry_hash.clone());
        let entity_id = entity_id.into();
        let transition = transition.into();
        let payload_sha256 = sha256_hex(payload);
        let entry_hash = sha256_hex(
            format!(
                "{}:{}:{}:{:?}:{}:{}:{}:{}",
                seq,
                entry_id,
                self.machine_id,
                domain,
                entity_id,
                transition,
                payload_sha256,
                prev_hash
            )
            .as_bytes(),
        );
        let entry = JournalEntry {
            seq,
            entry_id,
            machine_id: self.machine_id.clone(),
            domain,
            entity_id,
            transition,
            payload_sha256,
            prev_hash,
            entry_hash,
            timestamp_epoch,
            committed,
        };
        self.entries.push(entry.clone());
        entry
    }

    pub fn replay(&self) -> Result<JournalReplaySummary, String> {
        let mut seen_ids = BTreeSet::new();
        let mut entity_states = BTreeMap::new();
        let mut incomplete_transitions = Vec::new();
        let mut duplicate_entries_ignored = 0usize;
        let mut committed_count = 0usize;
        let mut expected_prev = "genesis".to_string();

        for entry in &self.entries {
            if entry.prev_hash != expected_prev {
                return Err(format!(
                    "JOURNAL_CHAIN_CORRUPT: seq={} expected prev_hash '{}' observed '{}'",
                    entry.seq, expected_prev, entry.prev_hash
                ));
            }
            expected_prev = entry.entry_hash.clone();
            if !seen_ids.insert(entry.entry_id.clone()) {
                duplicate_entries_ignored += 1;
                continue;
            }
            if !entry.committed {
                // Never invent success for incomplete local transitions.
                incomplete_transitions.push(entry.entry_id.clone());
                continue;
            }
            committed_count += 1;
            entity_states.insert(
                (entry.domain, entry.entity_id.clone()),
                entry.transition.clone(),
            );
        }

        Ok(JournalReplaySummary {
            committed_count,
            duplicate_entries_ignored,
            incomplete_transitions,
            entity_states,
            chain_head_hash: expected_prev,
        })
    }

    pub fn checkpoint(
        &mut self,
        checkpoint_id: impl Into<String>,
    ) -> Result<JournalCheckpoint, String> {
        let summary = self.replay()?;
        let last_committed_seq = self
            .entries
            .iter()
            .rev()
            .find(|e| e.committed)
            .map_or(0, |e| e.seq);
        let state_bytes = serde_json::to_vec(&summary.entity_states).unwrap_or_default();
        let cp = JournalCheckpoint {
            checkpoint_id: checkpoint_id.into(),
            last_committed_seq,
            chain_head_hash: summary.chain_head_hash,
            entity_states_digest: sha256_hex(&state_bytes),
        };
        self.checkpoints.push(cp.clone());
        Ok(cp)
    }
}

/// Conflict-aware multi-machine journal reconciliation.
/// Never applies silent last-writer-wins when two machines record contradictory
/// committed transitions for the same `(domain, entity_id)` without shared ancestry.
pub fn reconcile_machine_journals(
    local: &DurableJournal,
    remote: &DurableJournal,
) -> MultiMachineSyncResult {
    let mut local_by_entity: BTreeMap<(JournalDomain, String), &JournalEntry> = BTreeMap::new();
    let mut remote_by_entity: BTreeMap<(JournalDomain, String), &JournalEntry> = BTreeMap::new();
    let mut preserved_evidence_hashes = BTreeSet::new();

    for e in local.entries.iter().filter(|e| e.committed) {
        preserved_evidence_hashes.insert(e.payload_sha256.clone());
        local_by_entity.insert((e.domain, e.entity_id.clone()), e);
    }
    for e in remote.entries.iter().filter(|e| e.committed) {
        preserved_evidence_hashes.insert(e.payload_sha256.clone());
        remote_by_entity.insert((e.domain, e.entity_id.clone()), e);
    }

    let mut conflicts = Vec::new();
    for (key, local_entry) in &local_by_entity {
        if let Some(remote_entry) = remote_by_entity.get(key)
            && local_entry.entry_id != remote_entry.entry_id
            && (local_entry.transition != remote_entry.transition
                || local_entry.payload_sha256 != remote_entry.payload_sha256)
        {
            conflicts.push(SyncConflict {
                domain: key.0,
                entity_id: key.1.clone(),
                local_entry_id: local_entry.entry_id.clone(),
                local_transition: local_entry.transition.clone(),
                remote_entry_id: remote_entry.entry_id.clone(),
                remote_transition: remote_entry.transition.clone(),
                resolution_required: true,
            });
        }
    }

    let mut seen = BTreeSet::new();
    let mut merged_entries = Vec::new();
    if conflicts.is_empty() {
        for e in local
            .entries
            .iter()
            .chain(remote.entries.iter())
            .filter(|e| e.committed)
        {
            if seen.insert(e.entry_id.clone()) {
                merged_entries.push(e.clone());
            }
        }
        merged_entries.sort_by(|a, b| {
            a.timestamp_epoch
                .cmp(&b.timestamp_epoch)
                .then_with(|| a.entry_id.cmp(&b.entry_id))
        });
    }

    MultiMachineSyncResult {
        clean: conflicts.is_empty(),
        merged_entries,
        conflicts,
        preserved_evidence_hashes,
    }
}

// ============================================================================
// #857: Houseboat 7 — MotionAdjudicator (Premature / Out-of-Scope Work & Recovery)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MotionViolationKind {
    PrematureImplementation,
    OutOfScopeMutation,
    StalledRetryLoop,
    AuthorityProjectionMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkunitMotionSnapshot {
    pub workunit_id: String,
    pub plan_converged: bool,
    pub prerequisite_proofs_passed: bool,
    pub declared_scope_prefixes: Vec<String>,
    pub touched_paths: Vec<String>,
    pub consecutive_failures: u32,
    pub failure_signatures: Vec<String>,
    pub authority_state: String,
    pub projected_motion_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedRecoveryProposal {
    pub proposal_id: String,
    pub violation: MotionViolationKind,
    pub bounded_steps: Vec<String>,
    pub preserves_proof_gates: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionAdjudication {
    pub workunit_id: String,
    pub permitted: bool,
    pub dual_path_consistent: bool,
    pub violations: Vec<(MotionViolationKind, String)>,
    pub recovery_proposals: Vec<BoundedRecoveryProposal>,
}

pub struct MotionAdjudicator;

impl MotionAdjudicator {
    pub fn adjudicate(snapshot: &WorkunitMotionSnapshot) -> MotionAdjudication {
        let mut violations = Vec::new();
        let mut recovery_proposals = Vec::new();

        if !snapshot.touched_paths.is_empty()
            && (!snapshot.plan_converged || !snapshot.prerequisite_proofs_passed)
        {
            violations.push((
                MotionViolationKind::PrematureImplementation,
                format!(
                    "workunit '{}' mutated {} path(s) before plan convergence ({}) and prerequisite proofs ({})",
                    snapshot.workunit_id,
                    snapshot.touched_paths.len(),
                    snapshot.plan_converged,
                    snapshot.prerequisite_proofs_passed
                ),
            ));
            recovery_proposals.push(BoundedRecoveryProposal {
                proposal_id: format!("recover:{}:premature", snapshot.workunit_id),
                violation: MotionViolationKind::PrematureImplementation,
                bounded_steps: vec![
                    "halt further source edits".to_string(),
                    "converge governed plan and resolve prerequisite proofs".to_string(),
                    "re-verify touched paths against converged scope".to_string(),
                ],
                preserves_proof_gates: true,
            });
        }

        let out_of_scope: Vec<String> = snapshot
            .touched_paths
            .iter()
            .filter(|path| {
                !snapshot.declared_scope_prefixes.is_empty()
                    && !snapshot
                        .declared_scope_prefixes
                        .iter()
                        .any(|prefix| path.starts_with(prefix))
            })
            .cloned()
            .collect();
        if !out_of_scope.is_empty() {
            violations.push((
                MotionViolationKind::OutOfScopeMutation,
                format!(
                    "workunit '{}' touched out-of-scope paths: {:?}",
                    snapshot.workunit_id, out_of_scope
                ),
            ));
            recovery_proposals.push(BoundedRecoveryProposal {
                proposal_id: format!("recover:{}:out-of-scope", snapshot.workunit_id),
                violation: MotionViolationKind::OutOfScopeMutation,
                bounded_steps: vec![
                    format!("revert or split out-of-scope edits: {:?}", out_of_scope),
                    "open dedicated scoped todo if cross-boundary work is required".to_string(),
                ],
                preserves_proof_gates: true,
            });
        }

        let repeated_identical_failure = snapshot.consecutive_failures >= 3
            || (snapshot.failure_signatures.len() >= 2
                && snapshot.failure_signatures.windows(2).any(|w| w[0] == w[1]));
        if repeated_identical_failure {
            violations.push((
                MotionViolationKind::StalledRetryLoop,
                format!(
                    "workunit '{}' stalled with {} consecutive failure(s)",
                    snapshot.workunit_id, snapshot.consecutive_failures
                ),
            ));
            recovery_proposals.push(BoundedRecoveryProposal {
                proposal_id: format!("recover:{}:stalled", snapshot.workunit_id),
                violation: MotionViolationKind::StalledRetryLoop,
                bounded_steps: vec![
                    "stop identical retries".to_string(),
                    "capture causal diagnostic diff against failing check".to_string(),
                    "escalate or revise strategy before next bounded attempt".to_string(),
                ],
                preserves_proof_gates: true,
            });
        }

        let dual_path_consistent = snapshot.authority_state == snapshot.projected_motion_state
            || (snapshot.authority_state == "VERIFIED"
                && snapshot.projected_motion_state == "completed");
        if !dual_path_consistent {
            violations.push((
                MotionViolationKind::AuthorityProjectionMismatch,
                format!(
                    "authority state '{}' disagrees with projected motion state '{}'",
                    snapshot.authority_state, snapshot.projected_motion_state
                ),
            ));
            recovery_proposals.push(BoundedRecoveryProposal {
                proposal_id: format!("recover:{}:dual-path", snapshot.workunit_id),
                violation: MotionViolationKind::AuthorityProjectionMismatch,
                bounded_steps: vec![
                    "recompute motion projection from canonical authority store".to_string(),
                    "block publication until authority and motion projections agree".to_string(),
                ],
                preserves_proof_gates: true,
            });
        }

        MotionAdjudication {
            workunit_id: snapshot.workunit_id.clone(),
            permitted: violations.is_empty(),
            dual_path_consistent,
            violations,
            recovery_proposals,
        }
    }
}

// ============================================================================
// #858: Houseboat 8 — Dependency Classifier, Typed Degraded Modes, Partial-Write
// Recovery & Exit-Degraded Evidence Contract
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCriticality {
    Critical,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    LocalStore,
    GitRepository,
    ProofGate,
    CloudBackend,
    ContainerRuntime,
    ExternalProvider,
    OptionalTooling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedMode {
    Nominal,
    CloudUnavailable,
    ContainerUnavailable,
    ProviderUnavailable,
    ToolingUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyStatus {
    pub name: String,
    pub kind: DependencyKind,
    pub criticality: DependencyCriticality,
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitDegradedContract {
    pub active_modes: Vec<DegradedMode>,
    pub recovery_objectives: Vec<String>,
    pub required_evidence: BTreeSet<String>,
    pub repo_health_check_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalState {
    Nominal,
    Degraded,
    StoppedRecoverable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyClassificationReport {
    pub state: OperationalState,
    pub critical_failures: Vec<String>,
    pub active_degraded_modes: Vec<DegradedMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_contract: Option<ExitDegradedContract>,
}

pub fn classify_dependencies(statuses: &[DependencyStatus]) -> DependencyClassificationReport {
    let mut critical_failures = Vec::new();
    let mut degraded_modes = BTreeSet::new();
    let mut recovery_objectives = Vec::new();
    let mut required_evidence = BTreeSet::new();

    for dep in statuses {
        if dep.available {
            continue;
        }
        match dep.criticality {
            DependencyCriticality::Critical => {
                critical_failures.push(format!(
                    "CRITICAL_DEPENDENCY_UNAVAILABLE: '{}' ({:?}): {}",
                    dep.name, dep.kind, dep.detail
                ));
            }
            DependencyCriticality::Optional => {
                let (mode, objective, evidence) = match dep.kind {
                    DependencyKind::CloudBackend => (
                        DegradedMode::CloudUnavailable,
                        "re-establish authenticated cloud connectivity and reconcile offline state",
                        "evidence:cloud_connectivity_and_sync_ok",
                    ),
                    DependencyKind::ContainerRuntime => (
                        DegradedMode::ContainerUnavailable,
                        "restore container daemon health and verify isolated workspace image",
                        "evidence:container_runtime_health_ok",
                    ),
                    DependencyKind::ExternalProvider => (
                        DegradedMode::ProviderUnavailable,
                        "restore external provider reachability and revalidate pending checks",
                        "evidence:external_provider_probe_ok",
                    ),
                    _ => (
                        DegradedMode::ToolingUnavailable,
                        "restore optional diagnostic tooling and refresh derived projections",
                        "evidence:optional_tooling_probe_ok",
                    ),
                };
                degraded_modes.insert(mode);
                recovery_objectives.push(format!("{}: {}", dep.name, objective));
                required_evidence.insert(evidence.to_string());
            }
        }
    }

    if !critical_failures.is_empty() {
        return DependencyClassificationReport {
            state: OperationalState::StoppedRecoverable,
            critical_failures,
            active_degraded_modes: degraded_modes.into_iter().collect(),
            exit_contract: None,
        };
    }

    if !degraded_modes.is_empty() {
        let modes: Vec<DegradedMode> = degraded_modes.into_iter().collect();
        return DependencyClassificationReport {
            state: OperationalState::Degraded,
            critical_failures,
            active_degraded_modes: modes.clone(),
            exit_contract: Some(ExitDegradedContract {
                active_modes: modes,
                recovery_objectives,
                required_evidence,
                repo_health_check_required: true,
            }),
        };
    }

    DependencyClassificationReport {
        state: OperationalState::Nominal,
        critical_failures,
        active_degraded_modes: vec![DegradedMode::Nominal],
        exit_contract: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritePhase {
    Staging,
    Synced,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialWriteMarker {
    pub operation_id: String,
    pub target_path: String,
    pub staging_path: String,
    pub expected_sha256: String,
    pub phase: WritePhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartialWriteRecoveryAction {
    AlreadyCommittedIdempotent,
    PromoteStagingToTarget,
    RollbackIncompleteStaging,
    FailClosedCorruptTarget,
}

pub fn recover_partial_write(
    marker: &PartialWriteMarker,
    observed_target_sha256: Option<&str>,
    observed_staging_sha256: Option<&str>,
) -> PartialWriteRecoveryAction {
    if observed_target_sha256 == Some(marker.expected_sha256.as_str()) {
        return PartialWriteRecoveryAction::AlreadyCommittedIdempotent;
    }
    if marker.phase == WritePhase::Committed
        && observed_target_sha256.is_some()
        && observed_target_sha256 != Some(marker.expected_sha256.as_str())
    {
        return PartialWriteRecoveryAction::FailClosedCorruptTarget;
    }
    if observed_staging_sha256 == Some(marker.expected_sha256.as_str()) {
        return PartialWriteRecoveryAction::PromoteStagingToTarget;
    }
    PartialWriteRecoveryAction::RollbackIncompleteStaging
}

pub fn evaluate_exit_degraded(
    contract: &ExitDegradedContract,
    provided_evidence: &BTreeSet<String>,
    repo_health_passed: bool,
) -> Result<(), Vec<String>> {
    let mut missing = Vec::new();
    if contract.repo_health_check_required && !repo_health_passed {
        missing.push("REPO_HEALTH_CHECK_REQUIRED".to_string());
    }
    for req in &contract.required_evidence {
        if !provided_evidence.contains(req) {
            missing.push(format!("MISSING_RECOVERY_EVIDENCE:{req}"));
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

// ============================================================================
// #859: Houseboat 9 — Shared Principal Resource Budgets & Uncertain-Effect Holds
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceUnit {
    Tokens,
    ComputeSeconds,
    WallSeconds,
    StorageBytes,
    WorkunitSlots,
    HumanAttentionUnits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetPolicy {
    pub principal_id: String,
    pub unit: ResourceUnit,
    pub principal_ceiling: u64,
    pub per_child_limit: Option<u64>,
    pub warning_threshold_pct: u8,
    pub hard_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    Reserved,
    /// Effect is uncertain (e.g., lost reply or crash mid-operation).
    /// MUST NEVER be auto-refunded or reclaimed on expiry!
    UncertainHold,
    Settled,
    ReleasedNoEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetReservation {
    pub reservation_id: String,
    pub principal_id: String,
    pub workunit_id: String,
    pub task_id: String,
    pub phase: String,
    pub unit: ResourceUnit,
    pub reserved_amount: u64,
    pub settled_amount: Option<u64>,
    pub issued_at_epoch: u64,
    pub expires_at_epoch: u64,
    pub fence_token: u64,
    pub state: ReservationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_authority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty_reason: Option<String>,
}

impl BudgetReservation {
    /// Active capacity occupied by this reservation.
    /// Uncertain holds ALWAYS occupy their full reserved allocation until
    /// authoritatively resolved; unknowns are never charged as zero.
    pub fn occupied_units(&self) -> u64 {
        match self.state {
            ReservationState::Reserved | ReservationState::UncertainHold => self.reserved_amount,
            ReservationState::Settled => self.settled_amount.unwrap_or(self.reserved_amount),
            ReservationState::ReleasedNoEffect => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationLevel {
    Normal,
    Advisory,
    Escalate,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetForecast {
    pub principal_id: String,
    pub unit: ResourceUnit,
    pub ceiling: u64,
    pub reserved_units: u64,
    pub uncertain_held_units: u64,
    pub settled_units: u64,
    pub available_units: u64,
    pub utilization_pct: u8,
    pub escalation_level: EscalationLevel,
    pub assumptions: Vec<String>,
    pub confidence_pct: u8,
    pub strategy_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeSettlement {
    Settled { actual_amount: u64 },
    ConfirmedNoEffect,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceBudgetLedger {
    policies: BTreeMap<(String, ResourceUnit), BudgetPolicy>,
    reservations: BTreeMap<String, BudgetReservation>,
    next_fence: u64,
}

impl ResourceBudgetLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_policy(&mut self, policy: BudgetPolicy) {
        self.policies
            .insert((policy.principal_id.clone(), policy.unit), policy);
    }

    pub fn get_reservation(&self, reservation_id: &str) -> Option<&BudgetReservation> {
        self.reservations.get(reservation_id)
    }

    fn principal_occupied(&self, principal_id: &str, unit: ResourceUnit) -> u64 {
        self.reservations
            .values()
            .filter(|r| r.principal_id == principal_id && r.unit == unit)
            .map(BudgetReservation::occupied_units)
            .sum()
    }

    fn child_occupied(&self, principal_id: &str, workunit_id: &str, unit: ResourceUnit) -> u64 {
        self.reservations
            .values()
            .filter(|r| {
                r.principal_id == principal_id && r.workunit_id == workunit_id && r.unit == unit
            })
            .map(BudgetReservation::occupied_units)
            .sum()
    }

    /// Atomically reserve capacity under the principal ceiling and optional per-child limit.
    /// Idempotent when called repeatedly with the same `reservation_id` and identical parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        &mut self,
        reservation_id: impl Into<String>,
        principal_id: impl Into<String>,
        workunit_id: impl Into<String>,
        task_id: impl Into<String>,
        phase: impl Into<String>,
        unit: ResourceUnit,
        amount: u64,
        issued_at_epoch: u64,
        expires_at_epoch: u64,
    ) -> Result<BudgetReservation, String> {
        let reservation_id = reservation_id.into();
        let principal_id = principal_id.into();
        let workunit_id = workunit_id.into();
        let task_id = task_id.into();
        let phase = phase.into();

        if let Some(existing) = self.reservations.get(&reservation_id) {
            if existing.principal_id == principal_id
                && existing.workunit_id == workunit_id
                && existing.unit == unit
                && existing.reserved_amount == amount
            {
                return Ok(existing.clone());
            }
            return Err(format!(
                "RESERVATION_ID_CONFLICT: '{reservation_id}' already bound to different parameters"
            ));
        }

        let policy = self
            .policies
            .get(&(principal_id.clone(), unit))
            .ok_or_else(|| {
                format!(
                    "BUDGET_POLICY_MISSING: no policy for principal '{}' unit '{:?}'",
                    principal_id, unit
                )
            })?;

        if let Some(child_limit) = policy.per_child_limit {
            let child_used = self.child_occupied(&principal_id, &workunit_id, unit);
            if child_used.saturating_add(amount) > child_limit {
                return Err(format!(
                    "BUDGET_CHILD_LIMIT_EXCEEDED: workunit '{}' requested {} with {} already occupied (limit {})",
                    workunit_id, amount, child_used, child_limit
                ));
            }
        }

        let used = self.principal_occupied(&principal_id, unit);
        if used.saturating_add(amount) > policy.principal_ceiling {
            return Err(format!(
                "BUDGET_PRINCIPAL_CEILING_EXCEEDED: principal '{}' requested {} with {} occupied (ceiling {})",
                principal_id, amount, used, policy.principal_ceiling
            ));
        }

        self.next_fence += 1;
        let reservation = BudgetReservation {
            reservation_id: reservation_id.clone(),
            principal_id,
            workunit_id,
            task_id,
            phase,
            unit,
            reserved_amount: amount,
            settled_amount: None,
            issued_at_epoch,
            expires_at_epoch,
            fence_token: self.next_fence,
            state: ReservationState::Reserved,
            release_authority: None,
            uncertainty_reason: None,
        };
        self.reservations
            .insert(reservation_id, reservation.clone());
        Ok(reservation)
    }

    /// Place an active reservation into an explicit `UncertainHold` (e.g., lost reply).
    /// Uncertain holds continue occupying their full allocation even past `expires_at_epoch`.
    pub fn mark_effect_uncertain(
        &mut self,
        reservation_id: &str,
        fence_token: u64,
        reason: impl Into<String>,
    ) -> Result<BudgetReservation, String> {
        let res = self
            .reservations
            .get_mut(reservation_id)
            .ok_or_else(|| format!("UNKNOWN_RESERVATION: '{reservation_id}'"))?;
        if res.fence_token != fence_token {
            return Err(format!(
                "STALE_FENCE_TOKEN: reservation '{}' expects fence {} (got {})",
                reservation_id, res.fence_token, fence_token
            ));
        }
        if matches!(
            res.state,
            ReservationState::Settled | ReservationState::ReleasedNoEffect
        ) {
            return Err(format!(
                "RESERVATION_ALREADY_FINALIZED: '{}' is {:?}",
                reservation_id, res.state
            ));
        }
        res.state = ReservationState::UncertainHold;
        res.uncertainty_reason = Some(reason.into());
        Ok(res.clone())
    }

    /// Authoritatively reconcile a `Reserved` or `UncertainHold` allocation.
    /// Rejects stale fence tokens, missing release authority, or double-releases.
    pub fn reconcile_outcome(
        &mut self,
        reservation_id: &str,
        fence_token: u64,
        authority: impl Into<String>,
        outcome: OutcomeSettlement,
    ) -> Result<BudgetReservation, String> {
        let authority = authority.into();
        if authority.trim().is_empty() {
            return Err("MISSING_RELEASE_AUTHORITY: authoritative owner required".to_string());
        }
        let res = self
            .reservations
            .get_mut(reservation_id)
            .ok_or_else(|| format!("UNKNOWN_RESERVATION: '{reservation_id}'"))?;
        if res.fence_token != fence_token {
            return Err(format!(
                "STALE_FENCE_TOKEN: reservation '{}' expects fence {} (got {})",
                reservation_id, res.fence_token, fence_token
            ));
        }
        match (&res.state, outcome) {
            (ReservationState::ReleasedNoEffect, OutcomeSettlement::ConfirmedNoEffect) => {
                // Idempotent no-op without inflating available capacity.
                Ok(res.clone())
            }
            (ReservationState::Settled, OutcomeSettlement::Settled { actual_amount })
                if res.settled_amount == Some(actual_amount) =>
            {
                Ok(res.clone())
            }
            (ReservationState::ReleasedNoEffect | ReservationState::Settled, _) => Err(format!(
                "DOUBLE_RELEASE_OR_CONFLICTING_SETTLEMENT: reservation '{}' already finalized as {:?}",
                reservation_id, res.state
            )),
            (
                ReservationState::Reserved | ReservationState::UncertainHold,
                OutcomeSettlement::Settled { actual_amount },
            ) => {
                res.state = ReservationState::Settled;
                res.settled_amount = Some(actual_amount);
                res.release_authority = Some(authority);
                Ok(res.clone())
            }
            (
                ReservationState::Reserved | ReservationState::UncertainHold,
                OutcomeSettlement::ConfirmedNoEffect,
            ) => {
                res.state = ReservationState::ReleasedNoEffect;
                res.settled_amount = Some(0);
                res.release_authority = Some(authority);
                Ok(res.clone())
            }
        }
    }

    /// Sweep expired reservations.
    /// CRITICAL INVARIANT: `UncertainHold` reservations are NEVER released on expiry,
    /// because an unresolved effect may still commit! Only unstarted `Reserved` leases
    /// explicitly marked `confirmed_unused_by_owner` may be reclaimed.
    pub fn sweep_expired_unused(
        &mut self,
        now_epoch: u64,
        confirmed_unused_ids: &BTreeSet<String>,
        authority: &str,
    ) -> Vec<String> {
        let mut released = Vec::new();
        for res in self.reservations.values_mut() {
            if res.state == ReservationState::Reserved
                && now_epoch >= res.expires_at_epoch
                && confirmed_unused_ids.contains(&res.reservation_id)
            {
                res.state = ReservationState::ReleasedNoEffect;
                res.settled_amount = Some(0);
                res.release_authority = Some(authority.to_string());
                released.push(res.reservation_id.clone());
            }
        }
        released
    }

    pub fn forecast(
        &self,
        principal_id: &str,
        unit: ResourceUnit,
    ) -> Result<BudgetForecast, String> {
        let policy = self
            .policies
            .get(&(principal_id.to_string(), unit))
            .ok_or_else(|| {
                format!(
                    "BUDGET_POLICY_MISSING: principal '{}' unit '{:?}'",
                    principal_id, unit
                )
            })?;
        let mut reserved_units = 0u64;
        let mut uncertain_held_units = 0u64;
        let mut settled_units = 0u64;
        for res in self
            .reservations
            .values()
            .filter(|r| r.principal_id == principal_id && r.unit == unit)
        {
            match res.state {
                ReservationState::Reserved => reserved_units += res.reserved_amount,
                ReservationState::UncertainHold => uncertain_held_units += res.reserved_amount,
                ReservationState::Settled => {
                    settled_units += res.settled_amount.unwrap_or(res.reserved_amount)
                }
                ReservationState::ReleasedNoEffect => {}
            }
        }
        let occupied = reserved_units + uncertain_held_units + settled_units;
        let available_units = policy.principal_ceiling.saturating_sub(occupied);
        let utilization_pct = occupied
            .saturating_mul(100)
            .checked_div(policy.principal_ceiling)
            .map_or(100, |pct| pct.min(100) as u8);

        let escalation_level = if occupied >= policy.principal_ceiling && policy.hard_stop {
            EscalationLevel::Blocked
        } else if uncertain_held_units > 0 && utilization_pct >= policy.warning_threshold_pct {
            EscalationLevel::Escalate
        } else if utilization_pct >= policy.warning_threshold_pct {
            EscalationLevel::Advisory
        } else {
            EscalationLevel::Normal
        };

        let mut assumptions = vec![
            "Provider billing metrics are not treated as kernel truth".to_string(),
            "Uncertain holds remain counted at 100% of reserved amount until settled".to_string(),
        ];
        if unit == ResourceUnit::HumanAttentionUnits {
            assumptions.push(
                "Human attention is an ordinal advisory estimate, not an exact physical unit"
                    .to_string(),
            );
        }
        let confidence_pct = if uncertain_held_units > 0 {
            65
        } else if unit == ResourceUnit::HumanAttentionUnits {
            50
        } else {
            95
        };

        let mut strategy_hints = Vec::new();
        if uncertain_held_units > 0 {
            strategy_hints.push(
                "reconcile uncertain-effect holds with authoritative owners before spawning new workunits"
                    .to_string(),
            );
        }
        if available_units == 0 {
            strategy_hints.push(
                "ceiling exhausted: serialize remaining tasks or request explicit principal ceiling increase"
                    .to_string(),
            );
        } else if utilization_pct >= policy.warning_threshold_pct {
            strategy_hints.push(
                "approaching budget ceiling: prefer bounded incremental steps over parallel fan-out"
                    .to_string(),
            );
        }

        Ok(BudgetForecast {
            principal_id: principal_id.to_string(),
            unit,
            ceiling: policy.principal_ceiling,
            reserved_units,
            uncertain_held_units,
            settled_units,
            available_units,
            utilization_pct,
            escalation_level,
            assumptions,
            confidence_pct,
            strategy_hints,
        })
    }
}

// ============================================================================
// #657: Write-Time Source Attestation for Advisory Knowledge Entries
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeClass {
    UserPreference,
    ProjectContext,
    EpisodicLesson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    HumanInput,
    IssueOrPr,
    RepoFile,
    CommandOutput,
    Commit,
    AgentObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeAttestation {
    pub schema_version: String,
    pub record_id: String,
    pub knowledge_class: KnowledgeClass,
    pub source_kind: KnowledgeSourceKind,
    pub source_ref: String,
    pub captured_at_epoch: u64,
    pub content_sha256: String,
    pub author_actor: String,
    pub negative_guidance: bool,
    pub verified_by_human: bool,
    pub promoted_to_doctrine: bool,
    pub promotion_evidence_refs: Vec<String>,
    pub secret_scan_passed: bool,
}

impl KnowledgeAttestation {
    #[allow(clippy::too_many_arguments)]
    pub fn create_attested(
        record_id: impl Into<String>,
        knowledge_class: KnowledgeClass,
        source_kind: KnowledgeSourceKind,
        source_ref: impl Into<String>,
        content: &str,
        author_actor: impl Into<String>,
        captured_at_epoch: u64,
        negative_guidance: bool,
        verified_by_human: bool,
    ) -> Result<Self, String> {
        let source_ref = source_ref.into();
        if source_ref.trim().is_empty() {
            return Err("KNOWLEDGE_ATTESTATION_MISSING_SOURCE_REF".to_string());
        }
        if gatekeeper::contains_secret_material(content)
            || gatekeeper::contains_secret_material(&source_ref)
        {
            return Err(
                "KNOWLEDGE_SECRET_REJECTED: secret or credential material cannot be stored in advisory knowledge"
                    .to_string(),
            );
        }
        Ok(Self {
            schema_version: KNOWLEDGE_ATTESTATION_SCHEMA_VERSION.to_string(),
            record_id: record_id.into(),
            knowledge_class,
            source_kind,
            source_ref,
            captured_at_epoch,
            content_sha256: sha256_hex(content.as_bytes()),
            author_actor: author_actor.into(),
            negative_guidance,
            verified_by_human,
            promoted_to_doctrine: false,
            promotion_evidence_refs: Vec::new(),
            secret_scan_passed: true,
        })
    }

    pub fn promote_to_doctrine(
        &mut self,
        human_approver: &str,
        evidence_refs: Vec<String>,
    ) -> Result<(), String> {
        if human_approver.trim().is_empty() || !self.verified_by_human {
            return Err(
                "KNOWLEDGE_PROMOTION_REQUIRES_HUMAN_APPROVAL: cannot promote without human verification"
                    .to_string(),
            );
        }
        if evidence_refs.is_empty() {
            return Err(
                "KNOWLEDGE_PROMOTION_REQUIRES_EVIDENCE: episodic/advisory promotion needs proof references"
                    .to_string(),
            );
        }
        self.promoted_to_doctrine = true;
        self.promotion_evidence_refs = evidence_refs;
        Ok(())
    }
}

// ============================================================================
// #1367: Cross-Crash Unknown-Outcome Reconciliation
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrashRecoveryVerdict {
    SafeToResume,
    RefusedStaleRepoState,
    RefusedInstructionChanged,
    RefusedUnresolvedContradiction,
    RefusedUnknownOutcomePending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptedExecutionRecord {
    pub workunit_id: String,
    pub task_id: String,
    pub instruction_revision: String,
    pub repo_head_sha: String,
    pub checkpoint_id: String,
    pub pending_operation_id: Option<String>,
    pub unresolved_contradictions: Vec<String>,
    pub model_hop_history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReconciliationOutcome {
    pub verdict: CrashRecoveryVerdict,
    pub can_resume: bool,
    pub reason: String,
}

#[must_use]
pub fn reconcile_after_crash(
    record: &InterruptedExecutionRecord,
    current_instruction_revision: &str,
    current_repo_head_sha: &str,
    pending_outcome_resolved: Option<bool>,
) -> CrashReconciliationOutcome {
    if !record.unresolved_contradictions.is_empty() {
        return CrashReconciliationOutcome {
            verdict: CrashRecoveryVerdict::RefusedUnresolvedContradiction,
            can_resume: false,
            reason: format!(
                "cannot resume workunit '{}' with unresolved contradictions: {:?}",
                record.workunit_id, record.unresolved_contradictions
            ),
        };
    }
    if current_instruction_revision != record.instruction_revision {
        return CrashReconciliationOutcome {
            verdict: CrashRecoveryVerdict::RefusedInstructionChanged,
            can_resume: false,
            reason: format!(
                "instruction revision changed since checkpoint '{}' (recorded {}, current {})",
                record.checkpoint_id, record.instruction_revision, current_instruction_revision
            ),
        };
    }
    if current_repo_head_sha != record.repo_head_sha {
        return CrashReconciliationOutcome {
            verdict: CrashRecoveryVerdict::RefusedStaleRepoState,
            can_resume: false,
            reason: format!(
                "repository HEAD changed since checkpoint '{}' (recorded {}, current {}); stale assumptions refused",
                record.checkpoint_id, record.repo_head_sha, current_repo_head_sha
            ),
        };
    }
    if let Some(pending_op) = &record.pending_operation_id
        && pending_outcome_resolved != Some(true)
    {
        return CrashReconciliationOutcome {
            verdict: CrashRecoveryVerdict::RefusedUnknownOutcomePending,
            can_resume: false,
            reason: format!(
                "pending operation '{}' has unknown outcome; reconcile authoritatively before resuming",
                pending_op
            ),
        };
    }

    CrashReconciliationOutcome {
        verdict: CrashRecoveryVerdict::SafeToResume,
        can_resume: true,
        reason: format!(
            "workunit '{}' custody and repository state verified at checkpoint '{}'",
            record.workunit_id, record.checkpoint_id
        ),
    }
}

// ============================================================================
// #1711: Darkmatter Read-Only Consumer Snapshot Contract Verification
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkmatterEvidenceRef {
    pub evidence_id: String,
    pub kind: String,
    pub status: String,
    pub digest: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkmatterObligation {
    pub obligation_id: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkmatterReadonlySnapshot {
    pub schema_version: String,
    pub read_only: bool,
    pub mutation_endpoints_exposed: bool,
    pub ui_generated_proof_allowed: bool,
    pub repository: String,
    pub head_commit: String,
    pub human_requested_intent: String,
    pub current_agent_objective: String,
    pub task_id: String,
    pub task_scope: Vec<String>,
    pub dependencies: Vec<String>,
    pub relevant_files: Vec<String>,
    pub governance_json_path: String,
    pub completion_evidence: Vec<DarkmatterEvidenceRef>,
    pub outstanding_obligations: Vec<DarkmatterObligation>,
}

fn is_repo_bounded_relative_path(raw: &str) -> bool {
    if raw.trim().is_empty() || raw.starts_with('/') || raw.starts_with('\\') {
        return false;
    }
    !raw.split('/').any(|seg| seg == "..")
}

pub fn validate_darkmatter_snapshot(
    snapshot: &DarkmatterReadonlySnapshot,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if snapshot.schema_version != DARKMATTER_SNAPSHOT_SCHEMA_VERSION {
        errors.push(format!(
            "unsupported schema_version '{}', expected '{}'",
            snapshot.schema_version, DARKMATTER_SNAPSHOT_SCHEMA_VERSION
        ));
    }
    if !snapshot.read_only {
        errors.push("DARKMATTER_BOUNDARY_VIOLATION: read_only must be true".to_string());
    }
    if snapshot.mutation_endpoints_exposed {
        errors.push(
            "DARKMATTER_BOUNDARY_VIOLATION: mutation_endpoints_exposed must be false".to_string(),
        );
    }
    if snapshot.ui_generated_proof_allowed {
        errors.push(
            "DARKMATTER_BOUNDARY_VIOLATION: ui_generated_proof_allowed must be false".to_string(),
        );
    }
    if snapshot.human_requested_intent.trim().is_empty() {
        errors.push("human_requested_intent must be present and non-empty".to_string());
    }
    if snapshot.current_agent_objective.trim().is_empty() {
        errors.push("current_agent_objective must be present and non-empty".to_string());
    }
    if snapshot.task_id.trim().is_empty() {
        errors.push("task_id must be present and non-empty".to_string());
    }
    if snapshot.relevant_files.is_empty() {
        errors.push("relevant_files must not be empty".to_string());
    }
    if !is_repo_bounded_relative_path(&snapshot.governance_json_path) {
        errors.push(format!(
            "PATH_ESCAPE: governance_json_path '{}' escapes repository boundary",
            snapshot.governance_json_path
        ));
    }
    for path in &snapshot.relevant_files {
        if !is_repo_bounded_relative_path(path) {
            errors.push(format!(
                "PATH_ESCAPE: relevant_file '{}' escapes repository boundary",
                path
            ));
        }
    }
    if snapshot.completion_evidence.is_empty() {
        errors.push("completion_evidence must include at least one traceable record".to_string());
    }
    for ev in &snapshot.completion_evidence {
        if !is_repo_bounded_relative_path(&ev.path) {
            errors.push(format!(
                "PATH_ESCAPE: evidence path '{}' escapes repository boundary",
                ev.path
            ));
        }
        if !ev.digest.starts_with("sha256:") {
            errors.push(format!(
                "EVIDENCE_DIGEST_INVALID: evidence '{}' must carry a sha256 digest",
                ev.evidence_id
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ============================================================================
// #1630: Local Storage Compaction Benchmark & Metrics Report
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageByteSnapshot {
    pub main_db_bytes: u64,
    pub wal_sidecar_bytes: u64,
    pub shm_sidecar_bytes: u64,
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_count: u64,
    pub live_bytes: u64,
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageCompactionBenchmarkReport {
    pub schema_version: String,
    pub binary_version: String,
    pub storage_format_version: u32,
    pub backend_scope: String,
    pub hosted_backend_status: String,
    pub workload_tasks: usize,
    pub workload_operations: usize,
    pub before_compaction: StorageByteSnapshot,
    pub after_compaction: StorageByteSnapshot,
    pub after_explicit_reclaim: StorageByteSnapshot,
    pub bytes_per_operation_before: u64,
    pub bytes_per_operation_after_reclaim: u64,
    pub p50_op_micros: u64,
    pub p95_op_micros: u64,
    pub max_op_micros: u64,
    pub events_compacted: usize,
    pub unreferenced_components_collected: usize,
    pub sampling_method: String,
    pub unknown_fields: Vec<String>,
}

fn file_len_or_zero(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn measure_sqlite_bytes(
    conn: &db::Connection,
    db_path: &Path,
) -> Result<StorageByteSnapshot, DecapodError> {
    let page_size: u64 = conn.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))? as u64;
    let page_count: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0))? as u64;
    let freelist_count: u64 =
        conn.query_row("PRAGMA freelist_count", [], |r| r.get::<_, i64>(0))? as u64;
    let live_pages = page_count.saturating_sub(freelist_count);
    let wal_path = Path::new(&format!("{}-wal", db_path.display())).to_path_buf();
    let shm_path = Path::new(&format!("{}-shm", db_path.display())).to_path_buf();
    Ok(StorageByteSnapshot {
        main_db_bytes: file_len_or_zero(db_path),
        wal_sidecar_bytes: file_len_or_zero(&wal_path),
        shm_sidecar_bytes: file_len_or_zero(&shm_path),
        page_size,
        page_count,
        freelist_count,
        live_bytes: live_pages.saturating_mul(page_size),
        reclaimable_bytes: freelist_count.saturating_mul(page_size),
    })
}

/// Run a deterministic local SQLite task/lease/checkpoint/retry workload against
/// Decapod's real `todo` + `storage_compaction` subsystems and report measured
/// before/after bytes, reclaimable pages, and tail latency.
pub fn run_local_storage_compaction_benchmark(
    scratch_root: &Path,
    task_count: usize,
) -> Result<StorageCompactionBenchmarkReport, DecapodError> {
    let data_dir = scratch_root.join(".decapod/data");
    fs::create_dir_all(&data_dir).map_err(DecapodError::IoError)?;
    todo::initialize_todo_db(&data_dir)?;

    let mut latencies_us = Vec::new();
    let mut ops = 0usize;

    for idx in 0..task_count {
        let t0 = Instant::now();
        let cmd = TodoCommand::Add {
            title: format!("Benchmark task {idx}: verify lossless storage growth"),
            description: format!(
                "Repeated structured payload for workload item {idx}: {}",
                "governance-custody-checkpoint-proof-".repeat(8)
            ),
            tags: "benchmark,storage".to_string(),
            owner: "bench-agent".to_string(),
            due: None,
            r#ref: format!("ref:bench:{idx}"),
            scope: format!("paths:[\"src/module_{}.rs\"]", idx % 4),
            dir: Some(scratch_root.to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        };
        let added = todo::add_task(&data_dir, &cmd)?;
        latencies_us.push(t0.elapsed().as_micros() as u64);
        ops += 1;

        let task_id = added["id"].as_str().unwrap_or_default().to_string();

        let t1 = Instant::now();
        let _ = todo::claim_task(&data_dir, &task_id, "bench-agent", ClaimMode::Exclusive)?;
        latencies_us.push(t1.elapsed().as_micros() as u64);
        ops += 1;

        let t2 = Instant::now();
        let _ = todo::release_task_as_actor(&data_dir, &task_id, "bench-agent")?;
        latencies_us.push(t2.elapsed().as_micros() as u64);
        ops += 1;
    }

    let db_path = data_dir.join("decapod.db");
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    let before_compaction = measure_sqlite_bytes(&conn, &db_path)?;

    let event_report = storage_compaction::compact_historical_events(&conn)?;
    let gc_report = storage_compaction::gc_unreferenced_components(&conn, false)?;
    let after_compaction = measure_sqlite_bytes(&conn, &db_path)?;

    let _vac = storage_compaction::reclaim_pages_explicit(&conn)?;
    let after_explicit_reclaim = measure_sqlite_bytes(&conn, &db_path)?;

    latencies_us.sort_unstable();
    let p50 = latencies_us
        .get(latencies_us.len() / 2)
        .copied()
        .unwrap_or(0);
    let p95_idx =
        ((latencies_us.len().saturating_mul(95)) / 100).min(latencies_us.len().saturating_sub(1));
    let p95 = latencies_us.get(p95_idx).copied().unwrap_or(0);
    let max_us = latencies_us.last().copied().unwrap_or(0);

    let ops_u64 = ops.max(1) as u64;
    Ok(StorageCompactionBenchmarkReport {
        schema_version: STORAGE_BENCHMARK_SCHEMA_VERSION.to_string(),
        binary_version: env!("CARGO_PKG_VERSION").to_string(),
        storage_format_version: schemas::STORAGE_FORMAT_VERSION,
        backend_scope: "local_sqlite_benchmark".to_string(),
        hosted_backend_status: "unavailable_local_only".to_string(),
        workload_tasks: task_count,
        workload_operations: ops,
        bytes_per_operation_before: before_compaction.main_db_bytes / ops_u64,
        bytes_per_operation_after_reclaim: after_explicit_reclaim.main_db_bytes / ops_u64,
        before_compaction,
        after_compaction,
        after_explicit_reclaim,
        p50_op_micros: p50,
        p95_op_micros: p95,
        max_op_micros: max_us,
        events_compacted: event_report.compacted_rows as usize,
        unreferenced_components_collected: gc_report.deleted_components,
        sampling_method: "exact_per_operation_monotonic_instant".to_string(),
        unknown_fields: vec![
            "hosted_postgresql_live_bytes: unavailable_local_only".to_string(),
            "process_rss_delta_bytes: not_sampled_in_inprocess_harness".to_string(),
        ],
    })
}

// ============================================================================
// CLI Surface (`decapod houseboat ...`)
// ============================================================================

#[derive(clap::Args, Debug)]
pub struct HouseboatCli {
    #[clap(subcommand)]
    pub command: HouseboatCommand,
}

#[derive(clap::Subcommand, Debug)]
pub enum HouseboatCommand {
    /// Emit structured Houseboat engine capability status (JSON).
    Status,
    /// Validate an offline Darkmatter read-only snapshot JSON fixture (#1711).
    VerifyDarkmatterSnapshot {
        /// Path to the read-only snapshot JSON file.
        #[clap(long)]
        file: std::path::PathBuf,
    },
    /// Run a local SQLite storage compaction benchmark in a temporary directory (#1630).
    StorageBenchmark {
        /// Number of synthetic governed task/lease lifecycles to execute.
        #[clap(long, default_value_t = 12)]
        tasks: usize,
    },
    /// Verify a sample or JSON-encoded mooring collaboration contract (#855).
    VerifyMooring {
        /// Optional path to a JSON CollaborationContract file (uses built-in smoke check if omitted).
        #[clap(long)]
        file: Option<std::path::PathBuf>,
        /// Evaluation epoch timestamp.
        #[clap(long, default_value_t = 1_800_000_000)]
        now_epoch: u64,
    },
}

pub fn run_houseboat_command(cli: &HouseboatCli) -> Result<(), DecapodError> {
    match &cli.command {
        HouseboatCommand::Status => {
            let status = serde_json::json!({
                "schema_version": HOUSEBOAT_SCHEMA_VERSION,
                "engine_version": env!("CARGO_PKG_VERSION"),
                "subsystems": {
                    "fleet_coord_graph": "#852",
                    "workspace_lifecycle_and_leaks": "#853",
                    "seaworthiness_drift_policy": "#854",
                    "mooring_contracts": "#855",
                    "durable_journal_and_sync": "#856",
                    "motion_adjudicator": "#857",
                    "resilience_degraded_modes": "#858",
                    "resource_budget_ledger": "#859",
                    "knowledge_write_attestation": "#657",
                    "crash_unknown_outcome_reconciliation": "#1367",
                    "storage_compaction_benchmark": "#1630",
                    "darkmatter_readonly_contract": "#1711",
                }
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&status).map_err(|e| {
                    DecapodError::ValidationError(format!("houseboat status serialize: {e}"))
                })?
            );
            Ok(())
        }
        HouseboatCommand::VerifyDarkmatterSnapshot { file } => {
            let raw = fs::read_to_string(file).map_err(DecapodError::IoError)?;
            let snapshot: DarkmatterReadonlySnapshot = serde_json::from_str(&raw).map_err(|e| {
                DecapodError::ValidationError(format!("invalid darkmatter snapshot JSON: {e}"))
            })?;
            match validate_darkmatter_snapshot(&snapshot) {
                Ok(()) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "ok",
                            "schema_version": snapshot.schema_version,
                            "read_only": snapshot.read_only,
                            "task_id": snapshot.task_id,
                            "evidence_count": snapshot.completion_evidence.len(),
                        }))
                        .unwrap()
                    );
                    Ok(())
                }
                Err(errs) => Err(DecapodError::ValidationError(format!(
                    "DARKMATTER_SNAPSHOT_INVALID: {}",
                    errs.join("; ")
                ))),
            }
        }
        HouseboatCommand::StorageBenchmark { tasks } => {
            let tmp = tempfile::tempdir().map_err(DecapodError::IoError)?;
            let report = run_local_storage_compaction_benchmark(tmp.path(), *tasks)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| {
                    DecapodError::ValidationError(format!("benchmark serialize: {e}"))
                })?
            );
            Ok(())
        }
        HouseboatCommand::VerifyMooring { file, now_epoch } => {
            let contract = if let Some(path) = file {
                let raw = fs::read_to_string(path).map_err(DecapodError::IoError)?;
                serde_json::from_str::<CollaborationContract>(&raw).map_err(|e| {
                    DecapodError::ValidationError(format!("invalid mooring contract JSON: {e}"))
                })?
            } else {
                CollaborationContract::new(
                    "mooring:smoke:001",
                    MooringSubjectKind::ExternalArtifact,
                    "artifact:demo",
                    [MooringOperation::Read, MooringOperation::ImportArtifact],
                    [MooringOperation::Admin],
                    vec!["src/".to_string()],
                    ["verify:sha256".to_string()],
                    1_799_999_000,
                    1_800_010_000,
                    "maintainer:smoke",
                    Some(ArtifactProvenance {
                        artifact_id: "artifact:demo".to_string(),
                        source_uri: "https://example.invalid/demo.tar.gz".to_string(),
                        content_sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                        wrapper_sha256: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                        verification_obligations: ["sha256_match".to_string()].into_iter().collect(),
                        verified_obligations: ["sha256_match".to_string()].into_iter().collect(),
                        approved_by: Some("maintainer:smoke".to_string()),
                    }),
                )
            };
            let verdict = contract.enforce(&MooringEnforcementRequest {
                operation: MooringOperation::Read,
                target_path: Some("src/lib.rs".to_string()),
                required_capability: None,
                now_epoch: *now_epoch,
                observed_content_sha256: None,
                observed_wrapper_sha256: None,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&verdict).map_err(|e| {
                    DecapodError::ValidationError(format!("mooring verdict serialize: {e}"))
                })?
            );
            Ok(())
        }
    }
}
