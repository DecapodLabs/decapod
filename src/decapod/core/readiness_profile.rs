//! Finite project profile compilation and the frozen-publication lifecycle.
//!
//! This is not another execution engine. It compiles the shared source catalog
//! and production release catalog into readiness_evidence::ExpectedPlan, then
//! verifies authentic runner/provider observations at the exact final boundary.
//! Source discovery, provider authentication, fresh target queries and actual
//! Git/filesystem capture belong to the caller. None may come from a receipt.
//!
//! The declaration's reviewed source digest is an explicitly reconciled policy
//! value, never auto-refreshed from discovered files. Unknown/changed CI source
//! blocks until its inventory and adapter are reviewed. Project configuration
//! itself is bound separately, avoiding a self-referential declaration digest.

use super::derived_release_profile::ReleaseProfile;
use super::readiness_evidence::{
    self as evidence, ApplicabilityFacts, CheckKey, CheckKind, Dependency, ExpectedCheck,
    ExpectedPlan, MaterialIdentity, Provider, Requirement, SuiteEvidence, Verdict,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[allow(dead_code)]
#[path = "../../../assets/tools/source_ci_profile.rs"]
pub(crate) mod source;

pub const REQUIRED_PROFILE: &str = "decapod.pr-readiness.v1";

pub fn catalog_source_paths() -> &'static [&'static str] {
    &[
        ".github/workflows/ci.yml",
        ".github/workflows/constitution-sync.yml",
        ".github/workflows/decapod-validate.yml",
        ".github/workflows/docs.yml",
        ".github/workflows/docs_sync.yml",
        ".github/workflows/release.yml",
        ".github/workflows/docs_autosync.yml",
        ".github/workflows/nix-flake.yml",
        ".github/workflows/security-audit.yml",
        "assets/tools/source_ci_profile.rs",
        "assets/tools/source_ci_custody.rs",
        "assets/tools/integration_custody.rs",
        "assets/tools/integration_shards.rs",
        "assets/tools/ci_event_context.rs",
        "assets/tools/source_ci_toolchain.rs",
        "assets/tools/source_ci_workflow_owned.rs",
        "src/decapod/core/release_metadata.rs",
    ]
}

/// Exact reviewed workflow definitions for this finite adapter. These constants
/// are updated only with a review of jobs, triggers, matrices, actions and source
/// predicates. A project declaration cannot override this compiled contract.
/// Integration must reconcile these after final workflow edits settle.
pub fn reviewed_workflow_digests() -> &'static [(&'static str, &'static str)] {
    &[
        (
            ".github/workflows/ci.yml",
            "sha256:faadf471008c789f760ee921c71c3ee42f7a15aa1bd84a95798071b3935dd682",
        ),
        (
            ".github/workflows/constitution-sync.yml",
            "sha256:36c04f68c23574218f4ea27d03f4d6dab7c6f5b24f18d943f18fdc7cb12d8669",
        ),
        (
            ".github/workflows/decapod-validate.yml",
            "sha256:bef2a7526eb250a9baf6831bf17896000c53395ed533d3d8e3c43ca9fff21a00",
        ),
        (
            ".github/workflows/docs.yml",
            "sha256:f06ab2354f70685d62d54fcfaaace3e9a0a9a9118d88cee34d1c0e9b7ceb3ae8",
        ),
        (
            ".github/workflows/docs_autosync.yml",
            "sha256:a638976699a517b079fe38a855f78ccc00570b22f1664ec7112a7b734c031ee3",
        ),
        (
            ".github/workflows/docs_sync.yml",
            "sha256:c3953bd209b16656640984051555c61941a210fb7d9549419e79c6cd1a7f05fe",
        ),
        (
            ".github/workflows/nix-flake.yml",
            "sha256:032863086272e2394f09751e416914a8d21e4aa7dad13801e04daf395dcf80fd",
        ),
        (
            ".github/workflows/release.yml",
            "sha256:440e58f4de5b57bc53944600067ed939b80ad4a3a0b1680d672dd51e49a65a6b",
        ),
        (
            ".github/workflows/security-audit.yml",
            "sha256:125928d5fe733bc080ac9a395a0c36275265a518bbeb8b9cb65d19b44a35fee0",
        ),
    ]
}

/// Exact helper bytes compiled into the reviewed finite execution adapter.
/// Tests reuse the same manifest; provider output never defines this authority.
pub fn definition_files() -> Vec<(&'static str, &'static [u8])> {
    let mut files = vec![
        (
            "assets/tools/source_ci_profile.rs",
            include_bytes!("../../../assets/tools/source_ci_profile.rs").as_slice(),
        ),
        (
            "assets/tools/integration_shards.rs",
            include_bytes!("../../../assets/tools/integration_shards.rs").as_slice(),
        ),
        (
            "assets/tools/source_ci_custody.rs",
            include_bytes!("../../../assets/tools/source_ci_custody.rs").as_slice(),
        ),
        (
            "assets/tools/integration_custody.rs",
            include_bytes!("../../../assets/tools/integration_custody.rs").as_slice(),
        ),
        (
            "assets/tools/ci_event_context.rs",
            include_bytes!("../../../assets/tools/ci_event_context.rs").as_slice(),
        ),
        (
            "assets/tools/source_ci_toolchain.rs",
            include_bytes!("../../../assets/tools/source_ci_toolchain.rs").as_slice(),
        ),
        (
            "assets/tools/source_ci_workflow_owned.rs",
            include_bytes!("../../../assets/tools/source_ci_workflow_owned.rs").as_slice(),
        ),
    ];
    files.extend(source::workflow_owned::SOURCES.iter().copied());
    files
}

fn bytes_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfileDeclaration {
    pub id: String,
    /// Explicitly reviewed complete CI workflow/action/helper definition inventory.
    /// Do not include config.toml, which contains this declaration itself.
    pub reviewed_sources_digest: String,
}

/// Independently discover ALL source CI definitions (all providers, including
/// non-PR workflows), local actions, and their policy/selection helpers. Missing,
/// newly added or changed source files differ from the reviewed declaration.
/// This type is deliberately not Deserialize: no receipt supplies source coverage.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SourceInventory {
    pub files: BTreeMap<String, String>,
}
impl SourceInventory {
    pub fn digest(&self) -> Result<String, PlanningFailure> {
        hash(self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum InvocationRole {
    SourceCheck,
    /// Explicit govern project-check, never the outer full-readiness validator.
    ProjectPrerequisites,
    DerivedRelease,
    CandidateBuild,
    FullValidator,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Obligation {
    pub key: CheckKey,
    pub kind: CheckKind,
    pub role: InvocationRole,
    pub definition_digest: String,
}

/// A real exclusion independently evaluated by the shared source planner from
/// current base-to-head changed paths. It is not a provider's skipped result.
/// Rebuilding the catalog at the final boundary re-proves every exclusion.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProvenExclusion {
    pub key: CheckKey,
    pub rule: String,
}

/// Governed execution intent, never an execution receipt. This finite source
/// profile cannot turn a real build/test into a simulation by caller choice.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageDispositionKind {
    Real,
    Mocked,
    NonApplicable,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StageDisposition {
    pub stage: String,
    pub key: Option<CheckKey>,
    pub disposition: StageDispositionKind,
    pub policy: String,
    pub reason: String,
    pub evidence_scope: String,
}
fn source_stage_dispositions(
    obligations: &[Obligation],
    exclusions: &[ProvenExclusion],
) -> Vec<StageDisposition> {
    let mut result: Vec<_> = obligations.iter().map(|row| StageDisposition {
        stage: row.key.profile.clone(), key: Some(row.key.clone()),
        disposition: StageDispositionKind::Real, policy: "decapod.source-pr-scope.v1".into(),
        reason: "Applicable actual PR source check or candidate build; real execution remains mandatory".into(),
        evidence_scope: "authenticated_execution_required".into(),
    }).chain(exclusions.iter().map(|row| StageDisposition {
        stage: row.key.profile.clone(), key: Some(row.key.clone()),
        disposition: StageDispositionKind::NonApplicable, policy: row.rule.clone(),
        reason: "Independently recomputed source applicability predicate is false".into(),
        evidence_scope: "source_predicate_exclusion_only".into(),
    })).collect();
    result.push(StageDisposition {
        stage: "github-pull-request-ci".into(), key: None,
        disposition: StageDispositionKind::NonApplicable,
        policy: "decapod.buildkite-only-preflight.v1".into(),
        reason: "Ordinary GitHub pull-request CI remains independent; preflight requires the complete Buildkite source contract instead".into(),
        evidence_scope: "no_github_actions_execution_or_assurance_claim".into(),
    });
    result.push(StageDisposition {
        stage: "derived-release-packaging".into(), key: None,
        disposition: StageDispositionKind::NonApplicable, policy: "decapod.source-pr-scope.v1".into(),
        reason: "Predictive release-only packaging is outside the actual source PR suite; issue #1399 remains unresolved".into(),
        evidence_scope: "no_release_packaging_assurance".into(),
    });
    for stage in ["publish", "deploy"] {
        result.push(StageDisposition {
            stage: stage.into(), key: None, disposition: StageDispositionKind::Mocked,
            policy: "decapod.source-pr-no-publication-side-effects.v1".into(),
            reason: "Validation may simulate publication/deployment intent; it does not perform these side effects".into(),
            evidence_scope: "simulation_only_no_execution_or_assurance_claim".into(),
        });
    }
    result
}

#[derive(Debug, Clone, Serialize)]
pub struct RequiredCatalog {
    obligations: Vec<Obligation>,
    exclusions: Vec<ProvenExclusion>,
    stage_dispositions: Vec<StageDisposition>,
    facts: ApplicabilityFacts,
    source_inventory_digest: String,
}

impl RequiredCatalog {
    pub fn obligations(&self) -> &[Obligation] {
        &self.obligations
    }
    pub fn exclusions(&self) -> &[ProvenExclusion] {
        &self.exclusions
    }
    pub fn stage_dispositions(&self) -> &[StageDisposition] {
        &self.stage_dispositions
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionBinding {
    pub role: InvocationRole,
    /// The adapter was reviewed against this exact independent source obligation.
    pub definition_digest: String,
    pub invocation_digest: String,
    pub toolchain_digest: String,
    pub executor: Provider,
    pub dependencies: Vec<Dependency>,
}

/// Selected before dispatch, independently of provider results. False means
/// native equivalent execution for EVERY applicable obligation, never no proof.
/// True mode executes the complete source contract on Buildkite. Ordinary
/// GitHub pull-request CI remains independent and is not preflight evidence.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionPolicy {
    pub buildkite_preflights: bool,
    pub executors: BTreeMap<CheckKey, Provider>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningFailure {
    MissingProfile,
    UnknownProfile(String),
    SourceContractChanged,
    MissingSource(String),
    UnsupportedSource(String),
    InvalidFacts,
    InvalidReleaseProfile,
    EmptyCatalog,
    DuplicateObligation(CheckKey),
    MissingBinding(CheckKey),
    ExtraBinding(CheckKey),
    InvalidBinding(CheckKey),
    InvalidPlan(String),
    Serialization,
}

impl std::fmt::Display for PlanningFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingProfile => write!(
                f,
                "MISSING_PROOF_PROFILE: declare proof.required_profile in .decapod/config.toml after reviewing the complete CI and release contract"
            ),
            Self::UnknownProfile(id) => write!(
                f,
                "UNKNOWN_PROOF_PROFILE: {id}; implement and review its complete source/release adapter"
            ),
            Self::SourceContractChanged => write!(
                f,
                "PROOF_PROFILE_RECONCILIATION_REQUIRED: source definitions or compiled catalog changed; review inventory and adapters before explicitly updating the declaration"
            ),
            Self::MissingBinding(key) => write!(
                f,
                "MISSING_REQUIRED_PROOF_ADAPTER: {key:?}; provide equivalent executed evidence, not a skip"
            ),
            Self::UnsupportedSource(path) => write!(
                f,
                "UNSUPPORTED_SOURCE_CONTRACT: {path}; the selected profile cannot prove this source inventory"
            ),
            other => write!(f, "PROOF_PROFILE_INVALID: {other:?}"),
        }
    }
}
impl std::error::Error for PlanningFailure {}

fn hash(value: &impl Serialize) -> Result<String, PlanningFailure> {
    evidence::digest(value).map_err(|_| PlanningFailure::Serialization)
}
fn valid_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|d| {
        d.len() == 64
            && d.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn matrix(job: &str, value: Option<&str>) -> Result<BTreeMap<String, String>, PlanningFailure> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let axis = match job {
        "musl-publication" | "test" => "target",
        "state-commit-golden-vectors" => "os",
        "test-integration" => "shard",
        _ => return Err(PlanningFailure::SourceContractChanged),
    };
    let mut axes: BTreeMap<String, String> = [(axis.into(), value.into())].into();
    if job == "musl-publication" {
        // Preserve the source include matrix, not an independent cross product.
        let execution = match value {
            "x86_64-unknown-linux-musl" => "native",
            "aarch64-unknown-linux-musl" => "QEMU user-mode",
            _ => return Err(PlanningFailure::SourceContractChanged),
        };
        axes.insert("execution".into(), execution.into());
    }
    Ok(axes)
}

fn key(workflow: &str, job: &str, matrix: BTreeMap<String, String>, detail: &str) -> CheckKey {
    CheckKey {
        provider: Provider::Buildkite,
        workflow: format!(".github/workflows/{workflow}"),
        job: job.into(),
        matrix,
        // Distinguish substantive subchecks without changing actual job/matrix IDs.
        profile: format!("{REQUIRED_PROFILE}/{detail}"),
    }
}

/// Compatibility wrapper. Release configuration cannot add packaging obligations
/// to the default core source-PR readiness scope.
pub fn required_catalog(
    declaration: Option<&ProfileDeclaration>,
    discovered: &SourceInventory,
    facts: &ApplicabilityFacts,
    _release: &ReleaseProfile,
) -> Result<RequiredCatalog, PlanningFailure> {
    required_source_catalog(declaration, discovered, facts)
}

/// Complete actual source suite and candidate build, remotely proven by Buildkite.
/// Release-only packaging is an explicit non-applicable disposition, not proof.
pub fn required_source_catalog(
    declaration: Option<&ProfileDeclaration>,
    discovered: &SourceInventory,
    facts: &ApplicabilityFacts,
) -> Result<RequiredCatalog, PlanningFailure> {
    let declaration = declaration.ok_or(PlanningFailure::MissingProfile)?;
    if declaration.id != REQUIRED_PROFILE {
        return Err(PlanningFailure::UnknownProfile(declaration.id.clone()));
    }
    if facts.event != "pull_request"
        || facts.target_branch.trim().is_empty()
        || facts
            .changed_paths
            .iter()
            .any(|p| source::parse_paths(p.as_bytes()).is_err())
    {
        return Err(PlanningFailure::InvalidFacts);
    }
    // The caller's discovery includes unknown/new providers, so equality detects
    // additions as well as altered/deleted workflows. Never regenerate this pin.
    if discovered.files.is_empty()
        || discovered.files.keys().any(|p| p == ".decapod/config.toml")
        || discovered
            .files
            .values()
            .any(|digest| !valid_digest(digest))
        || discovered.digest()? != declaration.reviewed_sources_digest
    {
        return Err(PlanningFailure::SourceContractChanged);
    }
    for path in catalog_source_paths() {
        if !discovered.files.contains_key(*path) {
            return Err(PlanningFailure::MissingSource((*path).into()));
        }
    }
    // This finite profile has no interpreter for a new source workflow or native
    // Buildkite pipeline. A changed review pin cannot add semantics by itself.
    for path in discovered.files.keys() {
        if (path.starts_with(".github/workflows/")
            && !catalog_source_paths().contains(&path.as_str()))
            || path.starts_with(".buildkite/")
            || path.starts_with(".circleci/")
            || path.starts_with(".gitlab/")
            || path.starts_with(".woodpecker/")
            || path.starts_with(".azure-pipelines/")
            || [
                ".buildkite.yml",
                ".buildkite.yaml",
                ".travis.yml",
                "bitbucket-pipelines.yml",
                "appveyor.yml",
                ".gitlab-ci.yml",
                ".gitlab-ci.yaml",
                "azure-pipelines.yml",
                "azure-pipelines.yaml",
                "Jenkinsfile",
                ".drone.yml",
                ".drone.yaml",
                ".woodpecker.yml",
                ".woodpecker.yaml",
            ]
            .contains(&path.as_str())
        {
            return Err(PlanningFailure::UnsupportedSource(path.clone()));
        }
    }
    for (path, expected) in reviewed_workflow_digests() {
        if discovered.files.get(*path).map(String::as_str) != Some(*expected) {
            return Err(PlanningFailure::SourceContractChanged);
        }
    }
    for (path, compiled) in definition_files() {
        if discovered.files.get(path) != Some(&bytes_digest(compiled)) {
            return Err(PlanningFailure::SourceContractChanged);
        }
    }
    let source_digest = discovered.digest()?;
    let paths: Vec<_> = facts.changed_paths.iter().cloned().collect();
    let mut obligations = Vec::new();
    let mut exclusions = Vec::new();
    for job in source::inventory(&paths, false) {
        let mut members = job.checks.clone();
        if members.is_empty() {
            members.push("workflow-owned");
        }
        for id in members {
            let key = key(job.workflow, job.job, matrix(job.job, job.matrix)?, id);
            let (applicable, definition) = if id == "workflow-owned" {
                (
                    job.applicable,
                    serde_json::json!({"workflow_owned": true, "job": job.job}),
                )
            } else {
                let check =
                    source::find_check(id).map_err(|_| PlanningFailure::InvalidPlan(id.into()))?;
                let commands = source::commands(check, &paths)
                    .map_err(|_| PlanningFailure::InvalidPlan(id.into()))?;
                (
                    job.applicable && source::selected(check, &paths, false),
                    serde_json::json!({
                        "id": id, "platform": check.platform.name(), "capabilities": check.capabilities,
                        "commands": commands.iter().map(source::invocation_json).collect::<Vec<_>>(),
                    }),
                )
            };
            // Preserve every applicable logical check and matrix instance once.
            // Buildkite imports the authored workflow contract; normal GitHub PR
            // execution remains a separate post-publication CI policy.
            for provider in [Provider::Buildkite] {
                let mut provider_key = key.clone();
                provider_key.provider = provider;
                if !applicable {
                    exclusions.push(ProvenExclusion {
                        key: provider_key,
                        rule: format!("{}:{id}:selected=false", source::SCHEMA),
                    });
                    continue;
                }
                let role = if matches!(id, "workflow-owned" | "health" | "standalone-validation")
                    && matches!(job.job, "health" | "validate")
                {
                    InvocationRole::ProjectPrerequisites
                } else {
                    InvocationRole::SourceCheck
                };
                obligations.push(Obligation {
                    definition_digest: hash(&(&source_digest, &provider_key, &definition, &role))?,
                    key: provider_key,
                    kind: CheckKind::SourceCi,
                    role,
                });
            }
        }
    }
    let key = key(
        "ci.yml",
        "candidate-build",
        BTreeMap::new(),
        "fresh-candidate-build",
    );
    obligations.push(Obligation {
        definition_digest: hash(&(&source_digest, &key, "actual artifact required"))?,
        key,
        kind: CheckKind::CandidateBuild,
        role: InvocationRole::CandidateBuild,
    });
    obligations.sort_by(|a, b| a.key.cmp(&b.key));
    exclusions.sort_by(|a, b| a.key.cmp(&b.key));
    let mut seen = BTreeSet::new();
    for key in obligations
        .iter()
        .map(|o| &o.key)
        .chain(exclusions.iter().map(|e| &e.key))
    {
        if !seen.insert(key) {
            return Err(PlanningFailure::DuplicateObligation(key.clone()));
        }
    }
    let stage_dispositions = source_stage_dispositions(&obligations, &exclusions);
    let source_inventory_digest = hash(&(
        &source_digest,
        &obligations,
        &exclusions,
        &stage_dispositions,
        facts,
        "decapod.source-pr-scope.v1",
    ))?;
    Ok(RequiredCatalog {
        obligations,
        exclusions,
        stage_dispositions,
        facts: facts.clone(),
        source_inventory_digest,
    })
}

#[derive(Debug, Clone)]
pub struct CompiledProfile {
    plan: ExpectedPlan,
    facts: ApplicabilityFacts,
    exclusions: Vec<ProvenExclusion>,
    configuration_digest: String,
    compiled_identity: MaterialIdentity,
    governance_policy_digest: String,
    provider_prerequisites_required: bool,
}
impl CompiledProfile {
    pub fn plan(&self) -> &ExpectedPlan {
        &self.plan
    }
    pub fn facts(&self) -> &ApplicabilityFacts {
        &self.facts
    }
    pub fn exclusions(&self) -> &[ProvenExclusion] {
        &self.exclusions
    }
}

/// Bind reviewed native Invocation::digest/toolchain_digest or authenticated
/// provider execution definitions. Bindings never choose expected coverage.
pub fn compile_plan(
    catalog: RequiredCatalog,
    policy: &ExecutionPolicy,
    bindings: &BTreeMap<CheckKey, ExecutionBinding>,
    current: &MaterialIdentity,
    governance_policy_digest: &str,
) -> Result<CompiledProfile, PlanningFailure> {
    let configuration_digest = current.configuration_digest.as_str();
    if catalog.obligations.is_empty() {
        return Err(PlanningFailure::EmptyCatalog);
    }
    if !valid_digest(configuration_digest) || !valid_digest(governance_policy_digest) {
        return Err(PlanningFailure::InvalidPlan(
            "missing configuration/governance policy identity".into(),
        ));
    }
    let expected: BTreeSet<_> = catalog.obligations.iter().map(|o| o.key.clone()).collect();
    for key in bindings.keys().chain(policy.executors.keys()) {
        if !expected.contains(key) {
            return Err(PlanningFailure::ExtraBinding(key.clone()));
        }
    }
    let mut checks = Vec::new();
    for obligation in &catalog.obligations {
        let binding = bindings
            .get(&obligation.key)
            .ok_or_else(|| PlanningFailure::MissingBinding(obligation.key.clone()))?;
        let executor = policy
            .executors
            .get(&obligation.key)
            .ok_or_else(|| PlanningFailure::MissingBinding(obligation.key.clone()))?;
        if binding.role == InvocationRole::FullValidator
            || binding.role != obligation.role
            || binding.definition_digest != obligation.definition_digest
            || binding.executor != *executor
            || (!policy.buildkite_preflights && *executor != Provider::Native)
            || (policy.buildkite_preflights
                && matches!(
                    obligation.kind,
                    CheckKind::SourceCi | CheckKind::CandidateBuild
                )
                && *executor != obligation.key.provider)
            || !valid_digest(&binding.invocation_digest)
            || !valid_digest(&binding.toolchain_digest)
        {
            return Err(PlanningFailure::InvalidBinding(obligation.key.clone()));
        }
        checks.push(ExpectedCheck {
            key: obligation.key.clone(),
            kind: obligation.kind,
            definition_digest: obligation.definition_digest.clone(),
            invocation_digest: binding.invocation_digest.clone(),
            toolchain_digest: binding.toolchain_digest.clone(),
            executor: *executor,
            dependencies: binding.dependencies.clone(),
            requirement: Requirement::Execute,
        });
    }
    let provider_prerequisites_required = policy.buildkite_preflights
        && catalog.obligations.iter().any(|row| {
            row.role == InvocationRole::ProjectPrerequisites
                && row.kind == CheckKind::SourceCi
                && row.key.provider == Provider::Buildkite
        });
    let executors: Vec<_> = policy.executors.iter().collect();
    let plan = ExpectedPlan {
        schema_version: evidence::SCHEMA_VERSION.into(),
        policy_digest: hash(&(
            REQUIRED_PROFILE,
            configuration_digest,
            governance_policy_digest,
            policy.buildkite_preflights,
            executors,
        ))?,
        source_inventory_digest: catalog.source_inventory_digest,
        buildkite_enabled: policy.buildkite_preflights,
        checks,
    };
    // Use the existing verifier as the plan validator, removing only the expected
    // missing-result diagnostics. Never manufacture successful proof results.
    let identity = current.clone();
    let empty = SuiteEvidence {
        schema_version: evidence::SCHEMA_VERSION.into(),
        expected_plan_digest: plan.digest().map_err(|_| PlanningFailure::Serialization)?,
        before: identity.clone(),
        after: identity.clone(),
        facts: catalog.facts.clone(),
        results: Vec::new(),
    };
    let invalid: Vec<_> = evidence::verify(&plan, &identity, &catalog.facts, &empty)
        .failures
        .into_iter()
        .filter(|f| !matches!(f, evidence::Failure::Missing(_)))
        .collect();
    if !invalid.is_empty() {
        return Err(PlanningFailure::InvalidPlan(format!("{invalid:?}")));
    }
    Ok(CompiledProfile {
        plan,
        facts: catalog.facts,
        exclusions: catalog.exclusions,
        configuration_digest: configuration_digest.into(),
        compiled_identity: current.clone(),
        governance_policy_digest: governance_policy_digest.into(),
        provider_prerequisites_required,
    })
}

/// Internal prerequisite result or provider obligation. Never full readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectPrerequisites {
    Passed,
    Failed,
    /// No local suite ran. The full Buildkite source plan must include and
    /// authenticate its actual project-check rows before readiness can pass.
    ProviderRequired,
}
impl ProjectPrerequisites {
    pub fn readiness_verdict(self) -> Verdict {
        Verdict::NotReady
    }
}

#[derive(Debug, Clone)]
pub struct PublicationState {
    pub identity: MaterialIdentity,
    pub governance_policy_digest: String,
    pub governance_worktree_digest: String,
    pub governance_index_digest: String,
    pub governance_commit_digest: String,
    /// Fresh Git status inventory, excluding only ordinary ignored runtime data.
    pub uncommitted_paths: BTreeSet<String>,
}

/// Fresh authenticated target query supplied by the provider adapter, not read
/// from the evidence. Query again at finish/publication, with this invocation ID.
#[derive(Debug, Clone)]
pub struct TargetObservation {
    pub invocation_id: String,
    pub repository: String,
    pub branch: String,
    pub revision: String,
}

#[derive(Debug, Clone)]
pub struct FrozenReadiness {
    invocation_id: String,
    identity: MaterialIdentity,
    governance_digest: String,
    plan_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleFailure {
    PrerequisitesFailed,
    DirtyPublication,
    StaleBase,
    StalePolicy,
    StaleCandidate,
    PendingEvidence,
    ProviderFailure(String),
    Evidence(Vec<evidence::Failure>),
    InvalidInvocation,
}

fn stable_state(
    profile: &CompiledProfile,
    state: &PublicationState,
) -> Result<(), LifecycleFailure> {
    if !state.uncommitted_paths.is_empty()
        || !valid_digest(&state.governance_commit_digest)
        || state.governance_worktree_digest != state.governance_commit_digest
        || state.governance_index_digest != state.governance_commit_digest
    {
        return Err(LifecycleFailure::DirtyPublication);
    }
    if state.identity != profile.compiled_identity {
        return Err(LifecycleFailure::StaleCandidate);
    }
    if state.identity.configuration_digest != profile.configuration_digest
        || state.governance_policy_digest != profile.governance_policy_digest
    {
        return Err(LifecycleFailure::StalePolicy);
    }
    Ok(())
}
fn target_matches(id: &str, identity: &MaterialIdentity, target: &TargetObservation) -> bool {
    target.invocation_id == id
        && target.repository == identity.repository
        && target.branch == identity.base_branch
        && target.revision == identity.accepted_base_revision
}

pub fn freeze(
    invocation_id: &str,
    prerequisites: ProjectPrerequisites,
    profile: &CompiledProfile,
    state: &PublicationState,
    target: &TargetObservation,
) -> Result<FrozenReadiness, LifecycleFailure> {
    if invocation_id.trim().is_empty() {
        return Err(LifecycleFailure::InvalidInvocation);
    }
    if prerequisites != ProjectPrerequisites::Passed
        && !(prerequisites == ProjectPrerequisites::ProviderRequired
            && profile.provider_prerequisites_required
            && profile.plan.buildkite_enabled)
    {
        return Err(LifecycleFailure::PrerequisitesFailed);
    }
    stable_state(profile, state)?;
    if !target_matches(invocation_id, &state.identity, target) {
        return Err(LifecycleFailure::StaleBase);
    }
    Ok(FrozenReadiness {
        invocation_id: invocation_id.into(),
        identity: state.identity.clone(),
        governance_digest: state.governance_commit_digest.clone(),
        plan_digest: profile
            .plan
            .digest()
            .map_err(|_| LifecycleFailure::StalePolicy)?,
    })
}

/// Complete is allowed only after native.failure==None or authenticated provider
/// terminal/custody checks. Unavailable/unsupported adapters remain explicit.
pub enum ProviderOutcome<'a> {
    Pending,
    Failed(&'a str),
    Complete(&'a SuiteEvidence),
}

/// Rebuild `current_profile` independently from current files/facts, recapture
/// current state, and freshly query the target before calling. Persist the final
/// evidence in ignored runtime/external storage; this function never writes R/H.
pub fn finish(
    frozen: &FrozenReadiness,
    current_profile: &CompiledProfile,
    current: &PublicationState,
    current_target: &TargetObservation,
    outcome: ProviderOutcome<'_>,
) -> Result<Verdict, LifecycleFailure> {
    stable_state(current_profile, current)?;
    if !target_matches(&frozen.invocation_id, &current.identity, current_target) {
        return Err(LifecycleFailure::StaleBase);
    }
    if current.identity != frozen.identity
        || current.governance_commit_digest != frozen.governance_digest
    {
        return Err(LifecycleFailure::StaleCandidate);
    }
    if current_profile.plan.digest().ok().as_ref() != Some(&frozen.plan_digest) {
        return Err(LifecycleFailure::StalePolicy);
    }
    let evidence = match outcome {
        ProviderOutcome::Pending => return Err(LifecycleFailure::PendingEvidence),
        ProviderOutcome::Failed(error) => {
            return Err(LifecycleFailure::ProviderFailure(error.into()));
        }
        ProviderOutcome::Complete(evidence) => evidence,
    };
    let report = evidence::verify(
        &current_profile.plan,
        &current.identity,
        &current_profile.facts,
        evidence,
    );
    if report.verdict != Verdict::Ready {
        return Err(LifecycleFailure::Evidence(report.failures));
    }
    Ok(Verdict::Ready)
}

pub fn publication_exit_code(result: &Result<Verdict, LifecycleFailure>) -> i32 {
    if matches!(result, Ok(Verdict::Ready)) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests;
