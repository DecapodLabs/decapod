//! Read-only immutable-history assessment shared with the strict publication gate.
//! Missing historical proof is an evidence gap, not proof that can be invented at HEAD.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryDisposition {
    Proven,
    UnprovenHistory,
    InvalidHistory,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryObligation {
    CanonicalRecord,
    CheckpointForMaterial,
    PreservePriorEvidence,
    WorkstreamIdentity,
    AcceptedBase,
    GitInput,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryFinding {
    pub commit: String,
    pub tree: String,
    pub disposition: HistoryDisposition,
    pub obligation: HistoryObligation,
    pub code: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_input_digest: Option<String>,
    pub prior_event_can_be_regenerated: bool,
    pub recovery: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckpointHistoryReport {
    pub schema: &'static str,
    pub scope: &'static str,
    pub disposition: HistoryDisposition,
    pub base_commit: String,
    pub head_commit: String,
    pub history_proven: bool,
    pub inspection_complete: bool,
    /// This report covers checkpoint history, not complete publication readiness.
    pub publication_ready: bool,
    pub verified_commits: Vec<String>,
    pub findings: Vec<HistoryFinding>,
}

impl CheckpointHistoryReport {
    pub fn require_proven(&self) -> Result<(), DecapodError> {
        match self.findings.first() {
            Some(finding) => Err(invalid(finding.detail.clone())),
            None if self.history_proven => Ok(()),
            None => Err(invalid(
                "GOVERNANCE_HISTORY_NOT_PROVEN: no history proof was produced",
            )),
        }
    }
}

struct HistoryFault {
    disposition: HistoryDisposition,
    obligation: HistoryObligation,
    code: &'static str,
    detail: String,
    expected_input_digest: Option<String>,
}
impl HistoryFault {
    fn missing_record(commit: &str) -> Self {
        Self {
            disposition: HistoryDisposition::UnprovenHistory,
            obligation: HistoryObligation::CanonicalRecord,
            code: "GOVERNANCE_CHECKPOINT_MISSING",
            detail: format!(
                "GOVERNANCE_CHECKPOINT_MISSING: commit {commit} has no canonical record"
            ),
            expected_input_digest: None,
        }
    }
    fn missing_checkpoint(commit: &str, expected: String) -> Self {
        Self {
            disposition: HistoryDisposition::UnprovenHistory,
            obligation: HistoryObligation::CheckpointForMaterial,
            code: "GOVERNANCE_CHECKPOINT_MISSING",
            detail: format!(
                "GOVERNANCE_CHECKPOINT_MISSING: commit {commit} needs a new checkpoint bound to its exact material tree and governance inputs"
            ),
            expected_input_digest: Some(expected),
        }
    }
    fn invalid(obligation: HistoryObligation, message: impl Into<String>) -> Self {
        Self {
            disposition: HistoryDisposition::InvalidHistory,
            obligation,
            code: "GOVERNANCE_HISTORY_INVALID",
            detail: message.into(),
            expected_input_digest: None,
        }
    }
    fn invalid_error(error: DecapodError) -> Self {
        Self::invalid(HistoryObligation::CanonicalRecord, error.to_string())
    }
}
impl From<DecapodError> for HistoryFault {
    fn from(error: DecapodError) -> Self {
        Self {
            disposition: HistoryDisposition::Unavailable,
            obligation: HistoryObligation::GitInput,
            code: "GOVERNANCE_HISTORY_INPUT_UNAVAILABLE",
            detail: error.to_string(),
            expected_input_digest: None,
        }
    }
}

/// Absence is observed from the immutable tree, not inferred from a failed read.
fn committed_record(root: &Path, commit: &str) -> Result<Option<String>, HistoryFault> {
    let entry = git(root, &["ls-tree", commit, "--", GOVERNANCE_PATH])?;
    if entry.is_empty() {
        return Ok(None);
    }
    if !entry.starts_with("100644 blob ") {
        return Err(HistoryFault::invalid(
            HistoryObligation::CanonicalRecord,
            format!(
                "GOVERNANCE_HISTORY_INVALID: commit {commit} canonical governance must be a regular non-executable blob"
            ),
        ));
    }
    git(root, &["show", &format!("{commit}:{GOVERNANCE_PATH}")])
        .map(Some)
        .map_err(HistoryFault::from)
}

struct HistoryContext<'a> {
    head: &'a GovernanceDocument,
    identity: &'a str,
    first_parent_commits: BTreeSet<&'a str>,
    accepted_commits: BTreeSet<&'a str>,
    base_has_document: bool,
}

fn inspect_commit(
    root: &Path,
    commit: &str,
    context: &HistoryContext<'_>,
) -> Result<bool, HistoryFault> {
    let content = match committed_record(root, commit)? {
        Some(content) => content,
        None if !context.base_has_document
            && context.first_parent_commits.contains(commit)
            && git(
                root,
                &["log", "-1", "--format=%H", commit, "--", GOVERNANCE_PATH],
            )?
            .is_empty() =>
        {
            return Ok(false);
        }
        None => return Err(HistoryFault::missing_record(commit)),
    };
    let document = parse(content.as_bytes()).map_err(HistoryFault::invalid_error)?;
    let change = document.change.as_ref().ok_or_else(|| {
        HistoryFault::invalid(
            HistoryObligation::WorkstreamIdentity,
            format!("GOVERNANCE_PR_IDENTITY_MISSING: commit {commit}"),
        )
    })?;
    if context.first_parent_commits.contains(commit) {
        if document
            .change
            .as_ref()
            .map(|change| (&change.base_commit, &change.target_base))
            != context
                .head
                .change
                .as_ref()
                .map(|change| (&change.base_commit, &change.target_base))
        {
            return Err(HistoryFault::invalid(
                HistoryObligation::AcceptedBase,
                "GOVERNANCE_PR_BASE_MISMATCH: base identity changed inside the PR",
            ));
        }
        if change.id != context.identity {
            return Err(HistoryFault::invalid(
                HistoryObligation::WorkstreamIdentity,
                format!(
                    "GOVERNANCE_PR_IDENTITY_CONFLICT: commit {commit} belongs to a different PR"
                ),
            ));
        }
    } else {
        // A historical lineage must start from an immutable, already
        // accepted base and prove its own original checkpoints. Neither a
        // branch name nor equality with a later accepted tree is a waiver.
        if !context
            .accepted_commits
            .contains(change.base_commit.as_str())
        {
            return Err(HistoryFault::invalid(
                HistoryObligation::AcceptedBase,
                format!(
                    "GOVERNANCE_PR_BASE_MISMATCH: imported commit {commit} has an unaccepted base"
                ),
            ));
        }
        verify_base_binding(
            root,
            &document,
            &change.base_commit,
            commit,
            context
                .head
                .change
                .as_ref()
                .map(|change| change.target_base.as_str()),
        )
        .map_err(|error| {
            HistoryFault::invalid(HistoryObligation::AcceptedBase, error.to_string())
        })?;
    }
    let parent_line = git(root, &["rev-list", "--parents", "-n", "1", commit])?;
    let parents: Vec<_> = parent_line.split_whitespace().skip(1).collect();
    let mut prior_ids = BTreeSet::new();
    for parent in parents.iter().take(1) {
        let Some(parent_text) = committed_record(root, parent)? else {
            continue;
        };
        let prior = parse(parent_text.as_bytes()).map_err(HistoryFault::invalid_error)?;
        if prior.change.as_ref().map(|change| change.id.as_str()) != Some(change.id.as_str()) {
            if prior.change.is_some() && !context.accepted_commits.contains(parent) {
                return Err(HistoryFault::invalid(
                    HistoryObligation::WorkstreamIdentity,
                    format!(
                        "GOVERNANCE_PR_IDENTITY_CONFLICT: commit {commit} changed identity inside an unaccepted lineage"
                    ),
                ));
            }
            continue;
        }
        if prior
            .change
            .as_ref()
            .map(|change| (&change.base_commit, &change.target_base))
            != Some((&change.base_commit, &change.target_base))
        {
            return Err(HistoryFault::invalid(
                HistoryObligation::AcceptedBase,
                format!(
                    "GOVERNANCE_PR_BASE_MISMATCH: commit {commit} changed its first-parent base identity"
                ),
            ));
        }
        for old in &prior.checkpoints {
            prior_ids.insert(old.id.clone());
            if !document.checkpoints.iter().any(|entry| {
                entry.id == old.id
                    && entry.input_digest == old.input_digest
                    && entry.summary == old.summary
                    && entry.proof_refs == old.proof_refs
            }) {
                return Err(HistoryFault::invalid(
                    HistoryObligation::PreservePriorEvidence,
                    format!(
                        "GOVERNANCE_CHECKPOINT_HISTORY_CHANGED: commit {commit} erased or rewrote a parent checkpoint"
                    ),
                ));
            }
        }
    }
    // Transport commits may reuse proof only for exact whole-tree equality,
    // including the governance document, and only with one source parent.
    // That source is independently checked in this topological walk (or is
    // already accepted). Material edits and all merges need a new checkpoint.
    let unchanged_transport = parents.len() == 1
        && git(root, &["rev-parse", &format!("{commit}^{{tree}}")])?
            == git(root, &["rev-parse", &format!("{}^{{tree}}", parents[0])])?;
    let expected = material_digest(root, &document, Some(commit))?;
    if !document.checkpoints.iter().any(|entry| {
        entry.input_digest == expected && (unchanged_transport || !prior_ids.contains(&entry.id))
    }) {
        return Err(HistoryFault::missing_checkpoint(commit, expected));
    }
    Ok(true)
}

/// Inspect all authored commits without changing Git, governance, or admission.
/// Capture ref identities once so concurrent branch movement cannot retarget a report.
pub fn inspect_checkpoint_history(
    root: &Path,
    base_ref: &str,
    head_ref: &str,
    target_base: Option<&str>,
) -> Result<CheckpointHistoryReport, DecapodError> {
    crate::core::entrypoint_integrity::ensure_ungrafted(root).map_err(invalid)?;
    let base = git(
        root,
        &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
    )?;
    let head_commit = git(
        root,
        &["rev-parse", "--verify", &format!("{head_ref}^{{commit}}")],
    )?;
    let ancestry = crate::core::entrypoint_integrity::history_git(
        root,
        &["merge-base", "--is-ancestor", &base, &head_commit],
    )
    .ok_or_else(|| invalid("GOVERNANCE_GIT_PROOF_UNAVAILABLE"))?;
    match ancestry.status.code() {
        Some(0) => (),
        Some(1) => {
            return incomplete_report(
                root,
                base,
                &head_commit,
                HistoryFault::invalid(
                    HistoryObligation::AcceptedBase,
                    "GOVERNANCE_PR_BASE_MISMATCH: accepted base must be an ancestor of the verified head",
                ),
            );
        }
        _ => return Err(invalid("GOVERNANCE_GIT_PROOF_UNAVAILABLE")),
    }
    let head_text = match committed_record(root, &head_commit) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return incomplete_report(
                root,
                base,
                &head_commit,
                HistoryFault::missing_record(&head_commit),
            );
        }
        Err(fault) => return incomplete_report(root, base, &head_commit, fault),
    };
    let head = match parse(head_text.as_bytes()) {
        Ok(document) => document,
        Err(error) => {
            return incomplete_report(root, base, &head_commit, HistoryFault::invalid_error(error));
        }
    };
    if let Err(error) = verify_base_binding(root, &head, &base, &head_commit, target_base) {
        return incomplete_report(
            root,
            base,
            &head_commit,
            HistoryFault::invalid(HistoryObligation::AcceptedBase, error.to_string()),
        );
    }
    let identity = &head
        .change
        .as_ref()
        .ok_or_else(|| invalid("GOVERNANCE_PR_IDENTITY_MISSING: final head"))?
        .id;
    let commits = git(
        root,
        &[
            "rev-list",
            "--reverse",
            "--topo-order",
            &format!("{base}..{head_commit}"),
        ],
    )?;
    let first_parent = git(
        root,
        &[
            "rev-list",
            "--first-parent",
            &format!("{base}..{head_commit}"),
        ],
    )?;
    let accepted = git(root, &["rev-list", &base])?;
    let base_has_document = !git(root, &["ls-tree", &base, "--", GOVERNANCE_PATH])?.is_empty();
    let context = HistoryContext {
        head: &head,
        identity,
        first_parent_commits: first_parent.lines().collect(),
        accepted_commits: accepted.lines().collect(),
        base_has_document,
    };
    let mut report = CheckpointHistoryReport {
        schema: "decapod.checkpoint-history.v1",
        scope: "checkpoint_history",
        disposition: HistoryDisposition::Unavailable,
        base_commit: base,
        head_commit: head_commit.clone(),
        history_proven: false,
        inspection_complete: true,
        publication_ready: false,
        verified_commits: Vec::new(),
        findings: Vec::new(),
    };
    for commit in commits.lines().filter(|line| !line.is_empty()) {
        match inspect_commit(root, commit, &context) {
            Ok(true) => report.verified_commits.push(commit.to_string()),
            Ok(false) => (), // Pre-governance bootstrap, as in the strict verifier.
            Err(fault) => report.findings.push(finding(root, commit, fault)?),
        }
    }
    if !commits.is_empty() && report.verified_commits.is_empty() && report.findings.is_empty() {
        report.findings.push(finding(
            root,
            &head_commit,
            HistoryFault::missing_record(&head_commit),
        )?);
    }
    let expected = material_digest(root, &head, Some(&head_commit))?;
    if !head
        .checkpoints
        .iter()
        .any(|entry| entry.input_digest == expected)
        && !report
            .findings
            .iter()
            .any(|finding| finding.commit == head_commit)
    {
        report.findings.push(finding(
            root,
            &head_commit,
            HistoryFault::missing_checkpoint(&head_commit, expected),
        )?);
    }
    report.history_proven = report.findings.is_empty();
    report.disposition = if report.history_proven {
        HistoryDisposition::Proven
    } else if report
        .findings
        .iter()
        .any(|finding| finding.disposition == HistoryDisposition::Unavailable)
    {
        HistoryDisposition::Unavailable
    } else if report
        .findings
        .iter()
        .any(|finding| finding.disposition == HistoryDisposition::InvalidHistory)
    {
        HistoryDisposition::InvalidHistory
    } else {
        HistoryDisposition::UnprovenHistory
    };
    report.inspection_complete = report.disposition != HistoryDisposition::Unavailable;
    Ok(report)
}

fn finding(root: &Path, commit: &str, fault: HistoryFault) -> Result<HistoryFinding, DecapodError> {
    let recovery = match fault.disposition {
        HistoryDisposition::UnprovenHistory => {
            "A checkpoint written now cannot establish that this immutable commit carried a checkpoint. Preserve the source ref and evidence; create a fresh claimed workspace from the accepted target, reauthor the reviewed material with current intent/spec reviews and checkpoints, and rerun independent validation. No current policy authorizes a candidate-local history waiver."
        }
        HistoryDisposition::InvalidHistory => {
            "Preserve the invalid commit and genuine prior evidence. Do not overwrite checkpoints or relabel provenance; inspect the affected obligation and reauthor in a fresh governed workspace if authentic historical evidence is unavailable."
        }
        HistoryDisposition::Unavailable => {
            "Restore access to the actual immutable Git objects and retry inspection. Missing or unreadable input is not proof of a historical violation."
        }
        HistoryDisposition::Proven => "No history recovery required.",
    };
    Ok(HistoryFinding {
        commit: commit.to_string(),
        tree: git(root, &["rev-parse", &format!("{commit}^{{tree}}")])?,
        disposition: fault.disposition,
        obligation: fault.obligation,
        code: fault.code.to_string(),
        detail: fault.detail,
        expected_input_digest: fault.expected_input_digest,
        prior_event_can_be_regenerated: false,
        recovery: recovery.to_string(),
    })
}

fn incomplete_report(
    root: &Path,
    base: String,
    head: &str,
    fault: HistoryFault,
) -> Result<CheckpointHistoryReport, DecapodError> {
    Ok(CheckpointHistoryReport {
        schema: "decapod.checkpoint-history.v1",
        scope: "checkpoint_history",
        disposition: fault.disposition,
        base_commit: base,
        head_commit: head.to_string(),
        history_proven: false,
        inspection_complete: false,
        publication_ready: false,
        verified_commits: Vec::new(),
        findings: vec![finding(root, head, fault)?],
    })
}
