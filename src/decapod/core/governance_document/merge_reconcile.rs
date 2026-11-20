//! Integrity-preserving reconciliation for Git-merged `.decapod/governance.json` journals.
//!
//! Reconciliation reads only immutable Git-committed parent records (`HEAD`,
//! `MERGE_HEAD`, and their merge base), never trusts unverified working-tree
//! hashes or mixed fields, refuses ambiguous or conflicting authority, and
//! never promotes failed, stale, or missing proof to passing.
use super::*;
use crate::core::custody::{Custody, CustodyState, IntentEvent, IntentRecord, IntentStatus};
use crate::core::trajectory::{
    TrajectoryArtifact, TrajectoryCheck, TrajectoryLoop, validate_loops,
};
use crate::plan_governance::{GovernedPlan, SpecReview};
use std::collections::{BTreeMap, BTreeSet};

/// Reconcile an in-progress Git merge of `.decapod/governance.json` from
/// immutable parent commits (`HEAD` and `MERGE_HEAD`).
pub fn reconcile_merge(root: &Path) -> Result<GovernanceDocument, DecapodError> {
    with_lock(root, || {
        crate::core::entrypoint_integrity::ensure_ungrafted(root)
            .map_err(|error| invalid(format!("GOVERNANCE_GIT_PROOF_UNAVAILABLE: {error}")))?;

        let merge_head = immutable_archive_git(root, &["rev-parse", "--verify", "MERGE_HEAD^{commit}"])
            .map_err(|_| {
                invalid(
                    "GOVERNANCE_MERGE_NOT_IN_PROGRESS: reconcile-merge requires an active Git merge (MERGE_HEAD); malformed non-merge documents are never silently repaired",
                )
            })?;
        let head = immutable_archive_git(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;

        let ours_doc = committed_governance(root, &head)?;
        let theirs_doc = committed_governance(root, &merge_head)?;
        let base_doc = resolve_merge_base_governance(root, &head, &merge_head)?;

        let mut reconciled = match (ours_doc, theirs_doc, base_doc) {
            (None, None, _) => {
                return Err(invalid(
                    "GOVERNANCE_MERGE_NO_DOCUMENT: neither HEAD nor MERGE_HEAD contains a governance document",
                ));
            }
            (Some(ours), None, Some(_)) => {
                if ours.change.is_some() {
                    return Err(invalid(
                        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: incoming merge deleted tracked governance while active workstream exists",
                    ));
                }
                ours
            }
            (Some(ours), None, None) => ours,
            (None, Some(_), Some(_)) => {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: active branch deleted tracked governance while incoming branch retains it",
                ));
            }
            (None, Some(mut theirs), None) => {
                if theirs.change.is_some()
                    && !is_accepted_on_target(
                        root,
                        &merge_head,
                        theirs.change.as_ref().map(|c| c.target_base.as_str()),
                    )
                {
                    return Err(invalid(
                        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: active branch has no governance record while incoming branch carries an unaccepted workstream",
                    ));
                }
                theirs.sections.remove("validation");
                theirs
            }
            (Some(ours), Some(theirs), base) => {
                reconcile_pair(root, &head, &merge_head, ours, theirs, base)?
            }
        };

        prune_epochs(&mut reconciled)?;
        validate_document(&reconciled)?;
        save(root, &reconciled)?;
        Ok(reconciled)
    })
}

fn committed_governance(
    root: &Path,
    commit: &str,
) -> Result<Option<GovernanceDocument>, DecapodError> {
    let entry = git(root, &["ls-tree", commit, "--", GOVERNANCE_PATH])?;
    if entry.is_empty() {
        return Ok(None);
    }
    if !entry.starts_with("100644 blob ") {
        return Err(invalid(format!(
            "GOVERNANCE_MERGE_INVALID_SOURCE: commit {commit} canonical governance must be a regular non-executable blob"
        )));
    }
    let text = git(root, &["show", &format!("{commit}:{GOVERNANCE_PATH}")])?;
    parse(text.as_bytes()).map(Some).map_err(|err| {
        invalid(format!(
            "GOVERNANCE_MERGE_INVALID_SOURCE: commit {commit}: {err}"
        ))
    })
}

fn resolve_merge_base_governance(
    root: &Path,
    head: &str,
    merge_head: &str,
) -> Result<Option<GovernanceDocument>, DecapodError> {
    let bases_raw = git(root, &["merge-base", "--all", head, merge_head]).unwrap_or_default();
    let bases: Vec<&str> = bases_raw.lines().filter(|s| !s.is_empty()).collect();
    let Some(first) = bases.first() else {
        return Ok(None);
    };
    let first_doc = committed_governance(root, first)?;
    if bases.len() > 1 {
        let first_digest = doc_digest(first_doc.as_ref())?;
        for extra in &bases[1..] {
            let extra_doc = committed_governance(root, extra)?;
            if doc_digest(extra_doc.as_ref())? != first_digest {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AMBIGUOUS_BASE: multiple merge bases disagree on governance baseline",
                ));
            }
        }
    }
    Ok(first_doc)
}

fn doc_digest(doc: Option<&GovernanceDocument>) -> Result<Option<String>, DecapodError> {
    doc.map(|d| {
        serde_json::to_value(d)
            .map_err(|e| invalid(e.to_string()))
            .and_then(|v| digest(&v))
    })
    .transpose()
}

fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    crate::core::entrypoint_integrity::history_git(
        root,
        &["merge-base", "--is-ancestor", ancestor, descendant],
    )
    .is_some_and(|out| out.status.success())
}

fn is_accepted_on_target(root: &Path, commit: &str, target_base: Option<&str>) -> bool {
    let Some(target) = target_base else {
        return false;
    };
    let norm = normalized_base_label(target);
    let candidates = [
        target.to_string(),
        format!("refs/remotes/origin/{norm}"),
        format!("refs/heads/{norm}"),
    ];
    for candidate in candidates {
        if let Ok(resolved) = immutable_archive_git(
            root,
            &["rev-parse", "--verify", &format!("{candidate}^{{commit}}")],
        ) && is_ancestor(root, commit, &resolved)
        {
            return true;
        }
    }
    false
}

fn reconcile_pair(
    root: &Path,
    head: &str,
    merge_head: &str,
    ours: GovernanceDocument,
    theirs: GovernanceDocument,
    base: Option<GovernanceDocument>,
) -> Result<GovernanceDocument, DecapodError> {
    match (&ours.change, &theirs.change) {
        (Some(c_ours), Some(c_theirs)) if c_ours.id != c_theirs.id => {
            if normalized_base_label(&c_ours.target_base)
                != normalized_base_label(&c_theirs.target_base)
            {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting target_base ('{}' vs '{}')",
                    c_ours.target_base, c_theirs.target_base
                )));
            }
            if is_accepted_on_target(root, merge_head, Some(&c_ours.target_base)) {
                return reconcile_accepted_target_merge(
                    root,
                    head,
                    merge_head,
                    ours,
                    theirs,
                    base.as_ref(),
                );
            }
            return Err(invalid(format!(
                "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: cannot merge two distinct unaccepted workstreams ('{}' vs '{}')",
                c_ours.id, c_theirs.id
            )));
        }
        (Some(c_ours), None) => {
            if base.as_ref().and_then(|b| b.change.as_ref()).is_some()
                && !is_accepted_on_target(root, merge_head, Some(&c_ours.target_base))
            {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: incoming branch dropped active PR identity",
                ));
            }
            return reconcile_accepted_target_merge(
                root,
                head,
                merge_head,
                ours,
                theirs,
                base.as_ref(),
            );
        }
        (None, Some(_)) => {
            return Err(invalid(
                "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: active branch lacks PR identity while incoming branch carries one",
            ));
        }
        (Some(c_ours), Some(c_theirs)) => {
            if c_ours.base_commit != c_theirs.base_commit
                || normalized_base_label(&c_ours.target_base)
                    != normalized_base_label(&c_theirs.target_base)
            {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting PR base or target for same change identity",
                ));
            }
        }
        (None, None) => {}
    }

    reconcile_same_workstream(ours, theirs, base)
}

/// Retain the complete active workstream journal when merging accepted target
/// history (`origin/<target_base>`), carrying forward newly introduced accepted
/// policy/unresolved obligations without overwriting active PR identity or proof.
fn reconcile_accepted_target_merge(
    root: &Path,
    head: &str,
    merge_head: &str,
    mut ours: GovernanceDocument,
    theirs: GovernanceDocument,
    base: Option<&GovernanceDocument>,
) -> Result<GovernanceDocument, DecapodError> {
    let base_policy = base.map(|b| &b.baseline.policy);
    ours.baseline.policy = merge_btreemap_3way(
        base_policy,
        &ours.baseline.policy,
        &theirs.baseline.policy,
        "baseline.policy",
    )?;
    for (key, value) in &theirs.baseline.unresolved {
        if !ours.baseline.unresolved.contains_key(key)
            && !ours.resolutions.contains_key(key)
            && !base.is_some_and(|b| b.baseline.unresolved.contains_key(key))
        {
            ours.baseline.unresolved.insert(key.clone(), value.clone());
        }
    }
    if git(root, &["rev-parse", &format!("{head}^{{tree}}")])?
        != git(root, &["rev-parse", &format!("{merge_head}^{{tree}}")])?
    {
        ours.sections.remove("validation");
    }
    Ok(ours)
}

fn reconcile_same_workstream(
    ours: GovernanceDocument,
    theirs: GovernanceDocument,
    base: Option<GovernanceDocument>,
) -> Result<GovernanceDocument, DecapodError> {
    let base_ref = base.as_ref();
    let resolutions = merge_btreemap_3way(
        base_ref.map(|b| &b.resolutions),
        &ours.resolutions,
        &theirs.resolutions,
        "resolutions",
    )?;
    let baseline = merge_baseline(
        base_ref.map(|b| &b.baseline),
        &ours.baseline,
        &theirs.baseline,
        &resolutions,
    )?;
    let claims = merge_claims_3way(base_ref.map(|b| &b.claims), &ours.claims, &theirs.claims)?;
    let legacy_claims = merge_option_3way(
        base_ref.and_then(|b| b.legacy_claims.as_ref()),
        ours.legacy_claims.as_ref(),
        theirs.legacy_claims.as_ref(),
        "legacy_claims",
    )?;
    let legacy_digests = merge_btreemap_3way(
        base_ref.map(|b| &b.legacy_digests),
        &ours.legacy_digests,
        &theirs.legacy_digests,
        "legacy_digests",
    )?;
    let recursive_passes = merge_btreemap_3way(
        base_ref.map(|b| &b.recursive_passes),
        &ours.recursive_passes,
        &theirs.recursive_passes,
        "recursive_passes",
    )?;
    let epochs = merge_btreemap_3way(
        base_ref.map(|b| &b.epochs),
        &ours.epochs,
        &theirs.epochs,
        "epochs",
    )?;
    let checkpoints = merge_checkpoints(
        base_ref.map(|b| b.checkpoints.as_slice()),
        &ours.checkpoints,
        &theirs.checkpoints,
    )?;

    let merged_plan = merge_plan_section(
        base_ref
            .map(|b| get_section(b, "plan"))
            .transpose()?
            .flatten(),
        get_section(&ours, "plan")?,
        get_section(&theirs, "plan")?,
    )?;

    let ours_traj = get_section(&ours, "trajectory")?;
    let theirs_traj = get_section(&theirs, "trajectory")?;
    let base_traj = base_ref
        .map(|b| get_section(b, "trajectory"))
        .transpose()?
        .flatten();
    let merged_traj = merge_trajectory_section(base_traj, ours_traj.clone(), theirs_traj)?;

    let merged_jev = merge_option_3way(
        base_ref.and_then(|b| b.sections.get("jev")),
        ours.sections.get("jev"),
        theirs.sections.get("jev"),
        "sections.jev",
    )?;

    let mut merged = GovernanceDocument {
        schema_version: SCHEMA_VERSION.into(),
        baseline,
        change: ours.change.clone(),
        claims,
        resolutions,
        checkpoints,
        intents: BTreeMap::new(),
        epochs,
        sections: BTreeMap::new(),
        recursive_passes,
        legacy_claims,
        legacy_digests,
    };

    if let Some(plan) = merged_plan {
        set_section(&mut merged, "plan", plan)?;
    }
    if let Some(traj) = &merged_traj {
        set_section(&mut merged, "trajectory", traj.clone())?;
    }
    if let Some(jev) = merged_jev {
        set_section(&mut merged, "jev", jev)?;
    }

    // Never keep a stale or cross-branch validation receipt: if trajectory changed
    // or validation receipts differ between branches, clear validation so proof
    // must be re-established against the merged candidate.
    if ours_traj == merged_traj
        && ours.sections.get("validation") == theirs.sections.get("validation")
        && let Some(validation) = ours.sections.get("validation")
    {
        merged
            .sections
            .insert("validation".into(), validation.clone());
    }

    Ok(merged)
}

fn merge_option_3way<T: Clone + PartialEq>(
    base: Option<&T>,
    ours: Option<&T>,
    theirs: Option<&T>,
    field: &str,
) -> Result<Option<T>, DecapodError> {
    if ours == theirs {
        return Ok(ours.cloned());
    }
    if base.is_some_and(|b| ours == Some(b)) || (base.is_none() && ours.is_none()) {
        return Ok(theirs.cloned());
    }
    if base.is_some_and(|b| theirs == Some(b)) || (base.is_none() && theirs.is_none()) {
        return Ok(ours.cloned());
    }
    Err(invalid(format!(
        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting edits to '{field}'"
    )))
}

fn merge_scalar_3way<T: Clone + PartialEq>(
    base: Option<&T>,
    ours: &T,
    theirs: &T,
    field: &str,
) -> Result<T, DecapodError> {
    if ours == theirs {
        return Ok(ours.clone());
    }
    if base == Some(ours) {
        return Ok(theirs.clone());
    }
    if base == Some(theirs) {
        return Ok(ours.clone());
    }
    Err(invalid(format!(
        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting edits to '{field}'"
    )))
}

fn merge_btreemap_3way<V: Clone + PartialEq>(
    base: Option<&BTreeMap<String, V>>,
    ours: &BTreeMap<String, V>,
    theirs: &BTreeMap<String, V>,
    field: &str,
) -> Result<BTreeMap<String, V>, DecapodError> {
    let mut keys = BTreeSet::new();
    keys.extend(ours.keys().cloned());
    keys.extend(theirs.keys().cloned());
    if let Some(b) = base {
        keys.extend(b.keys().cloned());
    }
    let mut out = BTreeMap::new();
    for key in keys {
        let b_val = base.and_then(|m| m.get(&key));
        let o_val = ours.get(&key);
        let t_val = theirs.get(&key);
        if let Some(val) = merge_option_3way(b_val, o_val, t_val, &format!("{field}.{key}"))? {
            out.insert(key, val);
        }
    }
    Ok(out)
}

fn merge_claims_3way(
    base: Option<&BTreeMap<String, ActiveClaim>>,
    ours: &BTreeMap<String, ActiveClaim>,
    theirs: &BTreeMap<String, ActiveClaim>,
) -> Result<BTreeMap<String, ActiveClaim>, DecapodError> {
    let mut keys = BTreeSet::new();
    keys.extend(ours.keys().cloned());
    keys.extend(theirs.keys().cloned());
    if let Some(b) = base {
        keys.extend(b.keys().cloned());
    }
    let mut out = BTreeMap::new();
    for key in keys {
        let b_val = base.and_then(|m| m.get(&key));
        let o_val = ours.get(&key);
        let t_val = theirs.get(&key);
        let merged = match (o_val, t_val) {
            (Some(o), Some(t)) if o == t => Some(o.clone()),
            (Some(o), Some(t)) => {
                if b_val == Some(o) {
                    Some(t.clone())
                } else if b_val == Some(t) {
                    Some(o.clone())
                } else {
                    return Err(invalid(format!(
                        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting claim '{key}'"
                    )));
                }
            }
            _ => merge_option_3way(b_val, o_val, t_val, &format!("claims.{key}"))?,
        };
        if let Some(claim) = merged {
            out.insert(key, claim);
        }
    }
    Ok(out)
}

impl PartialEq for ActiveClaim {
    fn eq(&self, other: &Self) -> bool {
        self.statement == other.statement
            && self.falsifier == other.falsifier
            && self.status == other.status
            && self.proof_refs == other.proof_refs
    }
}

impl PartialEq for ObligationResolution {
    fn eq(&self, other: &Self) -> bool {
        self.obligation_digest == other.obligation_digest
            && self.resolution == other.resolution
            && self.proof_refs == other.proof_refs
    }
}

fn merge_baseline(
    base: Option<&Baseline>,
    ours: &Baseline,
    theirs: &Baseline,
    merged_resolutions: &BTreeMap<String, ObligationResolution>,
) -> Result<Baseline, DecapodError> {
    let accepted_commit = merge_option_3way(
        base.and_then(|b| b.accepted_commit.as_ref()),
        ours.accepted_commit.as_ref(),
        theirs.accepted_commit.as_ref(),
        "baseline.accepted_commit",
    )?;
    let release = merge_option_3way(
        base.and_then(|b| b.release.as_ref()),
        ours.release.as_ref(),
        theirs.release.as_ref(),
        "baseline.release",
    )?;
    let policy = merge_btreemap_3way(
        base.map(|b| &b.policy),
        &ours.policy,
        &theirs.policy,
        "baseline.policy",
    )?;

    let mut keys = BTreeSet::new();
    keys.extend(ours.unresolved.keys().cloned());
    keys.extend(theirs.unresolved.keys().cloned());
    if let Some(b) = base {
        keys.extend(b.unresolved.keys().cloned());
    }
    let mut unresolved = BTreeMap::new();
    for key in keys {
        let b_val = base.and_then(|m| m.unresolved.get(&key));
        let o_val = ours.unresolved.get(&key);
        let t_val = theirs.unresolved.get(&key);
        if merged_resolutions.contains_key(&key) {
            // Resolved cleanly on one side while unchanged (or resolved identically) on the other.
            if let (Some(b), Some(o)) = (b_val, o_val)
                && o != b
            {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: obligation '{key}' modified on one branch and resolved on the other"
                )));
            }
            if let (Some(b), Some(t)) = (b_val, t_val)
                && t != b
            {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: obligation '{key}' modified on one branch and resolved on the other"
                )));
            }
            continue;
        }
        if let Some(val) =
            merge_option_3way(b_val, o_val, t_val, &format!("baseline.unresolved.{key}"))?
        {
            unresolved.insert(key, val);
        }
    }

    Ok(Baseline {
        accepted_commit,
        release,
        policy,
        unresolved,
    })
}

fn normalized_refs(refs: &[String]) -> Vec<String> {
    let mut sorted = refs.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
}

fn checkpoints_equivalent(a: &Checkpoint, b: &Checkpoint) -> bool {
    a.id == b.id
        && a.summary == b.summary
        && a.input_digest == b.input_digest
        && normalized_refs(&a.proof_refs) == normalized_refs(&b.proof_refs)
}

fn merge_checkpoints(
    base: Option<&[Checkpoint]>,
    ours: &[Checkpoint],
    theirs: &[Checkpoint],
) -> Result<Vec<Checkpoint>, DecapodError> {
    if let Some(base_cps) = base {
        for old in base_cps {
            if !ours.iter().any(|cp| checkpoints_equivalent(cp, old))
                || !theirs.iter().any(|cp| checkpoints_equivalent(cp, old))
            {
                return Err(invalid(format!(
                    "GOVERNANCE_CHECKPOINT_HISTORY_CHANGED: checkpoint '{}' was erased or rewritten before merge",
                    old.id
                )));
            }
        }
    }
    let mut merged = ours.to_vec();
    for cp in theirs {
        if let Some(existing) = merged.iter().find(|entry| entry.id == cp.id) {
            if !checkpoints_equivalent(existing, cp) {
                return Err(invalid(format!(
                    "GOVERNANCE_CHECKPOINT_ID_CONFLICT: checkpoint '{}' binds different material, summary, or proof references across merged branches",
                    cp.id
                )));
            }
        } else {
            merged.push(cp.clone());
        }
    }
    Ok(merged)
}

fn merge_string_set_3way(
    base: Option<&[String]>,
    ours: &[String],
    theirs: &[String],
) -> Vec<String> {
    let base_set: BTreeSet<&str> = base.unwrap_or(&[]).iter().map(String::as_str).collect();
    let ours_set: BTreeSet<&str> = ours.iter().map(String::as_str).collect();
    let theirs_set: BTreeSet<&str> = theirs.iter().map(String::as_str).collect();

    let mut out = BTreeSet::new();
    for item in ours_set.intersection(&theirs_set) {
        out.insert((*item).to_string());
    }
    for item in ours_set.difference(&base_set) {
        out.insert((*item).to_string());
    }
    for item in theirs_set.difference(&base_set) {
        out.insert((*item).to_string());
    }
    out.into_iter().collect()
}

fn merge_string_union(ours: &[String], theirs: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    out.extend(ours.iter().cloned());
    out.extend(theirs.iter().cloned());
    out.into_iter().collect()
}

fn merge_plan_section(
    base: Option<Value>,
    ours: Option<Value>,
    theirs: Option<Value>,
) -> Result<Option<Value>, DecapodError> {
    match (ours, theirs) {
        (None, None) => Ok(None),
        (Some(o), None) => {
            if base.is_some() {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: incoming branch deleted governed plan",
                ));
            }
            Ok(Some(o))
        }
        (None, Some(t)) => {
            if base.is_some() {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: active branch deleted governed plan",
                ));
            }
            Ok(Some(t))
        }
        (Some(o_val), Some(t_val)) => {
            if o_val == t_val {
                return Ok(Some(o_val));
            }
            let o: GovernedPlan =
                serde_json::from_value(o_val).map_err(|e| invalid(e.to_string()))?;
            let t: GovernedPlan =
                serde_json::from_value(t_val).map_err(|e| invalid(e.to_string()))?;
            let b: Option<GovernedPlan> = base
                .map(|v| serde_json::from_value(v).map_err(|e| invalid(e.to_string())))
                .transpose()?;
            let b_ref = b.as_ref();

            let title =
                merge_scalar_3way(b_ref.map(|p| &p.title), &o.title, &t.title, "plan.title")?;
            let intent = merge_scalar_3way(
                b_ref.map(|p| &p.intent),
                &o.intent,
                &t.intent,
                "plan.intent",
            )?;
            let state =
                merge_scalar_3way(b_ref.map(|p| &p.state), &o.state, &t.state, "plan.state")?;
            let constraints = merge_scalar_3way(
                b_ref.map(|p| &p.constraints),
                &o.constraints,
                &t.constraints,
                "plan.constraints",
            )?;
            let phases = merge_scalar_3way(
                b_ref.map(|p| &p.phases),
                &o.phases,
                &t.phases,
                "plan.phases",
            )?;
            let reflection = merge_option_3way(
                b_ref.and_then(|p| p.reflection.as_ref()),
                o.reflection.as_ref(),
                t.reflection.as_ref(),
                "plan.reflection",
            )?;

            let spec_reviews = merge_spec_reviews(
                b_ref.map(|p| p.spec_reviews.as_slice()),
                &o.spec_reviews,
                &t.spec_reviews,
            )?;

            let merged = GovernedPlan {
                schema_version: if o.schema_version == "1.1.0" || t.schema_version == "1.1.0" {
                    "1.1.0".into()
                } else {
                    o.schema_version
                },
                title,
                intent,
                state,
                todo_ids: merge_string_union(&o.todo_ids, &t.todo_ids),
                proof_hooks: merge_string_union(&o.proof_hooks, &t.proof_hooks),
                unknowns: merge_string_set_3way(
                    b_ref.map(|p| p.unknowns.as_slice()),
                    &o.unknowns,
                    &t.unknowns,
                ),
                human_questions: merge_string_set_3way(
                    b_ref.map(|p| p.human_questions.as_slice()),
                    &o.human_questions,
                    &t.human_questions,
                ),
                stop_conditions: merge_string_union(&o.stop_conditions, &t.stop_conditions),
                unresolved_contradictions: merge_string_set_3way(
                    b_ref.map(|p| p.unresolved_contradictions.as_slice()),
                    &o.unresolved_contradictions,
                    &t.unresolved_contradictions,
                ),
                deferred_questions: merge_string_set_3way(
                    b_ref.map(|p| p.deferred_questions.as_slice()),
                    &o.deferred_questions,
                    &t.deferred_questions,
                ),
                constraints,
                phases,
                updated_at: o.updated_at.max(t.updated_at),
                spec_reviews,
                reflection,
            };
            serde_json::to_value(merged)
                .map(Some)
                .map_err(|e| invalid(e.to_string()))
        }
    }
}

fn merge_spec_reviews(
    base: Option<&[SpecReview]>,
    ours: &[SpecReview],
    theirs: &[SpecReview],
) -> Result<Vec<SpecReview>, DecapodError> {
    let base_map: BTreeMap<&str, &SpecReview> = base
        .unwrap_or(&[])
        .iter()
        .map(|r| (r.path.as_str(), r))
        .collect();
    let ours_map: BTreeMap<&str, &SpecReview> = ours.iter().map(|r| (r.path.as_str(), r)).collect();
    let theirs_map: BTreeMap<&str, &SpecReview> =
        theirs.iter().map(|r| (r.path.as_str(), r)).collect();
    let mut keys = BTreeSet::new();
    keys.extend(ours_map.keys().copied());
    keys.extend(theirs_map.keys().copied());
    keys.extend(base_map.keys().copied());

    let mut out = Vec::new();
    for path in keys {
        let b_val = base_map.get(path).copied().cloned();
        let o_val = ours_map.get(path).copied().cloned();
        let t_val = theirs_map.get(path).copied().cloned();
        if let Some(review) = merge_option_3way(
            b_val.as_ref(),
            o_val.as_ref(),
            t_val.as_ref(),
            &format!("plan.spec_reviews.{path}"),
        )? {
            out.push(review);
        }
    }
    Ok(out)
}

fn merge_trajectory_section(
    base: Option<Value>,
    ours: Option<Value>,
    theirs: Option<Value>,
) -> Result<Option<Value>, DecapodError> {
    match (ours, theirs) {
        (None, None) => Ok(None),
        (Some(o), None) => {
            if base.is_some() {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: incoming branch deleted trajectory journal",
                ));
            }
            let art: TrajectoryArtifact =
                serde_json::from_value(o).map_err(|e| invalid(e.to_string()))?;
            let recomputed = art
                .with_recomputed_hash()
                .map_err(|e| invalid(e.to_string()))?;
            serde_json::to_value(recomputed)
                .map(Some)
                .map_err(|e| invalid(e.to_string()))
        }
        (None, Some(t)) => {
            if base.is_some() {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: active branch deleted trajectory journal",
                ));
            }
            let art: TrajectoryArtifact =
                serde_json::from_value(t).map_err(|e| invalid(e.to_string()))?;
            let recomputed = art
                .with_recomputed_hash()
                .map_err(|e| invalid(e.to_string()))?;
            serde_json::to_value(recomputed)
                .map(Some)
                .map_err(|e| invalid(e.to_string()))
        }
        (Some(o_val), Some(t_val)) => {
            let o: TrajectoryArtifact =
                serde_json::from_value(o_val).map_err(|e| invalid(e.to_string()))?;
            let t: TrajectoryArtifact =
                serde_json::from_value(t_val).map_err(|e| invalid(e.to_string()))?;
            let b: Option<TrajectoryArtifact> = base
                .map(|v| serde_json::from_value(v).map_err(|e| invalid(e.to_string())))
                .transpose()?;
            let b_ref = b.as_ref();

            if o.run_id != t.run_id {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting trajectory run_id ('{}' vs '{}')",
                    o.run_id, t.run_id
                )));
            }
            if o.intent_id != t.intent_id {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting trajectory intent_id ({:?} vs {:?})",
                    o.intent_id, t.intent_id
                )));
            }

            let task_id = merge_option_3way(
                b_ref.and_then(|x| x.task_id.as_ref()),
                o.task_id.as_ref(),
                t.task_id.as_ref(),
                "trajectory.task_id",
            )?;
            let original_intent = merge_scalar_3way(
                b_ref.map(|x| &x.original_intent),
                &o.original_intent,
                &t.original_intent,
                "trajectory.original_intent",
            )?;
            let derived_intent = merge_scalar_3way(
                b_ref.map(|x| &x.derived_intent),
                &o.derived_intent,
                &t.derived_intent,
                "trajectory.derived_intent",
            )?;
            let destination = merge_option_3way(
                b_ref.and_then(|x| x.destination.as_ref()),
                o.destination.as_ref(),
                t.destination.as_ref(),
                "trajectory.destination",
            )?;
            let current_phase = merge_option_3way(
                b_ref.and_then(|x| x.current_phase.as_ref()),
                o.current_phase.as_ref(),
                t.current_phase.as_ref(),
                "trajectory.current_phase",
            )?;
            let completion_claim = merge_option_3way(
                b_ref.and_then(|x| x.completion_claim.as_ref()),
                o.completion_claim.as_ref(),
                t.completion_claim.as_ref(),
                "trajectory.completion_claim",
            )?;

            let checks = merge_checks(b_ref.map(|x| x.checks.as_slice()), &o.checks, &t.checks)?;

            let (custody, event_remap) =
                merge_custody_state(b_ref.map(|x| &x.custody), &o.custody, &t.custody, &o.run_id)?;

            let loops = merge_loops(&o.loops, &t.loops, &event_remap)?;
            let execution_checkpoints =
                merge_execution_checkpoints(&o.execution_checkpoints, &t.execution_checkpoints)?;

            let mut merged = TrajectoryArtifact {
                schema_version: if o.schema_version == "1.1.0" || t.schema_version == "1.1.0" {
                    "1.1.0".into()
                } else {
                    o.schema_version
                },
                run_id: o.run_id,
                intent_id: o.intent_id,
                task_id,
                original_intent,
                derived_intent,
                destination,
                current_phase,
                next_transitions: merge_string_set_3way(
                    b_ref.map(|x| x.next_transitions.as_slice()),
                    &o.next_transitions,
                    &t.next_transitions,
                ),
                blockers: merge_string_set_3way(
                    b_ref.map(|x| x.blockers.as_slice()),
                    &o.blockers,
                    &t.blockers,
                ),
                active_boundaries: merge_string_union(&o.active_boundaries, &t.active_boundaries),
                repo_scope: merge_string_union(&o.repo_scope, &t.repo_scope),
                inspected_files: merge_string_union(&o.inspected_files, &t.inspected_files),
                modified_files: merge_string_union(&o.modified_files, &t.modified_files),
                declared_commands: merge_string_union(&o.declared_commands, &t.declared_commands),
                tool_calls: merge_string_union(&o.tool_calls, &t.tool_calls),
                loops,
                checks,
                evidence: merge_string_union(&o.evidence, &t.evidence),
                shortcut_risk_signals: merge_string_union(
                    &o.shortcut_risk_signals,
                    &t.shortcut_risk_signals,
                ),
                unresolved_assumptions: merge_string_union(
                    &o.unresolved_assumptions,
                    &t.unresolved_assumptions,
                ),
                completion_claim,
                proof_status: o.proof_status,
                verdicts: o.verdicts,
                artifact_hash: String::new(),
                custody,
                execution_checkpoints,
            };

            validate_loops(&merged)?;
            merged = merged
                .with_recomputed_hash()
                .map_err(|e| invalid(e.to_string()))?;

            serde_json::to_value(merged)
                .map(Some)
                .map_err(|e| invalid(e.to_string()))
        }
    }
}

fn merge_checks(
    base: Option<&[TrajectoryCheck]>,
    ours: &[TrajectoryCheck],
    theirs: &[TrajectoryCheck],
) -> Result<Vec<TrajectoryCheck>, DecapodError> {
    let base_map: BTreeMap<&str, &TrajectoryCheck> = base
        .unwrap_or(&[])
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let ours_map: BTreeMap<&str, &TrajectoryCheck> =
        ours.iter().map(|c| (c.name.as_str(), c)).collect();
    let theirs_map: BTreeMap<&str, &TrajectoryCheck> =
        theirs.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut names = BTreeSet::new();
    names.extend(base_map.keys().copied());
    names.extend(ours_map.keys().copied());
    names.extend(theirs_map.keys().copied());

    let mut out = Vec::new();
    for name in names {
        match (ours_map.get(name), theirs_map.get(name)) {
            (Some(o), Some(t)) if o.status == t.status => out.push((*o).clone()),
            (Some(o), Some(t)) => {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_PROOF_CONFLICT: check '{name}' has conflicting statuses ({:?} vs {:?}) across merged branches",
                    o.status, t.status
                )));
            }
            (Some(o), None) => {
                if base_map.contains_key(name) {
                    return Err(invalid(format!(
                        "GOVERNANCE_MERGE_PROOF_CONFLICT: check '{name}' was removed on incoming branch"
                    )));
                }
                out.push((*o).clone());
            }
            (None, Some(t)) => {
                if base_map.contains_key(name) {
                    return Err(invalid(format!(
                        "GOVERNANCE_MERGE_PROOF_CONFLICT: check '{name}' was removed on active branch"
                    )));
                }
                out.push((*t).clone());
            }
            (None, None) => {}
        }
    }
    Ok(out)
}

fn loop_semantic_equal(a: &TrajectoryLoop, b: &TrajectoryLoop) -> bool {
    let mut norm_a = a.clone();
    let mut norm_b = b.clone();
    norm_a.custody_event_id = None;
    norm_b.custody_event_id = None;
    for item in [&mut norm_a, &mut norm_b] {
        item.tool_calls.sort();
        item.tool_calls.dedup();
        item.observations.sort();
        item.observations.dedup();
        item.proof_refs.sort();
        item.proof_refs.dedup();
    }
    norm_a == norm_b
}

fn merge_loops(
    ours: &[TrajectoryLoop],
    theirs: &[TrajectoryLoop],
    event_remap: &BTreeMap<String, String>,
) -> Result<Vec<TrajectoryLoop>, DecapodError> {
    let mut map: BTreeMap<(String, u32), TrajectoryLoop> = BTreeMap::new();
    for item in ours {
        map.insert((item.loop_id.clone(), item.attempt), item.clone());
    }
    for item in theirs {
        let key = (item.loop_id.clone(), item.attempt);
        if let Some(existing) = map.get(&key) {
            if !loop_semantic_equal(existing, item) {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_CONFLICT: conflicting trajectory loop '{}' attempt {}",
                    item.loop_id, item.attempt
                )));
            }
        } else {
            let mut remapped = item.clone();
            if let Some(old_ev) = &remapped.custody_event_id
                && let Some(new_ev) = event_remap.get(old_ev)
            {
                remapped.custody_event_id = Some(new_ev.clone());
            }
            map.insert(key, remapped);
        }
    }
    Ok(map.into_values().collect())
}

fn merge_execution_checkpoints(
    ours: &[crate::core::trajectory::ExecutionCheckpoint],
    theirs: &[crate::core::trajectory::ExecutionCheckpoint],
) -> Result<Vec<crate::core::trajectory::ExecutionCheckpoint>, DecapodError> {
    let min_len = ours.len().min(theirs.len());
    if ours[..min_len] != theirs[..min_len] {
        return Err(invalid(
            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: diverging trajectory execution checkpoints across merged branches",
        ));
    }
    if theirs.len() > ours.len() {
        Ok(theirs.to_vec())
    } else {
        Ok(ours.to_vec())
    }
}

fn common_event_prefix(a: &[IntentEvent], b: &[IntentEvent]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn merge_intent_record_3way(
    base: Option<&IntentRecord>,
    ours: &IntentRecord,
    theirs: &IntentRecord,
    id: &str,
) -> Result<IntentRecord, DecapodError> {
    if ours.raw_intent != theirs.raw_intent
        || ours.instruction_origin != theirs.instruction_origin
        || base.is_some_and(|record| {
            record.raw_intent != ours.raw_intent
                || record.instruction_origin != ours.instruction_origin
        })
    {
        return Err(invalid(format!(
            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: original instruction for '{id}' changed"
        )));
    }
    if base.is_some_and(|record| {
        !ours
            .instruction_revisions
            .starts_with(&record.instruction_revisions)
            || !theirs
                .instruction_revisions
                .starts_with(&record.instruction_revisions)
    }) {
        return Err(invalid(format!(
            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: instruction history for '{id}' diverged from common base"
        )));
    }
    let instruction_revisions = merge_scalar_3way(
        base.map(|record| &record.instruction_revisions),
        &ours.instruction_revisions,
        &theirs.instruction_revisions,
        &format!("custody.intents.{id}.instruction_revisions"),
    )?;
    let refined_intent = merge_option_3way(
        base.and_then(|x| x.refined_intent.as_ref()),
        ours.refined_intent.as_ref(),
        theirs.refined_intent.as_ref(),
        &format!("custody.intents.{id}.refined_intent"),
    )?;
    let status = merge_intent_status(base.map(|x| &x.status), &ours.status, &theirs.status, id)?;
    let merged = IntentRecord {
        id: ours.id.clone(),
        raw_intent: ours.raw_intent.clone(),
        instruction_origin: ours.instruction_origin.clone(),
        instruction_revisions,
        refined_intent,
        acceptance_criteria: merge_string_union(
            &ours.acceptance_criteria,
            &theirs.acceptance_criteria,
        ),
        constraints: merge_string_union(&ours.constraints, &theirs.constraints),
        assumptions: merge_string_union(&ours.assumptions, &theirs.assumptions),
        boundaries: merge_string_union(&ours.boundaries, &theirs.boundaries),
        out_of_scope: merge_string_union(&ours.out_of_scope, &theirs.out_of_scope),
        proof_requirements: merge_string_union(
            &ours.proof_requirements,
            &theirs.proof_requirements,
        ),
        status,
        created_sequence: ours.created_sequence.min(theirs.created_sequence),
        updated_sequence: ours.updated_sequence.max(theirs.updated_sequence),
    };
    merged.validate_instruction_history().map_err(|err| {
        invalid(format!(
            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: invalid merged instructions for '{id}': {err}"
        ))
    })?;
    Ok(merged)
}

fn merge_intent_status(
    base: Option<&IntentStatus>,
    ours: &IntentStatus,
    theirs: &IntentStatus,
    id: &str,
) -> Result<IntentStatus, DecapodError> {
    if ours == theirs {
        return Ok(ours.clone());
    }
    if ours.is_terminal() && theirs.is_terminal() {
        return Err(invalid(format!(
            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting terminal intent statuses for '{id}' ({ours:?} vs {theirs:?})"
        )));
    }
    if base == Some(ours) {
        return Ok(theirs.clone());
    }
    if base == Some(theirs) {
        return Ok(ours.clone());
    }
    Err(invalid(format!(
        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting intent status transitions for '{id}' ({ours:?} vs {theirs:?})"
    )))
}

fn merge_custody_state(
    base: Option<&CustodyState>,
    ours: &CustodyState,
    theirs: &CustodyState,
    run_id: &str,
) -> Result<(CustodyState, BTreeMap<String, String>), DecapodError> {
    if ours == theirs {
        return Ok((ours.clone(), BTreeMap::new()));
    }
    if ours.is_empty() {
        return Ok((theirs.clone(), BTreeMap::new()));
    }
    if theirs.is_empty() {
        return Ok((ours.clone(), BTreeMap::new()));
    }

    let mut intents = BTreeMap::new();
    let mut intent_ids = BTreeSet::new();
    intent_ids.extend(ours.intents.keys().cloned());
    intent_ids.extend(theirs.intents.keys().cloned());
    if let Some(b) = base {
        intent_ids.extend(b.intents.keys().cloned());
    }
    for id in intent_ids {
        let b_rec = base.and_then(|s| s.intents.get(&id));
        let o_rec = ours.intents.get(&id);
        let t_rec = theirs.intents.get(&id);
        let merged_rec = match (o_rec, t_rec) {
            (Some(o), Some(t)) => Some(merge_intent_record_3way(b_rec, o, t, &id)?),
            _ => merge_option_3way(b_rec, o_rec, t_rec, &format!("custody.intents.{id}"))?,
        };
        if let Some(rec) = merged_rec {
            intents.insert(id, rec);
        }
    }

    let prefix_len = match base {
        Some(b) => {
            if !ours.events.starts_with(&b.events) || !theirs.events.starts_with(&b.events) {
                return Err(invalid(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: custody event history diverged from common base",
                ));
            }
            b.events.len()
        }
        None => common_event_prefix(&ours.events, &theirs.events),
    };

    let mut events = ours.events.clone();
    let mut next_seq = ours
        .next_sequence
        .max(events.iter().map(|e| e.sequence).max().unwrap_or(0));
    let mut remap = BTreeMap::new();

    for ev in &theirs.events[prefix_len..] {
        next_seq = next_seq.saturating_add(1);
        let new_id = format!("event:{next_seq}");
        remap.insert(ev.event_id.clone(), new_id.clone());
        events.push(IntentEvent {
            sequence: next_seq,
            event_id: new_id,
            intent_id: ev.intent_id.clone(),
            kind: ev.kind.clone(),
            detail: ev.detail.clone(),
        });
    }

    let mut trajectories = ours.trajectories.clone();
    for (traj_id, t_traj) in &theirs.trajectories {
        let o_traj = trajectories.get(traj_id);
        let base_steps_len = base
            .and_then(|b| b.trajectories.get(traj_id))
            .map_or(0, |bt| bt.steps.len());
        match o_traj {
            None => {
                let mut cloned = t_traj.clone();
                for step in &mut cloned.steps {
                    if let Some(new_ev) = remap.get(&step.custody_event_id) {
                        step.custody_event_id = new_ev.clone();
                        if let Some(seq_num) = new_ev
                            .strip_prefix("event:")
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            step.sequence = seq_num;
                        }
                    }
                }
                trajectories.insert(traj_id.clone(), cloned);
            }
            Some(existing) => {
                if existing.intent_id != t_traj.intent_id {
                    return Err(invalid(format!(
                        "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: custody trajectory '{traj_id}' intent mismatch"
                    )));
                }
                let start = if base.is_some() {
                    if existing.steps.len() < base_steps_len || t_traj.steps.len() < base_steps_len
                    {
                        return Err(invalid(
                            "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: custody trajectory steps were truncated",
                        ));
                    }
                    base_steps_len
                } else {
                    existing
                        .steps
                        .iter()
                        .zip(&t_traj.steps)
                        .take_while(|(a, b)| a == b)
                        .count()
                };
                let target = trajectories.get_mut(traj_id).expect("checked above");
                for step in &t_traj.steps[start..] {
                    let mut new_step = step.clone();
                    if let Some(new_ev) = remap.get(&step.custody_event_id) {
                        new_step.custody_event_id = new_ev.clone();
                        if let Some(seq_num) = new_ev
                            .strip_prefix("event:")
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            new_step.sequence = seq_num;
                        }
                    }
                    target.steps.push(new_step);
                }
            }
        }
    }

    let mut candidates = ours.knowledge_candidates.clone();
    for cand in &theirs.knowledge_candidates {
        if let Some(existing) = candidates.iter().find(|c| c.id == cand.id) {
            if existing != cand {
                return Err(invalid(format!(
                    "GOVERNANCE_MERGE_AUTHORITY_CONFLICT: conflicting knowledge candidate '{}'",
                    cand.id
                )));
            }
        } else {
            candidates.push(cand.clone());
        }
    }

    for rec in intents.values_mut() {
        let max_ev = events
            .iter()
            .filter(|e| e.intent_id == rec.id)
            .map(|e| e.sequence)
            .max()
            .unwrap_or(rec.updated_sequence);
        rec.updated_sequence = rec.updated_sequence.max(max_ev);
    }

    let _ = run_id;
    let mut custody = Custody::from_state(CustodyState {
        schema_version: if !ours.schema_version.is_empty() {
            ours.schema_version.clone()
        } else {
            theirs.schema_version.clone()
        },
        intents,
        events,
        trajectories,
        summaries: BTreeMap::new(),
        knowledge_candidates: candidates,
        next_sequence: next_seq,
    });
    let all_intent_ids: Vec<String> = custody.state().intents.keys().cloned().collect();
    for id in all_intent_ids {
        custody
            .summarize(&id)
            .map_err(|e| invalid(format!("GOVERNANCE_MERGE_INVALID_CUSTODY: {e}")))?;
    }
    Ok((custody.into_state(), remap))
}

#[cfg(test)]
mod instruction_merge_tests {
    use super::*;
    use crate::core::custody::{
        InstructionRevisionInput, RecordedInstructionKind, ReportedInstructionSource,
    };

    fn append(base: &CustodyState, revision_id: &str) -> IntentRecord {
        let mut custody = Custody::from_state(base.clone());
        custody
            .append_instruction(
                "intent",
                InstructionRevisionInput {
                    revision_id: revision_id.into(),
                    expected_revision: base.intents["intent"].instruction_revision().unwrap(),
                    instruction: "A reported clarification".into(),
                    source_claim: ReportedInstructionSource {
                        source_ref: format!("fixture:{revision_id}"),
                        principal_claim: None,
                    },
                    kind: RecordedInstructionKind::Clarification,
                    supersedes: Vec::new(),
                    derived_intent: Some("A clarified interpretation".into()),
                },
            )
            .unwrap();
        custody.into_state().intents.remove("intent").unwrap()
    }

    #[test]
    fn retains_an_appended_instruction_from_one_parent() {
        let mut custody = Custody::in_memory();
        custody
            .create_intent("intent", "Original request".into())
            .unwrap();
        let base = custody.into_state();
        let original = &base.intents["intent"];
        let incoming = append(&base, "incoming");
        let merged =
            merge_intent_record_3way(Some(original), original, &incoming, "intent").unwrap();
        assert_eq!(
            merged.instruction_lineage().unwrap(),
            incoming.instruction_lineage().unwrap()
        );
    }

    #[test]
    fn refuses_divergent_rewritten_or_truncated_instruction_history() {
        let mut custody = Custody::in_memory();
        custody
            .create_intent("intent", "Original request".into())
            .unwrap();
        let base = custody.into_state();
        let original = &base.intents["intent"];
        let left = append(&base, "left");
        let right = append(&base, "right");
        let mut rewritten = left.clone();
        rewritten.instruction_origin = None;
        for (base, ours, theirs) in [
            (original, &left, &right),
            (original, &left, &rewritten),
            (&left, &left, original),
        ] {
            let error = merge_intent_record_3way(Some(base), ours, theirs, "intent").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("GOVERNANCE_MERGE_AUTHORITY_CONFLICT")
            );
        }
    }
}
