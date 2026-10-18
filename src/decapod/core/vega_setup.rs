//! Explicit, bounded installation of the native Vega data dependency.
//!
//! Nothing in this module runs during a build. Inference only calls
//! `verify_installation` against an explicitly configured immutable manifest;
//! it never follows `active.json`, discovers versions, or accesses the network.
//! The model and backbone have independent upstream revisions. The source pin
//! identifies the reviewed native-port reference, not an upstream claim about
//! which source revision produced a checkpoint. Adapters are explicitly off.
use super::bounded_process::{
    BoundedCommand, Cancellation, ProcessLimits, SignalCancellationGuard, Termination,
};
use super::{atomic, fs_permissions};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const MODEL: &str = "nandakishorm/vega-08b-public-intents";
pub const BACKBONE: &str = "Qwen/Qwen3.5-0.8B";
pub const SOURCE: &str = "NandhaKishorM/vegaml";
pub const BASELINE_REVISION: &str = "42ac8d4fa9efb61f96e53d80962cf28f6dfe667e";
pub const BASELINE_BACKBONE_REVISION: &str = "2fc06364715b967f1860aea9cf38778875588b17";
pub const BASELINE_SOURCE_REVISION: &str = "21a17c90624aa85cec60b1092fa72cb8b720f71a";
pub const MANIFEST_SCHEMA: &str = "decapod.vega-install.v1";
pub const PROFILE: &str = "vega-0.8b-base-text-v1";
const MAX_METADATA: u64 = 1024 * 1024;
const MAX_ARTIFACT: u64 = 2 * 1024 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(3600);
const SOURCE_LICENSE: &str = "LICENSE";

#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    pub cache_directory: Option<PathBuf>,
    /// Explicitly register this maintainer's already installed native worker.
    pub runtime: Option<PathBuf>,
    /// Optional separately built CPU worker, verified and registered atomically.
    pub cpu_runtime: Option<PathBuf>,
    pub check_updates: bool,
    pub refresh: bool,
    pub offline: bool,
    /// Immutable manifest SHA-256, not a floating upstream branch or tag.
    pub rollback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilePin {
    pub role: String,
    pub repository: String,
    pub revision: String,
    pub upstream_path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstallManifest {
    pub schema: String,
    pub profile: String,
    pub model: String,
    pub revision: String,
    pub backbone: String,
    pub backbone_revision: String,
    pub source: String,
    pub source_revision: String,
    pub license: String,
    /// This profile must never silently ignore a requested adapter.
    pub adapters: String,
    /// SHA-256 of canonical serde_json encoding of /calibration in vega_config.
    pub calibration_sha256: String,
    /// Paths are local to the immutable installation directory.
    pub files: BTreeMap<String, FilePin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Active {
    schema: String,
    manifest_digest: String,
    previous_manifest_digest: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateReport {
    pub status: String,
    pub current_manifest_digest: String,
    pub latest_model_revision: Option<String>,
    pub latest_backbone_revision: Option<String>,
    pub latest_source_revision: Option<String>,
    pub candidate_manifest_digest: Option<String>,
    pub metadata_compatible: bool,
    pub weights_verified: bool,
    pub source_update_requires_review: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct SetupReport {
    pub schema: &'static str,
    pub status: String,
    pub model_directory: Option<PathBuf>,
    pub manifest_digest: Option<String>,
    pub manifest: Option<InstallManifest>,
    pub updates: Option<UpdateReport>,
    pub configuration_toml: Option<String>,
    pub runtime_installed: bool,
    pub runtime_registered: bool,
    pub detail: &'static str,
}

struct Artifact {
    local: &'static str,
    upstream: &'static str,
    role: &'static str,
    repository: &'static str,
    bytes: u64,
    digest: &'static str,
}
const ARTIFACTS: &[Artifact] = &[
    Artifact {
        local: "vega_config.json",
        upstream: "vega_config.json",
        role: "engine_configuration_and_calibration",
        repository: MODEL,
        bytes: 11236,
        digest: "12f4f73784d58532ef244288c425df24944a8bda9c8414aa7c0981dfd2d88494",
    },
    Artifact {
        local: "engine.safetensors",
        upstream: "engine.safetensors",
        role: "engine_weights",
        repository: MODEL,
        bytes: 57433776,
        digest: "09c4c5fe203cf166d419dfaa72d75b26da00f47f66bafbfd750b2263ee1ca66b",
    },
    Artifact {
        local: "backbone/config.json",
        upstream: "config.json",
        role: "backbone_configuration",
        repository: BACKBONE,
        bytes: 2907,
        digest: "b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204",
    },
    Artifact {
        local: "backbone/model.safetensors.index.json",
        upstream: "model.safetensors.index.json",
        role: "backbone_weight_index",
        repository: BACKBONE,
        bytes: 50900,
        digest: "d8a08838a613b025eb7952ed9db11696213e57e76a375661ef5c12f9dd5dcf4e",
    },
    Artifact {
        local: "backbone/model.safetensors-00001-of-00001.safetensors",
        upstream: "model.safetensors-00001-of-00001.safetensors",
        role: "backbone_weights",
        repository: BACKBONE,
        bytes: 1746942600,
        digest: "04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696",
    },
    Artifact {
        local: "backbone/tokenizer.json",
        upstream: "tokenizer.json",
        role: "tokenizer",
        repository: BACKBONE,
        bytes: 12807982,
        digest: "5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42",
    },
    Artifact {
        local: "backbone/tokenizer_config.json",
        upstream: "tokenizer_config.json",
        role: "tokenizer_configuration",
        repository: BACKBONE,
        bytes: 16709,
        digest: "49e2b6e395f959f077f1e992b338919c0d4a9732fc6e613995e06557f843500c",
    },
    Artifact {
        local: "backbone/LICENSE",
        upstream: "LICENSE",
        role: "backbone_license",
        repository: BACKBONE,
        bytes: 11544,
        digest: "bbedc3fda3305820b977265f01b8619d87570a6739de3a5582c3464840f1e57a",
    },
    Artifact {
        local: "licenses/vega.LICENSE",
        upstream: SOURCE_LICENSE,
        role: "source_license",
        repository: SOURCE,
        bytes: 10759,
        digest: "5660238d37f7ae816966cc09d704bf1d2ed08dd16daf54c52a373d8ef38eef1c",
    },
];

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
pub fn valid_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn manifest_digest(manifest: &InstallManifest) -> io::Result<String> {
    Ok(hash(
        &serde_json::to_vec(manifest).map_err(io::Error::other)?,
    ))
}

pub(crate) fn baseline_digest() -> io::Result<String> {
    manifest_digest(&baseline())
}
fn json_digest(value: &Value) -> io::Result<String> {
    Ok(hash(&serde_json::to_vec(value).map_err(io::Error::other)?))
}
fn file_limit(artifact: &Artifact) -> u64 {
    match artifact.role {
        "backbone_weights" => MAX_ARTIFACT,
        "engine_weights" => 64 * 1024 * 1024,
        "tokenizer" => 16 * 1024 * 1024,
        _ => 64 * 1024,
    }
}
fn baseline() -> InstallManifest {
    InstallManifest {
        schema: MANIFEST_SCHEMA.into(),
        profile: PROFILE.into(),
        model: MODEL.into(),
        revision: BASELINE_REVISION.into(),
        backbone: BACKBONE.into(),
        backbone_revision: BASELINE_BACKBONE_REVISION.into(),
        source: SOURCE.into(),
        source_revision: BASELINE_SOURCE_REVISION.into(),
        license: "apache-2.0".into(),
        adapters: "off".into(),
        calibration_sha256: json_digest(
            &serde_json::from_str(BASELINE_CALIBRATION).expect("static calibration JSON"),
        )
        .expect("JSON serialization"),
        files: ARTIFACTS
            .iter()
            .map(|artifact| {
                (
                    artifact.local.into(),
                    FilePin {
                        role: artifact.role.into(),
                        repository: artifact.repository.into(),
                        revision: match artifact.repository {
                            MODEL => BASELINE_REVISION,
                            BACKBONE => BASELINE_BACKBONE_REVISION,
                            _ => BASELINE_SOURCE_REVISION,
                        }
                        .into(),
                        upstream_path: artifact.upstream.into(),
                        bytes: artifact.bytes,
                        sha256: format!("sha256:{}", artifact.digest),
                    },
                )
            })
            .collect(),
    }
}

fn validate_manifest(manifest: &InstallManifest) -> io::Result<()> {
    if manifest.schema != MANIFEST_SCHEMA
        || manifest.profile != PROFILE
        || manifest.model != MODEL
        || manifest.backbone != BACKBONE
        || manifest.source != SOURCE
        || manifest.source_revision != BASELINE_SOURCE_REVISION
        || manifest.license != "apache-2.0"
        || manifest.adapters != "off"
        || !valid_revision(&manifest.revision)
        || !valid_revision(&manifest.backbone_revision)
        || !valid_digest(&manifest.calibration_sha256)
        || manifest.files.len() != ARTIFACTS.len()
    {
        return Err(invalid(
            "unsupported Vega identity/profile, adapters, source contract or manifest",
        ));
    }
    let baseline = baseline();
    for artifact in ARTIFACTS {
        let pin = manifest
            .files
            .get(artifact.local)
            .ok_or_else(|| invalid("incomplete Vega manifest"))?;
        let revision = match artifact.repository {
            MODEL => &manifest.revision,
            BACKBONE => &manifest.backbone_revision,
            _ => &manifest.source_revision,
        };
        if pin.role != artifact.role
            || pin.repository != artifact.repository
            || pin.upstream_path != artifact.upstream
            || &pin.revision != revision
            || pin.bytes == 0
            || pin.bytes > file_limit(artifact)
            || !valid_digest(&pin.sha256)
        {
            return Err(invalid(
                "invalid Vega component identity, length or SHA-256",
            ));
        }
        // Backbone feature semantics, tokenization and the source license are
        // reviewed together. A new repo revision is compatible only if every
        // backbone byte is unchanged. A future native port may widen this rule.
        if artifact.repository != MODEL {
            let reviewed = &baseline.files[artifact.local];
            if pin.sha256 != reviewed.sha256 || pin.bytes != reviewed.bytes {
                return Err(invalid(
                    "changed backbone/tokenizer/source bytes require reviewed native compatibility support",
                ));
            }
        }
        if manifest.revision == BASELINE_REVISION
            && artifact.repository == MODEL
            && pin != &baseline.files[artifact.local]
        {
            return Err(invalid(
                "baseline Vega revision differs from compiled artifact pins",
            ));
        }
    }
    if manifest.revision == BASELINE_REVISION
        && manifest.calibration_sha256 != baseline.calibration_sha256
    {
        return Err(invalid(
            "baseline Vega calibration differs from compiled pin",
        ));
    }
    Ok(())
}
/// No lexical traversal or symbolic-link component, including a missing leaf's
/// existing ancestry. Private cache state additionally rejects unsafe ownership
/// and group/world access. Same-account/root mutation is outside this boundary.
fn safe_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return Err(invalid("Vega paths must be absolute without traversal"));
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid("symlinked Vega cache paths are refused"));
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn private_directory(path: &Path, create: bool) -> io::Result<()> {
    safe_path(path)?;
    if create {
        fs_permissions::ensure_private_dir(path)?;
    }
    fs_permissions::check_directory(path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "Vega cache directories must be owned by this user and mode 0700; no permissions changed",
            ));
        }
    }
    Ok(())
}
fn checked_file(path: &Path) -> io::Result<File> {
    safe_path(path)?;
    fs_permissions::check_file(path, false)?;
    let file = fs_permissions::open_private_file(path, OpenOptions::new().read(true))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("Vega artifact must be a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "Vega cache files must be privately owned regular files",
            ));
        }
    }
    Ok(file)
}
fn read_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    let file = checked_file(path)?;
    if file.metadata()?.len() > maximum {
        return Err(invalid("Vega metadata overflow"));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("Vega metadata grew beyond bound"));
    }
    Ok(bytes)
}
fn verify_file(path: &Path, pin: &FilePin, transport: &impl Transport) -> io::Result<()> {
    transport.checkpoint()?;
    let mut file = checked_file(path)?;
    if file.metadata()?.len() != pin.bytes {
        return Err(invalid("Vega artifact length mismatch"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0_u64;
    loop {
        transport.checkpoint()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > pin.bytes {
            return Err(invalid("Vega artifact grew beyond pinned length"));
        }
        hasher.update(&buffer[..count]);
    }
    if total != pin.bytes || format!("sha256:{:x}", hasher.finalize()) != pin.sha256 {
        return Err(invalid("Vega artifact SHA-256 or length mismatch"));
    }
    Ok(())
}

const BASELINE_CALIBRATION: &str = r#"
{
  "mode": "thermal",
  "theta": [
   [
    0.1461930922493163,
    -0.40584399832083756,
    0.11879132339394936,
    -0.00019810563682904256,
    0.24664722325750524
   ],
   [
    0.21445810704669266,
    -0.2183359739988292,
    -0.43754816532834395,
    -0.00012643962106031876,
    -0.00020349672358602032
   ],
   [
    -0.10818959060657497,
    -0.12029448675164649,
    0.16704428513758954,
    0.000146840045909918,
    0.0001017817640975235
   ]
  ],
  "theta_scalar": [
   [
    -0.1633803803398695,
    0.0,
    0.0,
    0.0,
    0.0
   ],
   [
    -0.07954575320364916,
    0.0,
    0.0,
    0.0,
    0.0
   ],
   [
    -0.22501592435718562,
    0.0,
    0.0,
    0.0,
    0.0
   ]
  ],
  "features": [
   "log1p(kinetic)",
   "log1p(nearest)",
   "steps_used/n_steps",
   "log(k)"
  ],
  "heldout_calib_nll": {
   "scalar": 0.4684978104567378,
   "thermal": 0.46753050828577386
  },
  "zero_shot_fit": {
   "items": 800,
   "tasks": 6,
   "weight_per_item": 11.342,
   "eval_zs_items_left": 5862
  },
  "conformal": {
   "choice": {
    "0.1": 0.7476024932584395,
    "0.05": 0.807213519514113
   },
   "score": {
    "0.1": 0.8495210275489015,
    "0.05": 0.9207551695493317
   },
   "boolean": {
    "0.1": 0.5823749681631878,
    "0.05": 0.6826231225713268
   }
  },
  "abstain_threshold": {
   "0.05": 0.7215784949904454,
   "0.1": 0.5958565839690714,
   "0.2": 0.2554241498778211
  },
  "default_risk": "0.1",
  "unbound_threshold": 1.269094705581665
 }
"#;

fn load_json(directory: &Path, name: &str) -> io::Result<Value> {
    serde_json::from_slice(&read_bounded(&directory.join(name), MAX_METADATA)?)
        .map_err(io::Error::other)
}

/// The initial native contract is deliberately narrower than upstream: text
/// 0.8B base engine, fixed feature taps and no adapters or test-time training.
fn compatible_config(directory: &Path, manifest: &InstallManifest) -> io::Result<()> {
    let config = load_json(directory, "vega_config.json")?;
    let mut architecture = config.clone();
    let object = architecture
        .as_object_mut()
        .ok_or_else(|| invalid("Vega config must be an object"))?;
    object.remove("training");
    object.remove("calibration");
    let expected = json!({
        "backbone": BACKBONE, "backbone_name": BACKBONE, "d_model": 1024,
        "dropout": 0.1, "dt_max": 0.5, "feature_dropout": 0.1,
        "feature_layers": [13, 19], "force_max": 20.0, "latent_dim": 64,
        "max_len": 73728, "memory_chunk_tokens": 128, "memory_chunks": 16,
        "memory_min_tokens": 128, "model_name": "vega", "n_layers": 24,
        "n_steps": 12, "opt_max_tokens": 32, "option_layers": [19],
        "prefix_min_tokens": 4096, "proj_hidden": 2048,
        "query_first_max_state": 4096, "settle_eps": 0.01, "ssm_state": 32,
        "template": 0, "world_dim": 256
    });
    if architecture != expected {
        return Err(invalid(
            "unsupported Vega architecture, feature taps, options or added configuration",
        ));
    }
    let calibration = config
        .get("calibration")
        .ok_or_else(|| invalid("Vega calibration missing"))?;
    validate_calibration(calibration)?;
    if json_digest(calibration)? != manifest.calibration_sha256 {
        return Err(invalid(
            "Vega calibration differs from separately pinned calibration digest",
        ));
    }
    // Every backbone byte is constrained by validate_manifest, including the
    // tokenizer and index. Do not accept a config that requests remote code.
    let backbone = load_json(directory, "backbone/config.json")?;
    if backbone.get("model_type").and_then(Value::as_str) != Some("qwen3_5")
        || backbone.get("architectures") != Some(&json!(["Qwen3_5ForConditionalGeneration"]))
        || backbone
            .pointer("/text_config/model_type")
            .and_then(Value::as_str)
            != Some("qwen3_5_text")
        || backbone
            .pointer("/text_config/hidden_size")
            .and_then(Value::as_u64)
            != Some(1024)
        || backbone
            .pointer("/text_config/num_hidden_layers")
            .and_then(Value::as_u64)
            != Some(24)
        || backbone.get("auto_map").is_some()
        || backbone.pointer("/text_config/auto_map").is_some()
    {
        return Err(invalid("unsupported Qwen3.5 backbone configuration"));
    }
    Ok(())
}

fn exact_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == expected.iter().copied().collect()
    })
}
fn finite_number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn probability(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
}
fn validate_calibration(calibration: &Value) -> io::Result<()> {
    if !exact_keys(
        calibration,
        &[
            "mode",
            "theta",
            "theta_scalar",
            "features",
            "heldout_calib_nll",
            "zero_shot_fit",
            "conformal",
            "abstain_threshold",
            "default_risk",
            "unbound_threshold",
        ],
    ) || calibration.get("mode").and_then(Value::as_str) != Some("thermal")
        || calibration.get("features")
            != Some(&json!([
                "log1p(kinetic)",
                "log1p(nearest)",
                "steps_used/n_steps",
                "log(k)"
            ]))
        || calibration.get("default_risk").and_then(Value::as_str) != Some("0.1")
    {
        return Err(invalid("unsupported Vega calibration schema or mode"));
    }
    for field in ["theta", "theta_scalar"] {
        if !calibration[field].as_array().is_some_and(|rows| {
            rows.len() == 3
                && rows.iter().all(|row| {
                    row.as_array()
                        .is_some_and(|values| values.len() == 5 && values.iter().all(finite_number))
                })
        }) {
            return Err(invalid(
                "Vega thermal calibration must contain three finite five-coefficient rows",
            ));
        }
    }
    let conformal = &calibration["conformal"];
    if !exact_keys(conformal, &["choice", "score", "boolean"]) {
        return Err(invalid("unsupported Vega conformal kinds"));
    }
    for kind in ["choice", "score", "boolean"] {
        let thresholds = &conformal[kind];
        if !exact_keys(thresholds, &["0.1", "0.05"])
            || !thresholds
                .as_object()
                .is_some_and(|values| values.values().all(probability))
            || thresholds["0.05"].as_f64() < thresholds["0.1"].as_f64()
        {
            return Err(invalid("invalid Vega conformal thresholds"));
        }
    }
    let abstain = &calibration["abstain_threshold"];
    if !exact_keys(abstain, &["0.05", "0.1", "0.2"])
        || !abstain
            .as_object()
            .is_some_and(|values| values.values().all(probability))
        || abstain["0.05"].as_f64() < abstain["0.1"].as_f64()
        || abstain["0.1"].as_f64() < abstain["0.2"].as_f64()
        || !calibration["unbound_threshold"]
            .as_f64()
            .is_some_and(|value| value.is_finite() && value > 0.0)
    {
        return Err(invalid("invalid Vega abstention or unbound threshold"));
    }
    let heldout = &calibration["heldout_calib_nll"];
    let zero_shot = &calibration["zero_shot_fit"];
    if !exact_keys(heldout, &["scalar", "thermal"])
        || !heldout.as_object().is_some_and(|values| {
            values.values().all(|value| {
                value
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && value >= 0.0)
            })
        })
        || !exact_keys(
            zero_shot,
            &["items", "tasks", "weight_per_item", "eval_zs_items_left"],
        )
        || !["items", "tasks", "eval_zs_items_left"]
            .iter()
            .all(|key| zero_shot[*key].as_u64().is_some())
        || !zero_shot["weight_per_item"]
            .as_f64()
            .is_some_and(|value| value.is_finite() && value > 0.0)
    {
        return Err(invalid("invalid Vega calibration evaluation metadata"));
    }
    Ok(())
}

fn verify_checkpoint(
    directory: &Path,
    manifest: &InstallManifest,
    transport: &impl Transport,
) -> io::Result<()> {
    transport.checkpoint()?;
    validate_manifest(manifest)?;
    private_directory(directory, false)?;
    for (name, pin) in &manifest.files {
        verify_file(&directory.join(name), pin, transport)?;
    }
    transport.checkpoint()?;
    compatible_config(directory, manifest)?;
    transport.checkpoint()?;
    verify_weight_headers(directory)?;
    transport.checkpoint()
}

fn cached_manifest(
    root: &Path,
    digest: &str,
    transport: &impl Transport,
) -> io::Result<InstallManifest> {
    let directory = installation_directory(root, digest)?;
    let manifest: InstallManifest = serde_json::from_slice(&read_bounded(
        &directory.join("manifest.json"),
        MAX_METADATA,
    )?)
    .map_err(io::Error::other)?;
    if manifest_digest(&manifest)? != digest {
        return Err(invalid("Vega cache identity differs from manifest digest"));
    }
    verify_checkpoint(&directory, &manifest, transport)?;
    Ok(manifest)
}
fn installation_directory(root: &Path, digest: &str) -> io::Result<PathBuf> {
    if !valid_digest(digest) {
        return Err(invalid("expected an immutable sha256: manifest digest"));
    }
    Ok(root.join("installations").join(&digest[7..]))
}
fn active(root: &Path) -> io::Result<Option<Active>> {
    let path = root.join("active.json");
    safe_path(&path)?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let active: Active =
        serde_json::from_slice(&read_bounded(&path, 4096)?).map_err(io::Error::other)?;
    if active.schema != MANIFEST_SCHEMA
        || !valid_digest(&active.manifest_digest)
        || active
            .previous_manifest_digest
            .as_deref()
            .is_some_and(|value| !valid_digest(value))
    {
        return Err(invalid("invalid Vega setup active pointer"));
    }
    Ok(Some(active))
}
fn activate(root: &Path, digest: &str, transport: &impl Transport) -> io::Result<()> {
    transport.checkpoint()?;
    let previous = active(root)?;
    if previous
        .as_ref()
        .is_some_and(|value| value.manifest_digest == digest)
    {
        return Ok(());
    }
    let pointer = Active {
        schema: MANIFEST_SCHEMA.into(),
        manifest_digest: digest.into(),
        previous_manifest_digest: previous.map(|value| value.manifest_digest),
    };
    transport.checkpoint()?;
    // A directory-sync error may leave the complete new pointer visible. The
    // atomic helper reports that uncertainty, never a fictitious rollback.
    fs_permissions::write_private(
        root.join("active.json"),
        serde_json::to_vec_pretty(&pointer).map_err(io::Error::other)?,
    )
}
pub(crate) fn default_cache() -> io::Result<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home().map(|path| path.join("Library/Caches"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|path| PathBuf::from(path).join("AppData/Local"))
            })
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|path| path.join(".cache")))
    };
    let root = base
        .ok_or_else(|| invalid("local cache home unavailable; supply --cache-directory"))?
        .join("decapod/vega");
    safe_path(&root)?;
    Ok(root)
}

/// Read-only, full content verification for native inference. No network,
/// environment-selected model, active pointer, or mutable upstream tag is used.
pub fn verify_installation(
    directory: &Path,
    expected_manifest_digest: &str,
) -> io::Result<InstallManifest> {
    verify_installation_bounded(
        directory,
        expected_manifest_digest,
        Instant::now() + Duration::from_secs(120),
        &Cancellation::default(),
    )
}
pub fn verify_installation_bounded(
    directory: &Path,
    expected_manifest_digest: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> io::Result<InstallManifest> {
    let transport = LocalVerification {
        deadline,
        cancellation,
    };
    transport.checkpoint()?;
    if !valid_digest(expected_manifest_digest) {
        return Err(invalid("invalid configured Vega manifest digest"));
    }
    let manifest: InstallManifest = serde_json::from_slice(&read_bounded(
        &directory.join("manifest.json"),
        MAX_METADATA,
    )?)
    .map_err(io::Error::other)?;
    if manifest_digest(&manifest)? != expected_manifest_digest {
        return Err(invalid(
            "configured Vega manifest digest differs from installation",
        ));
    }
    verify_checkpoint(directory, &manifest, &transport)?;
    transport.checkpoint()?;
    Ok(manifest)
}

trait Transport {
    fn checkpoint(&self) -> io::Result<()> {
        Ok(())
    }
    fn download(&self, url: &str, destination: &Path, maximum: u64) -> io::Result<()>;
}
#[cfg(test)]
struct Offline;
#[cfg(test)]
impl Transport for Offline {
    fn download(&self, _: &str, _: &Path, _: u64) -> io::Result<()> {
        Err(invalid("offline Vega verification cannot use the network"))
    }
}
struct PublicCurl<'a> {
    deadline: Instant,
    cancellation: &'a Cancellation,
}
fn permitted_url(url: &str) -> bool {
    if [MODEL, BACKBONE].iter().any(|repository| {
        url == format!("https://huggingface.co/api/models/{repository}?blobs=true")
    }) || url == format!("https://api.github.com/repos/{SOURCE}/commits?per_page=1")
    {
        return true;
    }
    ARTIFACTS.iter().any(|artifact| {
        let prefix = if artifact.repository == SOURCE {
            format!("https://raw.githubusercontent.com/{SOURCE}/")
        } else {
            format!("https://huggingface.co/{}/resolve/", artifact.repository)
        };
        url.strip_prefix(&prefix)
            .and_then(|suffix| suffix.split_once('/'))
            .is_some_and(|(revision, path)| {
                valid_revision(revision)
                    && path == artifact.upstream
                    && (artifact.repository != SOURCE || revision == BASELINE_SOURCE_REVISION)
            })
    })
}
impl Transport for PublicCurl<'_> {
    fn checkpoint(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Vega setup cancelled; no further activation",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Vega setup deadline exceeded; no further activation",
            ));
        }
        Ok(())
    }
    fn download(&self, url: &str, destination: &Path, maximum: u64) -> io::Result<()> {
        if !permitted_url(url) || maximum == 0 || maximum > MAX_ARTIFACT {
            return Err(invalid("unsupported public Vega transport request"));
        }
        self.checkpoint()?;
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "Vega setup deadline exceeded")
            })?;
        let timeout = remaining.min(if maximum > 4 * 1024 * 1024 {
            DEADLINE
        } else {
            Duration::from_secs(60)
        });
        if timeout.as_secs() == 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Vega setup deadline exceeded",
            ));
        }
        safe_path(destination)?;
        private_directory(
            destination
                .parent()
                .ok_or_else(|| invalid("download parent missing"))?,
            false,
        )?;
        let output_file = fs_permissions::open_private_file(
            destination,
            OpenOptions::new().write(true).create_new(true),
        )?;
        let mut command = curl_command(url, destination, maximum, timeout);
        // Enforce a child-only file-size bound even for chunked responses.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(move || {
                    let limit = libc::rlimit {
                        rlim_cur: maximum as libc::rlim_t,
                        rlim_max: maximum as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    libc::umask(0o077);
                    Ok(())
                });
            }
        }
        let capture = command.bounded_capture(
            ProcessLimits {
                timeout,
                max_output_bytes: 8192,
            },
            self.cancellation,
        )?;
        if capture.termination == Termination::Cancelled {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Vega download cancelled",
            ));
        }
        if capture.termination == Termination::TimedOut {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Vega download deadline exceeded",
            ));
        }
        self.checkpoint()?;
        let http_status = std::str::from_utf8(&capture.output.stdout)
            .ok()
            .and_then(|text| text.trim().parse::<u16>().ok());
        if matches!(http_status, Some(401 | 403)) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "public Vega access denied; no fallback, credential or agreement accepted",
            ));
        }
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(invalid(format!(
                "public Vega download failed ({:?}, exit {:?}); no checkpoint activated",
                capture.termination,
                capture.output.status.code()
            )));
        }
        let file = checked_file(destination)?;
        if file.metadata()?.len() == 0 || file.metadata()?.len() > maximum {
            return Err(invalid("Vega download length outside bound"));
        }
        output_file.sync_all()
    }
}
fn curl_command(url: &str, destination: &Path, maximum: u64, timeout: Duration) -> Command {
    let mut command = Command::new("/usr/bin/curl");
    // No ambient token, netrc, proxy, cookies, user config or custom trust roots.
    command
        .env_clear()
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-redirs",
            "5",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--noproxy",
            "*",
            "--user-agent",
            "decapod-vega-setup",
            "--connect-timeout",
            "10",
            "--speed-limit",
            "1024",
            "--speed-time",
            "30",
            "--max-time",
        ])
        .arg(timeout.as_secs().to_string())
        .arg("--max-filesize")
        .arg(maximum.to_string())
        .args(["--write-out", "%{http_code}", "--output"])
        .arg(destination)
        .arg("--url")
        .arg(url);
    command
}
fn artifact_url(pin: &FilePin) -> String {
    if pin.repository == SOURCE {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            pin.repository, pin.revision, pin.upstream_path
        )
    } else {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            pin.repository, pin.revision, pin.upstream_path
        )
    }
}
fn fetch_json(
    transport: &impl Transport,
    scratch: &Path,
    name: &str,
    url: &str,
) -> io::Result<Value> {
    let path = scratch.join(name);
    transport.download(url, &path, MAX_METADATA)?;
    serde_json::from_slice(&read_bounded(&path, MAX_METADATA)?).map_err(io::Error::other)
}
fn public_identity(value: &Value, repository: &str) -> io::Result<String> {
    let revision = value
        .get("sha")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("upstream revision missing"))?;
    if value.get("id").and_then(Value::as_str) != Some(repository)
        || !valid_revision(revision)
        || value.get("private").and_then(Value::as_bool) != Some(false)
        || value.get("gated").and_then(Value::as_bool) != Some(false)
        || value.get("disabled").and_then(Value::as_bool) != Some(false)
        || value.pointer("/cardData/license").and_then(Value::as_str) != Some("apache-2.0")
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "upstream Vega identity/license/access changed; no credential or agreement accepted",
        ));
    }
    Ok(revision.into())
}
struct Candidate {
    manifest: InstallManifest,
    metadata: tempfile::TempDir,
}
fn discover(
    transport: &impl Transport,
    current: &str,
) -> io::Result<(UpdateReport, Option<Candidate>)> {
    let scratch = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let model = fetch_json(
        transport,
        scratch.path(),
        "model-hub.json",
        &format!("https://huggingface.co/api/models/{MODEL}?blobs=true"),
    )?;
    let backbone = fetch_json(
        transport,
        scratch.path(),
        "backbone-hub.json",
        &format!("https://huggingface.co/api/models/{BACKBONE}?blobs=true"),
    )?;
    let revision = public_identity(&model, MODEL)?;
    let backbone_revision = public_identity(&backbone, BACKBONE)?;
    let source = fetch_json(
        transport,
        scratch.path(),
        "source-github.json",
        &format!("https://api.github.com/repos/{SOURCE}/commits?per_page=1"),
    )?;
    let source_revision = source
        .pointer("/0/sha")
        .and_then(Value::as_str)
        .filter(|revision| valid_revision(revision))
        .ok_or_else(|| invalid("upstream source revision missing"))?;
    let mut report = UpdateReport {
        status: "checked".into(), current_manifest_digest: current.into(),
        latest_model_revision: Some(revision.clone()), latest_backbone_revision: Some(backbone_revision.clone()),
        latest_source_revision: Some(source_revision.into()), candidate_manifest_digest: None,
        metadata_compatible: false, weights_verified: false,
        source_update_requires_review: source_revision != BASELINE_SOURCE_REVISION,
        detail: "Metadata compatibility is not native numerical parity or readiness-domain calibration. The native port reference remains pinned; an upstream source change needs review. No weight is downloaded or activated by a check.".into(),
    };
    let resolve = (|| -> io::Result<InstallManifest> {
        let mut manifest = baseline();
        manifest.revision = revision.clone();
        manifest.backbone_revision = backbone_revision.clone();
        for artifact in ARTIFACTS {
            if artifact.repository == SOURCE {
                continue;
            }
            let metadata = if artifact.repository == MODEL {
                &model
            } else {
                &backbone
            };
            let siblings = metadata
                .get("siblings")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("upstream file inventory missing"))?;
            let rows: Vec<_> = siblings
                .iter()
                .filter(|row| {
                    row.get("rfilename").and_then(Value::as_str) == Some(artifact.upstream)
                })
                .collect();
            if rows.len() != 1 {
                return Err(invalid(
                    "missing or duplicate required upstream Vega component",
                ));
            }
            let row = rows[0];
            let bytes = row
                .get("size")
                .and_then(Value::as_u64)
                .filter(|bytes| *bytes > 0 && *bytes <= file_limit(artifact))
                .ok_or_else(|| invalid("unsupported upstream component length"))?;
            let pin = manifest
                .files
                .get_mut(artifact.local)
                .ok_or_else(|| invalid("missing static Vega component"))?;
            pin.revision = if artifact.repository == MODEL {
                revision.clone()
            } else {
                backbone_revision.clone()
            };
            pin.bytes = bytes;
            pin.sha256 = if matches!(
                artifact.role,
                "engine_weights" | "backbone_weights" | "tokenizer"
            ) {
                if row.pointer("/lfs/size").and_then(Value::as_u64) != Some(bytes) {
                    return Err(invalid("upstream LFS length provenance mismatch"));
                }
                let digest = format!(
                    "sha256:{}",
                    row.pointer("/lfs/sha256")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("upstream LFS SHA-256 missing"))?
                );
                if !valid_digest(&digest) {
                    return Err(invalid("invalid upstream LFS SHA-256"));
                }
                digest
            } else {
                let destination = scratch.path().join(artifact.local);
                private_directory(
                    destination
                        .parent()
                        .ok_or_else(|| invalid("metadata parent missing"))?,
                    true,
                )?;
                transport.download(&artifact_url(pin), &destination, bytes)?;
                let data = read_bounded(&destination, bytes)?;
                if data.len() as u64 != bytes {
                    return Err(invalid("upstream metadata length mismatch"));
                }
                hash(&data)
            };
        }
        let config = load_json(scratch.path(), "vega_config.json")?;
        manifest.calibration_sha256 = json_digest(
            config
                .get("calibration")
                .ok_or_else(|| invalid("Vega calibration missing"))?,
        )?;
        validate_manifest(&manifest)?;
        compatible_config(scratch.path(), &manifest)?;
        Ok(manifest)
    })();
    match resolve {
        Ok(manifest) => {
            let digest = manifest_digest(&manifest)?;
            report.status = if digest == current {
                "current"
            } else {
                "compatible_update_available"
            }
            .into();
            report.candidate_manifest_digest = Some(digest);
            report.metadata_compatible = true;
            Ok((
                report,
                Some(Candidate {
                    manifest,
                    metadata: scratch,
                }),
            ))
        }
        Err(error) if terminal_error(&error) => Err(error),
        Err(error) => {
            report.status = "incompatible_update".into();
            report.detail = format!(
                "{error}; existing verified checkpoint remains unchanged. Update reviewed native support before using this revision."
            );
            Ok((report, None))
        }
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file =
        fs_permissions::open_private_file(path, OpenOptions::new().write(true).create_new(true))?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Content-addressed components avoid repeated backbone downloads. Installation
/// makes separately verified copies to preserve the single-link storage contract.
fn component(
    transport: &impl Transport,
    root: &Path,
    pin: &FilePin,
    metadata: Option<&Path>,
) -> io::Result<PathBuf> {
    if !valid_digest(&pin.sha256) {
        return Err(invalid("invalid component digest"));
    }
    let objects = root.join("components");
    private_directory(&objects, true)?;
    let object = objects.join(&pin.sha256[7..]);
    safe_path(&object)?;
    if object.try_exists()? {
        verify_file(&object, pin, transport)?;
        return Ok(object);
    }
    let temporary = tempfile::Builder::new()
        .prefix(".component-")
        .tempdir_in(&objects)?;
    let path = temporary.path().join("data");
    if let Some(metadata) = metadata {
        write_new(&path, &read_bounded(metadata, pin.bytes)?)?;
    } else {
        transport.download(&artifact_url(pin), &path, pin.bytes)?;
    }
    verify_file(&path, pin, transport)?;
    transport.checkpoint()?;
    // The exclusive setup lock serializes writers. Existing corrupt bytes are
    // preserved for review rather than silently overwritten or repaired.
    if object.try_exists()? {
        return Err(invalid(
            "Vega component appeared during installation; review cache and retry",
        ));
    }
    fs::rename(&path, &object)?;
    atomic::sync_parent_directory(&objects).map_err(|error| {
        invalid(format!(
            "component complete but durability unconfirmed; active pointer unchanged: {error}"
        ))
    })?;
    Ok(object)
}
fn copy_component(
    source: &Path,
    destination: &Path,
    pin: &FilePin,
    transport: &impl Transport,
) -> io::Result<()> {
    transport.checkpoint()?;
    let mut source = checked_file(source)?;
    if source.metadata()?.len() != pin.bytes {
        return Err(invalid("Vega component length mismatch before copy"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("artifact parent missing"))?;
    private_directory(parent, true)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".artifact-")
        .tempfile_in(parent)?;
    let mut buffer = [0; 65536];
    let mut total = 0_u64;
    loop {
        transport.checkpoint()?;
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > pin.bytes {
            return Err(invalid(
                "Vega component grew beyond pinned length during copy",
            ));
        }
        temporary.write_all(&buffer[..count])?;
    }
    if total != pin.bytes {
        return Err(invalid("Vega component shortened during copy"));
    }
    temporary.as_file().sync_all()?;
    // Read back the copied bytes independently before publishing even inside
    // the private installation scratch. Errors/cancellation remove the temp file.
    verify_file(temporary.path(), pin, transport)?;
    transport.checkpoint()?;
    temporary
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn install(
    transport: &impl Transport,
    root: &Path,
    manifest: &InstallManifest,
    metadata: Option<&Path>,
) -> io::Result<PathBuf> {
    transport.checkpoint()?;
    validate_manifest(manifest)?;
    let digest = manifest_digest(manifest)?;
    let destination = installation_directory(root, &digest)?;
    let installations = root.join("installations");
    private_directory(&installations, true)?;
    safe_path(&destination)?;
    if destination.try_exists()? {
        if cached_manifest(root, &digest, transport)? != *manifest {
            return Err(invalid(
                "cached Vega manifest differs from immutable identity",
            ));
        }
        return Ok(destination);
    }
    let scratch = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&installations)?;
    for (name, pin) in &manifest.files {
        transport.checkpoint()?;
        let metadata_path = metadata.map(|directory| directory.join(name));
        let available_metadata = match metadata_path.as_deref() {
            Some(path) if path.try_exists()? => Some(path),
            _ => None,
        };
        let object = component(transport, root, pin, available_metadata)?;
        let destination = scratch.path().join(name);
        private_directory(
            destination
                .parent()
                .ok_or_else(|| invalid("artifact parent missing"))?,
            true,
        )?;
        copy_component(&object, &destination, pin, transport)?;
    }
    verify_checkpoint(scratch.path(), manifest, transport)?;
    write_new(
        &scratch.path().join("manifest.json"),
        &serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?,
    )?;
    for directory in [
        scratch.path().join("backbone"),
        scratch.path().join("licenses"),
        scratch.path().to_path_buf(),
    ] {
        atomic::sync_parent_directory(&directory)?;
    }
    if destination.try_exists()? {
        return Err(invalid(
            "Vega installation appeared during setup; review cache and retry",
        ));
    }
    transport.checkpoint()?;
    fs::rename(scratch.path(), &destination)?;
    atomic::sync_parent_directory(&installations).map_err(|error| {
        invalid(format!(
            "installation complete but durability unconfirmed; active pointer unchanged: {error}"
        ))
    })?;
    Ok(destination)
}
fn configuration(_directory: &Path, digest: &str) -> io::Result<String> {
    let value = toml::Value::try_from(serde_json::json!({
        "vega": {"mode": "disabled", "model_manifest_digest": digest}
    }))
    .map_err(io::Error::other)?;
    toml::to_string_pretty(&value).map_err(io::Error::other)
}
fn report(
    status: &str,
    directory: Option<PathBuf>,
    manifest: Option<InstallManifest>,
    updates: Option<UpdateReport>,
) -> io::Result<SetupReport> {
    let digest = manifest.as_ref().map(manifest_digest).transpose()?;
    let configuration_toml = match (&directory, &digest) {
        (Some(directory), Some(digest)) => Some(configuration(directory, digest)?),
        _ => None,
    };
    Ok(SetupReport {
        schema: "decapod.vega-setup.v1",
        status: status.into(),
        model_directory: directory,
        manifest_digest: digest,
        manifest,
        updates,
        configuration_toml,
        runtime_installed: false,
        runtime_registered: false,
        detail: "Model data only, base engine with adapters explicitly off. Optional --runtime and --cpu-runtime register this maintainer's verified executable pins privately; setup does not build or execute workers. Apply shared model configuration explicitly; setup does not edit project policy. Inference uses the immutable policy pin and local binding, never the active pointer. Registration and metadata compatibility do not establish numerical parity or readiness-domain calibration.",
    })
}
fn validate_options(options: &SetupOptions) -> io::Result<()> {
    if (options.check_updates && (options.runtime.is_some() || options.cpu_runtime.is_some()))
        || (options.cpu_runtime.is_some() && options.runtime.is_none())
    {
        return Err(invalid(
            "CPU runtime registration requires --runtime and cannot be a read-only update check",
        ));
    }
    if [
        options.check_updates,
        options.refresh,
        options.rollback.is_some(),
    ]
    .into_iter()
    .filter(|flag| *flag)
    .count()
        > 1
        || (options.offline && (options.check_updates || options.refresh))
        || options
            .rollback
            .as_deref()
            .is_some_and(|digest| !valid_digest(digest))
    {
        return Err(invalid(
            "conflicting Vega setup actions or invalid rollback manifest digest",
        ));
    }
    Ok(())
}
pub fn run(options: &SetupOptions) -> io::Result<SetupReport> {
    validate_options(options)?;
    let root = options
        .cache_directory
        .clone()
        .map(Ok)
        .unwrap_or_else(default_cache)?;
    safe_path(&root)?;
    let guard = SignalCancellationGuard::new()?;
    let transport = PublicCurl {
        deadline: Instant::now() + DEADLINE,
        cancellation: guard.cancellation(),
    };
    let mut report = run_with_transport(options, &root, &transport)?;
    if let Some(runtime) = &options.runtime {
        transport.checkpoint()?;
        let directory = report
            .model_directory
            .as_ref()
            .ok_or_else(|| invalid("runtime registration needs a verified installation"))?
            .canonicalize()?;
        let manifest_digest = report
            .manifest_digest
            .clone()
            .ok_or_else(|| invalid("runtime registration needs a pinned manifest"))?;
        let runtime = runtime.canonicalize()?;
        let runtime_sha256 = super::vega_runtime::runtime_digest(
            &runtime,
            transport.deadline,
            transport.cancellation,
        )?;
        let cpu_runtime = options
            .cpu_runtime
            .as_ref()
            .map(|path| {
                let runtime = path.canonicalize()?;
                let runtime_sha256 = super::vega_runtime::runtime_digest(
                    &runtime,
                    transport.deadline,
                    transport.cancellation,
                )?;
                Ok::<_, io::Error>(super::vega_readiness::RuntimePin {
                    runtime,
                    runtime_sha256,
                })
            })
            .transpose()?;
        transport.checkpoint()?;
        super::vega_runtime::register(super::vega_readiness::Installation {
            directory,
            manifest_digest,
            runtime,
            runtime_sha256,
            cpu_runtime,
        })?;
        report.runtime_registered = true;
    }
    Ok(report)
}
fn run_with_transport(
    options: &SetupOptions,
    root: &Path,
    transport: &impl Transport,
) -> io::Result<SetupReport> {
    validate_options(options)?;
    transport.checkpoint()?;
    safe_path(root)?;
    if root.try_exists()? {
        private_directory(root, false)?;
    }
    let baseline_digest = manifest_digest(&baseline())?;
    let current = active(root)?
        .map(|active| active.manifest_digest)
        .unwrap_or_else(|| baseline_digest.clone());
    if options.check_updates {
        let (updates, _) = discover(transport, &current)?;
        return report("checked", None, None, Some(updates));
    }
    if options.offline && options.rollback.is_none() {
        let manifest = cached_manifest(root, &current, transport)?;
        return report(
            "verified_offline",
            Some(installation_directory(root, &current)?),
            Some(manifest),
            None,
        );
    }
    private_directory(root, true)?;
    let lock = fs_permissions::open_private_file(
        &root.join("install.lock"),
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false),
    )?;
    lock.try_lock_exclusive()
        .map_err(|_| invalid("another Vega setup owns this cache; retry after it finishes"))?;
    let current = active(root)?
        .map(|active| active.manifest_digest)
        .unwrap_or(baseline_digest.clone());
    if let Some(digest) = &options.rollback {
        let manifest = cached_manifest(root, digest, transport)?;
        activate(root, digest, transport)?;
        return report(
            "activated_cached_manifest",
            Some(installation_directory(root, digest)?),
            Some(manifest),
            None,
        );
    }
    let (mut updates, candidate) = match discover(transport, &current) {
        Ok(value) => value,
        Err(error) if options.refresh || terminal_error(&error) => return Err(error),
        Err(error) => (
            UpdateReport {
                status: "unavailable".into(),
                current_manifest_digest: current.clone(),
                latest_model_revision: None,
                latest_backbone_revision: None,
                latest_source_revision: None,
                candidate_manifest_digest: None,
                metadata_compatible: false,
                weights_verified: false,
                source_update_requires_review: false,
                detail: format!(
                    "Version check unavailable: {error}; only the existing immutable manifest or compiled baseline may be installed"
                ),
            },
            None,
        ),
    };
    // Discovery never selects an upgrade implicitly. Only --refresh selects
    // the newly resolved, immutable candidate and downloads any changed bytes.
    let (manifest, metadata) = if options.refresh {
        let candidate = candidate.ok_or_else(|| invalid(updates.detail.clone()))?;
        (candidate.manifest, Some(candidate.metadata))
    } else if current == baseline_digest {
        (baseline(), None)
    } else {
        (cached_manifest(root, &current, transport)?, None)
    };
    let directory = install(
        transport,
        root,
        &manifest,
        metadata.as_ref().map(tempfile::TempDir::path),
    )?;
    let digest = manifest_digest(&manifest)?;
    activate(root, &digest, transport)?;
    updates.weights_verified =
        updates.candidate_manifest_digest.as_deref() == Some(digest.as_str());
    report("installed", Some(directory), Some(manifest), Some(updates))
}
fn terminal_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
    )
}

fn tensor_header(path: &Path) -> io::Result<(Value, u64)> {
    let mut file = checked_file(path)?;
    let bytes = file.metadata()?.len();
    let mut prefix = [0_u8; 8];
    file.read_exact(&mut prefix)?;
    let header_bytes = u64::from_le_bytes(prefix);
    if header_bytes == 0 || header_bytes > MAX_METADATA || header_bytes > bytes.saturating_sub(8) {
        return Err(invalid("invalid or oversized safetensors header"));
    }
    let mut data = vec![0_u8; header_bytes as usize];
    file.read_exact(&mut data)?;
    let header: Value = serde_json::from_slice(&data).map_err(io::Error::other)?;
    let data_bytes = bytes - 8 - header_bytes;
    validate_tensor_layout(&header, data_bytes)?;
    Ok((header, data_bytes))
}
fn validate_tensor_layout(header: &Value, data_bytes: u64) -> io::Result<()> {
    let object = header
        .as_object()
        .ok_or_else(|| invalid("safetensors header must be an object"))?;
    let mut intervals = Vec::new();
    for (name, tensor) in object {
        if name == "__metadata__" {
            if !tensor
                .as_object()
                .is_some_and(|metadata| metadata.values().all(Value::is_string))
            {
                return Err(invalid("invalid safetensors metadata"));
            }
            continue;
        }
        if name.is_empty() || !exact_keys(tensor, &["dtype", "shape", "data_offsets"]) {
            return Err(invalid("invalid tensor schema"));
        }
        let width = match tensor["dtype"].as_str() {
            Some("F32") => 4_u64,
            Some("F16" | "BF16") => 2_u64,
            _ => return Err(invalid("unsupported Vega tensor dtype")),
        };
        let shape = tensor["shape"]
            .as_array()
            .filter(|shape| shape.len() <= 8)
            .ok_or_else(|| invalid("invalid tensor rank"))?;
        let mut size = width;
        for dimension in shape {
            let dimension = dimension
                .as_u64()
                .filter(|dimension| *dimension > 0)
                .ok_or_else(|| invalid("invalid tensor dimension"))?;
            size = size
                .checked_mul(dimension)
                .ok_or_else(|| invalid("tensor size overflow"))?;
        }
        let offsets = tensor["data_offsets"]
            .as_array()
            .filter(|offsets| offsets.len() == 2)
            .ok_or_else(|| invalid("invalid tensor offsets"))?;
        let start = offsets[0]
            .as_u64()
            .ok_or_else(|| invalid("invalid tensor start"))?;
        let end = offsets[1]
            .as_u64()
            .ok_or_else(|| invalid("invalid tensor end"))?;
        if end > data_bytes || end.checked_sub(start) != Some(size) {
            return Err(invalid("tensor extent differs from shape or file size"));
        }
        intervals.push((start, end));
    }
    intervals.sort_unstable();
    let mut end = 0;
    for (start, next) in intervals {
        if start != end {
            return Err(invalid("safetensors data contains a gap or overlap"));
        }
        end = next;
    }
    if end != data_bytes || end == 0 {
        return Err(invalid("safetensors payload is incomplete or empty"));
    }
    Ok(())
}
fn verify_weight_headers(directory: &Path) -> io::Result<()> {
    let (engine, engine_bytes) = tensor_header(&directory.join("engine.safetensors"))?;
    super::vega_engine::validate_weight_header(
        &super::vega_engine::EngineConfig::from_json(&load_json(directory, "vega_config.json")?)
            .map_err(io::Error::other)?,
        &engine,
        engine_bytes,
    )
    .map_err(io::Error::other)?;
    let (backbone, _) =
        tensor_header(&directory.join("backbone/model.safetensors-00001-of-00001.safetensors"))?;
    let index = load_json(directory, "backbone/model.safetensors.index.json")?;
    let map = index
        .get("weight_map")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("missing backbone tensor index"))?;
    if !map
        .values()
        .all(|file| file.as_str() == Some("model.safetensors-00001-of-00001.safetensors"))
        || map.keys().collect::<BTreeSet<_>>()
            != backbone
                .as_object()
                .ok_or_else(|| invalid("invalid backbone header"))?
                .keys()
                .filter(|name| name.as_str() != "__metadata__")
                .collect::<BTreeSet<_>>()
    {
        return Err(invalid(
            "backbone tensor inventory differs from the pinned index",
        ));
    }
    Ok(())
}

struct LocalVerification<'a> {
    deadline: Instant,
    cancellation: &'a Cancellation,
}
impl Transport for LocalVerification<'_> {
    fn checkpoint(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Vega verification cancelled",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Vega verification deadline exceeded",
            ));
        }
        Ok(())
    }
    fn download(&self, _: &str, _: &Path, _: u64) -> io::Result<()> {
        Err(invalid("local Vega verification cannot use the network"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct MemoryTransport {
        responses: BTreeMap<String, Vec<u8>>,
        requests: RefCell<Vec<String>>,
        cancelled: Cell<bool>,
    }
    impl Transport for MemoryTransport {
        fn checkpoint(&self) -> io::Result<()> {
            if self.cancelled.get() {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "injected cancellation",
                ))
            } else {
                Ok(())
            }
        }
        fn download(&self, url: &str, destination: &Path, maximum: u64) -> io::Result<()> {
            self.checkpoint()?;
            self.requests.borrow_mut().push(url.into());
            let bytes = self
                .responses
                .get(url)
                .ok_or_else(|| invalid("injected missing response"))?;
            if bytes.len() as u64 > maximum {
                return Err(invalid("injected download overflow"));
            }
            write_new(destination, bytes)
        }
    }
    fn temporary() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
    }
    fn small_pin(data: &[u8]) -> FilePin {
        FilePin {
            role: "engine_weights".into(),
            repository: MODEL.into(),
            revision: BASELINE_REVISION.into(),
            upstream_path: "engine.safetensors".into(),
            bytes: data.len() as u64,
            sha256: hash(data),
        }
    }
    fn hub(repository: &str, revision: &str) -> Value {
        json!({"id": repository, "sha": revision, "private": false, "gated": false, "disabled": false,
            "cardData": {"license": "apache-2.0"}, "siblings": []})
    }
    fn discovery_transport() -> MemoryTransport {
        MemoryTransport {
            responses: [
                (
                    format!("https://huggingface.co/api/models/{MODEL}?blobs=true"),
                    serde_json::to_vec(&hub(MODEL, BASELINE_REVISION)).unwrap(),
                ),
                (
                    format!("https://huggingface.co/api/models/{BACKBONE}?blobs=true"),
                    serde_json::to_vec(&hub(BACKBONE, BASELINE_BACKBONE_REVISION)).unwrap(),
                ),
                (
                    format!("https://api.github.com/repos/{SOURCE}/commits?per_page=1"),
                    serde_json::to_vec(&json!([{"sha": BASELINE_SOURCE_REVISION}])).unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn baseline_binds_independent_components_calibration_and_source() {
        let manifest = baseline();
        validate_manifest(&manifest).unwrap();
        assert_eq!(manifest.files.len(), 9);
        assert_eq!(manifest.adapters, "off");
        assert_ne!(manifest.revision, manifest.backbone_revision);
        assert_eq!(
            manifest.files["engine.safetensors"].revision,
            manifest.revision
        );
        assert_eq!(
            manifest.files["backbone/tokenizer.json"].revision,
            manifest.backbone_revision
        );
        assert_eq!(
            manifest.files["licenses/vega.LICENSE"].revision,
            manifest.source_revision
        );
        assert!(valid_digest(&manifest.calibration_sha256));
        assert!(
            !manifest
                .files
                .keys()
                .any(|name| name.ends_with(".py") || name.starts_with("adapters/"))
        );
    }

    #[test]
    fn every_material_manifest_change_changes_cache_identity() {
        let original = baseline();
        let digest = manifest_digest(&original).unwrap();
        let mut changed = original.clone();
        changed.calibration_sha256 = hash(b"different calibration");
        assert_ne!(manifest_digest(&changed).unwrap(), digest);
        changed = original.clone();
        changed.backbone_revision = "b".repeat(40);
        assert_ne!(manifest_digest(&changed).unwrap(), digest);
        changed = original;
        changed.files.get_mut("engine.safetensors").unwrap().sha256 = hash(b"different weights");
        assert_ne!(manifest_digest(&changed).unwrap(), digest);
    }

    #[test]
    fn adapters_remote_code_and_unreviewed_backbones_fail_closed() {
        let mut manifest = baseline();
        manifest.adapters = "public_intents".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest = baseline();
        manifest
            .files
            .insert("remote.py".into(), small_pin(b"code"));
        assert!(validate_manifest(&manifest).is_err());
        manifest = baseline();
        manifest
            .files
            .get_mut("backbone/tokenizer.json")
            .unwrap()
            .sha256 = hash(b"changed vocabulary");
        assert!(validate_manifest(&manifest).is_err());
        manifest = baseline();
        manifest.source_revision = "b".repeat(40);
        assert!(validate_manifest(&manifest).is_err());
        manifest = baseline();
        manifest
            .files
            .get_mut("engine.safetensors")
            .unwrap()
            .upstream_path = "../remote.py".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn new_model_revision_can_pin_new_engine_but_not_reinterpret_backbone() {
        let mut manifest = baseline();
        manifest.revision = "a".repeat(40);
        for pin in manifest
            .files
            .values_mut()
            .filter(|pin| pin.repository == MODEL)
        {
            pin.revision = manifest.revision.clone();
        }
        manifest.files.get_mut("engine.safetensors").unwrap().sha256 = hash(b"new engine");
        validate_manifest(&manifest).unwrap();
        manifest
            .files
            .get_mut("backbone/config.json")
            .unwrap()
            .sha256 = hash(b"new architecture");
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn calibration_accepts_reviewed_schema_and_rejects_disabled_abstention() {
        let baseline: Value = serde_json::from_str(BASELINE_CALIBRATION).unwrap();
        validate_calibration(&baseline).unwrap();
        let mut changed = baseline.clone();
        changed["mode"] = json!("disabled");
        assert!(validate_calibration(&changed).is_err());
        changed = baseline.clone();
        changed["conformal"]["boolean"]["0.1"] = json!(-0.1);
        assert!(validate_calibration(&changed).is_err());
        changed = baseline.clone();
        changed["theta"][0] = json!([0.1]);
        assert!(validate_calibration(&changed).is_err());
        changed = baseline.clone();
        changed["adapters"] = json!(["public_intents"]);
        assert!(validate_calibration(&changed).is_err());
        changed = baseline;
        changed["abstain_threshold"]["0.05"] = json!(0.0);
        assert!(validate_calibration(&changed).is_err());
    }

    #[test]
    fn exact_public_urls_only_never_floating_weights_or_remote_source() {
        for artifact in ARTIFACTS {
            assert!(permitted_url(&artifact_url(
                &baseline().files[artifact.local]
            )));
        }
        assert!(!permitted_url(&format!(
            "https://huggingface.co/{MODEL}/resolve/main/engine.safetensors"
        )));
        assert!(!permitted_url(&format!(
            "https://huggingface.co/{MODEL}/resolve/{BASELINE_REVISION}/remote.py"
        )));
        assert!(!permitted_url(&format!(
            "https://raw.githubusercontent.com/{SOURCE}/{BASELINE_SOURCE_REVISION}/vega/_api.py"
        )));
        assert!(!permitted_url(&format!(
            "https://huggingface.co/api/models/{MODEL}?blobs=true&token=secret"
        )));
        assert!(!permitted_url("http://huggingface.co/anything"));
    }

    #[test]
    fn license_access_and_identity_changes_are_terminal() {
        let mut metadata = hub(MODEL, BASELINE_REVISION);
        assert_eq!(
            public_identity(&metadata, MODEL).unwrap(),
            BASELINE_REVISION
        );
        for field in ["private", "gated", "disabled"] {
            metadata = hub(MODEL, BASELINE_REVISION);
            metadata[field] = json!(true);
            let error = public_identity(&metadata, MODEL).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert!(terminal_error(&error));
        }
        metadata = hub(MODEL, BASELINE_REVISION);
        metadata["cardData"]["license"] = json!("other");
        assert!(public_identity(&metadata, MODEL).is_err());
        metadata = hub(MODEL, "main");
        assert!(public_identity(&metadata, MODEL).is_err());
    }

    #[test]
    fn check_only_does_not_create_cache_or_fetch_weights() {
        let scratch = temporary();
        let root = scratch.path().join("absent-cache");
        let transport = discovery_transport();
        let options = SetupOptions {
            check_updates: true,
            ..Default::default()
        };
        let report = run_with_transport(&options, &root, &transport).unwrap();
        assert_eq!(report.status, "checked");
        let updates = report.updates.unwrap();
        assert_eq!(updates.status, "incompatible_update");
        assert!(!updates.weights_verified);
        assert!(!updates.metadata_compatible);
        assert!(!root.exists());
        assert_eq!(transport.requests.borrow().len(), 3);
        assert!(
            !transport
                .requests
                .borrow()
                .iter()
                .any(|url| url.contains("/resolve/"))
        );
    }

    #[test]
    fn offline_missing_cache_fails_without_network_or_mutation() {
        let scratch = temporary();
        let root = scratch.path().join("absent-cache");
        let transport = MemoryTransport::default();
        let options = SetupOptions {
            offline: true,
            ..Default::default()
        };
        assert!(run_with_transport(&options, &root, &transport).is_err());
        assert!(transport.requests.borrow().is_empty());
        assert!(!root.exists());
    }

    #[test]
    fn invalid_actions_fail_before_cache_or_transport() {
        let scratch = temporary();
        let root = scratch.path().join("absent-cache");
        let transport = MemoryTransport::default();
        for options in [
            SetupOptions {
                cpu_runtime: Some(PathBuf::from("/unavailable-cpu-worker")),
                ..Default::default()
            },
            SetupOptions {
                offline: true,
                refresh: true,
                ..Default::default()
            },
            SetupOptions {
                check_updates: true,
                refresh: true,
                ..Default::default()
            },
            SetupOptions {
                rollback: Some("main".into()),
                ..Default::default()
            },
            SetupOptions {
                check_updates: true,
                runtime: Some(PathBuf::from("/unavailable-worker")),
                ..Default::default()
            },
        ] {
            assert!(run_with_transport(&options, &root, &transport).is_err());
        }
        assert!(transport.requests.borrow().is_empty());
        assert!(!root.exists());
    }

    #[test]
    fn component_reuse_has_one_download_and_separate_verified_single_link_copies() {
        let root = temporary();
        let pin = small_pin(b"component fixture");
        let url = artifact_url(&pin);
        let transport = MemoryTransport {
            responses: [(url, b"component fixture".to_vec())].into_iter().collect(),
            ..Default::default()
        };
        let first = component(&transport, root.path(), &pin, None).unwrap();
        let second = component(&transport, root.path(), &pin, None).unwrap();
        assert_eq!(first, second);
        assert_eq!(transport.requests.borrow().len(), 1);
        let installation = root.path().join("engine.safetensors");
        copy_component(&first, &installation, &pin, &Offline).unwrap();
        verify_file(&installation, &pin, &Offline).unwrap();
        let another = root.path().join("another-engine.safetensors");
        copy_component(&second, &another, &pin, &Offline).unwrap();
        verify_file(&another, &pin, &Offline).unwrap();
        assert_eq!(
            component(&transport, root.path(), &pin, None).unwrap(),
            first
        );
        assert_eq!(transport.requests.borrow().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let source = fs::metadata(first).unwrap();
            for path in [&installation, &another] {
                let installed = fs::metadata(path).unwrap();
                assert_ne!(source.ino(), installed.ino());
                assert_eq!(installed.nlink(), 1);
            }
            assert_eq!(source.nlink(), 1);
            assert_ne!(
                fs::metadata(installation).unwrap().ino(),
                fs::metadata(another).unwrap().ino()
            );
        }
    }

    #[test]
    fn bad_download_never_becomes_a_cached_component_or_active_pointer() {
        let root = temporary();
        let pin = small_pin(b"good");
        let transport = MemoryTransport {
            responses: [(artifact_url(&pin), b"evil".to_vec())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        assert!(component(&transport, root.path(), &pin, None).is_err());
        assert!(
            !root
                .path()
                .join("components")
                .join(&pin.sha256[7..])
                .exists()
        );
        assert!(active(root.path()).unwrap().is_none());
    }

    #[test]
    fn copied_artifact_rejects_corruption_and_preserves_existing_destination() {
        let root = temporary();
        let source = root.path().join("source");
        let output = root.path().join("output");
        let pin = small_pin(b"good");
        write_new(&source, b"evil").unwrap();
        assert!(copy_component(&source, &output, &pin, &Offline).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read(&source).unwrap(), b"evil");
        fs::write(&source, b"good").unwrap();
        write_new(&output, b"keep existing").unwrap();
        assert!(copy_component(&source, &output, &pin, &Offline).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"keep existing");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn unsafe_hardlinked_components_remain_rejected() {
        let root = temporary();
        let source = root.path().join("source");
        let alias = root.path().join("alias");
        let output = root.path().join("output");
        write_new(&source, b"good").unwrap();
        fs::hard_link(&source, &alias).unwrap();
        let pin = small_pin(b"good");
        assert!(verify_file(&source, &pin, &Offline).is_err());
        assert!(copy_component(&source, &output, &pin, &Offline).is_err());
        assert!(!output.exists());
        assert_eq!(fs::read(alias).unwrap(), b"good");
    }

    #[test]
    fn cancelled_copy_never_publishes_partial_artifact_and_cache_can_retry() {
        struct CancelDuringCopy(Cell<usize>);
        impl Transport for CancelDuringCopy {
            fn checkpoint(&self) -> io::Result<()> {
                let calls = self.0.get() + 1;
                self.0.set(calls);
                if calls >= 3 {
                    Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "injected copy cancellation",
                    ))
                } else {
                    Ok(())
                }
            }
            fn download(&self, _: &str, _: &Path, _: u64) -> io::Result<()> {
                panic!("copy must remain offline")
            }
        }
        let root = temporary();
        let source = root.path().join("source");
        let output = root.path().join("output");
        let bytes = vec![42; 2 * 65536 + 1];
        let pin = small_pin(&bytes);
        write_new(&source, &bytes).unwrap();
        let error =
            copy_component(&source, &output, &pin, &CancelDuringCopy(Cell::new(0))).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(!output.exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        verify_file(&source, &pin, &Offline).unwrap();
        copy_component(&source, &output, &pin, &Offline).unwrap();
        verify_file(&output, &pin, &Offline).unwrap();
    }

    #[test]
    fn failed_install_preserves_prior_activation_and_reusable_cache() {
        let root = temporary();
        let pin = small_pin(b"cached fixture");
        let transport = MemoryTransport {
            responses: [(artifact_url(&pin), b"cached fixture".to_vec())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let cached = component(&transport, root.path(), &pin, None).unwrap();
        let prior = hash(b"prior manifest");
        activate(root.path(), &prior, &Offline).unwrap();
        assert!(install(&transport, root.path(), &baseline(), None).is_err());
        assert_eq!(active(root.path()).unwrap().unwrap().manifest_digest, prior);
        verify_file(&cached, &pin, &Offline).unwrap();
        assert_eq!(
            component(&transport, root.path(), &pin, None).unwrap(),
            cached
        );
        assert_eq!(
            fs::read_dir(root.path().join("installations"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn corrupt_existing_component_is_preserved_and_never_refetched() {
        let root = temporary();
        let objects = root.path().join("components");
        private_directory(&objects, true).unwrap();
        let pin = small_pin(b"good");
        let path = objects.join(&pin.sha256[7..]);
        write_new(&path, b"evil").unwrap();
        let transport = MemoryTransport::default();
        assert!(component(&transport, root.path(), &pin, None).is_err());
        assert_eq!(fs::read(path).unwrap(), b"evil");
        assert!(transport.requests.borrow().is_empty());
    }

    #[test]
    fn activation_tracks_previous_identity_and_cancellation_preserves_it() {
        let root = temporary();
        let first = hash(b"first manifest");
        let second = hash(b"second manifest");
        let transport = MemoryTransport::default();
        activate(root.path(), &first, &transport).unwrap();
        activate(root.path(), &second, &transport).unwrap();
        let pointer = active(root.path()).unwrap().unwrap();
        assert_eq!(pointer.manifest_digest, second);
        assert_eq!(pointer.previous_manifest_digest, Some(first.clone()));
        transport.cancelled.set(true);
        assert!(activate(root.path(), &first, &transport).is_err());
        assert_eq!(
            active(root.path()).unwrap().unwrap().manifest_digest,
            second
        );
    }

    #[test]
    fn rollback_requires_verified_cached_manifest_without_network() {
        let root = temporary();
        let digest = hash(b"missing manifest");
        let options = SetupOptions {
            rollback: Some(digest),
            offline: true,
            ..Default::default()
        };
        let transport = MemoryTransport::default();
        assert!(run_with_transport(&options, root.path(), &transport).is_err());
        assert!(transport.requests.borrow().is_empty());
        assert!(active(root.path()).unwrap().is_none());
    }

    #[test]
    fn inference_verifier_rejects_floating_or_mismatched_identity_locally() {
        let root = temporary();
        write_new(
            &root.path().join("manifest.json"),
            &serde_json::to_vec(&baseline()).unwrap(),
        )
        .unwrap();
        assert!(verify_installation(root.path(), "main").is_err());
        let error = verify_installation(root.path(), &hash(b"wrong manifest")).unwrap_err();
        assert!(error.to_string().contains("differs from installation"));
        assert!(!root.path().join("active.json").exists());
    }

    #[test]
    fn inference_deadline_and_cancellation_apply_before_io_and_each_hash_chunk() {
        let root = temporary();
        let cancellation = Cancellation::default();
        let digest = manifest_digest(&baseline()).unwrap();
        let error =
            verify_installation_bounded(root.path(), &digest, Instant::now(), &cancellation)
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        cancellation.cancel();
        let error = verify_installation_bounded(
            root.path(),
            &digest,
            Instant::now() + Duration::from_secs(120),
            &cancellation,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let transport = LocalVerification {
            deadline: Instant::now() + Duration::from_secs(120),
            cancellation: &cancellation,
        };
        assert_eq!(
            verify_file(&root.path().join("missing"), &small_pin(b"x"), &transport)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(
            transport
                .download("https://example.invalid", root.path(), 1)
                .is_err()
        );
    }

    #[test]
    fn tensor_extent_checks_reject_overflow_gaps_and_code_dtypes() {
        let header = json!({"w": {"dtype":"F32", "shape":[2], "data_offsets":[0,8]}});
        validate_tensor_layout(&header, 8).unwrap();
        let mut changed = header.clone();
        changed["w"]["data_offsets"] = json!([4, 12]);
        assert!(validate_tensor_layout(&changed, 12).is_err());
        changed = header.clone();
        changed["w"]["shape"] = json!([u64::MAX, 2]);
        assert!(validate_tensor_layout(&changed, 8).is_err());
        changed = header;
        changed["w"]["dtype"] = json!("PICKLE");
        assert!(validate_tensor_layout(&changed, 8).is_err());
    }

    #[test]
    fn emitted_configuration_keeps_readiness_disabled_and_has_no_fake_runtime() {
        let text = configuration(Path::new("/private/models/vega"), &hash(b"manifest")).unwrap();
        let config: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(config["vega"]["mode"].as_str(), Some("disabled"));
        assert_eq!(
            config["vega"]["model_manifest_digest"].as_str(),
            Some(hash(b"manifest").as_str())
        );
        assert!(config["vega"].get("installation").is_none());
        assert!(!text.contains("/private/models"));
        assert!(config.get("decision").is_none());
        let typed: super::super::vega_readiness::Configuration =
            config["vega"].clone().try_into().unwrap();
        assert_eq!(typed.mode, super::super::vega_readiness::Mode::Disabled);
        assert!(typed.installation.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_world_readable_cache_files_are_refused() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = temporary();
        let real = root.path().join("real");
        write_new(&real, b"data").unwrap();
        let link = root.path().join("link");
        symlink(&real, &link).unwrap();
        assert!(checked_file(&link).is_err());
        fs::set_permissions(&real, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(checked_file(&real).is_err());
        assert!(safe_path(Path::new("/tmp/../escape")).is_err());
    }
}
