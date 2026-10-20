//! Ephemeral scientific collection within one kernel-provided evidence directory.
//! These bindings detect incomplete/mixed/replayed collection, not a malicious
//! worker or same-user forgery. The accepted controller retains proof authority.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const SHARDS: usize = 6;
const CONTEXT: &str = "vega-scientific-context.json";
const SCHEMA: &str = "decapod.vega-scientific-collection.v1";

// This worker's ephemeral nonce is not a model-installation API or credential.
fn valid_nonce(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Numeric,
    Shard(usize),
}
impl Phase {
    fn name(self) -> Result<String> {
        match self {
            Self::Numeric => Ok("numeric".into()),
            Self::Shard(i) if i < SHARDS => Ok(format!("shard-{i}")),
            _ => Err("invalid collection shard".into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextRecord {
    schema: String,
    directory: PathBuf,
    runtime_sha256: String,
    nonce: String,
    shards: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    file: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    schema: String,
    context_sha256: String,
    phase: Phase,
    collection_only: bool,
    publication_ready: bool,
    pub scientific_checks_passed: bool,
    pub report: Value,
}
pub struct Collection {
    directory: PathBuf,
    context_sha256: String,
    pub runtime_sha256: String,
}

pub fn runtime_digest() -> Result<String> {
    Ok(decapod::core::vega_readiness::hash(&super::read_bounded(
        &std::env::current_exe()?,
        256 * 1024 * 1024,
    )?))
}

fn directory(proof: &Path) -> Result<PathBuf> {
    let parent = proof
        .parent()
        .ok_or("missing collection directory")?
        .canonicalize()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(&parent)?;
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("collection directory must be private and locally owned".into());
        }
    }
    Ok(parent)
}
fn filename(file: &str) -> Result<()> {
    if file.is_empty()
        || file.len() > 128
        || !file.ends_with(".json")
        || !file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || file.contains("..")
        || file == CONTEXT
    {
        return Err("unsafe collection receipt filename".into());
    }
    Ok(())
}
#[cfg(unix)]
fn private_bytes(path: &Path) -> Result<Vec<u8>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() > super::MAX_PROOF_BYTES as u64
    {
        return Err("unsafe or oversized scientific receipt".into());
    }
    let mut bytes = Vec::new();
    file.take(super::MAX_PROOF_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > super::MAX_PROOF_BYTES {
        return Err("scientific receipt grew beyond bound".into());
    }
    Ok(bytes)
}
#[cfg(not(unix))]
fn private_bytes(_: &Path) -> Result<Vec<u8>> {
    Err("scientific collection requires reviewed private-file platform support".into())
}

impl Collection {
    /// Preserve every independently collected phase even when strict aggregate
    /// verification stops at an earlier missing export. These are bound
    /// diagnostics, not a substitute for rechecking their scientific contents.
    pub fn diagnostics(&self) -> Value {
        let phases = std::iter::once(Phase::Numeric).chain((0..SHARDS).map(Phase::Shard));
        let reports: Vec<_> = phases
            .map(|phase| match self.read(phase) {
                Ok(envelope) => serde_json::json!({
                    "phase":phase,"binding_verified":true,
                    "scientific_contents_reverified":false,"report":envelope.report
                }),
                Err(error) => serde_json::json!({
                    "phase":phase,"binding_verified":false,"error":error.to_string()
                }),
            })
            .collect();
        serde_json::json!({"publication_ready":false,"phases":reports})
    }

    pub fn create(proof: &Path) -> Result<Self> {
        let directory = directory(proof)?;
        let runtime_sha256 = runtime_digest()?;
        let record = ContextRecord {
            schema: SCHEMA.into(),
            directory: directory.clone(),
            runtime_sha256,
            nonce: decapod::core::vega_readiness::hash(
                format!(
                    "{}|{}|{:?}",
                    directory.display(),
                    std::process::id(),
                    std::time::SystemTime::now()
                )
                .as_bytes(),
            ),
            shards: SHARDS,
        };
        // Exclusive publication. Never overwrite/recover an existing context by
        // deleting it; rerun normal validation into a fresh kernel directory.
        super::write_private_proof(&directory.join(CONTEXT), &record)?;
        Self::load(proof)
    }
    pub fn load(proof: &Path) -> Result<Self> {
        let directory = directory(proof)?;
        let bytes = private_bytes(&directory.join(CONTEXT))?;
        let record: ContextRecord = serde_json::from_slice(&bytes)?;
        let runtime_sha256 = runtime_digest()?;
        if record.schema != SCHEMA
            || record.directory != directory
            || record.shards != SHARDS
            || record.runtime_sha256 != runtime_sha256
            || !valid_nonce(&record.nonce)
        {
            return Err("foreign, malformed or stale scientific collection context".into());
        }
        Ok(Self {
            directory,
            runtime_sha256,
            context_sha256: decapod::core::vega_readiness::hash(&bytes),
        })
    }
    fn pointer(&self, phase: Phase) -> Result<PathBuf> {
        Ok(self
            .directory
            .join(format!("vega-collection-{}.json", phase.name()?)))
    }
    pub fn reference(&self, path: &Path) -> Result<Value> {
        if directory(path)? != self.directory {
            return Err("foreign scientific child directory".into());
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid child filename")?;
        filename(name)?;
        let bytes = private_bytes(path)?;
        let _: Value = serde_json::from_slice(&bytes)?;
        Ok(serde_json::to_value(Pointer {
            file: name.into(),
            sha256: decapod::core::vega_readiness::hash(&bytes),
        })?)
    }
    pub fn read_reference(&self, reference: &Value) -> Result<Value> {
        let pointer: Pointer = serde_json::from_value(reference.clone())?;
        filename(&pointer.file)?;
        let bytes = private_bytes(&self.directory.join(pointer.file))?;
        if decapod::core::vega_readiness::hash(&bytes) != pointer.sha256 {
            return Err("scientific child digest differs".into());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub fn read_export(&self, export: &Value) -> Result<Value> {
        let path = Path::new(
            export["file"]
                .as_str()
                .ok_or("missing numeric export path")?,
        );
        if directory(path)? != self.directory || export["max_bytes"] != super::MAX_PROOF_BYTES {
            return Err("foreign or unbounded numeric export".into());
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid numeric export name")?;
        self.read_reference(&serde_json::json!({"file":name,"sha256":export["sha256"]}))
    }
    pub fn publish(&self, phase: Phase, proof: &Path, passed: bool, report: Value) -> Result<()> {
        phase.name()?;
        if directory(proof)? != self.directory {
            return Err("foreign scientific output directory".into());
        }
        let name = proof
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("invalid proof filename")?;
        filename(name)?;
        let envelope = Envelope {
            schema: SCHEMA.into(),
            context_sha256: self.context_sha256.clone(),
            phase,
            collection_only: true,
            publication_ready: false,
            scientific_checks_passed: passed,
            report,
        };
        super::write_private_proof(proof, &envelope)?;
        let bytes = private_bytes(proof)?;
        super::write_private_proof(
            &self.pointer(phase)?,
            &Pointer {
                file: name.into(),
                sha256: decapod::core::vega_readiness::hash(&bytes),
            },
        )?;
        Ok(())
    }
    pub fn read(&self, phase: Phase) -> Result<Envelope> {
        let pointer: Pointer = serde_json::from_slice(&private_bytes(&self.pointer(phase)?)?)?;
        filename(&pointer.file)?;
        let bytes = private_bytes(&self.directory.join(pointer.file))?;
        if decapod::core::vega_readiness::hash(&bytes) != pointer.sha256 {
            return Err("scientific phase digest differs".into());
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if envelope.schema != SCHEMA
            || envelope.context_sha256 != self.context_sha256
            || envelope.phase != phase
            || !envelope.collection_only
            || envelope.publication_ready
        {
            return Err("scientific phase binding or collection-only contract differs".into());
        }
        Ok(envelope)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn nonce_requires_the_canonical_local_hash_format() {
        assert!(valid_nonce(&decapod::core::vega_readiness::hash(
            b"unit fixture"
        )));
        for malformed in [
            String::new(),
            "f".repeat(64),
            format!("sha256:{}", "f".repeat(63)),
            format!("sha256:{}", "F".repeat(64)),
            format!("sha256:{}", "g".repeat(64)),
        ] {
            assert!(!valid_nonce(&malformed));
        }
    }
    fn fixture() -> (tempfile::TempDir, Collection) {
        let mut builder = tempfile::Builder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let directory = builder.tempdir().unwrap();
        let collection = Collection::create(&directory.path().join("numeric.json")).unwrap();
        (directory, collection)
    }
    #[cfg(unix)]
    #[test]
    fn public_directory_cannot_supply_collection_evidence() {
        use std::os::unix::fs::PermissionsExt;
        let (directory, _) = fixture();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let proof = directory.path().join("numeric.json");
        assert!(Collection::load(&proof).is_err());
        assert!(Collection::create(&proof).is_err());
    }
    #[test]
    fn missing_early_phase_keeps_later_diagnostics_without_authorizing_readiness() {
        let (dir, context) = fixture();
        context
            .publish(
                Phase::Shard(5),
                &dir.path().join("last.json"),
                false,
                serde_json::json!({"cpu_receipt":"available_private_evidence"}),
            )
            .unwrap();
        let diagnostics = context.diagnostics();
        assert_eq!(diagnostics["publication_ready"], false);
        let phases = diagnostics["phases"].as_array().unwrap();
        assert_eq!(phases.len(), SHARDS + 1);
        assert_eq!(phases[0]["binding_verified"], false);
        assert_eq!(phases[6]["binding_verified"], true);
        assert_eq!(phases[6]["scientific_contents_reverified"], false);
        assert_eq!(
            phases[6]["report"]["cpu_receipt"],
            "available_private_evidence"
        );
    }
    #[test]
    fn collection_success_is_not_scientific_acceptance_and_replay_never_overwrites() {
        let (dir, context) = fixture();
        let proof = dir.path().join("numeric.json");
        context
            .publish(
                Phase::Numeric,
                &proof,
                false,
                serde_json::json!({"synthetic_fixture":true}),
            )
            .unwrap();
        assert!(
            !context
                .read(Phase::Numeric)
                .unwrap()
                .scientific_checks_passed
        );
        assert!(
            context
                .publish(Phase::Numeric, &proof, true, Value::Null)
                .is_err()
        );
        assert!(Collection::create(&proof).is_err());
        assert!(context.read(Phase::Shard(0)).is_err());
        assert!(context.read(Phase::Shard(SHARDS)).is_err());
    }
    #[test]
    fn foreign_context_and_modified_or_oversized_records_fail_closed() {
        let (dir, context) = fixture();
        let (other, other_context) = fixture();
        let proof = dir.path().join("numeric.json");
        context
            .publish(Phase::Numeric, &proof, true, Value::Null)
            .unwrap();
        assert!(
            context
                .publish(
                    Phase::Shard(0),
                    &other.path().join("receipt.json"),
                    true,
                    Value::Null
                )
                .is_err()
        );
        fs::copy(&proof, other.path().join("numeric.json")).unwrap();
        fs::copy(
            context.pointer(Phase::Numeric).unwrap(),
            other_context.pointer(Phase::Numeric).unwrap(),
        )
        .unwrap();
        assert!(other_context.read(Phase::Numeric).is_err());
        fs::write(&proof, b"{}").unwrap();
        assert!(context.read(Phase::Numeric).is_err());
        fs::write(&proof, vec![b' '; super::super::MAX_PROOF_BYTES + 1]).unwrap();
        assert!(private_bytes(&proof).is_err());
        for unsafe_name in ["../other.json", "/other.json", "a/other.json", CONTEXT] {
            assert!(filename(unsafe_name).is_err());
        }
    }
    #[test]
    fn symlinks_and_hardlinks_cannot_supply_phase_evidence() {
        let (dir, context) = fixture();
        let proof = dir.path().join("numeric.json");
        context
            .publish(Phase::Numeric, &proof, true, Value::Null)
            .unwrap();
        fs::hard_link(&proof, dir.path().join("hard.json")).unwrap();
        assert!(context.read(Phase::Numeric).is_err());
        std::os::unix::fs::symlink(&proof, dir.path().join("link.json")).unwrap();
        assert!(private_bytes(&dir.path().join("link.json")).is_err());
    }
    #[test]
    fn changed_runtime_or_a_ready_claim_fails_even_with_rehashed_fixture_bytes() {
        let (dir, context) = fixture();
        let proof = dir.path().join("numeric.json");
        context
            .publish(
                Phase::Numeric,
                &proof,
                true,
                serde_json::json!({"synthetic_fixture":true}),
            )
            .unwrap();
        let mut envelope: Value = serde_json::from_slice(&fs::read(&proof).unwrap()).unwrap();
        envelope["publication_ready"] = true.into();
        let bytes = serde_json::to_vec(&envelope).unwrap();
        fs::write(&proof, &bytes).unwrap();
        fs::write(
            context.pointer(Phase::Numeric).unwrap(),
            serde_json::to_vec(&Pointer {
                file: "numeric.json".into(),
                sha256: decapod::core::vega_readiness::hash(&bytes),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(context.read(Phase::Numeric).is_err());
        let mut record: Value =
            serde_json::from_slice(&fs::read(dir.path().join(CONTEXT)).unwrap()).unwrap();
        record["runtime_sha256"] =
            "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into();
        fs::write(
            dir.path().join(CONTEXT),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert!(Collection::load(&proof).is_err());
    }
}
