//! Read-only intent-to-spec traceability. A link is an authored claim, never proof.
//! The harness proposes changes; the kernel reports observable gaps and freshness.
use crate::core::error::DecapodError;
use crate::core::project_specs::{
    self, LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_INTENT, LOCAL_PROJECT_SPECS_VALIDATION,
};
use crate::plan_governance::{self, SpecReviewDisposition};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize)]
pub struct SpecObservation {
    pub path: String,
    pub material_hash: Option<String>,
    /// Byte-level match to the last manifest; not a correctness judgment.
    pub matches_manifest: Option<bool>,
    pub requirement_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequirementLink {
    pub id: String,
    /// Authored cross-references, excluding the defining INTENT document.
    pub spec_paths: Vec<String>,
    /// An author claims a validation obligation exists; no check has run.
    pub validation_declared: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub code: String,
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationReport {
    pub schema_version: String,
    pub intent_source: String,
    pub intent_hash: Option<String>,
    pub plan_state: Option<String>,
    pub unresolved_questions: Vec<String>,
    pub repo_signal_fingerprint: String,
    pub last_refresh_fingerprint: Option<String>,
    pub code_changed_since_refresh: Option<bool>,
    pub spec_inputs_match_manifest: Option<bool>,
    pub specs: Vec<SpecObservation>,
    pub requirements: Vec<RequirementLink>,
    pub findings: Vec<Finding>,
    /// Semantic alignment needs external judgment and applicable proof.
    pub semantic_alignment: String,
}

fn finding(code: &str, source: &str, detail: impl Into<String>) -> Finding {
    Finding {
        code: code.to_string(),
        source: source.to_string(),
        detail: detail.into(),
    }
}

fn valid_requirement_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("REQ-") else {
        return false;
    };
    !rest.is_empty()
        && id.len() <= 64
        && rest
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// Only exact, stand-alone markers in authored Markdown count. Code fences and
/// generated projections never establish requirement references.
fn authored_requirement_ids(body: &str, path: &str, findings: &mut Vec<Finding>) -> Vec<String> {
    let material = project_specs::material_spec_body(body);
    let mut in_fence = false;
    let mut markers = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in material.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || !trimmed.starts_with("<!-- decapod:requirement") {
            continue;
        }
        let id = trimmed
            .strip_prefix("<!-- decapod:requirement ")
            .and_then(|rest| rest.strip_suffix(" -->"));
        match id {
            Some(id) if valid_requirement_id(id) => {
                if !seen.insert(id.to_owned()) {
                    findings.push(finding(
                        "duplicate_requirement_marker",
                        path,
                        format!("{} occurs more than once in this document", id),
                    ));
                } else {
                    markers.push(id.to_owned());
                }
            }
            _ => findings.push(finding(
                "invalid_requirement_marker",
                path,
                format!("Line {} must use <!-- decapod:requirement REQ-ID -->", index + 1),
            )),
        }
    }
    markers
}

/// Resolve current, repo-owned evidence without changing specs, plans, or proof.
/// Matching hashes and marker presence establish traceability only, not semantic
/// conformance, user approval, or successful validation.
pub fn reconcile(project_root: &Path) -> Result<ReconciliationReport, DecapodError> {
    let fingerprint = project_specs::repo_signal_fingerprint(project_root)?;
    let manifest = project_specs::read_specs_manifest(project_root)?;
    let plan = plan_governance::load_plan(project_root)?;
    let current_spec_input_hash = project_specs::spec_input_hash(project_root)?;

    let mut findings = Vec::new();
    let mut unresolved = Vec::new();
    let (intent_source, intent_hash, plan_state) = match plan.as_ref() {
        Some(plan) => {
            unresolved.extend(plan.unknowns.iter().cloned());
            unresolved.extend(plan.human_questions.iter().cloned());
            unresolved.extend(plan.unresolved_contradictions.iter().cloned());
            if plan.intent.trim().is_empty() {
                findings.push(finding(
                    "missing_active_intent",
                    ".decapod/governance.json",
                    "The governed plan has no human-originated change intent",
                ));
            }
            if !unresolved.is_empty() {
                findings.push(finding(
                    "unresolved_intent",
                    ".decapod/governance.json",
                    "The governed plan contains unresolved questions or contradictions",
                ));
            }
            (
                "governed_plan".to_string(),
                (!plan.intent.trim().is_empty())
                    .then(|| project_specs::hash_text(&plan.intent)),
                Some(format!("{:?}", plan.state).to_ascii_lowercase()),
            )
        }
        None => {
            findings.push(finding(
                "missing_governed_plan",
                ".decapod/governance.json",
                "No active governed plan binds the current human request",
            ));
            ("unbound".to_string(), None, None)
        }
    };

    let mut spec_observations = Vec::new();
    let mut links: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut defined: BTreeSet<String> = BTreeSet::new();

    for spec in LOCAL_PROJECT_SPECS {
        let path = project_root.join(spec.path);
        if !path.exists() {
            findings.push(finding("missing_spec", spec.path, "Canonical spec file is missing"));
            spec_observations.push(SpecObservation {
                path: spec.path.to_string(),
                material_hash: None,
                matches_manifest: None,
                requirement_ids: Vec::new(),
            });
            continue;
        }
        let body = fs::read_to_string(&path).map_err(DecapodError::IoError)?;
        let ids = authored_requirement_ids(&body, spec.path, &mut findings);
        for id in &ids {
            if spec.path == LOCAL_PROJECT_SPECS_INTENT {
                defined.insert(id.clone());
            } else {
                links.entry(id.clone()).or_default().insert(spec.path.to_string());
            }
        }
        let matches_manifest = manifest.as_ref().and_then(|m| {
            m.files
                .iter()
                .find(|entry| entry.path == spec.path)
                .map(|entry| entry.content_hash == project_specs::hash_text(&body))
        });
        spec_observations.push(SpecObservation {
            path: spec.path.to_string(),
            material_hash: Some(project_specs::material_spec_body_hash(&body)),
            matches_manifest,
            requirement_ids: ids,
        });
    }

    if defined.is_empty() {
        findings.push(finding(
            "no_explicit_requirement_ids",
            LOCAL_PROJECT_SPECS_INTENT,
            "Authored intent has no traceable REQ-* markers; no coverage is inferred",
        ));
    }

    let mut requirements = Vec::new();
    for id in &defined {
        let refs = links.get(id).cloned().unwrap_or_default();
        let validation_declared = refs.contains(LOCAL_PROJECT_SPECS_VALIDATION);
        if !refs.iter().any(|p| p != LOCAL_PROJECT_SPECS_VALIDATION) {
            findings.push(finding(
                "missing_spec_mapping",
                LOCAL_PROJECT_SPECS_INTENT,
                format!("{id} has no authored reference in a non-validation spec"),
            ));
        }
        if !validation_declared {
            findings.push(finding(
                "missing_validation_mapping",
                LOCAL_PROJECT_SPECS_VALIDATION,
                format!("{id} has no authored validation obligation reference"),
            ));
        }
        requirements.push(RequirementLink {
            id: id.clone(),
            spec_paths: refs.into_iter().collect(),
            validation_declared,
        });
    }
    for id in links.keys() {
        if !defined.contains(id) {
            for source in &links[id] {
                findings.push(finding(
                    "orphan_requirement_reference",
                    source,
                    format!("{id} is not defined in canonical INTENT.md"),
                ));
            }
        }
    }

    if let Some(m) = manifest.as_ref() {
        if m.repo_signal_fingerprint != fingerprint {
            findings.push(finding(
                "code_changed_since_refresh",
                project_specs::LOCAL_PROJECT_SPECS_MANIFEST,
                "Code fingerprint differs from the last generated spec manifest",
            ));
        }
        if m.spec_input_hash != current_spec_input_hash {
            findings.push(finding(
                "spec_inputs_changed_since_refresh",
                project_specs::LOCAL_PROJECT_SPECS_MANIFEST,
                "Spec bytes differ from the last generated manifest; this does not establish semantic drift",
            ));
        }
    } else {
        findings.push(finding(
            "missing_specs_manifest",
            project_specs::LOCAL_PROJECT_SPECS_MANIFEST,
            "There is no generated spec snapshot to compare against",
        ));
    }

    if let Some(plan) = plan.as_ref() {
        for review in &plan.spec_reviews {
            let current = spec_observations.iter().find(|s| s.path == review.path);
            let stale = current
                .and_then(|s| s.material_hash.as_deref())
                != Some(review.spec_material_hash.as_str())
                || review.reviewed_code_fingerprint != fingerprint;
            if stale {
                findings.push(finding(
                    "stale_spec_review",
                    &review.path,
                    "Recorded review is bound to different authored prose or repository code",
                ));
            }
            if review.disposition == SpecReviewDisposition::RequiresDecision {
                findings.push(finding(
                    "human_spec_decision_required",
                    &review.path,
                    "A pending human decision cannot be resolved by this report",
                ));
            }
        }
    }

    Ok(ReconciliationReport {
        schema_version: SCHEMA_VERSION.to_string(),
        intent_source,
        intent_hash,
        plan_state,
        unresolved_questions: unresolved,
        repo_signal_fingerprint: fingerprint.clone(),
        last_refresh_fingerprint: manifest.as_ref().map(|m| m.repo_signal_fingerprint.clone()),
        code_changed_since_refresh: manifest
            .as_ref()
            .map(|m| m.repo_signal_fingerprint != fingerprint),
        spec_inputs_match_manifest: manifest
            .as_ref()
            .map(|m| m.spec_input_hash == current_spec_input_hash),
        specs: spec_observations,
        requirements,
        findings,
        semantic_alignment: "not_proven".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_governance::{InitPlanInput, ScopeConstraints};
    use std::fs;

    fn scaffold(root: &Path) {
        fs::create_dir_all(root.join(".decapod/managed/specs")).unwrap();
        for spec in LOCAL_PROJECT_SPECS {
            fs::write(root.join(spec.path), format!("# {}\n", spec.role)).unwrap();
        }
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    }

    fn has(report: &ReconciliationReport, code: &str) -> bool {
        report.findings.iter().any(|finding| finding.code == code)
    }

    #[test]
    fn traceability_is_explicit_and_does_not_promote_markers_into_proof() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root);
        fs::write(
            root.join(LOCAL_PROJECT_SPECS_INTENT),
            "# Intent\n<!-- decapod:requirement REQ-OFFLINE -->\nOffline must work.\n\
             <!-- decapod:requirement REQ-SECOND -->\nAnother outcome.\n\
             <!-- decapod:requirement REQ-SECOND -->\n\
             ```markdown\n<!-- decapod:requirement REQ-IN-FENCE -->\n```\n\
             <!-- decapod:codebase-attestation:start -->\n\
             <!-- decapod:requirement REQ-GENERATED -->\n\
             <!-- decapod:codebase-attestation:end -->\n",
        )
        .unwrap();
        fs::write(
            root.join(project_specs::LOCAL_PROJECT_SPECS_ARCHITECTURE),
            "<!-- decapod:requirement REQ-OFFLINE -->\n\
             <!-- decapod:requirement REQ-ORPHAN -->\n",
        )
        .unwrap();
        fs::write(
            root.join(LOCAL_PROJECT_SPECS_VALIDATION),
            "<!-- decapod:requirement REQ-OFFLINE -->\n",
        )
        .unwrap();
        let report = reconcile(root).unwrap();
        assert_eq!(report.semantic_alignment, "not_proven");
        assert_eq!(report.requirements.len(), 2);
        let offline = report.requirements.iter().find(|r| r.id == "REQ-OFFLINE").unwrap();
        assert!(offline.validation_declared);
        assert!(offline.spec_paths.contains(&project_specs::LOCAL_PROJECT_SPECS_ARCHITECTURE.to_string()));
        assert!(has(&report, "duplicate_requirement_marker"));
        assert!(has(&report, "orphan_requirement_reference"));
        assert!(has(&report, "missing_spec_mapping"));
        assert!(has(&report, "missing_validation_mapping"));
        assert!(!report.requirements.iter().any(|r| r.id == "REQ-IN-FENCE" || r.id == "REQ-GENERATED"));
    }

    #[test]
    fn invalid_markers_never_create_authority() {
        let mut findings = Vec::new();
        let ids = authored_requirement_ids(
            "<!-- decapod:requirement REQ-valid -->\n\
             <!-- decapod:requirement REQ-GOOD -->\n\
             <!-- decapod:requirement REQ-WHITESPACE  -->\n",
            LOCAL_PROJECT_SPECS_INTENT,
            &mut findings,
        );
        assert_eq!(ids, vec!["REQ-GOOD"]);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.code == "invalid_requirement_marker"));
    }

    #[test]
    fn provenance_detects_new_code_without_claiming_semantic_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root);
        let fingerprint = project_specs::repo_signal_fingerprint(root).unwrap();
        let spec_hash = project_specs::spec_input_hash(root).unwrap();
        fs::write(root.join(project_specs::LOCAL_PROJECT_SPECS_MANIFEST),
            serde_json::json!({
                "schema_version":"1.1.0",
                "template_version":"scaffold-v5",
                "generated_at":"0Z",
                "repo_signal_fingerprint":fingerprint,
                "spec_input_hash":spec_hash,
                "files":[]
            }).to_string(),
        ).unwrap();
        let before = reconcile(root).unwrap();
        assert_eq!(before.code_changed_since_refresh, Some(false));
        assert_eq!(before.spec_inputs_match_manifest, Some(true));
        fs::write(root.join("src/main.rs"), "fn main() { println!(\"changed\"); }\n").unwrap();
        let after = reconcile(root).unwrap();
        assert_eq!(after.code_changed_since_refresh, Some(true));
        assert!(has(&after, "code_changed_since_refresh"));
        assert_eq!(after.semantic_alignment, "not_proven");
        assert_eq!(after.spec_inputs_match_manifest, Some(true));
    }

    #[test]
    fn governed_human_intent_and_unknowns_remain_visible() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root);
        plan_governance::init_plan(root, InitPlanInput {
            title: "offline mode".into(),
            intent: "Offline edits remain usable".into(),
            todo_ids: vec![],
            proof_hooks: vec![],
            unknowns: vec!["conflict semantics".into()],
            human_questions: vec!["Who may override?".into()],
            stop_conditions: vec![],
            unresolved_contradictions: vec!["shared writer ambiguity".into()],
            deferred_questions: vec![],
            constraints: ScopeConstraints::default(),
            phases: vec![],
        }).unwrap();
        let report = reconcile(root).unwrap();
        assert_eq!(report.intent_source, "governed_plan");
        assert!(report.intent_hash.is_some());
        assert_eq!(report.plan_state.as_deref(), Some("draft"));
        assert_eq!(report.unresolved_questions.len(), 3);
        assert!(has(&report, "unresolved_intent"));
        assert_eq!(report.semantic_alignment, "not_proven");
    }
}
