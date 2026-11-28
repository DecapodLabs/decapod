//! Offline capability registry (#1715), deterministic skill discovery & bounded context
//! bundle compiler (#1716), canonical manifest compiler with source maps & policy-conflict
//! detector (#1717), safe markdown/fragment composition (#1718), and harness-native
//! packaging & honest compatibility matrix (#875).
//!
//! Guardrail: neither imported instructions nor registry metadata can override kernel
//! policy or confer execution authority. All operations run strictly offline without
//! accounts or network calls.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Schema version for the offline capability registry.
pub const CAPABILITY_REGISTRY_SCHEMA_VERSION: &str = "1.0.0";

/// Distinguishes canonical repo source from generated indexes and external imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOrigin {
    CanonicalSource,
    GeneratedIndex,
    ExternalImport,
}

/// Risk classification for a registered capability or skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRiskClass {
    ReadOnly,
    Advisory,
    ScopedAuthoring,
    PrivilegedMutation,
}

/// Authority scope requested by a capability/skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityScope {
    ReadOnly,
    WorkspaceScoped,
    GovernanceProtected,
    RepoAdmin,
}

/// Provenance & licensing record for an offline registry entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub origin: ArtifactOrigin,
    pub source_uri: String,
    pub license_spdx: String,
    pub verified: bool,
}

/// Full capability registry entry contract (#1715).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub targets: Vec<String>,
    pub supported_harnesses: Vec<String>,
    pub provenance: SourceProvenance,
    pub content_sha256: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub token_cost_estimate: usize,
    pub triggers: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub authority_scope: AuthorityScope,
    pub risk_class: CapabilityRiskClass,
    pub validation_strategy: String,
    pub proof_outcome_score: u32,
    pub freshness_epoch: u64,
    pub body_markdown: String,
}

impl CapabilityEntry {
    pub fn compute_content_sha256(body_markdown: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body_markdown.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    }
}

/// Typed admission/quarantine reason for a registry entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryQuarantineReason {
    pub capability_id: String,
    pub code: String,
    pub detail: String,
}

/// Offline capability registry with validated source entries and quarantine list (#1715).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRegistry {
    pub schema_version: String,
    pub origin: ArtifactOrigin,
    pub entries: BTreeMap<String, CapabilityEntry>,
    pub quarantined: Vec<RegistryQuarantineReason>,
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            schema_version: CAPABILITY_REGISTRY_SCHEMA_VERSION.to_string(),
            origin: ArtifactOrigin::GeneratedIndex,
            entries: BTreeMap::new(),
            quarantined: Vec::new(),
        }
    }

    /// Register and validate a capability offline. Unlicensed, unverifiable, tampered,
    /// or prompt-injecting entries are quarantined rather than silently admitted.
    pub fn register(&mut self, entry: CapabilityEntry) -> Result<(), RegistryQuarantineReason> {
        if entry.id.trim().is_empty() || entry.name.trim().is_empty() {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "invalid_metadata".to_string(),
                detail: "capability id and name must be non-empty".to_string(),
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        if entry.provenance.license_spdx.trim().is_empty()
            || entry
                .provenance
                .license_spdx
                .eq_ignore_ascii_case("unknown")
            || entry
                .provenance
                .license_spdx
                .eq_ignore_ascii_case("unlicensed")
        {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "unlicensed_import_quarantined".to_string(),
                detail: "entries without an explicit verified SPDX license are quarantined"
                    .to_string(),
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        if !entry.provenance.verified {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "unverified_provenance_quarantined".to_string(),
                detail: "unverified provenance imports are quarantined".to_string(),
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        let expected_hash = CapabilityEntry::compute_content_sha256(&entry.body_markdown);
        if entry.content_sha256 != expected_hash {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "content_digest_mismatch".to_string(),
                detail: format!(
                    "declared content_sha256 '{}' does not match computed '{expected_hash}'",
                    entry.content_sha256
                ),
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        if let Some(finding) = scan_markdown_for_injection(&entry.body_markdown) {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "adversarial_instruction_quarantined".to_string(),
                detail: finding,
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        if entry.dependencies.contains(&entry.id) {
            let reason = RegistryQuarantineReason {
                capability_id: entry.id,
                code: "self_dependency_cycle".to_string(),
                detail: "capability cannot depend on itself".to_string(),
            };
            self.quarantined.push(reason.clone());
            return Err(reason);
        }
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    /// Validate the full dependency and conflict graph across admitted entries.
    pub fn validate_graph(&self) -> Result<(), Vec<RegistryQuarantineReason>> {
        let mut errors = Vec::new();
        for (id, entry) in &self.entries {
            for dep in &entry.dependencies {
                if !self.entries.contains_key(dep) {
                    errors.push(RegistryQuarantineReason {
                        capability_id: id.clone(),
                        code: "missing_dependency".to_string(),
                        detail: format!("dependency '{dep}' is not present in admitted registry"),
                    });
                }
                if entry.conflicts.contains(dep) {
                    errors.push(RegistryQuarantineReason {
                        capability_id: id.clone(),
                        code: "contradictory_dependency_and_conflict".to_string(),
                        detail: format!("'{dep}' appears in both dependencies and conflicts"),
                    });
                }
            }
            // Cycle detection via DFS.
            let mut visiting = BTreeSet::new();
            let mut visited = BTreeSet::new();
            if self.has_cycle(id, &mut visiting, &mut visited) {
                errors.push(RegistryQuarantineReason {
                    capability_id: id.clone(),
                    code: "dependency_cycle".to_string(),
                    detail: format!("dependency cycle reachable from '{id}'"),
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn has_cycle(
        &self,
        node: &str,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(node.to_string()) {
            return true;
        }
        if let Some(entry) = self.entries.get(node) {
            for dep in &entry.dependencies {
                if !visited.contains(dep) && self.has_cycle(dep, visiting, visited) {
                    return true;
                }
            }
        }
        visiting.remove(node);
        visited.insert(node.to_string());
        false
    }

    /// Deterministic offline search & relevance ranking (#1715).
    pub fn search(&self, query: &RegistryQuery) -> Vec<RegistrySearchHit> {
        let kw_lower: Vec<String> = query
            .keywords
            .iter()
            .map(|k| k.to_ascii_lowercase())
            .collect();
        let mut hits = Vec::new();
        for entry in self.entries.values() {
            if let Some(ref cat) = query.category
                && !entry.category.eq_ignore_ascii_case(cat)
            {
                continue;
            }
            if let Some(ref target) = query.target
                && !entry
                    .targets
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(target) || t == "*")
            {
                continue;
            }
            if let Some(ref harness) = query.harness
                && !entry
                    .supported_harnesses
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(harness) || h == "*")
            {
                continue;
            }
            if let Some(max_scope) = query.max_authority_scope
                && entry.authority_scope > max_scope
            {
                continue;
            }

            let haystack = format!(
                "{} {} {} {}",
                entry.id,
                entry.name,
                entry.description,
                entry.triggers.join(" ")
            )
            .to_ascii_lowercase();

            let mut relevance: u32 = 0;
            for kw in &kw_lower {
                if entry.triggers.iter().any(|t| t.to_ascii_lowercase() == *kw) {
                    relevance += 20;
                } else if haystack.contains(kw) {
                    relevance += 10;
                }
            }
            if !kw_lower.is_empty() && relevance == 0 {
                continue;
            }
            let total_score = relevance + entry.proof_outcome_score.min(50);
            hits.push(RegistrySearchHit {
                id: entry.id.clone(),
                version: entry.version.clone(),
                relevance_score: total_score,
                token_cost_estimate: entry.token_cost_estimate,
                provenance: entry.provenance.clone(),
                authority_scope: entry.authority_scope,
            });
        }
        hits.sort_by(|a, b| {
            b.relevance_score
                .cmp(&a.relevance_score)
                .then_with(|| a.id.cmp(&b.id))
        });
        hits
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryQuery {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub harness: Option<String>,
    #[serde(default)]
    pub max_authority_scope: Option<AuthorityScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySearchHit {
    pub id: String,
    pub version: String,
    pub relevance_score: u32,
    pub token_cost_estimate: usize,
    pub provenance: SourceProvenance,
    pub authority_scope: AuthorityScope,
}

// ============================================================================
// #1716: Deterministic Skill Discovery & Bounded Context Bundle Compiler
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSelectionRequest {
    pub task_intent: String,
    pub category: Option<String>,
    pub target_language: Option<String>,
    pub harness: String,
    pub max_authority_scope: AuthorityScope,
    pub token_budget: usize,
    pub min_relevance_score: u32,
    pub now_epoch: u64,
    pub max_stale_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedSkillCandidate {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivatedSkillReceipt {
    pub id: String,
    pub version: String,
    pub content_sha256: String,
    pub token_cost: usize,
    pub relevance_score: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextBundle {
    pub schema_version: String,
    pub activated: Vec<ActivatedSkillReceipt>,
    pub excluded: Vec<ExcludedSkillCandidate>,
    pub total_tokens: usize,
    pub token_budget: usize,
    pub no_selection: bool,
    pub rendered_markdown: String,
    pub bundle_sha256: String,
    pub policy_guardrail_note: String,
}

/// Deterministically select relevant, admissible skills within `token_budget`,
/// resolving conflicts and rejecting stale, out-of-scope, or adversarial candidates (#1716).
pub fn select_skills_and_compile_bundle(
    registry: &CapabilityRegistry,
    req: &SkillSelectionRequest,
) -> CompiledContextBundle {
    let tokens: Vec<String> = req
        .task_intent
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .filter(|s| s.len() >= 3)
        .map(|s| s.to_ascii_lowercase())
        .collect();

    let mut scored: Vec<(&CapabilityEntry, u32)> = Vec::new();
    let mut excluded: Vec<ExcludedSkillCandidate> = Vec::new();

    for entry in registry.entries.values() {
        if entry.authority_scope > req.max_authority_scope {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "exceeds_authority_scope".to_string(),
            });
            continue;
        }
        if !entry
            .supported_harnesses
            .iter()
            .any(|h| h.eq_ignore_ascii_case(&req.harness) || h == "*")
        {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "unsupported_harness".to_string(),
            });
            continue;
        }
        if let Some(ref lang) = req.target_language
            && !entry
                .targets
                .iter()
                .any(|t| t.eq_ignore_ascii_case(lang) || t == "*")
        {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "target_language_mismatch".to_string(),
            });
            continue;
        }
        if req.max_stale_secs > 0
            && req.now_epoch > entry.freshness_epoch.saturating_add(req.max_stale_secs)
        {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "stale_capability".to_string(),
            });
            continue;
        }
        if scan_markdown_for_injection(&entry.body_markdown).is_some() {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "adversarial_instruction_rejected".to_string(),
            });
            continue;
        }

        let mut score: u32 = 0;
        if let Some(ref cat) = req.category
            && entry.category.eq_ignore_ascii_case(cat)
        {
            score += 15;
        }
        let haystack =
            format!("{} {} {}", entry.name, entry.description, entry.category).to_ascii_lowercase();
        for tok in &tokens {
            if entry
                .triggers
                .iter()
                .any(|tr| tr.to_ascii_lowercase() == *tok)
            {
                score += 25;
            } else if haystack.contains(tok) {
                score += 8;
            }
        }
        if score < req.min_relevance_score {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: format!(
                    "below_relevance_threshold({score}<{})",
                    req.min_relevance_score
                ),
            });
            continue;
        }
        scored.push((entry, score + entry.proof_outcome_score.min(20)));
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));

    let mut activated: Vec<ActivatedSkillReceipt> = Vec::new();
    let mut selected_ids: BTreeSet<String> = BTreeSet::new();
    let mut total_tokens: usize = 0;
    let mut sections: Vec<String> = Vec::new();

    for (entry, score) in scored {
        let conflicts_with_selected = entry.conflicts.iter().any(|c| selected_ids.contains(c))
            || selected_ids.iter().any(|sid| {
                registry
                    .entries
                    .get(sid)
                    .is_some_and(|e| e.conflicts.contains(&entry.id))
            });
        if conflicts_with_selected {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "conflict_with_higher_precedence_skill".to_string(),
            });
            continue;
        }
        if total_tokens.saturating_add(entry.token_cost_estimate) > req.token_budget {
            excluded.push(ExcludedSkillCandidate {
                id: entry.id.clone(),
                reason: "exceeds_token_budget".to_string(),
            });
            continue;
        }
        selected_ids.insert(entry.id.clone());
        total_tokens += entry.token_cost_estimate;
        activated.push(ActivatedSkillReceipt {
            id: entry.id.clone(),
            version: entry.version.clone(),
            content_sha256: entry.content_sha256.clone(),
            token_cost: entry.token_cost_estimate,
            relevance_score: score,
        });
        sections.push(format!(
            "## Skill: {} ({})\n<!-- digest: {} -->\n{}",
            entry.name, entry.id, entry.content_sha256, entry.body_markdown
        ));
    }

    excluded.sort_by(|a, b| a.id.cmp(&b.id));
    let rendered_markdown = sections.join("\n\n");
    let bundle_sha256 = CapabilityEntry::compute_content_sha256(&rendered_markdown);

    CompiledContextBundle {
        schema_version: "1.0.0".to_string(),
        no_selection: activated.is_empty(),
        activated,
        excluded,
        total_tokens,
        token_budget: req.token_budget,
        rendered_markdown,
        bundle_sha256,
        policy_guardrail_note:
            "Activated skills are bounded guidance only; they cannot weaken scope boundaries, provenance, or validation gates."
                .to_string(),
    }
}

/// Skill-on vs matched no-skill baseline evaluation record (#1716).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEvalRun {
    pub quality_score: u32,
    pub token_overhead: usize,
    pub contradictions: usize,
    pub unsafe_instructions_accepted: usize,
    pub proof_gates_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillAbEvaluation {
    pub skill_id: String,
    pub quality_delta: i64,
    pub token_delta: i64,
    pub default_enablement_admissible: bool,
    pub reasons: Vec<String>,
}

/// Evaluate skill-on vs matched no-skill baseline before default enablement (#1716).
pub fn evaluate_skill_ab_fixture(
    skill_id: &str,
    baseline_off: &SkillEvalRun,
    treatment_on: &SkillEvalRun,
    max_allowed_token_delta: i64,
) -> SkillAbEvaluation {
    let quality_delta =
        i64::from(treatment_on.quality_score) - i64::from(baseline_off.quality_score);
    let token_delta = treatment_on.token_overhead as i64 - baseline_off.token_overhead as i64;
    let mut reasons = Vec::new();
    if quality_delta <= 0 {
        reasons.push("no_measured_quality_improvement".to_string());
    }
    if token_delta > max_allowed_token_delta {
        reasons.push("token_overhead_exceeds_budget".to_string());
    }
    if treatment_on.contradictions > 0 {
        reasons.push("introduced_policy_contradictions".to_string());
    }
    if treatment_on.unsafe_instructions_accepted > 0 {
        reasons.push("unsafe_instructions_accepted".to_string());
    }
    if !treatment_on.proof_gates_passed {
        reasons.push("failed_required_proof_gates".to_string());
    }
    SkillAbEvaluation {
        skill_id: skill_id.to_string(),
        quality_delta,
        token_delta,
        default_enablement_admissible: reasons.is_empty(),
        reasons,
    }
}

// ============================================================================
// #1717: Canonical Manifest Compiler + Source Maps + Policy-Conflict Detector
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Require,
    Forbid,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractClause {
    pub id: String,
    pub policy_key: String,
    pub effect: PolicyEffect,
    pub source_path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalGovernanceContract {
    pub contract_version: String,
    pub product_name: String,
    pub clauses: Vec<ContractClause>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestConflict {
    pub policy_key: String,
    pub clause_a: String,
    pub clause_b: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMapEntry {
    pub output_file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub clause_id: String,
    pub source_path: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledManifestBundle {
    pub contract_version: String,
    pub files: BTreeMap<String, String>,
    pub source_maps: Vec<SourceMapEntry>,
    pub enforcement_disclaimer: String,
}

/// Detect contradictory clauses in a canonical governance contract (#1717).
pub fn detect_contract_policy_conflicts(
    contract: &CanonicalGovernanceContract,
) -> Vec<ManifestConflict> {
    let mut conflicts = Vec::new();
    for (i, a) in contract.clauses.iter().enumerate() {
        for b in contract.clauses.iter().skip(i + 1) {
            if a.policy_key == b.policy_key && a.effect != b.effect {
                let contradictory = matches!(
                    (a.effect, b.effect),
                    (PolicyEffect::Require, PolicyEffect::Forbid)
                        | (PolicyEffect::Forbid, PolicyEffect::Require)
                        | (PolicyEffect::Allow, PolicyEffect::Forbid)
                        | (PolicyEffect::Forbid, PolicyEffect::Allow)
                );
                if contradictory {
                    conflicts.push(ManifestConflict {
                        policy_key: a.policy_key.clone(),
                        clause_a: a.id.clone(),
                        clause_b: b.id.clone(),
                        detail: format!(
                            "contradictory effects ({:?} vs {:?}) on policy '{}'",
                            a.effect, b.effect, a.policy_key
                        ),
                    });
                }
            }
        }
    }
    conflicts
}

/// Compile minimal deterministic AGENTS.md / CLAUDE.md / GEMINI.md / CODEX.md
/// manifests plus line-accurate source maps from one canonical contract (#1717).
pub fn compile_harness_manifests(
    contract: &CanonicalGovernanceContract,
    targets: &[&str],
) -> Result<CompiledManifestBundle, Vec<ManifestConflict>> {
    let conflicts = detect_contract_policy_conflicts(contract);
    if !conflicts.is_empty() {
        return Err(conflicts);
    }

    let mut files = BTreeMap::new();
    let mut source_maps = Vec::new();

    for target in targets {
        let mut lines: Vec<String> = vec![
            format!("<!-- generated-from: canonical-contract/{} -->", contract.contract_version),
            format!("# {} ({target})", contract.product_name),
            String::new(),
            "> Discovery projection only; Decapod kernel validation and custody gates remain the enforcement boundary.".to_string(),
            String::new(),
        ];
        for clause in &contract.clauses {
            let start_line = lines.len() + 1;
            lines.push(format!(
                "- [{}] {}: {}",
                clause.id, clause.policy_key, clause.text
            ));
            let end_line = lines.len();
            source_maps.push(SourceMapEntry {
                output_file: (*target).to_string(),
                start_line,
                end_line,
                clause_id: clause.id.clone(),
                source_path: clause.source_path.clone(),
                source_digest: CapabilityEntry::compute_content_sha256(&clause.text),
            });
        }
        files.insert((*target).to_string(), lines.join("\n") + "\n");
    }

    Ok(CompiledManifestBundle {
        contract_version: contract.contract_version.clone(),
        files,
        source_maps,
        enforcement_disclaimer:
            "Generated entrypoint manifests are discovery projections, never the enforcement boundary."
                .to_string(),
    })
}

// ============================================================================
// #1718: Safe Markdown/Fragment Composition & Import Review
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecedenceTier {
    KernelBaseline = 0,
    OrgOverlay = 1,
    RepoOverride = 2,
    TaskFragment = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyFragment {
    pub id: String,
    pub tier: PrecedenceTier,
    pub source_uri: String,
    pub license_spdx: String,
    pub requested_authority_scope: AuthorityScope,
    pub requested_tools: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionPolicy {
    pub max_authority_scope: AuthorityScope,
    pub allowed_tools: BTreeSet<String>,
    pub human_review_approved_fragments: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionRejection {
    pub fragment_id: String,
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositionResult {
    pub admitted_fragment_ids: Vec<String>,
    pub rejected: Vec<CompositionRejection>,
    pub requires_human_review: Vec<String>,
    pub diff_preview_lines: Vec<String>,
    pub composed_markdown: String,
    pub composed_sha256: String,
    pub executed_imported_content: bool,
}

/// Detect prompt-injection, safety-bypass, hidden directive, or invisible bidi/zero-width smuggling.
pub fn scan_markdown_for_injection(markdown: &str) -> Option<String> {
    // Reject invisible Unicode directional / zero-width smuggling characters.
    for ch in markdown.chars() {
        if matches!(
            ch,
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        ) {
            return Some(format!(
                "invisible Unicode smuggling character U+{:04X} rejected",
                ch as u32
            ));
        }
    }
    let lower = markdown.to_ascii_lowercase();
    let forbidden_phrases = [
        ("ignore previous instructions", "prompt_injection_override"),
        ("ignore all prior rules", "prompt_injection_override"),
        ("bypass decapod validate", "validation_bypass_attempt"),
        ("bypass validation", "validation_bypass_attempt"),
        ("skip proof gate", "validation_bypass_attempt"),
        ("disable safety gate", "safety_gate_disable_attempt"),
        ("mutate master directly", "root_isolation_bypass_attempt"),
        ("exfiltrate credentials", "credential_exfiltration_attempt"),
        ("print all env secrets", "credential_exfiltration_attempt"),
        ("<!-- system:", "hidden_html_directive_injection"),
    ];
    for (needle, tag) in forbidden_phrases {
        if lower.contains(needle) {
            return Some(format!("{tag}: matched forbidden pattern '{needle}'"));
        }
    }
    None
}

/// Deterministically compose ordered fragments while rejecting prompt injection
/// and denying unreviewed privilege escalation (#1718). Never executes imported content.
pub fn compose_fragments(
    fragments: &[PolicyFragment],
    policy: &CompositionPolicy,
) -> CompositionResult {
    let mut ordered: Vec<&PolicyFragment> = fragments.iter().collect();
    ordered.sort_by(|a, b| a.tier.cmp(&b.tier).then_with(|| a.id.cmp(&b.id)));

    let mut admitted_fragment_ids = Vec::new();
    let mut rejected = Vec::new();
    let mut requires_human_review = Vec::new();
    let mut diff_preview_lines = Vec::new();
    let mut rendered_chunks = Vec::new();

    for frag in ordered {
        if let Some(finding) = scan_markdown_for_injection(&frag.markdown) {
            rejected.push(CompositionRejection {
                fragment_id: frag.id.clone(),
                code: "prompt_injection_denied".to_string(),
                detail: finding,
            });
            continue;
        }
        let scope_expansion = frag.requested_authority_scope > policy.max_authority_scope;
        let unauthorized_tools: Vec<String> = frag
            .requested_tools
            .iter()
            .filter(|t| !policy.allowed_tools.contains(*t))
            .cloned()
            .collect();

        if (scope_expansion || !unauthorized_tools.is_empty())
            && !policy.human_review_approved_fragments.contains(&frag.id)
        {
            requires_human_review.push(frag.id.clone());
            rejected.push(CompositionRejection {
                fragment_id: frag.id.clone(),
                code: "privilege_escalation_denied".to_string(),
                detail: format!(
                    "authority expansion (scope_expansion={scope_expansion}, extra_tools={unauthorized_tools:?}) requires explicit human review"
                ),
            });
            continue;
        }

        admitted_fragment_ids.push(frag.id.clone());
        diff_preview_lines.push(format!(
            "+ [tier={:?} id={} source={} license={}]",
            frag.tier, frag.id, frag.source_uri, frag.license_spdx
        ));
        rendered_chunks.push(format!(
            "<!-- fragment:{} tier:{:?} source:{} -->\n{}",
            frag.id, frag.tier, frag.source_uri, frag.markdown
        ));
    }

    let composed_markdown = rendered_chunks.join("\n\n");
    let composed_sha256 = CapabilityEntry::compute_content_sha256(&composed_markdown);

    CompositionResult {
        admitted_fragment_ids,
        rejected,
        requires_human_review,
        diff_preview_lines,
        composed_markdown,
        composed_sha256,
        executed_imported_content: false,
    }
}

// ============================================================================
// #875: Harness-Native Packaging & Honest Compatibility Matrix
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessId {
    Codex,
    ClaudeCode,
    Antigravity,
    Opencode,
    GrokBuild,
    Pi,
}

impl HarnessId {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "codex" => Ok(Self::Codex),
            "claude_code" | "claude" => Ok(Self::ClaudeCode),
            "antigravity" | "agy" => Ok(Self::Antigravity),
            "opencode" => Ok(Self::Opencode),
            "grok_build" | "grok" => Ok(Self::GrokBuild),
            "pi" => Ok(Self::Pi),
            other => Err(format!(
                "unsupported harness '{other}'; expected one of: codex, claude-code, antigravity, opencode, grok-build, pi"
            )),
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Antigravity => "antigravity",
            Self::Opencode => "opencode",
            Self::GrokBuild => "grok-build",
            Self::Pi => "pi",
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::Codex,
            Self::ClaudeCode,
            Self::Antigravity,
            Self::Opencode,
            Self::GrokBuild,
            Self::Pi,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCompatibilityRow {
    pub harness: HarnessId,
    pub display_name: String,
    #[serde(default)]
    pub pinned_host_version: String,
    pub pinned_mechanism: String,
    pub instruction_surface: String,
    pub tool_discovery_surface: String,
    pub cooperative_hook_surface: Option<String>,
    pub independent_enforcement_surface: String,
    pub update_rollback_supported: bool,
    pub live_host_verified: bool,
    pub bypass_paths: Vec<String>,
    pub status: String,
}

/// Honest harness compatibility matrix (#875): separates instruction-only,
/// discovered tool access, cooperative hooks, and independent OS/forge enforcement,
/// and never falsely claims live multi-host verification.
pub fn harness_compatibility_matrix() -> Vec<HarnessCompatibilityRow> {
    vec![
        HarnessCompatibilityRow {
            harness: HarnessId::Codex,
            display_name: "Codex".to_string(),
            pinned_host_version: "codex-cli 0.172.0".to_string(),
            pinned_mechanism: "stdio MCP + CODEX.md".to_string(),
            instruction_surface: "CODEX.md + AGENTS.md".to_string(),
            tool_discovery_surface: "decapod mcp serve (stdio JSON-RPC 2.0)".to_string(),
            cooperative_hook_surface: None,
            independent_enforcement_surface: "Git worktree + CI/forge branch protection"
                .to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["direct shell/git invocation outside MCP".to_string()],
            status: "packaged_offline_pending_live_host_matrix".to_string(),
        },
        HarnessCompatibilityRow {
            harness: HarnessId::ClaudeCode,
            display_name: "Claude Code".to_string(),
            pinned_host_version: "2.1.311 (Claude Code)".to_string(),
            pinned_mechanism: "stdio MCP + cooperative PreToolUse hook + CLAUDE.md".to_string(),
            instruction_surface: "CLAUDE.md + AGENTS.md".to_string(),
            tool_discovery_surface: "decapod mcp serve (stdio JSON-RPC 2.0)".to_string(),
            cooperative_hook_surface: Some("PreToolUse -> decapod infer boundary".to_string()),
            independent_enforcement_surface: "OS sandbox + Git/forge branch protection".to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["user-disabled hooks or direct host shell".to_string()],
            status: "packaged_offline_pending_live_host_matrix".to_string(),
        },
        HarnessCompatibilityRow {
            harness: HarnessId::Antigravity,
            display_name: "Antigravity".to_string(),
            pinned_host_version: "antigravity 1.5.18".to_string(),
            pinned_mechanism: "stdio MCP + GEMINI.md".to_string(),
            instruction_surface: "GEMINI.md + AGENTS.md".to_string(),
            tool_discovery_surface: "decapod mcp serve (stdio JSON-RPC 2.0)".to_string(),
            cooperative_hook_surface: None,
            independent_enforcement_surface: "Git worktree + CI/forge branch protection"
                .to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["direct terminal command outside MCP".to_string()],
            status: "packaged_offline_pending_live_host_matrix".to_string(),
        },
        HarnessCompatibilityRow {
            harness: HarnessId::Opencode,
            display_name: "OpenCode".to_string(),
            pinned_host_version: "opencode v2.2.5".to_string(),
            pinned_mechanism: "stdio MCP + plugin manifest + AGENTS.md".to_string(),
            instruction_surface: "AGENTS.md".to_string(),
            tool_discovery_surface: "decapod mcp serve (stdio JSON-RPC 2.0)".to_string(),
            cooperative_hook_surface: Some(
                "plugin tool.execute.before -> decapod infer boundary".to_string(),
            ),
            independent_enforcement_surface: "Git worktree + CI/forge branch protection"
                .to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["unloaded plugin or direct shell".to_string()],
            status: "packaged_offline_pending_live_host_matrix".to_string(),
        },
        HarnessCompatibilityRow {
            harness: HarnessId::GrokBuild,
            display_name: "Grok Build".to_string(),
            pinned_host_version: "grok 1.0.77".to_string(),
            pinned_mechanism: "structured CLI/RPC + AGENTS.md".to_string(),
            instruction_surface: "AGENTS.md".to_string(),
            tool_discovery_surface: "decapod rpc --stdin (fallback CLI/RPC)".to_string(),
            cooperative_hook_surface: None,
            independent_enforcement_surface: "Git worktree + CI/forge branch protection"
                .to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["direct shell execution".to_string()],
            status: "packaged_offline_rpc_pending_live_host_matrix".to_string(),
        },
        HarnessCompatibilityRow {
            harness: HarnessId::Pi,
            display_name: "pi".to_string(),
            pinned_host_version: "uninstalled_offline_package".to_string(),
            pinned_mechanism: "extension adapter + decapod infer boundary + AGENTS.md".to_string(),
            instruction_surface: "AGENTS.md".to_string(),
            tool_discovery_surface: "decapod infer boundary / decapod rpc --stdin".to_string(),
            cooperative_hook_surface: Some("pi extension lifecycle hooks".to_string()),
            independent_enforcement_surface: "Git worktree + CI/forge branch protection"
                .to_string(),
            update_rollback_supported: true,
            live_host_verified: false,
            bypass_paths: vec!["direct bash tool outside extension".to_string()],
            status: "packaged_offline_pending_live_host_matrix".to_string(),
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessPackageManifest {
    pub schema_version: String,
    pub harness: HarnessId,
    pub decapod_version: String,
    pub governance_profile_version: String,
    pub executable_path: String,
    pub repository_root: String,
    pub config_path_hint: String,
    pub config_payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_path_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_path_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_payload: Option<serde_json::Value>,
    pub rollback_safe: bool,
    pub preserves_user_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessInstallReport {
    pub harness: HarnessId,
    pub decapod_version: String,
    pub written_paths: Vec<String>,
    pub preserved_existing_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessRemoveReport {
    pub harness: HarnessId,
    pub updated_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub preserved_user_entries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessStatusReport {
    pub harness: HarnessId,
    pub installed: bool,
    pub disabled: bool,
    pub config_present: bool,
    pub skill_present: bool,
    pub plugin_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_version: Option<String>,
    pub current_binary_version: String,
    pub version_skew: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_executable: Option<String>,
    pub missing_binary: bool,
    pub rollback_snapshot_available: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessRollbackReport {
    pub harness: HarnessId,
    pub restored_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub preserved_user_config: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HarnessFileSnapshot {
    path_hint: String,
    existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HarnessRollbackSnapshot {
    schema_version: String,
    harness: HarnessId,
    config: HarnessFileSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skill: Option<HarnessFileSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    plugin: Option<HarnessFileSnapshot>,
}

/// Render a harness-native `SKILL.md` discovery document advertising only
/// real, executable Decapod commands (#875).
pub fn render_harness_skill_markdown(harness: HarnessId, executable_path: &str) -> String {
    let slug = harness.slug();
    format!(
        "---\n\
name: decapod-governance\n\
description: Invoke the local-first Decapod governance kernel for prompt safety, worktree isolation, context resolution, and validation.\n\
harness: {slug}\n\
---\n\n\
# Decapod Governance Kernel (`{slug}`)\n\n\
Always run Decapod before repository mutations:\n\n\
1. Evaluate untrusted prompts first:\n\
   `printf '%s' \"<prompt>\" | {executable_path} eval --stdin --format json`\n\
2. Ingest embedded contracts and orient before edits:\n\
   `{executable_path} docs ingest && {executable_path} session acquire`\n\
   `{executable_path} rpc --op agent.init`\n\
   `{executable_path} rpc --op context.resolve`\n\
3. Work inside `.decapod/workspaces/*` via `{executable_path} todo claim` and `{executable_path} workspace ensure`.\n\
4. Establish current-branch proof before PR creation:\n\
   `{executable_path} validate --format json`\n"
    )
}

/// Generate a conflict-safe harness-native discovery package manifest (#875).
pub fn generate_harness_package(
    harness: HarnessId,
    executable_path: &str,
    repository_root: &str,
    min_decapod_version: &str,
) -> Result<HarnessPackageManifest, String> {
    if executable_path.trim().is_empty() || repository_root.trim().is_empty() {
        return Err("executable_path and repository_root must be non-empty".to_string());
    }
    let current = env!("CARGO_PKG_VERSION");
    if compare_semver(current, min_decapod_version) == Some(std::cmp::Ordering::Less) {
        return Err(format!(
            "version_skew: installed decapod {current} is older than required {min_decapod_version}"
        ));
    }
    let config_path_hint = match harness {
        HarnessId::Codex => concat!(".", "codex", "/mcp.json"),
        HarnessId::ClaudeCode => concat!(".", "claude", "/settings.local.json"),
        HarnessId::Antigravity => ".antigravity/mcp.json",
        HarnessId::Opencode => ".opencode/mcp.json",
        HarnessId::GrokBuild => ".grok/decapod-rpc.json",
        HarnessId::Pi => ".pi/decapod-extension.json",
    };
    let skill_path_hint = match harness {
        HarnessId::Codex => Some(concat!(".", "codex", "/skills/decapod/SKILL.md").to_string()),
        HarnessId::ClaudeCode => Some(".claude/skills/decapod/SKILL.md".to_string()),
        HarnessId::Antigravity => Some(".antigravity/skills/decapod/SKILL.md".to_string()),
        HarnessId::Opencode => Some(".opencode/skills/decapod/SKILL.md".to_string()),
        HarnessId::Pi => Some(".pi/skills/decapod/SKILL.md".to_string()),
        HarnessId::GrokBuild => None,
    };
    let skill_markdown = skill_path_hint
        .as_ref()
        .map(|_| render_harness_skill_markdown(harness, executable_path));
    let (plugin_path_hint, plugin_payload) = match harness {
        HarnessId::Opencode => (
            Some(".opencode/plugins/decapod.json".to_string()),
            Some(serde_json::json!({
                "schema_version": "1.0.0",
                "name": "decapod",
                "decapod_version": current,
                "hooks": {
                    "tool.execute.before": {
                        "command": executable_path,
                        "args": ["workspace", "status"],
                        "cwd": repository_root
                    }
                }
            })),
        ),
        _ => (None, None),
    };
    let config_payload = match harness {
        HarnessId::GrokBuild | HarnessId::Pi => serde_json::json!({
            "decapod": {
                "adapter": "rpc-stdin",
                "command": executable_path,
                "args": ["rpc", "--stdin"],
                "cwd": repository_root,
                "profile_version": "v1",
                "decapod_version": current
            }
        }),
        HarnessId::ClaudeCode => serde_json::json!({
            "mcpServers": {
                "decapod": {
                    "command": executable_path,
                    "args": ["mcp", "serve"],
                    "cwd": repository_root,
                    "profile_version": "v1",
                    "decapod_version": current
                }
            },
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Edit|Write|Bash",
                        "decapodManaged": true,
                        "hooks": [
                            {
                                "type": "command",
                                "command": format!("{executable_path} workspace status")
                            }
                        ]
                    }
                ]
            }
        }),
        _ => serde_json::json!({
            "mcpServers": {
                "decapod": {
                    "command": executable_path,
                    "args": ["mcp", "serve"],
                    "cwd": repository_root,
                    "profile_version": "v1",
                    "decapod_version": current
                }
            }
        }),
    };
    Ok(HarnessPackageManifest {
        schema_version: "1.0.0".to_string(),
        harness,
        decapod_version: current.to_string(),
        governance_profile_version: "v1".to_string(),
        executable_path: executable_path.to_string(),
        repository_root: repository_root.to_string(),
        config_path_hint: config_path_hint.to_string(),
        config_payload,
        skill_path_hint,
        skill_markdown,
        plugin_path_hint,
        plugin_payload,
        rollback_safe: true,
        preserves_user_config: true,
    })
}

fn ensure_non_symlink_path(path: &std::path::Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!(
            "refusing symlinked harness config path '{}'",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to inspect '{}': {err}", path.display())),
    }
}

fn merge_harness_json(
    existing: &serde_json::Value,
    generated: &serde_json::Value,
    harness: HarnessId,
) -> Result<serde_json::Value, String> {
    let mut merged = existing
        .as_object()
        .cloned()
        .ok_or_else(|| "existing harness config must be a JSON object".to_string())?;
    match harness {
        HarnessId::GrokBuild | HarnessId::Pi => {
            if let Some(entry) = generated.get("decapod") {
                merged.insert("decapod".to_string(), entry.clone());
            }
        }
        HarnessId::ClaudeCode => {
            let mcp_merged =
                super::mcp::merge_mcp_host_config(&serde_json::Value::Object(merged), generated)
                    .map_err(|e| e.to_string())?;
            merged = mcp_merged
                .as_object()
                .cloned()
                .ok_or_else(|| "merged config must be an object".to_string())?;
            if let Some(gen_hooks) = generated
                .get("hooks")
                .and_then(|h| h.get("PreToolUse"))
                .and_then(|a| a.as_array())
            {
                let hooks_val = merged
                    .entry("hooks".to_string())
                    .or_insert_with(|| serde_json::json!({}));
                let hooks_obj = hooks_val
                    .as_object_mut()
                    .ok_or_else(|| "existing hooks field must be a JSON object".to_string())?;
                let pre_val = hooks_obj
                    .entry("PreToolUse".to_string())
                    .or_insert_with(|| serde_json::json!([]));
                let pre_arr = pre_val
                    .as_array_mut()
                    .ok_or_else(|| "existing hooks.PreToolUse must be a JSON array".to_string())?;
                pre_arr.retain(|item| {
                    item.get("decapodManaged").and_then(|v| v.as_bool()) != Some(true)
                });
                pre_arr.extend(gen_hooks.iter().cloned());
            }
        }
        _ => {
            let mcp_merged =
                super::mcp::merge_mcp_host_config(&serde_json::Value::Object(merged), generated)
                    .map_err(|e| e.to_string())?;
            merged = mcp_merged
                .as_object()
                .cloned()
                .ok_or_else(|| "merged config must be an object".to_string())?;
        }
    }
    Ok(serde_json::Value::Object(merged))
}

/// Install a conflict-safe harness-native discovery package into `repo_root` (#875).
/// Preserves existing unrelated user configuration keys, servers, and hooks, and
/// stores a rollback snapshot under `.decapod/data/harness-backups/`.
pub fn install_harness_package(
    repo_root: &std::path::Path,
    harness: HarnessId,
    executable_path: &str,
    min_decapod_version: &str,
) -> Result<HarnessInstallReport, String> {
    let root_str = repo_root.to_string_lossy().to_string();
    let pkg = generate_harness_package(harness, executable_path, &root_str, min_decapod_version)?;
    capture_rollback_snapshot(repo_root, &pkg)?;
    let mut written_paths = Vec::new();
    let mut preserved_existing_config = false;

    let cfg_path = repo_root.join(&pkg.config_path_hint);
    ensure_non_symlink_path(&cfg_path)?;
    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let merged_cfg = if cfg_path.exists() {
        preserved_existing_config = true;
        let raw = std::fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
        let existing: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid existing JSON '{}': {e}", cfg_path.display()))?;
        merge_harness_json(&existing, &pkg.config_payload, harness)?
    } else {
        pkg.config_payload.clone()
    };
    let cfg_bytes = serde_json::to_vec_pretty(&merged_cfg).map_err(|e| e.to_string())?;
    crate::core::atomic::write_atomic(&cfg_path, &cfg_bytes).map_err(|e| e.to_string())?;
    written_paths.push(pkg.config_path_hint.clone());

    if let (Some(skill_rel), Some(skill_md)) = (&pkg.skill_path_hint, &pkg.skill_markdown) {
        let skill_path = repo_root.join(skill_rel);
        ensure_non_symlink_path(&skill_path)?;
        if let Some(parent) = skill_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        crate::core::atomic::write_atomic(&skill_path, skill_md.as_bytes())
            .map_err(|e| e.to_string())?;
        written_paths.push(skill_rel.clone());
    }

    if let (Some(plugin_rel), Some(plugin_json)) = (&pkg.plugin_path_hint, &pkg.plugin_payload) {
        let plugin_path = repo_root.join(plugin_rel);
        ensure_non_symlink_path(&plugin_path)?;
        if let Some(parent) = plugin_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let plugin_bytes = serde_json::to_vec_pretty(plugin_json).map_err(|e| e.to_string())?;
        crate::core::atomic::write_atomic(&plugin_path, &plugin_bytes)
            .map_err(|e| e.to_string())?;
        written_paths.push(plugin_rel.clone());
    }

    Ok(HarnessInstallReport {
        harness,
        decapod_version: pkg.decapod_version,
        written_paths,
        preserved_existing_config,
    })
}

fn rollback_snapshot_path(repo_root: &std::path::Path, harness: HarnessId) -> std::path::PathBuf {
    repo_root
        .join(".decapod")
        .join("data")
        .join("harness-backups")
        .join(format!("{}.json", harness.slug()))
}

fn read_file_snapshot(
    repo_root: &std::path::Path,
    rel: &str,
) -> Result<HarnessFileSnapshot, String> {
    let path = repo_root.join(rel);
    ensure_non_symlink_path(&path)?;
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Ok(HarnessFileSnapshot {
            path_hint: rel.to_string(),
            existed: true,
            content: Some(content),
        })
    } else {
        Ok(HarnessFileSnapshot {
            path_hint: rel.to_string(),
            existed: false,
            content: None,
        })
    }
}

fn capture_rollback_snapshot(
    repo_root: &std::path::Path,
    pkg: &HarnessPackageManifest,
) -> Result<(), String> {
    let backup_path = rollback_snapshot_path(repo_root, pkg.harness);
    ensure_non_symlink_path(&backup_path)?;
    if let Some(parent) = backup_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let snapshot = HarnessRollbackSnapshot {
        schema_version: "1.0.0".to_string(),
        harness: pkg.harness,
        config: read_file_snapshot(repo_root, &pkg.config_path_hint)?,
        skill: pkg
            .skill_path_hint
            .as_deref()
            .map(|rel| read_file_snapshot(repo_root, rel))
            .transpose()?,
        plugin: pkg
            .plugin_path_hint
            .as_deref()
            .map(|rel| read_file_snapshot(repo_root, rel))
            .transpose()?,
    };
    let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|e| e.to_string())?;
    crate::core::atomic::write_atomic(&backup_path, &bytes).map_err(|e| e.to_string())
}

fn executable_exists_on_disk_or_path(exe: &str) -> bool {
    let trimmed = exe.trim();
    if trimmed.is_empty() {
        return false;
    }
    let path = std::path::Path::new(trimmed);
    if trimmed.contains('/') || trimmed.contains('\\') {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(trimmed).is_file()))
        .unwrap_or(false)
}

/// Update an installed harness discovery package while verifying the target binary
/// exists and storing a pre-update rollback snapshot (#875).
pub fn update_harness_package(
    repo_root: &std::path::Path,
    harness: HarnessId,
    executable_path: &str,
    min_decapod_version: &str,
) -> Result<HarnessInstallReport, String> {
    if !executable_exists_on_disk_or_path(executable_path) {
        return Err(format!(
            "missing_binary: decapod executable '{executable_path}' does not exist"
        ));
    }
    install_harness_package(repo_root, harness, executable_path, min_decapod_version)
}

/// Restore the pre-install/pre-update state of `harness` from its rollback snapshot (#875).
pub fn rollback_harness_package(
    repo_root: &std::path::Path,
    harness: HarnessId,
) -> Result<HarnessRollbackReport, String> {
    let backup_path = rollback_snapshot_path(repo_root, harness);
    ensure_non_symlink_path(&backup_path)?;
    if !backup_path.exists() {
        return Err(format!(
            "rollback_snapshot_missing: no stored harness snapshot for '{}'",
            harness.slug()
        ));
    }
    let raw = std::fs::read_to_string(&backup_path).map_err(|e| e.to_string())?;
    let snapshot: HarnessRollbackSnapshot =
        serde_json::from_str(&raw).map_err(|e| format!("invalid rollback snapshot: {e}"))?;

    let mut restored_paths = Vec::new();
    let mut removed_paths = Vec::new();
    let mut restore_one = |snap: &HarnessFileSnapshot| -> Result<(), String> {
        let target = repo_root.join(&snap.path_hint);
        ensure_non_symlink_path(&target)?;
        if snap.existed {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let content = snap.content.as_deref().unwrap_or("");
            crate::core::atomic::write_atomic(&target, content.as_bytes())
                .map_err(|e| e.to_string())?;
            restored_paths.push(snap.path_hint.clone());
        } else if target.exists() {
            std::fs::remove_file(&target).map_err(|e| e.to_string())?;
            removed_paths.push(snap.path_hint.clone());
        }
        Ok(())
    };

    restore_one(&snapshot.config)?;
    if let Some(ref skill) = snapshot.skill {
        restore_one(skill)?;
    }
    if let Some(ref plugin) = snapshot.plugin {
        restore_one(plugin)?;
    }
    std::fs::remove_file(&backup_path).map_err(|e| e.to_string())?;

    Ok(HarnessRollbackReport {
        harness,
        restored_paths,
        removed_paths,
        preserved_user_config: true,
    })
}

/// Inspect installed harness packaging state, version skew, disabled state, and
/// missing-binary failure conditions (#875).
pub fn inspect_harness_package(
    repo_root: &std::path::Path,
    harness: HarnessId,
) -> Result<HarnessStatusReport, String> {
    let pkg = generate_harness_package(harness, "decapod", &repo_root.to_string_lossy(), "0.0.0")?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let cfg_path = repo_root.join(&pkg.config_path_hint);
    ensure_non_symlink_path(&cfg_path)?;
    let config_present = cfg_path.exists();
    let skill_present = pkg
        .skill_path_hint
        .as_deref()
        .is_some_and(|rel| repo_root.join(rel).is_file());
    let plugin_present = pkg
        .plugin_path_hint
        .as_deref()
        .is_some_and(|rel| repo_root.join(rel).is_file());
    let rollback_snapshot_available = rollback_snapshot_path(repo_root, harness).is_file();

    let mut installed = false;
    let mut disabled = false;
    let mut configured_version: Option<String> = None;
    let mut configured_executable: Option<String> = None;

    if config_present {
        let raw = std::fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
        let val: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid existing JSON '{}': {e}", cfg_path.display()))?;
        if val.get("decapodDisabled").and_then(|v| v.as_bool()) == Some(true) {
            disabled = true;
        }
        match harness {
            HarnessId::GrokBuild | HarnessId::Pi => {
                if let Some(obj) = val.get("decapod").and_then(|v| v.as_object()) {
                    installed = true;
                    if obj.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                        disabled = true;
                    }
                    configured_version = obj
                        .get("decapod_version")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    configured_executable = obj
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                }
            }
            _ => {
                if let Some(entry) = val
                    .get("mcpServers")
                    .and_then(|m| m.get("decapod"))
                    .and_then(|v| v.as_object())
                {
                    installed = true;
                    if entry.get("disabled").and_then(|v| v.as_bool()) == Some(true) {
                        disabled = true;
                    }
                    configured_version = entry
                        .get("decapod_version")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    configured_executable = entry
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                }
            }
        }
    }

    let version_skew = configured_version
        .as_deref()
        .is_some_and(|v| v != current.as_str());
    let missing_binary = configured_executable
        .as_deref()
        .is_some_and(|exe| !executable_exists_on_disk_or_path(exe));
    let expected_skill_ok = pkg.skill_path_hint.is_none() || skill_present;
    let expected_plugin_ok = pkg.plugin_path_hint.is_none() || plugin_present;

    let status = if disabled {
        "disabled"
    } else if !installed {
        "not_installed"
    } else if missing_binary {
        "missing_binary"
    } else if version_skew {
        "version_skew"
    } else if !expected_skill_ok || !expected_plugin_ok {
        "partial_install"
    } else {
        "healthy"
    };

    Ok(HarnessStatusReport {
        harness,
        installed,
        disabled,
        config_present,
        skill_present,
        plugin_present,
        configured_version,
        current_binary_version: current,
        version_skew,
        configured_executable,
        missing_binary,
        rollback_snapshot_available,
        status: status.to_string(),
    })
}

/// Remove only Decapod-owned entries from `repo_root` harness configuration (#875),
/// preserving all unrelated user settings, servers, and hooks.
pub fn remove_harness_package(
    repo_root: &std::path::Path,
    harness: HarnessId,
) -> Result<HarnessRemoveReport, String> {
    let pkg = generate_harness_package(harness, "decapod", &repo_root.to_string_lossy(), "0.0.0")?;
    let mut updated_paths = Vec::new();
    let mut removed_paths = Vec::new();

    let cfg_path = repo_root.join(&pkg.config_path_hint);
    ensure_non_symlink_path(&cfg_path)?;
    if cfg_path.exists() {
        let raw = std::fs::read_to_string(&cfg_path).map_err(|e| e.to_string())?;
        let mut existing: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("invalid existing JSON '{}': {e}", cfg_path.display()))?;
        match harness {
            HarnessId::GrokBuild | HarnessId::Pi => {
                if let Some(obj) = existing.as_object_mut() {
                    obj.remove("decapod");
                }
            }
            HarnessId::ClaudeCode => {
                existing =
                    super::mcp::remove_mcp_host_config(&existing).map_err(|e| e.to_string())?;
                if let Some(arr) = existing
                    .get_mut("hooks")
                    .and_then(|h| h.get_mut("PreToolUse"))
                    .and_then(serde_json::Value::as_array_mut)
                {
                    arr.retain(|item| {
                        item.get("decapodManaged").and_then(|v| v.as_bool()) != Some(true)
                    });
                }
            }
            _ => {
                existing =
                    super::mcp::remove_mcp_host_config(&existing).map_err(|e| e.to_string())?;
            }
        }
        let cfg_bytes = serde_json::to_vec_pretty(&existing).map_err(|e| e.to_string())?;
        crate::core::atomic::write_atomic(&cfg_path, &cfg_bytes).map_err(|e| e.to_string())?;
        updated_paths.push(pkg.config_path_hint);
    }

    if let Some(skill_rel) = pkg.skill_path_hint {
        let skill_path = repo_root.join(&skill_rel);
        ensure_non_symlink_path(&skill_path)?;
        if skill_path.exists() {
            std::fs::remove_file(&skill_path).map_err(|e| e.to_string())?;
            removed_paths.push(skill_rel);
        }
    }

    if let Some(plugin_rel) = pkg.plugin_path_hint {
        let plugin_path = repo_root.join(&plugin_rel);
        ensure_non_symlink_path(&plugin_path)?;
        if plugin_path.exists() {
            std::fs::remove_file(&plugin_path).map_err(|e| e.to_string())?;
            removed_paths.push(plugin_rel);
        }
    }

    Ok(HarnessRemoveReport {
        harness,
        updated_paths,
        removed_paths,
        preserved_user_entries: true,
    })
}

fn compare_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let parse = |s: &str| -> Option<(u64, u64, u64)> {
        let core = s.split('-').next()?;
        let mut it = core.split('.');
        Some((
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
        ))
    };
    Some(parse(a)?.cmp(&parse(b)?))
}
