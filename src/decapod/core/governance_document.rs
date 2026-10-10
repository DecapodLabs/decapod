//! The single Git-owned governance document. Runtime coordination remains in data/.
//! Reads never migrate. Writers serialize complete read/modify/write transactions.
use crate::core::error::DecapodError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const GOVERNANCE_PATH: &str = ".decapod/governance.json";
pub const SCHEMA_VERSION: &str = "1.0.0";
const SECTIONS: &[&str] = &["plan", "trajectory", "validation", "jev"];
const LEGACY: &[(&str, &str)] = &[
    ("plan", ".decapod/governance/plan.json"),
    ("claims", ".decapod/governance/claims.json"),
    ("trajectory", ".decapod/governance/trajectory.json"),
    ("validation", ".decapod/governance/validation.json"),
    ("jev", ".decapod/governance/jev.json"),
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Baseline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_commit: Option<String>,
    /// A release is recorded only when an actual tag is known, never inferred from merge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub policy: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeIdentity {
    pub id: String,
    pub base_commit: String,
    pub target_base: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveClaim {
    pub statement: String,
    pub falsifier: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub id: String,
    pub summary: String,
    /// SHA-256 of material inputs, not of the containing Git commit or receipt.
    pub input_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationResolution {
    pub obligation_digest: String,
    pub resolution: String,
    pub proof_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceDocument {
    pub schema_version: String,
    #[serde(default)]
    pub baseline: Baseline,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<ChangeIdentity>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub claims: BTreeMap<String, ActiveClaim>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resolutions: BTreeMap<String, ObligationResolution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<Checkpoint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub intents: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub epochs: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sections: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub recursive_passes: BTreeMap<String, Value>,
    /// Lossless temporary import. An explicit begin-pr archives it only after Git proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_claims: Option<Value>,
    /// Digests identify inert legacy files after an interrupted migration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legacy_digests: BTreeMap<String, String>,
}

impl Default for GovernanceDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.into(),
            baseline: Baseline::default(),
            change: None,
            claims: BTreeMap::new(),
            resolutions: BTreeMap::new(),
            checkpoints: Vec::new(),
            intents: BTreeMap::new(),
            epochs: BTreeMap::new(),
            sections: BTreeMap::new(),
            recursive_passes: BTreeMap::new(),
            legacy_claims: None,
            legacy_digests: BTreeMap::new(),
        }
    }
}

fn invalid(message: impl Into<String>) -> DecapodError {
    DecapodError::ValidationError(message.into())
}

pub fn digest(value: &Value) -> Result<String, DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    Ok(bytes_digest(&bytes))
}

fn bytes_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn parse(bytes: &[u8]) -> Result<GovernanceDocument, DecapodError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| invalid(format!("Invalid governance document: {e}")))?;
    if value.get("schema_version").and_then(Value::as_str) != Some(SCHEMA_VERSION) {
        return Err(invalid(
            "GOVERNANCE_SCHEMA_UNSUPPORTED: use a compatible Decapod version; the document was not changed",
        ));
    }
    let document: GovernanceDocument = serde_json::from_value(value)
        .map_err(|e| invalid(format!("Invalid governance document: {e}")))?;
    validate_document(&document)?;
    Ok(document)
}

pub fn load(root: &Path) -> Result<Option<GovernanceDocument>, DecapodError> {
    let path = root.join(GOVERNANCE_PATH);
    if !path.exists() {
        return import_legacy(root);
    }
    let document = parse(&fs::read(path).map_err(DecapodError::IoError)?)?;
    check_legacy_conflicts(root, &document)?;
    Ok(Some(document))
}

fn recognized_legacy_path(path: &str) -> bool {
    if LEGACY.iter().any(|(_, known)| *known == path) {
        return true;
    }
    let Some(name) = path.strip_prefix(".decapod/governance/recursive_passes/") else {
        return false;
    };
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && name.ends_with(".json")
}

fn safe_legacy_file(root: &Path, relative: &str) -> Result<PathBuf, DecapodError> {
    if !recognized_legacy_path(relative) {
        return Err(invalid(
            "GOVERNANCE_UNSAFE_MIGRATION_PATH: only recognized legacy artifacts can be retired",
        ));
    }
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(invalid("Unsafe legacy path component"));
        }
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid(
                    "GOVERNANCE_UNSAFE_MIGRATION_PATH: symlink legacy artifact",
                ));
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(DecapodError::IoError(error)),
        }
    }
    Ok(path)
}

fn check_legacy_conflicts(root: &Path, document: &GovernanceDocument) -> Result<(), DecapodError> {
    for (path, expected) in &document.legacy_digests {
        let file = safe_legacy_file(root, path)?;
        if file.exists()
            && bytes_digest(&fs::read(&file).map_err(DecapodError::IoError)?) != *expected
        {
            return Err(invalid(format!(
                "GOVERNANCE_AUTHORITY_CONFLICT: {path} changed; no canonical mutation permitted"
            )));
        }
    }
    for (_, path) in LEGACY {
        let file = safe_legacy_file(root, path)?;
        if file.exists() && !document.legacy_digests.contains_key(*path) {
            return Err(invalid(format!(
                "GOVERNANCE_AUTHORITY_CONFLICT: unexpected legacy authority {path}"
            )));
        }
    }
    let passes = root.join(".decapod/governance/recursive_passes");
    if passes.is_dir() {
        for entry in fs::read_dir(passes).map_err(DecapodError::IoError)? {
            let entry = entry.map_err(DecapodError::IoError)?;
            let relative = format!(
                ".decapod/governance/recursive_passes/{}",
                entry.file_name().to_string_lossy()
            );
            safe_legacy_file(root, &relative)?;
            if !document.legacy_digests.contains_key(&relative) {
                return Err(invalid(
                    "GOVERNANCE_AUTHORITY_CONFLICT: unimported recursive pass",
                ));
            }
        }
    }
    Ok(())
}

fn import_legacy(root: &Path) -> Result<Option<GovernanceDocument>, DecapodError> {
    let mut document = GovernanceDocument::default();
    let mut found = false;
    for (section, path) in LEGACY {
        let file = safe_legacy_file(root, path)?;
        if !file.exists() {
            continue;
        }
        let bytes = fs::read(file).map_err(DecapodError::IoError)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|e| invalid(format!("Invalid legacy {path}: {e}")))?;
        validate_domain(section, &value)?;
        set_section(&mut document, section, value)?;
        document
            .legacy_digests
            .insert((*path).into(), bytes_digest(&bytes));
        found = true;
    }
    let passes = root.join(".decapod/governance/recursive_passes");
    if passes.is_dir() {
        for entry in fs::read_dir(&passes).map_err(DecapodError::IoError)? {
            let path = entry.map_err(DecapodError::IoError)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                return Err(invalid(
                    "Unrecognized recursive-pass file; migration requires explicit disposition",
                ));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|e| invalid(e.to_string()))?
                .to_string_lossy();
            safe_legacy_file(root, &relative)?;
            let bytes = fs::read(&path).map_err(DecapodError::IoError)?;
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
            if value["schema_version"] != "recursive-improvement-pass.v1" {
                return Err(invalid("Unsupported recursive-pass schema"));
            }
            let id = value["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("Recursive pass ID missing"))?
                .to_string();
            if document.recursive_passes.insert(id, value).is_some() {
                return Err(invalid("Duplicate recursive-pass ID"));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|e| invalid(e.to_string()))?
                .to_string_lossy()
                .into_owned();
            document
                .legacy_digests
                .insert(relative, bytes_digest(&bytes));
            found = true;
        }
    }
    if found {
        prune_epochs(&mut document)?;
        validate_document(&document)?;
        Ok(Some(document))
    } else {
        Ok(None)
    }
}

fn validate_domain(section: &str, value: &Value) -> Result<(), DecapodError> {
    let version = value.get("schema_version").and_then(Value::as_str);
    match section {
        "plan" => {
            if !matches!(version, Some("1.0.0" | "1.1.0")) {
                return Err(invalid("Unsupported plan schema"));
            }
            serde_json::from_value::<crate::plan_governance::GovernedPlan>(value.clone())
                .map_err(|e| invalid(e.to_string()))?;
        }
        "trajectory" => {
            if !matches!(version, Some("1.0.0" | "1.1.0")) {
                return Err(invalid("Unsupported trajectory schema"));
            }
            let artifact: crate::core::trajectory::TrajectoryArtifact =
                serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
            crate::core::trajectory::validate_loops(&artifact)?;
            if artifact
                .computed_hash_hex()
                .map_err(|e| invalid(e.to_string()))?
                != artifact.artifact_hash
            {
                return Err(invalid("Trajectory artifact hash mismatch"));
            }
        }
        "validation" => {
            let receipt: crate::core::validate::ValidationReceipt =
                serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
            receipt.validate_integrity()?;
        }
        "claims" => {
            if value.get("kind").and_then(Value::as_str) == Some("research_claims_ledger") {
                crate::core::research_claims::validate_legacy_value(value)?;
            } else {
                let claims: BTreeMap<String, ActiveClaim> =
                    serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
                for (id, claim) in claims {
                    validate_claim(&id, &claim)?;
                }
            }
        }
        "jev" => {
            if version != Some("1.0.0") {
                return Err(invalid("Unsupported Jev schema"));
            }
            let ledger: crate::core::jev_history::JevObservationLedger =
                serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
            crate::core::jev_history::validate(&ledger)?;
        }
        _ => return Err(invalid(format!("Unknown governance section {section}"))),
    }
    Ok(())
}

fn set_section(
    document: &mut GovernanceDocument,
    section: &str,
    mut value: Value,
) -> Result<(), DecapodError> {
    validate_domain(section, &value)?;
    if section == "claims" {
        if value.get("kind").and_then(Value::as_str) == Some("research_claims_ledger") {
            document.legacy_claims = Some(value);
        } else {
            document.claims = serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
        }
        return Ok(());
    }
    if section == "trajectory" {
        // Share exactly equal intent text. Do not conflate later scope with initial custody scope.
        if let Some(intents) = value
            .pointer_mut("/custody/intents")
            .and_then(Value::as_object_mut)
        {
            document.intents = std::mem::take(intents).into_iter().collect();
        }
        let intent = value
            .get("intent_id")
            .and_then(Value::as_str)
            .and_then(|id| document.intents.get(id));
        if let Some(intent) = intent {
            for (outer, inner) in [
                ("original_intent", "raw_intent"),
                ("derived_intent", "refined_intent"),
            ] {
                if value.get(outer) == intent.get(inner) {
                    value.as_object_mut().unwrap().remove(outer);
                }
            }
        }
        if let Some(custody) = value.get_mut("custody").and_then(Value::as_object_mut) {
            custody.remove("intents");
            if custody
                .get("summaries")
                .and_then(Value::as_object)
                .is_some_and(|s| s.is_empty())
            {
                custody.remove("summaries");
            }
        }
        // These values are deterministically recomputed by the public trajectory model.
        value.as_object_mut().unwrap().remove("proof_status");
        value.as_object_mut().unwrap().remove("verdicts");
    }
    if section == "validation" {
        if let Some(epoch) = value.as_object_mut().unwrap().remove("validation_epoch") {
            let id = epoch["epoch_id"]
                .as_str()
                .ok_or_else(|| invalid("Validation epoch ID missing"))?
                .to_string();
            document.epochs.insert(id.clone(), epoch);
            value["epoch_ref"] = json!(id);
        }
        if value.pointer("/ci_prediction/reasons") == value.get("warnings")
            && value.get("warnings").is_some()
        {
            value["ci_prediction"]
                .as_object_mut()
                .unwrap()
                .remove("reasons");
            value["prediction_uses_warnings"] = json!(true);
        }
    }
    document.sections.insert(section.into(), value);
    Ok(())
}

fn get_section(
    document: &GovernanceDocument,
    section: &str,
) -> Result<Option<Value>, DecapodError> {
    if section == "claims" {
        return Ok(Some(document.legacy_claims.clone().unwrap_or(
            serde_json::to_value(&document.claims).map_err(|e| invalid(e.to_string()))?,
        )));
    }
    let Some(mut value) = document.sections.get(section).cloned() else {
        return Ok(None);
    };
    if !value.is_object() {
        return Err(invalid(format!(
            "Invalid governance section {section}: expected an object"
        )));
    }
    if section == "trajectory" {
        if !matches!(
            value.get("schema_version").and_then(Value::as_str),
            Some("1.0.0" | "1.1.0")
        ) {
            return Err(invalid("Unsupported trajectory schema"));
        }
        if value
            .get("custody")
            .is_some_and(|custody| !custody.is_object())
        {
            return Err(invalid("Invalid trajectory custody object"));
        }

        let intent = value
            .get("intent_id")
            .and_then(Value::as_str)
            .and_then(|id| document.intents.get(id));
        if let Some(intent) = intent {
            for (outer, inner) in [
                ("original_intent", "raw_intent"),
                ("derived_intent", "refined_intent"),
            ] {
                if value.get(outer).is_none() {
                    value[outer] = intent.get(inner).cloned().unwrap_or(Value::Null);
                }
            }
        }
        value["custody"]["intents"] =
            serde_json::to_value(&document.intents).map_err(|e| invalid(e.to_string()))?;
        if value["custody"].get("summaries").is_none() {
            value["custody"]["summaries"] = json!({});
        }
        value["proof_status"] = json!("no_checks_run");
        value["verdicts"] = json!({"intent_alignment":"unassessed","boundary_discipline":"unassessed","shortcut_risk":"unassessed","completion_proof":"unassessed"});
        let artifact: crate::core::trajectory::TrajectoryArtifact =
            serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
        let recomputed = artifact
            .with_recomputed_hash()
            .map_err(|e| invalid(e.to_string()))?;
        // Keep the stored hash so validation can detect tampering rather than silently bless it.
        let stored = value["artifact_hash"].clone();
        value = serde_json::to_value(recomputed).map_err(|e| invalid(e.to_string()))?;
        value["artifact_hash"] = stored;
    }
    if section == "validation" {
        if let Some(id) = value.as_object_mut().unwrap().remove("epoch_ref") {
            let epoch = id
                .as_str()
                .and_then(|id| document.epochs.get(id))
                .ok_or_else(|| invalid("Dangling validation epoch reference"))?;
            value["validation_epoch"] = epoch.clone();
        }
        if value
            .as_object_mut()
            .unwrap()
            .remove("prediction_uses_warnings")
            == Some(json!(true))
        {
            if !value.get("ci_prediction").is_some_and(Value::is_object) {
                return Err(invalid("Invalid validation prediction object"));
            }
            value["ci_prediction"]["reasons"] = value["warnings"].clone();
        }
    }
    Ok(Some(value))
}

fn validate_claim(id: &str, claim: &ActiveClaim) -> Result<(), DecapodError> {
    if id.trim().is_empty()
        || claim.statement.trim().is_empty()
        || claim.falsifier.trim().is_empty()
        || !["open", "supported", "refuted", "blocked"].contains(&claim.status.as_str())
        || claim
            .proof_refs
            .iter()
            .any(|reference| reference.trim().is_empty())
        || (claim.status == "supported" && claim.proof_refs.is_empty())
    {
        return Err(invalid(
            "Claims require an ID, statement, falsifier, bounded status, and proof references for support",
        ));
    }
    Ok(())
}

fn live_epoch_refs(document: &GovernanceDocument) -> Result<BTreeSet<String>, DecapodError> {
    fn collect(value: &Value, refs: &mut BTreeSet<String>, proof: bool) {
        match value {
            Value::String(reference) if proof => {
                if let Some(id) = reference.strip_prefix("epoch:") {
                    refs.insert(id.to_string());
                }
            }
            Value::Array(items) => {
                for item in items {
                    collect(item, refs, proof);
                }
            }
            Value::Object(object) => {
                for (key, item) in object {
                    collect(
                        item,
                        refs,
                        proof
                            || [
                                "proof_refs",
                                "state_refs",
                                "artifact_ref",
                                "evidence_artifact",
                                "decision_ref",
                                "ref",
                            ]
                            .contains(&key.as_str()),
                    );
                }
            }
            _ => (),
        }
    }
    let mut value = serde_json::to_value(document).map_err(|e| invalid(e.to_string()))?;
    value.as_object_mut().unwrap().remove("epochs");
    let mut refs = BTreeSet::new();
    collect(&value, &mut refs, false);
    if let Some(id) = document
        .sections
        .get("validation")
        .and_then(|v| v.get("epoch_ref"))
        .and_then(Value::as_str)
    {
        refs.insert(id.to_string());
    }
    // Older callers may use the raw epoch ID in checkpoint references.
    for checkpoint in &document.checkpoints {
        for reference in &checkpoint.proof_refs {
            if document.epochs.contains_key(reference) {
                refs.insert(reference.clone());
            }
        }
    }
    Ok(refs)
}

fn prune_epochs(document: &mut GovernanceDocument) -> Result<(), DecapodError> {
    let live = live_epoch_refs(document)?;
    document.epochs.retain(|id, _| live.contains(id));
    Ok(())
}

fn validate_document(document: &GovernanceDocument) -> Result<(), DecapodError> {
    if document
        .legacy_digests
        .keys()
        .any(|path| !recognized_legacy_path(path))
    {
        return Err(invalid(
            "GOVERNANCE_UNSAFE_MIGRATION_PATH: unrecognized persisted cleanup path",
        ));
    }
    if document.schema_version != SCHEMA_VERSION {
        return Err(invalid("Unsupported governance schema"));
    }
    for section in document.sections.keys() {
        if !SECTIONS.contains(&section.as_str()) {
            return Err(invalid(format!("Unknown governance section {section}")));
        }
        if let Some(value) = get_section(document, section)? {
            validate_domain(section, &value)?;
        }
    }
    for id in live_epoch_refs(document)? {
        if !document.epochs.contains_key(&id) {
            return Err(invalid(format!("GOVERNANCE_DANGLING_EPOCH: epoch:{id}")));
        }
    }
    for resolution in document.resolutions.values() {
        if resolution.resolution.trim().is_empty()
            || resolution.proof_refs.is_empty()
            || resolution.proof_refs.iter().any(|r| r.trim().is_empty())
        {
            return Err(invalid(
                "Obligation resolutions require substantive text and proof references",
            ));
        }
    }
    for value in document.recursive_passes.values() {
        crate::core::validate::validate_recursive_value(value)?;
    }
    for (id, claim) in &document.claims {
        validate_claim(id, claim)?;
    }
    if let Some(legacy) = &document.legacy_claims {
        crate::core::research_claims::validate_legacy_value(legacy)?;
    }
    let mut ids = BTreeSet::new();
    for checkpoint in &document.checkpoints {
        if checkpoint.id.is_empty()
            || checkpoint.summary.trim().is_empty()
            || !ids.insert(&checkpoint.id)
            || checkpoint
                .proof_refs
                .iter()
                .any(|reference| reference.trim().is_empty())
            || !checkpoint.input_digest.starts_with("sha256:")
        {
            return Err(invalid("Invalid or duplicate governance checkpoint"));
        }
    }
    if let Some(change) = &document.change {
        if change.id.trim().is_empty()
            || change.base_commit.is_empty()
            || change.target_base.is_empty()
        {
            return Err(invalid("Invalid PR identity"));
        }
    } else if !document.checkpoints.is_empty() || !document.claims.is_empty() {
        return Err(invalid(
            "Active claims and checkpoints require an explicit PR identity",
        ));
    }
    Ok(())
}

thread_local! { static HELD: RefCell<BTreeSet<PathBuf>> = const { RefCell::new(BTreeSet::new()) }; }
struct HeldGuard {
    path: PathBuf,
    file: fs::File,
}
impl Drop for HeldGuard {
    fn drop(&mut self) {
        HELD.with(|held| {
            held.borrow_mut().remove(&self.path);
        });
        let _ = FileExt::unlock(&self.file);
    }
}

/// Reentrant on this thread only; distinct threads/processes must acquire the OS lock.
pub fn with_lock<T>(
    root: &Path,
    f: impl FnOnce() -> Result<T, DecapodError>,
) -> Result<T, DecapodError> {
    let root = root.canonicalize().map_err(DecapodError::IoError)?;
    let path = root.join(".decapod/data/governance-document.lock");
    if HELD.with(|held| held.borrow().contains(&path)) {
        return f();
    }
    crate::core::fs_permissions::ensure_storage_dir(path.parent().unwrap())
        .map_err(DecapodError::IoError)?;
    let file = crate::core::fs_permissions::open_storage_file(
        &path,
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false),
    )
    .map_err(DecapodError::IoError)?;
    let start = Instant::now();
    loop {
        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => break,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_secs(5) =>
            {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(e) => {
                return Err(invalid(format!(
                    "GOVERNANCE_LOCK_BUSY: bounded document lock unavailable: {e}"
                )));
            }
        }
    }
    HELD.with(|held| {
        held.borrow_mut().insert(path.clone());
    });
    let _guard = HeldGuard { path, file };
    f()
}

fn save(root: &Path, document: &GovernanceDocument) -> Result<(), DecapodError> {
    validate_document(document)?;
    check_legacy_conflicts(root, document)?;
    crate::core::workunit::validate_legacy_workunits(root)?;
    let mut bytes = serde_json::to_vec_pretty(document).map_err(|e| invalid(e.to_string()))?;
    bytes.push(b'\n');
    let canonical = root.join(GOVERNANCE_PATH);
    if fs::read(&canonical).ok().as_deref() != Some(bytes.as_slice()) {
        crate::core::atomic::write_atomic(&canonical, &bytes).map_err(DecapodError::IoError)?;
    }
    // Write first. Interrupted cleanup is recoverable using the recorded input digests.
    for (path, expected) in &document.legacy_digests {
        let file = safe_legacy_file(root, path)?;
        if file.exists() {
            if bytes_digest(&fs::read(&file).map_err(DecapodError::IoError)?) != *expected {
                return Err(invalid(
                    "Legacy file changed during migration; canonical document preserved, cleanup stopped",
                ));
            }
            fs::remove_file(file).map_err(DecapodError::IoError)?;
        }
    }
    crate::core::workunit::migrate_legacy_workunits(root)?;
    let _ = fs::remove_dir(root.join(".decapod/governance/recursive_passes"));
    let _ = fs::remove_dir(root.join(".decapod/governance"));
    Ok(())
}

pub fn migrate(root: &Path) -> Result<GovernanceDocument, DecapodError> {
    with_lock(root, || {
        let document = load(root)?.unwrap_or_default();
        save(root, &document)?;
        Ok(document)
    })
}

pub fn read_section(root: &Path, section: &str) -> Result<Option<Value>, DecapodError> {
    load(root)?
        .map(|document| get_section(&document, section))
        .transpose()
        .map(Option::flatten)
}

/// Apply a related logical batch with one locked read, validation, and atomic save.
/// No partial section becomes visible if any update is invalid. Identical bytes
/// are a no-op, while legacy cleanup is still completed when necessary.
pub fn write_sections(root: &Path, updates: &[(&str, Option<Value>)]) -> Result<(), DecapodError> {
    with_lock(root, || {
        let mut document = load(root)?.unwrap_or_default();
        for (section, value) in updates {
            if !SECTIONS.contains(section) && *section != "claims" {
                return Err(invalid(format!("Unknown governance section {section}")));
            }
            match value {
                Some(value) => set_section(&mut document, section, value.clone())?,
                None if *section == "claims" => {
                    document.claims.clear();
                    document.legacy_claims = None;
                }
                None => {
                    document.sections.remove(*section);
                }
            }
        }
        prune_epochs(&mut document)?;
        save(root, &document)
    })
}

pub fn write_section(root: &Path, section: &str, value: &Value) -> Result<(), DecapodError> {
    write_sections(root, &[(section, Some(value.clone()))])
}

pub fn remove_section(root: &Path, section: &str) -> Result<(), DecapodError> {
    write_sections(root, &[(section, None)])
}

pub fn section_hash(root: &Path, section: &str) -> Result<String, DecapodError> {
    match read_section(root, section)? {
        Some(value) => digest(&value),
        None => Ok("missing".into()),
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, DecapodError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(invalid(
            "GOVERNANCE_GIT_PROOF_UNAVAILABLE: inspect repository history and retry",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

/// Bind durable policy, PR identity and carried obligations without binding
/// generated receipts/checkpoints back to themselves.
pub fn validation_context_hash(root: &Path) -> Result<String, DecapodError> {
    let document = load(root)?.unwrap_or_default();
    digest(&json!({
        "baseline": document.baseline,
        "change": document.change,
        "resolutions": document.resolutions,
        "recursive_passes": document.recursive_passes,
    }))
}

fn imported_from_git(
    root: &Path,
    document: &GovernanceDocument,
) -> Result<GovernanceDocument, DecapodError> {
    let mut imported = GovernanceDocument::default();
    for (path, expected) in &document.legacy_digests {
        if !recognized_legacy_path(path) {
            return Err(invalid("Unsafe legacy proof reference"));
        }
        let output = Command::new("git")
            .args(["show", &format!("HEAD:{path}")])
            .current_dir(root)
            .output()
            .map_err(DecapodError::IoError)?;
        if !output.status.success() || bytes_digest(&output.stdout) != *expected {
            return Err(invalid(format!(
                "GOVERNANCE_UNARCHIVED_WORK: {path} differs from committed legacy evidence"
            )));
        }
        let value: Value =
            serde_json::from_slice(&output.stdout).map_err(|e| invalid(e.to_string()))?;
        if let Some((section, _)) = LEGACY.iter().find(|(_, legacy)| legacy == path) {
            set_section(&mut imported, section, value)?;
        } else {
            let id = value["id"]
                .as_str()
                .ok_or_else(|| invalid("Legacy pass ID missing"))?
                .to_string();
            imported.recursive_passes.insert(id, value);
        }
        imported
            .legacy_digests
            .insert(path.clone(), expected.clone());
    }
    validate_document(&imported)?;
    Ok(imported)
}

fn recoverable_in_git(root: &Path, document: &GovernanceDocument) -> Result<(), DecapodError> {
    if serde_json::to_value(document).map_err(|e| invalid(e.to_string()))?
        == serde_json::to_value(GovernanceDocument::default())
            .map_err(|e| invalid(e.to_string()))?
    {
        return Ok(());
    }
    let archived = if root.join(GOVERNANCE_PATH).exists() {
        match git(root, &["show", &format!("HEAD:{GOVERNANCE_PATH}")]) {
            Ok(content) => parse(content.as_bytes())?,
            Err(_) if !document.legacy_digests.is_empty() => imported_from_git(root, document)?,
            Err(_) => {
                return Err(invalid(
                    "GOVERNANCE_UNARCHIVED_WORK: no committed source can preserve the current document",
                ));
            }
        }
    } else {
        imported_from_git(root, document)?
    };
    if digest(&serde_json::to_value(archived).map_err(|e| invalid(e.to_string()))?)?
        != digest(&serde_json::to_value(document).map_err(|e| invalid(e.to_string()))?)?
    {
        return Err(invalid(
            "GOVERNANCE_UNARCHIVED_WORK: commit the prior work before starting a compact new PR",
        ));
    }
    Ok(())
}

pub fn begin_pr(
    root: &Path,
    change_id: &str,
    base_ref: &str,
) -> Result<GovernanceDocument, DecapodError> {
    if change_id.trim().is_empty() {
        return Err(invalid("A stable PR change ID is required"));
    }
    with_lock(root, || {
        let previous = load(root)?.unwrap_or_default();
        if let Some(change) = &previous.change
            && change.id == change_id
        {
            if change.target_base != base_ref {
                return Err(invalid(
                    "GOVERNANCE_PR_IDENTITY_CONFLICT: the same PR ID cannot silently change its target base",
                ));
            }
            return Ok(previous);
        }
        if root.join(GOVERNANCE_PATH).exists() || !previous.legacy_digests.is_empty() {
            recoverable_in_git(root, &previous)?;
        }
        let base_commit = git(
            root,
            &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
        )?;
        if let Some(old_change) = &previous.change {
            let accepted_text = git(root, &["show", &format!("{base_commit}:{GOVERNANCE_PATH}")])
                .map_err(|_| {
                invalid("GOVERNANCE_PR_NOT_ACCEPTED: previous PR record is absent from target base")
            })?;
            let accepted = parse(accepted_text.as_bytes())?;
            if accepted.change.as_ref().map(|change| change.id.as_str())
                != Some(old_change.id.as_str())
                || digest(&serde_json::to_value(&accepted).map_err(|e| invalid(e.to_string()))?)?
                    != digest(
                        &serde_json::to_value(&previous).map_err(|e| invalid(e.to_string()))?,
                    )?
            {
                return Err(invalid(
                    "GOVERNANCE_PR_NOT_ACCEPTED: merge/accept the previous record into its target base before starting a compact new PR",
                ));
            }
            if old_change.target_base != base_ref {
                return Err(invalid(
                    "GOVERNANCE_PR_TARGET_CHANGED: start from the accepted target, do not relabel unrelated base history",
                ));
            }
        }
        let mut baseline = previous.baseline.clone();
        baseline.accepted_commit = Some(base_commit.clone());
        baseline.release = git(root, &["describe", "--tags", "--exact-match", &base_commit]).ok();
        for (id, claim) in &previous.claims {
            if matches!(claim.status.as_str(), "open" | "blocked") {
                baseline.unresolved.insert(
                    format!("claim:{id}"),
                    serde_json::to_value(claim).map_err(|e| invalid(e.to_string()))?,
                );
            }
        }
        if let Some(plan) = get_section(&previous, "plan")? {
            for field in [
                "unknowns",
                "human_questions",
                "unresolved_contradictions",
                "deferred_questions",
            ] {
                if let Some(items) = plan.get(field).and_then(Value::as_array) {
                    for item in items {
                        if item.as_str().is_some_and(|s| !s.trim().is_empty()) {
                            baseline
                                .unresolved
                                .insert(format!("plan:{}", &digest(item)?[7..23]), item.clone());
                        }
                    }
                }
            }
        }
        if let Some(trajectory) = get_section(&previous, "trajectory")? {
            for field in ["unresolved_assumptions", "blockers"] {
                if let Some(items) = trajectory.get(field).and_then(Value::as_array) {
                    for item in items {
                        baseline.unresolved.insert(
                            format!("trajectory:{}", &digest(item)?[7..23]),
                            item.clone(),
                        );
                    }
                }
            }
        }
        if let Some(plan) = get_section(&previous, "plan")?
            && let Some(reviews) = plan.get("spec_reviews").and_then(Value::as_array)
        {
            for review in reviews {
                if review.get("disposition").and_then(Value::as_str) == Some("requires_decision") {
                    baseline.unresolved.insert(
                        format!(
                            "spec-review:{}",
                            review["path"].as_str().unwrap_or("unknown")
                        ),
                        review.clone(),
                    );
                }
            }
        }
        if let Some(ledger) = &previous.legacy_claims {
            if let Some(claims) = ledger.get("claims").and_then(Value::as_array) {
                for claim in claims {
                    if let Some(questions) = claim.get("open_questions").and_then(Value::as_array) {
                        for question in questions {
                            if matches!(
                                question.get("status").and_then(Value::as_str),
                                Some("open" | "deferred")
                            ) {
                                baseline.unresolved.insert(
                                    format!(
                                        "claim:{}:question:{}",
                                        claim["id"].as_str().unwrap_or("unknown"),
                                        question["id"].as_str().unwrap_or("unknown")
                                    ),
                                    question.clone(),
                                );
                            }
                        }
                    }
                }
            }
            for field in ["review_policy", "new_capability_rule", "change_control"] {
                if let Some(value) = ledger.pointer(&format!("/governance/{field}")) {
                    baseline.policy.insert(field.into(), value.clone());
                }
            }
        }
        let mut document = GovernanceDocument {
            baseline,
            change: Some(ChangeIdentity {
                id: change_id.into(),
                base_commit,
                target_base: base_ref.into(),
            }),
            legacy_digests: previous.legacy_digests,
            epochs: previous.epochs,
            ..Default::default()
        };
        prune_epochs(&mut document)?;
        save(root, &document)?;
        Ok(document)
    })
}

pub fn record_claim(
    root: &Path,
    id: &str,
    statement: &str,
    falsifier: &str,
    status: &str,
    mut proof_refs: Vec<String>,
) -> Result<GovernanceDocument, DecapodError> {
    proof_refs.sort();
    proof_refs.dedup();
    let claim = ActiveClaim {
        statement: statement.into(),
        falsifier: falsifier.into(),
        status: status.into(),
        proof_refs,
    };
    validate_claim(id, &claim)?;
    with_lock(root, || {
        let mut document =
            load(root)?.ok_or_else(|| invalid("Begin a PR before recording claims"))?;
        if document.change.is_none() {
            return Err(invalid("Begin a PR before recording claims"));
        }
        document.claims.insert(id.into(), claim);
        save(root, &document)?;
        Ok(document)
    })
}

pub fn resolve_obligation(
    root: &Path,
    id: &str,
    resolution: &str,
    mut proof_refs: Vec<String>,
) -> Result<GovernanceDocument, DecapodError> {
    if resolution.trim().is_empty()
        || proof_refs.is_empty()
        || proof_refs.iter().any(|r| r.trim().is_empty())
    {
        return Err(invalid(
            "Obligation resolution requires a substantive answer and supporting proof reference",
        ));
    }
    proof_refs.sort();
    proof_refs.dedup();
    with_lock(root, || {
        let mut document =
            load(root)?.ok_or_else(|| invalid("Begin a PR before resolving obligations"))?;
        if document.change.is_none() {
            return Err(invalid("Begin a PR before resolving obligations"));
        }
        let obligation = document
            .baseline
            .unresolved
            .remove(id)
            .ok_or_else(|| invalid("Unknown or already resolved obligation"))?;
        document.resolutions.insert(
            id.into(),
            ObligationResolution {
                obligation_digest: digest(&obligation)?,
                resolution: resolution.into(),
                proof_refs,
            },
        );
        prune_epochs(&mut document)?;
        save(root, &document)?;
        Ok(document)
    })
}

fn material_path(path: &str) -> bool {
    path != GOVERNANCE_PATH && !path.starts_with(".decapod/governance/")
}

/// Use Git object IDs and modes so the index and immutable commit use identical input bytes.
fn tree_material(root: &Path, revision: Option<&str>) -> Result<Value, DecapodError> {
    let output = if let Some(revision) = revision {
        Command::new("git")
            .args(["ls-tree", "-rz", "--full-tree", revision])
            .current_dir(root)
            .output()
    } else {
        Command::new("git")
            .args(["ls-files", "--stage", "-z"])
            .current_dir(root)
            .output()
    }
    .map_err(DecapodError::IoError)?;
    if !output.status.success() {
        return Err(invalid("Cannot inspect material Git tree"));
    }
    let mut files = BTreeMap::new();
    for entry in output.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let entry = std::str::from_utf8(entry)
            .map_err(|_| invalid("Non-UTF-8 Git path is unsupported by governance proof"))?;
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or_else(|| invalid("Malformed Git tree entry"))?;
        if !material_path(path) {
            continue;
        }
        let columns: Vec<_> = metadata.split_whitespace().collect();
        if columns.len() != 3 {
            return Err(invalid("Malformed Git object metadata"));
        }
        let (mode, object) = if revision.is_some() {
            (columns[0], columns[2])
        } else {
            if columns[2] != "0" {
                return Err(invalid("Resolve index conflicts before checkpointing"));
            }
            (columns[0], columns[1])
        };
        files.insert(path.to_string(), format!("{mode}:{object}"));
    }
    serde_json::to_value(files).map_err(|e| invalid(e.to_string()))
}

fn material_digest(
    root: &Path,
    document: &GovernanceDocument,
    revision: Option<&str>,
) -> Result<String, DecapodError> {
    let mut material = serde_json::Map::new();
    for section in ["plan", "trajectory", "claims", "jev"] {
        material.insert(
            section.into(),
            get_section(document, section)?.unwrap_or(Value::Null),
        );
    }
    material.insert(
        "baseline".into(),
        serde_json::to_value(&document.baseline).map_err(|e| invalid(e.to_string()))?,
    );
    material.insert(
        "change".into(),
        serde_json::to_value(&document.change).map_err(|e| invalid(e.to_string()))?,
    );
    material.insert(
        "recursive_passes".into(),
        serde_json::to_value(&document.recursive_passes).map_err(|e| invalid(e.to_string()))?,
    );
    material.insert(
        "resolutions".into(),
        serde_json::to_value(&document.resolutions).map_err(|e| invalid(e.to_string()))?,
    );
    // Epoch metadata is normalized proof material, not the generated receipt.
    // Bind its content so a stable epoch reference cannot be silently retargeted.
    material.insert(
        "epochs".into(),
        serde_json::to_value(&document.epochs).map_err(|e| invalid(e.to_string()))?,
    );
    material.insert("tree".into(), tree_material(root, revision)?);
    digest(&Value::Object(material))
}

fn require_staged_material(root: &Path) -> Result<(), DecapodError> {
    let changed = git(root, &["diff", "--name-only"])?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard"])?;
    if changed.lines().chain(untracked.lines()).any(material_path) {
        return Err(invalid(
            "GOVERNANCE_UNSTAGED_MATERIAL: stage authored code/spec changes before checkpoint; stage governance.json after checkpoint",
        ));
    }
    Ok(())
}

pub fn checkpoint(
    root: &Path,
    id: &str,
    summary: &str,
    mut proof_refs: Vec<String>,
) -> Result<GovernanceDocument, DecapodError> {
    if id.trim().is_empty() || summary.trim().is_empty() {
        return Err(invalid("A checkpoint needs an ID and substantive summary"));
    }
    proof_refs.sort();
    proof_refs.dedup();
    require_staged_material(root)?;
    with_lock(root, || {
        let mut document =
            load(root)?.ok_or_else(|| invalid("Begin a PR before recording checkpoints"))?;
        if document.change.is_none() {
            return Err(invalid("Begin a PR before recording checkpoints"));
        }
        let checkpoint = Checkpoint {
            id: id.into(),
            summary: summary.into(),
            input_digest: material_digest(root, &document, None)?,
            proof_refs,
        };
        if let Some(existing) = document.checkpoints.iter_mut().find(|entry| entry.id == id) {
            *existing = checkpoint;
        } else {
            document.checkpoints.push(checkpoint);
        }
        save(root, &document)?;
        Ok(document)
    })
}

fn normalized_base_label(reference: &str) -> &str {
    reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/remotes/origin/"))
        .or_else(|| reference.strip_prefix("origin/"))
        .unwrap_or(reference)
}

fn verify_base_binding(
    root: &Path,
    document: &GovernanceDocument,
    base_ref: &str,
    head_ref: &str,
    target_base: Option<&str>,
) -> Result<(), DecapodError> {
    let change = document
        .change
        .as_ref()
        .ok_or_else(|| invalid("GOVERNANCE_PR_IDENTITY_MISSING"))?;
    if let Some(target) = target_base
        && normalized_base_label(&change.target_base) != normalized_base_label(target)
    {
        return Err(invalid(
            "GOVERNANCE_PR_TARGET_MISMATCH: document target differs from captured publication base",
        ));
    }
    for descendant in [base_ref, head_ref] {
        let status = Command::new("git")
            .args([
                "merge-base",
                "--is-ancestor",
                &change.base_commit,
                descendant,
            ])
            .current_dir(root)
            .status()
            .map_err(DecapodError::IoError)?;
        if !status.success() {
            return Err(invalid(
                "GOVERNANCE_PR_BASE_MISMATCH: recorded base must be an ancestor of the verified base and head",
            ));
        }
    }
    if document.baseline.accepted_commit.as_deref() != Some(&change.base_commit) {
        return Err(invalid("Governance accepted baseline and PR base differ"));
    }
    if let Ok(content) = git(root, &["show", &format!("{base_ref}:{GOVERNANCE_PATH}")]) {
        let baseline = parse(content.as_bytes())?;
        if baseline.change.as_ref().map(|prior| prior.id.as_str()) == Some(change.id.as_str()) {
            return Err(invalid(
                "GOVERNANCE_PR_RESET_REQUIRED: merged PR identity was reused; explicitly begin a new PR",
            ));
        }
    }
    Ok(())
}

pub fn verify_pr_checkpoints_for_target(
    root: &Path,
    base_ref: &str,
    head_ref: &str,
    target_base: &str,
) -> Result<(), DecapodError> {
    verify_pr_checkpoints_inner(root, base_ref, head_ref, Some(target_base))
}

pub fn verify_pr_checkpoints(
    root: &Path,
    base_ref: &str,
    head_ref: &str,
) -> Result<(), DecapodError> {
    let named = if base_ref.bytes().all(|b| b.is_ascii_hexdigit()) {
        None
    } else {
        Some(base_ref)
    };
    verify_pr_checkpoints_inner(root, base_ref, head_ref, named)
}

pub fn verify_staged_checkpoint(root: &Path) -> Result<(), DecapodError> {
    require_staged_material(root)?;
    let content = git(root, &["show", &format!(":{GOVERNANCE_PATH}")]).map_err(|_| {
        invalid("GOVERNANCE_CHECKPOINT_MISSING: stage governance.json after recording a checkpoint")
    })?;
    let document = parse(content.as_bytes())?;
    let change = document
        .change
        .as_ref()
        .ok_or_else(|| invalid("GOVERNANCE_PR_IDENTITY_MISSING"))?;
    let previous = git(root, &["show", &format!("HEAD:{GOVERNANCE_PATH}")])
        .ok()
        .map(|text| parse(text.as_bytes()))
        .transpose()?;
    let mut previous_ids = BTreeSet::new();
    if let Some(previous) = previous.filter(|prior| {
        prior
            .change
            .as_ref()
            .is_some_and(|prior| prior.id == change.id)
    }) {
        for old in previous.checkpoints {
            previous_ids.insert(old.id.clone());
            if !document.checkpoints.iter().any(|entry| {
                entry.id == old.id
                    && entry.input_digest == old.input_digest
                    && entry.summary == old.summary
                    && entry.proof_refs == old.proof_refs
            }) {
                return Err(invalid(
                    "GOVERNANCE_CHECKPOINT_HISTORY_CHANGED: restore prior checkpoints before committing",
                ));
            }
        }
    }
    let expected = material_digest(root, &document, None)?;
    if !document
        .checkpoints
        .iter()
        .any(|entry| entry.input_digest == expected && !previous_ids.contains(&entry.id))
    {
        return Err(invalid(
            "GOVERNANCE_CHECKPOINT_MISSING: run govern artifacts checkpoint with a new ID after staging material, then stage governance.json; no commit was created",
        ));
    }
    Ok(())
}

fn verify_pr_checkpoints_inner(
    root: &Path,
    base_ref: &str,
    head_ref: &str,
    target_base: Option<&str>,
) -> Result<(), DecapodError> {
    // Always bind the exact final head, including merge-only ranges.
    let head_text =
        git(root, &["show", &format!("{head_ref}:{GOVERNANCE_PATH}")]).map_err(|_| {
            invalid("GOVERNANCE_CHECKPOINT_MISSING: final head has no canonical record")
        })?;
    let head = parse(head_text.as_bytes())?;
    verify_base_binding(root, &head, base_ref, head_ref, target_base)?;
    let identity = head
        .change
        .as_ref()
        .ok_or_else(|| invalid("GOVERNANCE_PR_IDENTITY_MISSING: final head"))?
        .id
        .clone();
    let commits = git(
        root,
        &[
            "rev-list",
            "--reverse",
            "--topo-order",
            &format!("{base_ref}..{head_ref}"),
        ],
    )?;
    let base_has_document = Command::new("git")
        .args(["cat-file", "-e", &format!("{base_ref}:{GOVERNANCE_PATH}")])
        .current_dir(root)
        .output()
        .map_err(DecapodError::IoError)?
        .status
        .success();
    let mut verified = false;
    for commit in commits.lines().filter(|line| !line.is_empty()) {
        let content = match git(root, &["show", &format!("{commit}:{GOVERNANCE_PATH}")]) {
            Ok(content) => content,
            Err(_)
                if !base_has_document
                    && git(
                        root,
                        &["log", "-1", "--format=%H", commit, "--", GOVERNANCE_PATH],
                    )?
                    .is_empty() =>
            {
                continue;
            }
            Err(_) => {
                return Err(invalid(format!(
                    "GOVERNANCE_CHECKPOINT_MISSING: commit {commit} has no canonical record"
                )));
            }
        };
        let document = parse(content.as_bytes())?;
        if document
            .change
            .as_ref()
            .map(|change| (&change.base_commit, &change.target_base))
            != head
                .change
                .as_ref()
                .map(|change| (&change.base_commit, &change.target_base))
        {
            return Err(invalid(
                "GOVERNANCE_PR_BASE_MISMATCH: base identity changed inside the PR",
            ));
        }
        if document.change.as_ref().map(|change| change.id.as_str()) != Some(identity.as_str()) {
            return Err(invalid(format!(
                "GOVERNANCE_PR_IDENTITY_CONFLICT: commit {commit} belongs to a different PR"
            )));
        }
        let parent_line = git(root, &["rev-list", "--parents", "-n", "1", commit])?;
        let mut prior_ids = BTreeSet::new();
        for parent in parent_line.split_whitespace().skip(1) {
            let Ok(parent_text) = git(root, &["show", &format!("{parent}:{GOVERNANCE_PATH}")])
            else {
                continue;
            };
            let prior = parse(parent_text.as_bytes())?;
            if prior.change.as_ref().map(|change| change.id.as_str()) != Some(identity.as_str()) {
                continue;
            }
            for old in &prior.checkpoints {
                prior_ids.insert(old.id.clone());
                if !document.checkpoints.iter().any(|entry| {
                    entry.id == old.id
                        && entry.input_digest == old.input_digest
                        && entry.summary == old.summary
                        && entry.proof_refs == old.proof_refs
                }) {
                    return Err(invalid(format!(
                        "GOVERNANCE_CHECKPOINT_HISTORY_CHANGED: commit {commit} erased or rewrote a parent checkpoint"
                    )));
                }
            }
        }
        let expected = material_digest(root, &document, Some(commit))?;
        if !document
            .checkpoints
            .iter()
            .any(|entry| entry.input_digest == expected && !prior_ids.contains(&entry.id))
        {
            return Err(invalid(format!(
                "GOVERNANCE_CHECKPOINT_MISSING: commit {commit} needs a new checkpoint bound to its exact material tree and governance inputs"
            )));
        }
        verified = true;
    }
    if !commits.is_empty() && !verified {
        return Err(invalid(
            "GOVERNANCE_CHECKPOINT_MISSING: PR has no canonical checkpoint",
        ));
    }
    let expected_head = material_digest(root, &head, Some(head_ref))?;
    if !head
        .checkpoints
        .iter()
        .any(|entry| entry.input_digest == expected_head)
    {
        return Err(invalid(
            "GOVERNANCE_CHECKPOINT_MISSING: final head material is not checkpointed",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/governance_document_tests.rs"]
mod tests;
