//! Read-only intent-to-spec traceability. A link is an authored claim, never proof.
//! The harness proposes changes; the kernel reports observable gaps and freshness.
use crate::core::bounded_process::{
    BoundedCommand, Cancellation, ProcessLimits, Termination, CONTROL_TIMEOUT,
};
use crate::core::error::DecapodError;
use crate::core::project_specs::{
    self, LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_INTENT, LOCAL_PROJECT_SPECS_VALIDATION,
};
use crate::plan_governance::{self, SpecReviewDisposition};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

pub const SCHEMA_VERSION: &str = "1.0.0";
const CHANGE_IMPACT_SCHEMA_VERSION: &str = "1.1.0";
const MAX_CHANGED_FILES: usize = 256;
const MAX_DIFF_BYTES: usize = 512 * 1024;
const MAX_PATH_BYTES: usize = 4096;

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
    /// Optional immutable, base-to-HEAD observation. Absent for legacy empty params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_impact: Option<ChangeImpactReport>,
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
                format!(
                    "Line {} must use <!-- decapod:requirement REQ-ID -->",
                    index + 1
                ),
            )),
        }
    }
    markers
}


#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub change: String,
    pub implementation: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeImpactReport {
    pub base_commit: String,
    pub head_commit: String,
    /// False when any changed path or material input could not be inspected.
    pub complete: bool,
    pub changed_files: Vec<ChangedFile>,
    pub material_spec_paths: Vec<String>,
    pub generated_only_spec_paths: Vec<String>,
    /// Explicitly authored requirement markers in materially changed specs.
    /// These are spec assertions, never a source-to-requirement mapping.
    pub referenced_requirement_ids: Vec<String>,
    /// No code-to-requirement assertion is inferred from source proximity.
    pub unmapped_implementation_paths: Vec<String>,
    pub review_needed: bool,
    pub evidence_needed: bool,
    pub findings: Vec<Finding>,
}

fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-replace-objects")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args);
    for key in [
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "GIT_DIR",
        "GIT_REPLACE_REF_BASE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(key);
    }
    command
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, DecapodError> {
    let output = git_command(root, args)
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(|error| {
            DecapodError::ValidationError(format!(
                "IMPACT_GIT_UNAVAILABLE: bounded immutable Git inspection failed: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(DecapodError::ValidationError(
            "IMPACT_GIT_UNAVAILABLE: immutable Git inspection failed or object is absent".into(),
        ));
    }
    Ok(output.stdout)
}

/// A large diff returns an incomplete observation, never unbounded allocation
/// or a misleadingly empty set of changed paths.
fn bounded_git_diff(
    root: &Path,
    base_commit: &str,
    head_commit: &str,
) -> Result<Option<Vec<u8>>, DecapodError> {
    let capture = git_command(
        root,
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
            base_commit,
            head_commit,
            "--",
        ],
    )
    .bounded_capture(
        ProcessLimits {
            timeout: CONTROL_TIMEOUT,
            max_output_bytes: MAX_DIFF_BYTES,
        },
        &Cancellation::default(),
    )
    .map_err(|error| {
        DecapodError::ValidationError(format!(
            "IMPACT_GIT_UNAVAILABLE: bounded change inspection failed: {error}"
        ))
    })?;
    match capture.termination {
        Termination::OutputLimit => Ok(None),
        Termination::Exited if capture.output.status.success() => Ok(Some(capture.output.stdout)),
        _ => Err(DecapodError::ValidationError(
            "IMPACT_GIT_UNAVAILABLE: Git diff failed, timed out or was cancelled".into(),
        )),
    }
}

fn immutable_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn utf8_path(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_PATH_BYTES {
        return None;
    }
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// Parse Git's NUL-delimited name/status output. Refuse to call partial output
/// complete: malformed, non-UTF8 and oversized paths remain visible as findings.
fn parse_changed_files(raw: &[u8]) -> (Vec<ChangedFile>, bool, Vec<Finding>) {
    let mut changes = Vec::new();
    let mut complete = true;
    let mut findings = Vec::new();
    let mut parts = raw.split_terminator(|byte| *byte == 0);
    if !raw.is_empty() && raw.last() != Some(&0) {
        complete = false;
        findings.push(finding(
            "incomplete_git_diff",
            "git_diff",
            "Git name/status output is missing a final NUL delimiter",
        ));
    }
    while let Some(raw_status) = parts.next() {
        let status = std::str::from_utf8(raw_status).unwrap_or("");
        let (kind, rename) = match status {
            "A" => ("added", false),
            "M" => ("modified", false),
            "D" => ("deleted", false),
            "T" => ("type_changed", false),
            value if value.len() > 1
                && value.starts_with('R')
                && value[1..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                ("renamed", true)
            }
            _ => {
                complete = false;
                findings.push(finding(
                    "unsupported_git_change",
                    "git_diff",
                    "Git returned an unsupported or malformed change kind",
                ));
                break;
            }
        };
        let Some(first) = parts.next() else {
            complete = false;
            findings.push(finding("incomplete_git_diff", "git_diff", "Change has no path"));
            break;
        };
        let second = if rename { parts.next() } else { None };
        if rename && second.is_none() {
            complete = false;
            findings.push(finding("incomplete_git_diff", "git_diff", "Rename has no destination"));
            break;
        }
        let (previous_path, path_bytes) = if let Some(destination) = second {
            (utf8_path(first), destination)
        } else {
            (None, first)
        };
        let path = utf8_path(path_bytes);
        if path.is_none() || (rename && previous_path.is_none()) {
            complete = false;
            findings.push(finding(
                "unsupported_git_path",
                "git_diff",
                "A Git path is not UTF-8 or exceeds the bounded path allowance",
            ));
            continue;
        }
        if changes.len() >= MAX_CHANGED_FILES {
            complete = false;
            if !findings.iter().any(|f| f.code == "impact_path_limit") {
                findings.push(finding(
                    "impact_path_limit",
                    "git_diff",
                    format!("More than {MAX_CHANGED_FILES} changed paths; coverage is incomplete"),
                ));
            }
            continue;
        }
        let path = path.expect("checked Git path");
        let implementation = project_specs::is_implementation_path(&path)
            || previous_path
                .as_deref()
                .is_some_and(project_specs::is_implementation_path);
        changes.push(ChangedFile {
            path,
            previous_path,
            change: kind.to_string(),
            implementation,
        });
    }
    changes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.previous_path.cmp(&right.previous_path))
    });
    (changes, complete, findings)
}

fn inspect_change_impact(root: &Path, requested_base: &str) -> Result<ChangeImpactReport, DecapodError> {
    if !immutable_oid(requested_base) {
        return Err(DecapodError::ValidationError(
            "IMPACT_BASE_NOT_IMMUTABLE: base_commit must be a full 40/64-byte hex commit ID".into(),
        ));
    }
    let base_commit = requested_base.to_ascii_lowercase();
    let kind = git_output(root, &["cat-file", "-t", &base_commit])?;
    if kind.as_slice() != b"commit\n" {
        return Err(DecapodError::ValidationError(
            "IMPACT_BASE_NOT_COMMIT: base_commit must identify a commit object".into(),
        ));
    }
    let head_bytes = git_output(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let head_commit = String::from_utf8(head_bytes)
        .map_err(|_| DecapodError::ValidationError("IMPACT_HEAD_INVALID: non-UTF8 commit ID".into()))?
        .trim()
        .to_ascii_lowercase();
    if !immutable_oid(&head_commit) || head_commit.len() != base_commit.len() {
        return Err(DecapodError::ValidationError(
            "IMPACT_HEAD_INVALID: HEAD is not an immutable commit of the same Git hash format".into(),
        ));
    }
    git_output(root, &["merge-base", "--is-ancestor", &base_commit, &head_commit])
        .map_err(|_| DecapodError::ValidationError(
            "IMPACT_BASE_NOT_ANCESTOR: base_commit is not an ancestor of HEAD".into(),
        ))?;
    let raw = bounded_git_diff(root, &base_commit, &head_commit)?;
    let (changed_files, mut complete, mut findings) = match raw {
        Some(bytes) => parse_changed_files(&bytes),
        None => (
            Vec::new(),
            false,
            vec![finding(
                "impact_output_limit",
                "git_diff",
                "Git diff exceeded the bounded output budget",
            )],
        ),
    };
    let specs = project_specs::material_specs_change_between_refs(
        root,
        &base_commit,
        &head_commit,
    )?;
    let mut material_spec_paths = specs.material_changed_paths;
    material_spec_paths.sort();
    let mut generated_only_spec_paths = specs.fingerprint_only_changed_paths;
    generated_only_spec_paths.sort();
    let mut referenced = BTreeSet::new();
    for path in &material_spec_paths {
        // An absent/deleted spec has no head markers; absence never signals coverage.
        let object = format!("{head_commit}:{path}");
        match git_output(root, &["show", &object]) {
            Ok(body) => match String::from_utf8(body) {
                Ok(body) => {
                    for id in authored_requirement_ids(&body, path, &mut findings) {
                        referenced.insert(id);
                    }
                }
                Err(_) => {
                    complete = false;
                    findings.push(finding("unsupported_spec_encoding", path, "Spec is not UTF-8"));
                }
            },
            Err(_) => {
                complete = false;
                findings.push(finding(
                    "missing_changed_spec",
                    path,
                    "Materially changed canonical spec is not readable at HEAD",
                ));
            }
        }
    }
    let unmapped_implementation_paths = changed_files
        .iter()
        .filter(|file| file.implementation)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    if !unmapped_implementation_paths.is_empty() {
        findings.push(finding(
            "implementation_mapping_unproven",
            "git_diff",
            "Changed implementation paths have no verified code-to-requirement mapping",
        ));
        if material_spec_paths.is_empty() {
            findings.push(finding(
                "authored_spec_review_needed",
                project_specs::LOCAL_PROJECT_SPECS_DIR,
                "Implementation changed without any materially authored living-spec change",
            ));
        }
    }
    if !complete {
        findings.push(finding(
            "impact_incomplete",
            "git_diff",
            "Do not infer complete impact or an absence of other changes",
        ));
    }
    let needs_evidence = !changed_files.is_empty() || !material_spec_paths.is_empty() || !complete;
    let needs_review = !unmapped_implementation_paths.is_empty()
        || !material_spec_paths.is_empty()
        || !complete;
    Ok(ChangeImpactReport {
        base_commit,
        head_commit,
        complete,
        changed_files,
        material_spec_paths,
        generated_only_spec_paths,
        referenced_requirement_ids: referenced.into_iter().collect(),
        unmapped_implementation_paths,
        review_needed: needs_review,
        evidence_needed: needs_evidence,
        findings,
    })
}

/// Resolve current, repo-owned evidence without changing specs, plans, or proof.
/// Matching hashes and marker presence establish traceability only, not semantic
/// conformance, user approval, or successful validation.
pub fn reconcile(project_root: &Path) -> Result<ReconciliationReport, DecapodError> {
    reconcile_with_base(project_root, None)
}

/// Optional immutable base-to-HEAD scope; uncommitted material is excluded.
pub fn reconcile_with_base(
    project_root: &Path,
    base_commit: Option<&str>,
) -> Result<ReconciliationReport, DecapodError> {
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
                (!plan.intent.trim().is_empty()).then(|| project_specs::hash_text(&plan.intent)),
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
            findings.push(finding(
                "missing_spec",
                spec.path,
                "Canonical spec file is missing",
            ));
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
                links
                    .entry(id.clone())
                    .or_default()
                    .insert(spec.path.to_string());
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
            let stale = current.and_then(|s| s.material_hash.as_deref())
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

    let change_impact = base_commit.map(|base| inspect_change_impact(project_root, base)).transpose()?;

    Ok(ReconciliationReport {
        schema_version: if change_impact.is_some() {
            CHANGE_IMPACT_SCHEMA_VERSION.to_string()
        } else {
            SCHEMA_VERSION.to_string()
        },
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
        change_impact,
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
        let offline = report
            .requirements
            .iter()
            .find(|r| r.id == "REQ-OFFLINE")
            .unwrap();
        assert!(offline.validation_declared);
        assert!(
            offline
                .spec_paths
                .contains(&project_specs::LOCAL_PROJECT_SPECS_ARCHITECTURE.to_string())
        );
        assert!(has(&report, "duplicate_requirement_marker"));
        assert!(has(&report, "orphan_requirement_reference"));
        assert!(has(&report, "missing_spec_mapping"));
        assert!(has(&report, "missing_validation_mapping"));
        assert!(
            !report
                .requirements
                .iter()
                .any(|r| r.id == "REQ-IN-FENCE" || r.id == "REQ-GENERATED")
        );
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
        assert!(
            findings
                .iter()
                .all(|f| f.code == "invalid_requirement_marker")
        );
    }

    #[test]
    fn provenance_detects_new_code_without_claiming_semantic_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        scaffold(root);
        let fingerprint = project_specs::repo_signal_fingerprint(root).unwrap();
        let spec_hash = project_specs::spec_input_hash(root).unwrap();
        fs::write(
            root.join(project_specs::LOCAL_PROJECT_SPECS_MANIFEST),
            serde_json::json!({
                "schema_version":"1.1.0",
                "template_version":"scaffold-v5",
                "generated_at":"0Z",
                "repo_signal_fingerprint":fingerprint,
                "spec_input_hash":spec_hash,
                "files":[]
            })
            .to_string(),
        )
        .unwrap();
        let before = reconcile(root).unwrap();
        assert_eq!(before.code_changed_since_refresh, Some(false));
        assert_eq!(before.spec_inputs_match_manifest, Some(true));
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"changed\"); }\n",
        )
        .unwrap();
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
        plan_governance::init_plan(
            root,
            InitPlanInput {
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
            },
        )
        .unwrap();
        let report = reconcile(root).unwrap();
        assert_eq!(report.intent_source, "governed_plan");
        assert!(report.intent_hash.is_some());
        assert_eq!(report.plan_state.as_deref(), Some("draft"));
        assert_eq!(report.unresolved_questions.len(), 3);
        assert!(has(&report, "unresolved_intent"));
        assert_eq!(report.semantic_alignment, "not_proven");
    }

    fn git_fixture(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-c")
            .arg("commit.gpgsign=false")
            .current_dir(root)
            .args(args)
            .output()
            .expect("Git fixture command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn immutable_diff_reports_unmapped_code_and_authored_spec_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git_fixture(root, &["init", "-b", "master"]);
        git_fixture(root, &["config", "user.name", "Decapod Test"]);
        git_fixture(root, &["config", "user.email", "test@example.invalid"]);
        scaffold(root);
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "baseline"]);
        let base = git_fixture(root, &["rev-parse", "HEAD"]);
        fs::write(root.join("src/main.rs"), "fn main() { println!(\"change\"); }\n").unwrap();
        fs::write(
            root.join(LOCAL_PROJECT_SPECS_INTENT),
            "# Intent\n<!-- decapod:requirement REQ-IMPACT -->\nBounded inspection.\n",
        )
        .unwrap();
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "source and authored spec change"]);
        let report = inspect_change_impact(root, &base).unwrap();
        assert!(report.complete);
        assert!(report.review_needed);
        assert!(report.evidence_needed);
        assert!(report.unmapped_implementation_paths.contains(&"src/main.rs".to_string()));
        assert!(report.material_spec_paths.contains(&LOCAL_PROJECT_SPECS_INTENT.to_string()));
        assert!(report.referenced_requirement_ids.contains(&"REQ-IMPACT".to_string()));
        assert!(report.findings.iter().any(|item| item.code == "implementation_mapping_unproven"));
        assert!(!report.findings.iter().any(|item| item.code == "authored_spec_review_needed"));
    }

    #[test]
    fn renamed_and_odd_paths_do_not_disappear() {
        let data = b"R100\0src/old name.rs\0src/new name.rs\0A\0src/tab\tname.rs\0D\0src/removed.rs\0";
        let (changed, complete, findings) = parse_changed_files(data);
        assert!(complete);
        assert!(findings.is_empty());
        assert_eq!(changed.len(), 3);
        let renamed = changed.iter().find(|file| file.change == "renamed").unwrap();
        assert_eq!(renamed.previous_path.as_deref(), Some("src/old name.rs"));
        assert_eq!(renamed.path, "src/new name.rs");
        assert!(renamed.implementation);
        assert!(changed.iter().any(|file| file.path == "src/tab\tname.rs"));
        assert!(changed.iter().any(|file| file.change == "deleted"));
    }

    #[test]
    fn path_limits_and_unknown_encodings_are_explicitly_incomplete() {
        let mut data = Vec::new();
        for i in 0..=MAX_CHANGED_FILES {
            data.extend_from_slice(b"M\0");
            data.extend_from_slice(format!("src/{i:04}.rs").as_bytes());
            data.push(0);
        }
        let (changed, complete, findings) = parse_changed_files(&data);
        assert!(!complete);
        assert_eq!(changed.len(), MAX_CHANGED_FILES);
        assert!(findings.iter().any(|item| item.code == "impact_path_limit"));

        let (_, complete, findings) = parse_changed_files(b"A\0src/\xff.rs\0");
        assert!(!complete);
        assert!(findings.iter().any(|item| item.code == "unsupported_git_path"));
        let (_, complete, findings) = parse_changed_files(b"A\0\0");
        assert!(!complete);
        assert!(findings.iter().any(|item| item.code == "unsupported_git_path"));

        let (_, complete, findings) = parse_changed_files(b"M\0src/unfinished.rs");
        assert!(!complete);
        assert!(findings.iter().any(|item| item.code == "incomplete_git_diff"));
    }

    #[test]
    fn symbolic_or_absent_base_never_means_no_change() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git_fixture(root, &["init", "-b", "master"]);
        git_fixture(root, &["config", "user.name", "Decapod Test"]);
        git_fixture(root, &["config", "user.email", "test@example.invalid"]);
        scaffold(root);
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "baseline"]);
        let base = git_fixture(root, &["rev-parse", "HEAD"]);
        assert!(inspect_change_impact(root, "HEAD").is_err());
        assert!(inspect_change_impact(root, "origin/master").is_err());
        assert!(inspect_change_impact(root, "0000000000000000000000000000000000000000").is_err());
        let empty = inspect_change_impact(root, &base).unwrap();
        assert!(empty.complete);
        assert!(empty.changed_files.is_empty());
        assert!(!empty.evidence_needed);
    }


    #[test]
    fn non_ancestor_base_is_rejected_without_reporting_empty_impact() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git_fixture(root, &["init", "-b", "master"]);
        git_fixture(root, &["config", "user.name", "Decapod Test"]);
        git_fixture(root, &["config", "user.email", "test@example.invalid"]);
        scaffold(root);
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "base"]);
        git_fixture(root, &["checkout", "-b", "other"]);
        fs::write(root.join("src/other.rs"), "pub fn other() {}\n").unwrap();
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "other branch"]);
        let foreign = git_fixture(root, &["rev-parse", "HEAD"]);
        git_fixture(root, &["checkout", "master"]);
        fs::write(root.join("src/main.rs"), "fn main() { println!(\"master\"); }\n").unwrap();
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "master branch"]);
        let error = inspect_change_impact(root, &foreign).unwrap_err();
        assert!(error.to_string().contains("IMPACT_BASE_NOT_ANCESTOR"));
    }

    #[test]
    fn projection_only_edit_does_not_claim_authored_spec_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git_fixture(root, &["init", "-b", "master"]);
        git_fixture(root, &["config", "user.name", "Decapod Test"]);
        git_fixture(root, &["config", "user.email", "test@example.invalid"]);
        scaffold(root);
        let spec = root.join(LOCAL_PROJECT_SPECS_INTENT);
        let make = |digest: &str| {
            format!(
                "# Intent\n<!-- decapod:codebase-attestation:start -->\n\
                 generated {digest}\n<!-- decapod:codebase-attestation:end -->\n"
            )
        };
        fs::write(&spec, make("before")).unwrap();
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "base"]);
        let base = git_fixture(root, &["rev-parse", "HEAD"]);
        fs::write(&spec, make("after")).unwrap();
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "refresh projection"]);
        let report = inspect_change_impact(root, &base).unwrap();
        assert!(report.complete);
        assert!(report.material_spec_paths.is_empty());
        assert!(report
            .generated_only_spec_paths
            .contains(&LOCAL_PROJECT_SPECS_INTENT.to_string()));
        assert!(report.referenced_requirement_ids.is_empty());
    }


    #[test]
    fn unstaged_worktree_edit_cannot_enter_immutable_diff() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git_fixture(root, &["init", "-b", "master"]);
        git_fixture(root, &["config", "user.name", "Decapod Test"]);
        git_fixture(root, &["config", "user.email", "test@example.invalid"]);
        scaffold(root);
        git_fixture(root, &["add", "."]);
        git_fixture(root, &["commit", "-m", "baseline"]);
        let base = git_fixture(root, &["rev-parse", "HEAD"]);
        fs::write(root.join("src/main.rs"), "fn main() { println!(\"uncommitted\"); }\n").unwrap();
        let report = inspect_change_impact(root, &base).unwrap();
        assert!(report.complete);
        assert_eq!(report.base_commit, report.head_commit);
        assert!(report.changed_files.is_empty());
        assert!(report.unmapped_implementation_paths.is_empty());
        assert!(!report.review_needed);
        assert!(!report.evidence_needed);
    }

}
