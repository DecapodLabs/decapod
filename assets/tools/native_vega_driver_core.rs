// Shared implementation for the standalone driver and its integration tests.
use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
const OUTPUT: &str = "target/source-ci-native-vega";
const DECAPOD: &str = "target/aarch64-apple-darwin/release/decapod";
const WORKER_BUILD: &str = "target/source-ci-native-vega/worker-build/aarch64-apple-darwin/release";
const WORKER: &str =
    "target/source-ci-native-vega/worker-build/aarch64-apple-darwin/release/decapod-vega-infer";
const REQUEST_SCHEMA: &str = "decapod.vega-worker.v1";
const STATE: &str = "Native capability fixture only. Intent: execute the pinned Vega base-text checkpoint with the pinned Qwen backbone on Apple Metal, pool layer activations, and run the Rust base engine and upstream calibration. A separate fixed Metal shader passed numerical verification. This fixture asserts no repository-readiness decision, golden parity, or readiness-domain calibration.";
fn invalid(reason: impl Into<String>) -> io::Error {
    io::Error::other(reason.into())
}
fn hash_file(path: &Path) -> io::Result<String> {
    hashing::file_sha256(path)
        .map(|s| format!("sha256:{s}"))
        .map_err(invalid)
}
fn hash(bytes: &[u8]) -> io::Result<String> {
    hashing::sha256(bytes)
        .map(|s| format!("sha256:{s}"))
        .map_err(invalid)
}
fn regular(path: &Path, maximum: u64) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > maximum
        || fs::canonicalize(path)? != path
    {
        return Err(invalid(format!(
            "required ordinary bounded file unavailable: {}",
            path.display()
        )));
    }
    Ok(())
}
fn new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    let mut file = new_file(path)?;
    serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()
}
fn read_bounded(path: &Path, maximum: u64) -> io::Result<Vec<u8>> {
    regular(path, maximum)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("file grew beyond bound"));
    }
    Ok(bytes)
}
fn read_json(path: &Path, maximum: u64) -> io::Result<Value> {
    serde_json::from_slice(&read_bounded(path, maximum)?).map_err(io::Error::other)
}

fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
fn bounded(command: &mut Command, stdout: &Path, stderr: &Path, seconds: u64) -> io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(new_file(stdout)?)
        .stderr(new_file(stderr)?);
    let mut child = command.spawn()?;
    let start = Instant::now();
    loop {
        let exceeded = [stdout, stderr]
            .iter()
            .any(|p| fs::metadata(p).is_ok_and(|m| m.len() > 4 * 1024 * 1024));
        if exceeded || start.elapsed() > Duration::from_secs(seconds) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(invalid("native child exceeded bounded time or output"));
        }
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            return Err(invalid(format!("native child failed: {status}")));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FilePin {
    role: String,
    repository: String,
    revision: String,
    upstream_path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    profile: String,
    model: String,
    revision: String,
    backbone: String,
    backbone_revision: String,
    source: String,
    source_revision: String,
    license: String,
    adapters: String,
    calibration_sha256: String,
    files: BTreeMap<String, FilePin>,
}
fn expected_manifest() -> io::Result<Manifest> {
    let config: Value = serde_json::from_slice(CONFIG_BYTES).map_err(io::Error::other)?;
    let mut files = BTreeMap::new();
    for ((name, bytes, sha256), role) in MODEL_FILES.iter().zip(FILE_ROLES) {
        let (repository, revision, upstream) = if let Some(path) = name.strip_prefix("backbone/") {
            (BACKBONE_ID, BACKBONE_REVISION, path)
        } else if *name == "licenses/vega.LICENSE" {
            (SOURCE_ID, SOURCE_REVISION, "LICENSE")
        } else {
            (MODEL_ID, MODEL_REVISION, *name)
        };
        files.insert(
            (*name).into(),
            FilePin {
                role: (*role).into(),
                repository: repository.into(),
                revision: revision.into(),
                upstream_path: upstream.into(),
                bytes: *bytes,
                sha256: (*sha256).into(),
            },
        );
    }
    Ok(Manifest {
        schema: "decapod.vega-install.v1".into(),
        profile: PROFILE.into(),
        model: MODEL_ID.into(),
        revision: MODEL_REVISION.into(),
        backbone: BACKBONE_ID.into(),
        backbone_revision: BACKBONE_REVISION.into(),
        source: SOURCE_ID.into(),
        source_revision: SOURCE_REVISION.into(),
        license: "apache-2.0".into(),
        adapters: "off".into(),
        calibration_sha256: hash(
            &serde_json::to_vec(&config["calibration"]).map_err(io::Error::other)?,
        )?,
        files,
    })
}
fn manifest_digest(manifest: &Manifest) -> io::Result<String> {
    hash(&serde_json::to_vec(manifest).map_err(io::Error::other)?)
}
fn model_directory(root: &Path) -> io::Result<PathBuf> {
    Ok(root
        .join(OUTPUT)
        .join("model-cache/installations")
        .join(&manifest_digest(&expected_manifest()?)?[7..]))
}
fn model_files(root: &Path) -> io::Result<Vec<Value>> {
    let directory = model_directory(root)?;
    if fs::canonicalize(&directory)? != directory {
        return Err(invalid("noncanonical installation directory"));
    }
    let manifest_path = directory.join("manifest.json");
    regular(&manifest_path, 65536)?;
    let manifest: Manifest =
        serde_json::from_slice(&read_bounded(&manifest_path, 65536)?).map_err(io::Error::other)?;
    if manifest != expected_manifest()? {
        return Err(invalid(
            "installation manifest differs from independently pinned base profile",
        ));
    }
    let mut files = Vec::new();
    for &(name, bytes, expected) in MODEL_FILES {
        let file = directory.join(name);
        regular(&file, bytes)?;
        if fs::metadata(&file)?.len() != bytes || hash_file(&file)? != expected {
            return Err(invalid(format!("pinned Vega component differs: {name}")));
        }
        files.push(json!({"path":name,"bytes":bytes,"sha256":expected}));
    }
    Ok(files)
}
fn collect_libraries(
    dir: &Path,
    depth: usize,
    remaining: &mut usize,
    found: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if depth > 16 {
        return Err(invalid("kernel build output nesting exceeds bound"));
    }
    for entry in fs::read_dir(dir)? {
        if *remaining == 0 {
            return Err(invalid("kernel build inventory exceeds bound"));
        }
        *remaining -= 1;
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_libraries(&entry.path(), depth + 1, remaining, found)?;
        } else if kind.is_file() && entry.path().extension().is_some_and(|e| e == "metallib") {
            found.push(entry.path());
        }
    }
    Ok(())
}
fn kernel_inventory(root: &Path) -> io::Result<Vec<Value>> {
    let mut libraries = Vec::new();
    let mut remaining = 20000;
    let build = root.join(WORKER_BUILD).join("build");
    for entry in fs::read_dir(build)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|s| s.starts_with("metal-kernels-"))
            && entry.file_type()?.is_dir()
        {
            let out = entry.path().join("out");
            if out.is_dir() {
                collect_libraries(&out, 0, &mut remaining, &mut libraries)?;
            }
        }
    }
    libraries.sort();
    // The reviewed attention.rs build compiles both SDKs. Candle's other Metal
    // kernels are embedded source in its locked crate and compiled at runtime.
    let mut result = Vec::new();
    for name in ["paged_attention.metallib", "paged_attention_ios.metallib"] {
        let matching: Vec<_> = libraries
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n == name))
            .collect();
        let first = matching
            .first()
            .ok_or_else(|| invalid(format!("required real compiled kernel absent: {name}")))?;
        regular(first, 512 * 1024 * 1024)?;
        let sha256 = hash_file(first)?;
        for path in &matching {
            regular(path, 512 * 1024 * 1024)?;
            if hash_file(path)? != sha256 {
                return Err(invalid("ambiguous compiled kernel identities"));
            }
        }
        result.push(json!({"path":first.strip_prefix(root).map_err(io::Error::other)?.to_string_lossy(),"name":name,"bytes":fs::metadata(first)?.len(),"sha256":sha256}));
    }
    Ok(result)
}
fn capture_worker(root: &Path) -> io::Result<()> {
    let mut binaries = Vec::new();
    for name in [DECAPOD, WORKER] {
        let file = root.join(name);
        regular(&file, 512 * 1024 * 1024)?;
        binaries.push(
            json!({"path":name,"bytes":fs::metadata(&file)?.len(),"sha256":hash_file(&file)?}),
        );
    }
    let lock = root.join("assets/tools/vega-native/Cargo.lock");
    regular(&lock, 2 * 1024 * 1024)?;
    let lock_text =
        String::from_utf8(read_bounded(&lock, 2 * 1024 * 1024)?).map_err(io::Error::other)?;
    // Full commit source IDs, not substrings in a floating branch or package name.
    for identity in [
        format!(
            "git+https://github.com/EricLBuehler/candle-vllm?rev={BACKBONE_RUNTIME_REVISION}#{BACKBONE_RUNTIME_REVISION}"
        ),
        format!("git+https://github.com/guoqingbao/candle.git?rev=b60efcf#{CANDLE_REVISION}"),
        format!(
            "git+https://github.com/guoqingbao/attention.rs.git?rev=5b4d645#{ATTENTION_REVISION}"
        ),
    ] {
        if !lock_text
            .lines()
            .any(|line| line == format!("source = \"{identity}\""))
        {
            return Err(invalid(
                "reviewed runtime source identity missing from locked worker closure",
            ));
        }
    }
    let kernels = kernel_inventory(root)?;
    write_json(
        &root.join(OUTPUT).join("worker.json"),
        &json!({
            "schema":"decapod.native-vega-worker.v1","target":"aarch64-apple-darwin","feature":"vega-native","backend":"candle-metal",
            "binaries":binaries,"worker_manifest_sha256":hash_file(&root.join("assets/tools/vega-native/Cargo.toml"))?,"worker_lock_sha256":hash_file(&lock)?,
            "candle_revision":CANDLE_REVISION,"backbone_runtime_revision":BACKBONE_RUNTIME_REVISION,"attention_revision":ATTENTION_REVISION,
            "compiled_kernel_libraries":kernels,"candle_kernels":"embedded source from pinned candle-metal-kernels, JIT on actual Metal device",
            "source_contract_tests":"base-profile-only; no unavailable upstream golden values asserted",
            "parity":"unverified","publication_ready":false
        }),
    )
}
fn setup_model(root: &Path) -> io::Result<()> {
    let cache = root.join(OUTPUT).join("model-cache");
    private_directory(&cache)?;
    let binary = root.join(DECAPOD);
    regular(&binary, 512 * 1024 * 1024)?;
    let mut command = Command::new(&binary);
    command
        .args(["setup", "vega", "--cache-directory"])
        .arg(&cache);
    let report = root.join(OUTPUT).join("setup.json");
    let stderr = root.join(OUTPUT).join("setup.stderr");
    bounded(&mut command, &report, &stderr, 3600)?;
    let result = read_json(&report, 65536)?;
    let expected = expected_manifest()?;
    let expected_digest = manifest_digest(&expected)?;
    let directory = model_directory(root)?;
    let actual: Manifest =
        serde_json::from_value(result["manifest"].clone()).map_err(io::Error::other)?;
    if result["schema"] != "decapod.vega-setup.v1"
        || result["model_directory"].as_str() != directory.to_str()
        || result["manifest_digest"] != expected_digest
        || actual != expected
        || result["runtime_installed"] != false
    {
        return Err(invalid(
            "explicit setup did not return the exact compiled Vega/Qwen profile",
        ));
    }
    let files = model_files(root)?;
    write_json(
        &root.join(OUTPUT).join("model.json"),
        &json!({
            "schema":"decapod.native-vega-model.v1","manifest":actual,"manifest_digest":expected_digest,
            "installation":directory,"files":files,"manifest_file_sha256":hash_file(&directory.join("manifest.json"))?,
            "setup_report_sha256":hash_file(&report)?,"download_only_during_explicit_setup":true,"refresh":false,"publication_ready":false
        }),
    )
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Precision {
    backbone: String,
    engine: String,
    calibration: String,
}
fn precision() -> Precision {
    Precision {
        backbone: "f32".into(),
        engine: "f32".into(),
        calibration: "f64".into(),
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    probabilities: [f64; 2],
    prediction_set: Option<Vec<bool>>,
    abstain: bool,
    unbound: bool,
    settle_steps: usize,
    energy: f64,
    kinetic: f64,
    nearest: f64,
    confidence: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    schema: String,
    request_id: String,
    input_digest: String,
    manifest_digest: String,
    backend: String,
    device: String,
    engine_device: String,
    precision: Precision,
    input_tokens: usize,
    outcome: Option<Outcome>,
    error: Option<String>,
}
fn verify_response(
    bytes: &[u8],
    request_id: &str,
    input_digest: &str,
    manifest: &str,
) -> io::Result<Response> {
    if bytes.is_empty() || bytes.len() > 65536 {
        return Err(invalid("worker response exceeds bound"));
    }
    let response: Response = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if response.schema != REQUEST_SCHEMA
        || response.request_id != request_id
        || response.input_digest != input_digest
        || response.manifest_digest != manifest
        || response.backend != "candle-metal"
        || response.device != "metal"
        || response.engine_device != "cpu"
        || response.precision != precision()
        || response.error.is_some()
        || response.input_tokens == 0
        || response.input_tokens > 4096
    {
        return Err(invalid("native response identity/device/precision differs"));
    }
    let outcome = response
        .outcome
        .as_ref()
        .ok_or_else(|| invalid("native outcome missing"))?;
    let [no, yes] = outcome.probabilities;
    if ![
        no,
        yes,
        outcome.energy,
        outcome.kinetic,
        outcome.nearest,
        outcome.confidence,
    ]
    .iter()
    .all(|n| n.is_finite())
        || !(0.0..=1.0).contains(&no)
        || !(0.0..=1.0).contains(&yes)
        || (no + yes - 1.0).abs() > 1e-8
        || (outcome.confidence - no.max(yes)).abs() > 1e-8
        || outcome.kinetic < 0.0
        || outcome.nearest < 0.0
        || !(1..=12).contains(&outcome.settle_steps)
    {
        return Err(invalid("native engine/calibration output malformed"));
    }
    if !outcome.abstain && !outcome.unbound {
        match outcome.prediction_set.as_deref() {
            Some([true]) if yes > no => {}
            Some([false]) if no >= yes => {}
            Some([false, true]) | Some([true, false]) | None => {}
            _ => return Err(invalid("native prediction set malformed or inconsistent")),
        }
    }
    Ok(response)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeProof {
    schema: String,
    request_id: String,
    input_digest: String,
    manifest_digest: String,
    backbone_source_revision: String,
    backend: String,
    device: String,
    engine_device: String,
    precision: Precision,
    feature_layers: [usize; 2],
    option_layers: [usize; 1],
    feats_shape: [usize; 4],
    opts_shape: [usize; 4],
    chunks_shape: [usize; 3],
    pooled_feats_sha256: String,
    pooled_opts_sha256: String,
    pooled_chunks_sha256: String,
    chunk_mask: [bool; 16],
    engine_readout_sha256: String,
    outcome_sha256: String,
    adapters: String,
    calibration_scope: String,
    parity: String,
}
fn verify_native_proof(bytes: &[u8], response: &Response) -> io::Result<()> {
    let proof: NativeProof = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if proof.schema != "decapod.vega-native-proof.v1"
        || proof.request_id != response.request_id
        || proof.input_digest != response.input_digest
        || proof.manifest_digest != response.manifest_digest
        || proof.backbone_source_revision != BACKBONE_RUNTIME_REVISION
        || proof.backend != "candle-metal"
        || proof.device != "metal"
        || proof.engine_device != "cpu"
        || proof.precision != precision()
        || proof.feature_layers != [13, 19]
        || proof.option_layers != [19]
        || proof.feats_shape != [1, 2, 3, 1024]
        || proof.opts_shape != [1, 2, 1, 1024]
        || proof.chunks_shape != [1, 16, 1024]
        || proof.adapters != "off"
        || proof.calibration_scope != "upstream-general-decisions"
        || proof.parity != "unverified"
        || proof.outcome_sha256
            != hash(
                &serde_json::to_vec(
                    response
                        .outcome
                        .as_ref()
                        .ok_or_else(|| invalid("missing outcome"))?,
                )
                .map_err(io::Error::other)?,
            )?
    {
        return Err(invalid(
            "native pooling/engine provenance differs from actual bounded request",
        ));
    }
    for value in [
        &proof.pooled_feats_sha256,
        &proof.pooled_opts_sha256,
        &proof.pooled_chunks_sha256,
        &proof.engine_readout_sha256,
    ] {
        if !digest(value) {
            return Err(invalid("native pooling/engine measurement missing"));
        }
    }
    // Mask content is retained, never guessed from a fixed token count.
    let _ = proof.chunk_mask;
    Ok(())
}
fn infer(root: &Path) -> io::Result<()> {
    let files = model_files(root)?;
    let output = root.join(OUTPUT);
    let worker = root.join(WORKER);
    regular(&worker, 512 * 1024 * 1024)?;
    let worker_hash = hash_file(&worker)?;
    let kernels = kernel_inventory(root)?;
    let manifest = expected_manifest()?;
    let manifest_digest = manifest_digest(&manifest)?;
    let input_digest = hash(STATE.as_bytes())?;
    let request_id = format!("native-vega-smoke-{}", &input_digest[7..23]);
    let request = output.join("request.json");
    write_json(
        &request,
        &json!({"schema":REQUEST_SCHEMA,"request_id":request_id,"input_digest":input_digest,
        "installation":model_directory(root)?,"manifest_digest":manifest_digest,"state":STATE,"alpha":"0.1","risk":"0.1"}),
    )?;
    let response_file = output.join("response.json");
    let stderr = output.join("worker.stderr");
    let proof = output.join("native-proof.json");
    let mut command = Command::new(&worker);
    command
        .arg("--request")
        .arg(&request)
        .arg("--proof-output")
        .arg(&proof);
    bounded(&mut command, &response_file, &stderr, 600)?;
    let response = verify_response(
        &read_bounded(&response_file, 65536)?,
        &request_id,
        &input_digest,
        &manifest_digest,
    )?;
    regular(&proof, 65536)?;
    verify_native_proof(&read_bounded(&proof, 65536)?, &response)?;
    if model_files(root)? != files
        || hash_file(&worker)? != worker_hash
        || kernel_inventory(root)? != kernels
    {
        return Err(invalid("native inputs changed during inference"));
    }
    write_json(
        &output.join("inference.json"),
        &json!({
            "schema":"decapod.native-vega-inference.v1","backend":"candle-metal","device":"metal","engine_device":"cpu","precision":precision(),
            "model":MODEL_ID,"revision":MODEL_REVISION,"backbone":BACKBONE_ID,"backbone_revision":BACKBONE_REVISION,
            "source":SOURCE_ID,"source_revision":SOURCE_REVISION,"manifest_digest":manifest_digest,"calibration_sha256":manifest.calibration_sha256,
            "request_id":request_id,"input_digest":input_digest,"request_sha256":hash_file(&request)?,"response_sha256":hash_file(&response_file)?,
            "worker_sha256":worker_hash,"compiled_kernel_libraries":kernels,"native_proof_sha256":hash_file(&proof)?,"model_files":files,
            "outcome":response.outcome,"input_tokens":response.input_tokens,"backbone_cpu_fallback":false,"adapters":"off",
            "publication_ready":false,"parity":"unverified","calibration_scope":"upstream-general-decisions",
            "proof_scope":"actual pinned base-profile backbone/pooling/engine/calibration execution only"
        }),
    )
}
fn main_result() -> io::Result<()> {
    if std::env::consts::OS != "macos" || std::env::consts::ARCH != "aarch64" {
        return Err(invalid("native Apple Silicon execution required"));
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err(invalid("expected one finite native proof operation"));
    }
    let root = fs::canonicalize(std::env::current_dir()?)?;
    match args[0].as_str() {
        "capture-worker" => capture_worker(&root),
        "setup-model" => setup_model(&root),
        "infer" => infer(&root),
        _ => Err(invalid("unknown native proof operation")),
    }
}
fn main() {
    if let Err(error) = main_result() {
        eprintln!("native Vega proof: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic values exercise the pure protocol validators only. These tests
    // do not run a model or Metal and are not upstream numerical golden tests.
    fn response_fixture() -> Value {
        json!({
            "schema": "decapod.vega-worker.v1",
            "request_id": "synthetic-protocol-request",
            "input_digest": hash(b"synthetic state").unwrap(),
            "manifest_digest": manifest_digest(&expected_manifest().unwrap()).unwrap(),
            "backend": "candle-metal",
            "device": "metal",
            "engine_device": "cpu",
            "precision": {"backbone": "f32", "engine": "f32", "calibration": "f64"},
            "input_tokens": 32,
            "outcome": {
                "probabilities": [0.25, 0.75], "prediction_set": [true],
                "abstain": false, "unbound": false, "settle_steps": 2,
                "energy": -0.5, "kinetic": 0.25, "nearest": 0.5, "confidence": 0.75
            },
            "error": null
        })
    }

    fn parse_fixture(value: &Value) -> io::Result<Response> {
        let expected = response_fixture();
        verify_response(
            &serde_json::to_vec(value).unwrap(),
            expected["request_id"].as_str().unwrap(),
            expected["input_digest"].as_str().unwrap(),
            expected["manifest_digest"].as_str().unwrap(),
        )
    }

    fn proof_fixture(response: &Response) -> Value {
        json!({
            "schema": "decapod.vega-native-proof.v1",
            "request_id": response.request_id,
            "input_digest": response.input_digest,
            "manifest_digest": response.manifest_digest,
            "backbone_source_revision": BACKBONE_RUNTIME_REVISION,
            "backend": "candle-metal", "device": "metal", "engine_device": "cpu",
            "precision": {"backbone": "f32", "engine": "f32", "calibration": "f64"},
            "feature_layers": [13, 19], "option_layers": [19],
            "feats_shape": [1, 2, 3, 1024], "opts_shape": [1, 2, 1, 1024],
            "chunks_shape": [1, 16, 1024],
            "pooled_feats_sha256": hash(b"synthetic features").unwrap(),
            "pooled_opts_sha256": hash(b"synthetic options").unwrap(),
            "pooled_chunks_sha256": hash(b"synthetic chunks").unwrap(),
            "chunk_mask": [true, false, false, false, false, false, false, false,
                false, false, false, false, false, false, false, false],
            "engine_readout_sha256": hash(b"synthetic readout").unwrap(),
            "outcome_sha256": hash(&serde_json::to_vec(response.outcome.as_ref().unwrap()).unwrap()).unwrap(),
            "adapters": "off", "calibration_scope": "upstream-general-decisions",
            "parity": "unverified"
        })
    }

    #[test]
    fn native_vega_manifest_binds_exact_base_profile_inventory() {
        let manifest = expected_manifest().unwrap();
        assert_eq!(manifest.schema, "decapod.vega-install.v1");
        assert_eq!(manifest.profile, "vega-0.8b-base-text-v1");
        assert_eq!(manifest.model, "nandakishorm/vega-08b-public-intents");
        assert_eq!(manifest.backbone, "Qwen/Qwen3.5-0.8B");
        assert_eq!(manifest.source, "NandhaKishorM/vegaml");
        assert_eq!(manifest.adapters, "off");
        assert_eq!(manifest.license, "apache-2.0");
        assert_eq!(manifest.revision, MODEL_REVISION);
        assert_eq!(manifest.backbone_revision, BACKBONE_REVISION);
        assert_eq!(manifest.source_revision, SOURCE_REVISION);
        assert_eq!(manifest.files.len(), 9);
        assert_eq!(MODEL_FILES.len(), FILE_ROLES.len());
        assert_eq!(
            manifest.files.values().map(|file| file.bytes).sum::<u64>(),
            MODEL_FILES.iter().map(|(_, bytes, _)| *bytes).sum::<u64>()
        );
        for ((name, bytes, sha256), role) in MODEL_FILES.iter().zip(FILE_ROLES) {
            let pin = &manifest.files[*name];
            assert_eq!(pin.bytes, *bytes);
            assert_eq!(pin.sha256, *sha256);
            assert_eq!(pin.role, *role);
            assert!(digest(&pin.sha256));
            if let Some(upstream) = name.strip_prefix("backbone/") {
                assert_eq!(pin.repository, BACKBONE_ID);
                assert_eq!(pin.revision, BACKBONE_REVISION);
                assert_eq!(pin.upstream_path, upstream);
            } else if *name == "licenses/vega.LICENSE" {
                assert_eq!(pin.repository, SOURCE_ID);
                assert_eq!(pin.revision, SOURCE_REVISION);
                assert_eq!(pin.upstream_path, "LICENSE");
            } else {
                assert_eq!(pin.repository, MODEL_ID);
                assert_eq!(pin.revision, MODEL_REVISION);
                assert_eq!(pin.upstream_path, *name);
            }
        }
        assert_eq!(
            hash(CONFIG_BYTES).unwrap(),
            manifest.files["vega_config.json"].sha256
        );
        let config: Value = serde_json::from_slice(CONFIG_BYTES).unwrap();
        assert_eq!(
            manifest.calibration_sha256,
            hash(&serde_json::to_vec(&config["calibration"]).unwrap()).unwrap()
        );
        let digest = manifest_digest(&manifest).unwrap();
        let root = Path::new("/synthetic-subject");
        assert_eq!(
            model_directory(root).unwrap(),
            root.join(OUTPUT)
                .join("model-cache/installations")
                .join(&digest[7..])
        );
    }

    #[test]
    fn native_vega_response_requires_typed_bound_request_and_base_precision() {
        let valid = response_fixture();
        assert!(parse_fixture(&valid).is_ok());
        for (pointer, replacement) in [
            ("/schema", json!("decapod.vega-worker.v2")),
            ("/request_id", json!("different-request")),
            ("/input_digest", json!(hash(b"different input").unwrap())),
            (
                "/manifest_digest",
                json!(hash(b"different manifest").unwrap()),
            ),
            ("/backend", json!("cpu")),
            ("/device", json!("cpu")),
            ("/engine_device", json!("metal")),
            ("/precision/backbone", json!("bf16")),
            ("/precision/engine", json!("f64")),
            ("/precision/calibration", json!("f32")),
            ("/input_tokens", json!(0)),
            ("/input_tokens", json!(4097)),
            ("/input_tokens", json!("32")),
            ("/error", json!("worker failed")),
            ("/outcome", Value::Null),
            ("/outcome/probabilities", json!([0.25])),
            ("/outcome/probabilities", json!([0.25, 0.75, 0.0])),
            ("/outcome/probabilities", json!([0.5, 0.75])),
            ("/outcome/probabilities", json!([-0.25, 1.25])),
            ("/outcome/confidence", json!(0.25)),
            ("/outcome/kinetic", json!(-0.25)),
            ("/outcome/nearest", json!(-0.25)),
            ("/outcome/energy", Value::Null),
            ("/outcome/settle_steps", json!(0)),
            ("/outcome/settle_steps", json!(13)),
            ("/outcome/prediction_set", json!([false])),
            ("/outcome/prediction_set", json!([])),
            ("/outcome/prediction_set", json!([true, true])),
            ("/outcome/abstain", json!("false")),
        ] {
            let mut changed = valid.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                parse_fixture(&changed).is_err(),
                "accepted changed field {pointer}"
            );
        }
        for pointer in ["", "/precision", "/outcome"] {
            let mut changed = valid.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert!(
                parse_fixture(&changed).is_err(),
                "accepted unknown field at {pointer}"
            );
        }
        for field in [
            "schema",
            "request_id",
            "input_digest",
            "manifest_digest",
            "backend",
            "device",
            "engine_device",
            "precision",
            "input_tokens",
            "outcome",
        ] {
            let mut changed = valid.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                parse_fixture(&changed).is_err(),
                "accepted absent field {field}"
            );
        }
    }

    #[test]
    fn native_vega_response_rejects_duplicate_trailing_and_oversize_json() {
        let value = response_fixture();
        let valid = serde_json::to_string(&value).unwrap();
        for bytes in [
            Vec::new(),
            format!("{valid}{{}}").into_bytes(),
            format!("{{\"schema\":\"decapod.vega-worker.v1\",{}", &valid[1..]).into_bytes(),
            format!("{valid}{}", " ".repeat(65536)).into_bytes(),
            valid
                .replace("\"energy\":-0.5", "\"energy\":1e999")
                .into_bytes(),
        ] {
            assert!(
                verify_response(
                    &bytes,
                    value["request_id"].as_str().unwrap(),
                    value["input_digest"].as_str().unwrap(),
                    value["manifest_digest"].as_str().unwrap()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn native_vega_proof_requires_typed_pooling_shapes_and_matching_outcome() {
        let response = parse_fixture(&response_fixture()).unwrap();
        let valid = proof_fixture(&response);
        assert!(verify_native_proof(&serde_json::to_vec(&valid).unwrap(), &response).is_ok());
        for (pointer, replacement) in [
            ("/schema", json!("unknown-proof")),
            ("/request_id", json!("different-request")),
            ("/input_digest", json!(hash(b"different input").unwrap())),
            (
                "/manifest_digest",
                json!(hash(b"different manifest").unwrap()),
            ),
            ("/backbone_source_revision", json!("main")),
            ("/backend", json!("cpu")),
            ("/device", json!("cpu")),
            ("/engine_device", json!("metal")),
            ("/precision/backbone", json!("bf16")),
            ("/precision/engine", json!("f64")),
            ("/precision/calibration", json!("f32")),
            ("/feature_layers", json!([12, 19])),
            ("/feature_layers", json!([13])),
            ("/option_layers", json!([18])),
            ("/feats_shape", json!([1, 2, 3, 2048])),
            ("/opts_shape", json!([1, 2, 2, 1024])),
            ("/chunks_shape", json!([1, 15, 1024])),
            ("/chunk_mask", json!([true])),
            ("/chunk_mask/0", json!(1)),
            (
                "/outcome_sha256",
                json!(hash(b"different outcome").unwrap()),
            ),
            ("/adapters", json!("on")),
            ("/calibration_scope", json!("repository-readiness")),
            ("/parity", json!("verified")),
        ] {
            let mut changed = valid.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                verify_native_proof(&serde_json::to_vec(&changed).unwrap(), &response).is_err(),
                "accepted changed field {pointer}"
            );
        }
        for field in [
            "pooled_feats_sha256",
            "pooled_opts_sha256",
            "pooled_chunks_sha256",
            "engine_readout_sha256",
        ] {
            for bad_digest in [
                "".to_string(),
                "a".repeat(64),
                format!("sha256:{}", "A".repeat(64)),
                format!("sha256:{}", "a".repeat(63)),
            ] {
                let mut changed = valid.clone();
                changed[field] = json!(bad_digest);
                assert!(
                    verify_native_proof(&serde_json::to_vec(&changed).unwrap(), &response).is_err(),
                    "accepted invalid digest {field}"
                );
            }
        }
        for field in valid.as_object().unwrap().keys() {
            let mut changed = valid.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                verify_native_proof(&serde_json::to_vec(&changed).unwrap(), &response).is_err(),
                "accepted absent proof field {field}"
            );
        }
        for pointer in ["", "/precision"] {
            let mut changed = valid.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert!(
                verify_native_proof(&serde_json::to_vec(&changed).unwrap(), &response).is_err()
            );
        }
        let serialized = serde_json::to_string(&valid).unwrap();
        for malformed in [
            String::new(),
            format!("{serialized}{{}}"),
            format!(
                "{{\"schema\":\"decapod.vega-native-proof.v1\",{}",
                &serialized[1..]
            ),
        ] {
            assert!(verify_native_proof(malformed.as_bytes(), &response).is_err());
        }
    }
}
