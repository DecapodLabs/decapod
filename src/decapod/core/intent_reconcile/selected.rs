//! Bounded read model of an explicitly selected, accepted policy and canonical
//! obligations. This never infers requirements from prose or accepts a candidate
//! author's policy rewrite. Mapping, satisfiability and executed proof differ.
use crate::core::{
    error::DecapodError,
    obligation,
    store::Store,
    validation_rules::{EvaluationPhase, RuleAuthority, RuleSeverity, RuleVerdict},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectedWork {
    pub schema_version: String,
    pub id: String,
    pub intent_id: String,
    pub instruction_revision: String,
    pub roots: Vec<RootRequirement>,
    #[serde(default)]
    pub relations: Vec<Relation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RootRequirement {
    pub id: String,
    /// Existing accepted required native check names, not agent proof assertions.
    pub checks: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Relation {
    pub id: String,
    pub requirement: String,
    pub left: String,
    pub right: String,
    /// Initially only eq/ne over uninterpreted values. Unknown kinds stay unknown.
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootCoverage {
    pub requirement: String,
    pub obligation_ref: String,
    pub obligations: Vec<String>,
    pub mapping_revision: String,
    pub checks: Vec<String>,
    pub verdict: RuleVerdict,
    /// A mapping is never an execution observation.
    pub behavior_proof: RuleVerdict,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectedJudgment {
    pub schema_version: String,
    pub rule_id: String,
    pub authority: RuleAuthority,
    pub severity: RuleSeverity,
    pub phase: EvaluationPhase,
    pub source: String,
    pub source_commit: String,
    pub source_authority: String,
    pub source_revision: String,
    pub instruction_revision: String,
    pub coverage: RuleVerdict,
    pub consistency: RuleVerdict,
    pub roots: Vec<RootCoverage>,
    pub conflicting_relations: Vec<String>,
    pub unsupported_relations: Vec<String>,
    pub remediation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstraintJudgment {
    pub source_commit: String,
    pub source_revision: String,
    pub instruction_revision: String,
    pub source_authority: String,
    pub verdict: RuleVerdict,
    pub conflicting_relations: Vec<String>,
    pub unsupported_relations: Vec<String>,
}

pub(crate) struct CachedSelection {
    pub work: SelectedWork,
    pub source_commit: String,
    pub candidate_matches: bool,
}

/// Preparation can inspect a cached target ref offline. This is explicitly not
/// the provider freshness check required by full validation/publication.
pub(crate) fn cached_selection(
    root: &std::path::Path,
) -> Result<Option<CachedSelection>, DecapodError> {
    if !root.join(".decapod/config.toml").exists() {
        return Ok(None);
    }
    use std::io::Read;
    let path = root.join(".decapod/config.toml");
    if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(invalid("selected source config must not be an alias"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(invalid("selected current config exceeds input bound"));
    }
    let value: toml::Value =
        toml::from_str(std::str::from_utf8(&bytes).map_err(|e| invalid(e.to_string()))?)
            .map_err(|e| invalid(e.to_string()))?;
    let config = crate::core::validation_runtime::proof_configuration(&bytes)
        .map_err(DecapodError::IoError)?;
    let current = config.selected_work.as_ref();
    let Some(branch) = value
        .get("repo")
        .and_then(|v| v.get("base_branch"))
        .and_then(toml::Value::as_str)
    else {
        return if current.is_none() {
            Ok(None)
        } else {
            Err(invalid(
                "SELECTED_SOURCE_UNAVAILABLE: set the explicit accepted base branch and fetch it before selecting policy",
            ))
        };
    };
    let local = config
        .repository_contract
        .as_ref()
        .is_some_and(|c| c.canonical_local_target);
    let reference = if local {
        format!("refs/heads/{branch}")
    } else {
        format!("refs/remotes/origin/{branch}")
    };
    let tip = match super::git_output(
        root,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    ) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|e| invalid(e.to_string()))?
            .trim()
            .to_owned(),
        Err(_) if current.is_none() => return Ok(None),
        Err(_) => {
            return Err(invalid(
                "SELECTED_SOURCE_UNAVAILABLE: fetch the configured accepted target; no candidate policy was accepted",
            ));
        }
    };
    if !super::immutable_oid(&tip) {
        return Err(invalid("selected cached target is not an immutable commit"));
    }
    let entry = super::git_output(root, &["ls-tree", "-z", &tip, "--", ".decapod/config.toml"])?;
    if entry.is_empty() {
        return if current.is_none() {
            Ok(None)
        } else {
            Err(invalid(
                "SELECTED_POLICY_NOT_ACCEPTED: selected_work is absent from the cached accepted target",
            ))
        };
    }
    if !entry.starts_with(b"100644 blob ") {
        return Err(invalid(
            "selected cached policy is not a regular tracked blob",
        ));
    }
    let bytes = super::git_output(root, &["show", &format!("{tip}:.decapod/config.toml")])?;
    if bytes.len() > 65536 {
        return Err(invalid("selected cached policy exceeds input bound"));
    }
    let accepted = configuration(&bytes).map_err(DecapodError::IoError)?;
    match accepted.selected_work {
        Some(work) => {
            work.validate()?;
            let candidate_matches = current == Some(&work);
            Ok(Some(CachedSelection {
                work,
                source_commit: tip,
                candidate_matches,
            }))
        }
        None if current.is_none() => Ok(None),
        None => Err(invalid(
            "SELECTED_POLICY_NOT_ACCEPTED: a candidate cannot first accept its own selected roots",
        )),
    }
}

fn invalid(message: impl Into<String>) -> DecapodError {
    DecapodError::ValidationError(format!("SELECTED_REQUIREMENTS_INVALID: {}", message.into()))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// Generic native-runner fixtures may have no Decapod config at all. Parse the
/// full policy only when this extension is declared; malformed declarations
/// must not disappear into an absent-policy default.
pub(crate) fn configuration(
    bytes: &[u8],
) -> std::io::Result<crate::core::validation_runtime::ProofConfiguration> {
    let text = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
    match toml::from_str::<toml::Value>(text) {
        Ok(value)
            if value
                .get("proof")
                .and_then(|p| p.get("selected_work"))
                .is_some() =>
        {
            crate::core::validation_runtime::proof_configuration(bytes)
        }
        Err(error) if text.contains("selected_work") => Err(std::io::Error::other(error)),
        _ => Ok(Default::default()),
    }
}

impl SelectedWork {
    pub fn validate(&self) -> Result<(), DecapodError> {
        if self.schema_version != "1.0.0"
            || self.instruction_revision.len() != 71
            || !self.instruction_revision.starts_with("sha256:")
            || !self.instruction_revision[7..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || !identifier(&self.id)
            || !identifier(&self.intent_id)
            || self.roots.is_empty()
            || self.roots.len() > 128
            || self.relations.len() > 256
        {
            return Err(invalid("unsupported schema, identity or bounded set size"));
        }
        let mut roots = BTreeSet::new();
        for root in &self.roots {
            if !identifier(&root.id)
                || !roots.insert(&root.id)
                || root.checks.is_empty()
                || root.checks.len() > 32
                || root.checks.iter().any(|name| !identifier(name))
                || root.checks.iter().collect::<BTreeSet<_>>().len() != root.checks.len()
            {
                return Err(invalid(
                    "roots need unique IDs and bounded nonempty check names",
                ));
            }
        }
        let mut ids = BTreeSet::new();
        for relation in &self.relations {
            if !identifier(&relation.id)
                || !ids.insert(&relation.id)
                || !roots.contains(&relation.requirement)
                || !identifier(&relation.left)
                || !identifier(&relation.right)
                || !identifier(&relation.kind)
            {
                return Err(invalid(
                    "relation identity, participating root or term is invalid",
                ));
            }
        }
        Ok(())
    }

    pub fn source_revision(&self) -> Result<String, DecapodError> {
        crate::core::governance_document::digest(
            &serde_json::to_value(self).map_err(|e| invalid(e.to_string()))?,
        )
    }

    pub fn obligation_ref(&self, requirement: &str) -> Result<String, DecapodError> {
        Ok(format!(
            "requirement:{}:{}:{}",
            self.id,
            requirement,
            self.source_revision()?
        ))
    }
}

/// Equality is transitive over arbitrary values. Three inequalities are NOT a
/// binary parity contradiction. The shortest equality path plus its inequality
/// gives a useful irreducible witness, with deterministic bounded traversal.
pub fn consistency(
    work: &SelectedWork,
) -> Result<(RuleVerdict, Vec<String>, Vec<String>), DecapodError> {
    work.validate()?;
    let mut equal: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    let mut unsupported = Vec::new();
    for relation in &work.relations {
        match relation.kind.as_str() {
            "eq" => {
                equal
                    .entry(&relation.left)
                    .or_default()
                    .push((&relation.right, &relation.id));
                equal
                    .entry(&relation.right)
                    .or_default()
                    .push((&relation.left, &relation.id));
            }
            "ne" => {}
            _ => unsupported.push(relation.id.clone()),
        }
    }
    for relation in work.relations.iter().filter(|r| r.kind == "ne") {
        let mut queue = VecDeque::from([relation.left.as_str()]);
        let mut parents: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
        let mut visited = BTreeSet::from([relation.left.as_str()]);
        while let Some(term) = queue.pop_front() {
            if term == relation.right {
                let mut witness = vec![relation.id.clone()];
                let mut at = term;
                while at != relation.left {
                    let (previous, id) = parents[at];
                    witness.push(id.to_owned());
                    at = previous;
                }
                witness.sort();
                return Ok((RuleVerdict::Fail, witness, unsupported));
            }
            for &(next, id) in equal.get(term).into_iter().flatten() {
                if visited.insert(next) {
                    parents.insert(next, (term, id));
                    queue.push_back(next);
                }
            }
        }
    }
    Ok((
        if unsupported.is_empty() {
            RuleVerdict::Pass
        } else {
            RuleVerdict::Unverifiable
        },
        vec![],
        unsupported,
    ))
}

/// The caller must obtain `work` from the independently captured accepted Git
/// target, compare candidate policy and bind the returned digest before/after
/// execution. No writer, status setter, `covers` edge or second database exists.
pub fn evaluate(
    work: &SelectedWork,
    base: &str,
    store: &Store,
    phase: EvaluationPhase,
) -> Result<SelectedJudgment, DecapodError> {
    work.validate()?;
    let refs = work
        .roots
        .iter()
        .map(|r| work.obligation_ref(&r.id))
        .collect::<Result<Vec<_>, _>>()?;
    let mappings = obligation::read_selected_requirement_mappings(store, &refs)?;
    let roots: Vec<_> = work
        .roots
        .iter()
        .zip(refs)
        .map(|(root, reference)| {
            let nodes = mappings.get(&reference).cloned().unwrap_or_default();
            let covered = nodes.len() == 1
                && root
                    .checks
                    .iter()
                    .all(|name| nodes[0].declared_checks.contains(name));
            Ok(RootCoverage {
                requirement: root.id.clone(),
                obligation_ref: reference,
                obligations: nodes
                    .iter()
                    .map(|mapping| mapping.obligation_id.clone())
                    .collect(),
                mapping_revision: crate::core::governance_document::digest(
                    &serde_json::to_value(&nodes).map_err(|e| invalid(e.to_string()))?,
                )?,
                checks: root.checks.clone(),
                verdict: if covered {
                    RuleVerdict::Pass
                } else {
                    RuleVerdict::Fail
                },
                behavior_proof: RuleVerdict::Unverifiable,
            })
        })
        .collect::<Result<Vec<_>, DecapodError>>()?;
    let (consistency, conflicting_relations, unsupported_relations) = consistency(work)?;
    Ok(SelectedJudgment {
        schema_version: "1.0.0".into(), rule_id: "selected_accepted_requirements".into(),
        authority: RuleAuthority::ProjectOverride, severity: RuleSeverity::Error, phase,
        source: format!("{base}:.decapod/config.toml#/proof/selected_work"),
        source_commit: base.into(), source_authority: "accepted_target_policy".into(), source_revision: work.source_revision()?,
        instruction_revision: work.instruction_revision.clone(),
        coverage: if roots.iter().all(|r| r.verdict == RuleVerdict::Pass) { RuleVerdict::Pass } else { RuleVerdict::Fail },
        consistency, roots, conflicting_relations, unsupported_relations,
        remediation: "Retain exactly one integration obligation per accepted root with its source-bound obligation_ref and accepted check names. Resolve conflicting or unsupported relations through a separately accepted policy revision. Then execute the same accepted required checks on the combined candidate; a mapping or satisfiable model is not behavior proof.".into(),
    })
}

/// Admission uses only policy independently read from the accepted target.
/// This initial adapter supports existing repository-local native execution;
/// unsupported hosted/cloud joins fail explicitly instead of manufacturing DB
/// parity. Candidate rewrites, deletion and new self-acceptance are refused.
pub(crate) fn admission(
    root: &std::path::Path,
    base: &str,
    candidate: &crate::core::validation_runtime::ProofConfiguration,
    accepted: &crate::core::validation_runtime::ProofConfiguration,
    governance: &serde_json::Value,
    phase: EvaluationPhase,
) -> Result<Option<SelectedJudgment>, DecapodError> {
    if candidate.selected_work.is_none() && accepted.selected_work.is_none() {
        return Ok(None);
    }
    if candidate.selected_work != accepted.selected_work || accepted.selected_work.is_none() {
        return Err(invalid(
            "SELECTED_POLICY_NOT_ACCEPTED: preserve the exact accepted selected_work contract; candidate self-repinning cannot change its roots or relations",
        ));
    }
    let work = accepted.selected_work.as_ref().unwrap();
    work.validate()?;
    if accepted.required_profile.as_ref().map(|p| p.id.as_str())
        != Some(crate::core::repository_profile::REQUIRED_PROFILE)
        || candidate.uses_remote_preflight()
        || accepted.uses_remote_preflight()
    {
        return Err(invalid(
            "SELECTED_REQUIREMENTS_PROFILE_UNSUPPORTED: selected DB roots require the repository-local native adapter; hosted/cloud authority is not inferred",
        ));
    }
    if governance
        .pointer("/sections/trajectory/intent_id")
        .and_then(serde_json::Value::as_str)
        != Some(work.intent_id.as_str())
    {
        return Err(invalid(
            "SELECTED_INTENT_STALE: current trajectory must retain the selected accepted intent identity",
        ));
    }
    if instruction_revision(governance, &work.intent_id)? != work.instruction_revision {
        return Err(invalid(
            "SELECTED_INSTRUCTION_STALE: current recorded instruction revision differs from accepted selected roots; reconcile through a separately accepted policy revision before new proof",
        ));
    }
    for required in work.roots.iter().flat_map(|r| &r.checks) {
        if !accepted
            .commands
            .iter()
            .any(|c| &c.name == required && c.required)
            || !candidate
                .commands
                .iter()
                .any(|c| &c.name == required && c.required)
        {
            return Err(invalid(format!(
                "SELECTED_CHECK_UNCOVERED: accepted root requires mandatory executed check {required}"
            )));
        }
    }
    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root: crate::core::store::find_governance_root(root).join(".decapod/data"),
    };
    let report = evaluate(work, base, &store, phase)?;
    if report.coverage != RuleVerdict::Pass || report.consistency != RuleVerdict::Pass {
        return Err(DecapodError::ValidationError(format!(
            "SELECTED_REQUIREMENTS_BLOCKED: {}",
            serde_json::to_string(&report).map_err(|e| invalid(e.to_string()))?
        )));
    }
    Ok(Some(report))
}

/// Origin and appended recorded instructions are separate from an agent's
/// refinement and execution status. The digest proves content, not authorship.
pub fn instruction_revision(
    governance: &serde_json::Value,
    intent_id: &str,
) -> Result<String, DecapodError> {
    let record = governance
        .get("intents")
        .and_then(|v| v.get(intent_id))
        .ok_or_else(|| invalid("selected intent origin is not recorded"))?;
    if record
        .get("raw_intent")
        .and_then(serde_json::Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(invalid("selected intent origin is missing"));
    }
    crate::core::governance_document::digest(&serde_json::json!([
        intent_id,
        record.get("raw_intent"),
        record.get("instruction_origin"),
        record.get("instruction_revisions")
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle() -> SelectedWork {
        SelectedWork {
            schema_version: "1.0.0".into(),
            id: "selected".into(),
            intent_id: "intent:one".into(),
            instruction_revision: format!("sha256:{}", "1".repeat(64)),
            roots: ["R1", "R2", "R3"]
                .into_iter()
                .map(|id| RootRequirement {
                    id: id.into(),
                    checks: vec!["accepted-check".into()],
                })
                .collect(),
            relations: [
                ("R1", "A", "B", "eq"),
                ("R2", "B", "C", "eq"),
                ("R3", "A", "C", "ne"),
            ]
            .into_iter()
            .map(|(id, left, right, kind)| Relation {
                id: id.into(),
                requirement: id.into(),
                left: left.into(),
                right: right.into(),
                kind: kind.into(),
            })
            .collect(),
        }
    }
    #[test]
    fn all_pairs_pass_but_complete_triangle_has_a_minimal_witness() {
        let work = triangle();
        for omitted in 0..3 {
            let mut pair = work.clone();
            pair.relations.remove(omitted);
            assert_eq!(consistency(&pair).unwrap().0, RuleVerdict::Pass);
            for relation in &pair.relations {
                let mut single = work.clone();
                single.relations = vec![relation.clone()];
                assert_eq!(consistency(&single).unwrap().0, RuleVerdict::Pass);
            }
        }
        let (verdict, witness, _) = consistency(&work).unwrap();
        assert_eq!(verdict, RuleVerdict::Fail);
        assert_eq!(witness, ["R1", "R2", "R3"]);
        let mut repair = work.clone();
        repair.relations[2].kind = "eq".into();
        assert_eq!(consistency(&repair).unwrap().0, RuleVerdict::Pass);
        assert_ne!(
            work.source_revision().unwrap(),
            repair.source_revision().unwrap()
        );
        for r in &mut repair.relations {
            r.kind = "ne".into();
        }
        assert_eq!(
            consistency(&repair).unwrap().0,
            RuleVerdict::Pass,
            "uninterpreted values need not be binary"
        );
        repair.relations[0].kind = "prose".into();
        assert_eq!(consistency(&repair).unwrap().0, RuleVerdict::Unverifiable);
    }
    #[test]
    fn four_relations_are_conflicting_only_as_a_complete_set() {
        let mut work = triangle();
        work.roots.push(RootRequirement {
            id: "R4".into(),
            checks: vec!["accepted-check".into()],
        });
        work.relations[2].right = "D".into();
        work.relations.push(Relation {
            id: "R4".into(),
            requirement: "R4".into(),
            left: "C".into(),
            right: "D".into(),
            kind: "eq".into(),
        });

        for omitted in 0..work.relations.len() {
            let mut subset = work.clone();
            subset.relations.remove(omitted);
            assert_eq!(consistency(&subset).unwrap().0, RuleVerdict::Pass);
        }
        let (verdict, witness, unsupported) = consistency(&work).unwrap();
        assert_eq!(verdict, RuleVerdict::Fail);
        assert_eq!(witness, ["R1", "R2", "R3", "R4"]);
        assert!(unsupported.is_empty());

        work.relations[2].kind = "eq".into();
        assert_eq!(consistency(&work).unwrap().0, RuleVerdict::Pass);
    }

    #[test]
    fn omitted_root_and_acknowledged_root_are_not_executed_proof() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root: temp.path().join(".decapod/data"),
        };
        obligation::initialize_obligation_db(&store.root).unwrap();
        let mut work = triangle();
        work.relations.clear();
        for root in &work.roots[..2] {
            obligation::add_obligation(
                &store,
                &work.obligation_ref(&root.id).unwrap(),
                "medium",
                "",
                "accepted-check",
            )
            .unwrap();
        }
        let missing = evaluate(
            &work,
            "accepted-source",
            &store,
            EvaluationPhase::Validation,
        )
        .unwrap();
        assert_eq!(missing.coverage, RuleVerdict::Fail);
        assert_eq!(missing.roots[2].verdict, RuleVerdict::Fail);
        obligation::add_obligation(
            &store,
            &work.obligation_ref("R3").unwrap(),
            "medium",
            "",
            "accepted-check",
        )
        .unwrap();
        obligation::add_obligation(&store, "unrelated backlog", "medium", "", "missing-proof")
            .unwrap();
        let mapped = evaluate(
            &work,
            "accepted-source",
            &store,
            EvaluationPhase::Validation,
        )
        .unwrap();
        assert_eq!(mapped.coverage, RuleVerdict::Pass);
        assert!(
            mapped
                .roots
                .iter()
                .all(|r| r.behavior_proof == RuleVerdict::Unverifiable)
        );
        assert_eq!(
            missing.roots[2].verdict,
            RuleVerdict::Fail,
            "historical failure remains distinct"
        );
        work.roots[2].checks = vec!["new-accepted-check".into()];
        assert_eq!(
            evaluate(&work, "new-source", &store, EvaluationPhase::Validation)
                .unwrap()
                .coverage,
            RuleVerdict::Fail
        );
    }
}
