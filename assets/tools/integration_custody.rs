//! Measured custody for compile-once integration artifacts and shard receipts.
//! Compile once with rustc --edition=2024. Workers use the measured executable;
//! they do not compile source. This is stage evidence, never whole-PR readiness.
//! `record` belongs immediately after the real shard command returns success.
//! The provider's immutable workflow controls that ordering; this tool validates
//! artifact/context identities and rejects incomplete or extra receipt sets.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

type Check<T> = Result<T, String>;
pub const SHARDS: &[&str] = &[
    "gatling-0",
    "gatling-1",
    "gatling-2",
    "gatling-3",
    "release-policies",
    "contract_conformance",
    "constitution_claims",
    "spec_conformance",
    "substrate_integrity",
];
const MANIFEST: &str = "integration-context.v1";
const HELPER: &str = "integration-custody";

fn fail(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn variable(key: &str) -> Check<String> {
    env::var(key).map_err(|_| format!("missing {key}"))
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|b| b != b'0')
}
fn digest(value: &str) -> bool {
    value.len() == 64 && oid(value)
}
fn shard_list(value: &str) -> Check<Vec<String>> {
    let items = if value.is_empty() {
        vec![]
    } else {
        value.split(',').map(str::to_string).collect::<Vec<_>>()
    };
    let expected: Vec<_> = SHARDS
        .iter()
        .filter(|s| items.iter().any(|item| item == **s))
        .map(|s| s.to_string())
        .collect();
    if items != expected {
        return Err("unknown, duplicate, or noncanonical shard selection".into());
    }
    Ok(items)
}
fn json_shards(value: &str) -> Check<Vec<String>> {
    let body = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .ok_or("invalid compact shard JSON")?;
    if body.is_empty() {
        return Ok(vec![]);
    }
    let items: Result<Vec<_>, _> = body
        .split(',')
        .map(|item| {
            item.strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or("invalid compact shard JSON")
        })
        .collect();
    shard_list(&items?.join(","))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub event: String,
    pub base: String,
    pub head: String,
    pub merge_base: String,
    pub validation_head: String,
    pub tree: String,
    pub shards: Vec<String>,
}
impl Context {
    pub fn from_env() -> Check<Self> {
        let value = Self {
            event: variable("CUSTODY_EVENT")?,
            base: variable("CUSTODY_BASE")?,
            head: variable("CUSTODY_HEAD")?,
            merge_base: variable("CUSTODY_MERGE_BASE")?,
            validation_head: variable("CUSTODY_VALIDATION_HEAD")?,
            tree: variable("CUSTODY_TREE")?,
            shards: json_shards(&variable("CUSTODY_SHARDS")?)?,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Check<()> {
        if self.event != "pull_request"
            || self.base == self.head
            || [
                &self.base,
                &self.head,
                &self.merge_base,
                &self.validation_head,
                &self.tree,
            ]
            .iter()
            .any(|id| !oid(id))
            || shard_list(&self.shards.join(","))? != self.shards
        {
            return Err("incomplete immutable integration context".into());
        }
        Ok(())
    }
    pub fn encode(&self) -> Check<String> {
        self.validate()?;
        Ok(format!(
            "decapod.integration-context.v1|{}|{}|{}|{}|{}|{}|{}",
            self.event,
            self.base,
            self.head,
            self.merge_base,
            self.validation_head,
            self.tree,
            self.shards.join(",")
        ))
    }
    pub fn parse(text: &str) -> Check<Self> {
        if text.len() > 1024 {
            return Err("integration context exceeds bound".into());
        }
        let fields: Vec<_> = text.split('|').collect();
        if fields.len() != 8 || fields[0] != "decapod.integration-context.v1" {
            return Err("invalid integration context schema".into());
        }
        let value = Self {
            event: fields[1].into(),
            base: fields[2].into(),
            head: fields[3].into(),
            merge_base: fields[4].into(),
            validation_head: fields[5].into(),
            tree: fields[6].into(),
            shards: shard_list(fields[7])?,
        };
        value.validate()?;
        Ok(value)
    }
}

// Standard SHA-256 compression, streamed over fixed blocks. No shell, external
// hashing executable, dependency download, or native-platform hash extension.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (slot, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
        *slot = u32::from_be_bytes(bytes.try_into().unwrap());
    }
    for i in 16..64 {
        let x = w[i - 15];
        let y = w[i - 2];
        w[i] = w[i - 16]
            .wrapping_add(x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3))
            .wrapping_add(w[i - 7])
            .wrapping_add(y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10));
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let t1 = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}
pub fn sha256(mut reader: impl Read) -> Check<String> {
    let mut state = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut block = [0u8; 64];
    let mut used = 0;
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut block[used..]).map_err(fail)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or("artifact length overflow")?;
        used += n;
        if used == 64 {
            compress(&mut state, &block);
            used = 0;
        }
    }
    block[used] = 0x80;
    block[used + 1..].fill(0);
    if used >= 56 {
        compress(&mut state, &block);
        block.fill(0);
    }
    block[56..].copy_from_slice(
        &total
            .checked_mul(8)
            .ok_or("artifact length overflow")?
            .to_be_bytes(),
    );
    compress(&mut state, &block);
    Ok(state.iter().map(|v| format!("{v:08x}")).collect())
}
pub fn file_sha256(path: &Path) -> Check<String> {
    sha256(io::BufReader::new(fs::File::open(path).map_err(fail)?))
}
fn read_record(path: &Path) -> Check<String> {
    let metadata = fs::symlink_metadata(path).map_err(fail)?;
    if !metadata.file_type().is_file() || metadata.len() > 4096 {
        return Err("custody record must be a bounded regular file".into());
    }
    fs::read_to_string(path).map_err(fail)
}
fn write_new(path: &Path, text: &str) -> Check<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(fail)?;
    file.write_all(text.as_bytes()).map_err(fail)
}
pub fn prepare(artifact: &Path, helper: &Path, context: &Context) -> Check<()> {
    if artifact.join(HELPER).exists() {
        return Err("custody helper already exists".into());
    }
    write_new(&artifact.join(MANIFEST), &(context.encode()? + "\n"))?;
    fs::copy(helper, artifact.join(HELPER)).map_err(fail)?;
    Ok(())
}
pub fn verify(
    artifact: &Path,
    archive: &Path,
    context: &Context,
    expected_sha: &str,
    helper_sha: &str,
) -> Check<()> {
    if !digest(expected_sha) || file_sha256(archive)? != expected_sha {
        return Err("integration archive digest mismatch".into());
    }
    if !digest(helper_sha) || file_sha256(&artifact.join(HELPER))? != helper_sha {
        return Err("integration verifier digest mismatch".into());
    }
    let manifest = read_record(&artifact.join(MANIFEST))?;
    if manifest != context.encode()? + "\n" {
        return Err("integration artifact context mismatch".into());
    }
    Ok(())
}
fn receipt(context: &Context, archive_sha: &str, shard: &str) -> Check<String> {
    if !digest(archive_sha) || !context.shards.iter().any(|s| s == shard) {
        return Err("receipt has unselected shard or invalid archive digest".into());
    }
    Ok(format!(
        "decapod.integration-receipt.v1\ncontext={}\narchive_sha256={archive_sha}\nshard={shard}\nstatus=passed\n",
        context.encode()?
    ))
}
pub fn record(receipts: &Path, context: &Context, archive_sha: &str, shard: &str) -> Check<()> {
    let text = receipt(context, archive_sha, shard)?;
    fs::create_dir_all(receipts).map_err(fail)?;
    write_new(&receipts.join(format!("{shard}.receipt")), &text)
}
pub fn aggregate(
    receipts: &Path,
    context: &Context,
    prepared_context: &str,
    archive_sha: &str,
    count: &str,
    prepare_result: &str,
    worker_result: &str,
) -> Check<()> {
    if prepare_result != "success"
        || Context::parse(prepared_context)? != *context
        || count != context.shards.len().to_string()
    {
        return Err("preparation failed or its exact context/shard count is missing".into());
    }
    if context.shards.is_empty() {
        if worker_result != "skipped" || !archive_sha.is_empty() {
            return Err(
                "empty selection requires explicitly skipped workers and no archive".into(),
            );
        }
    } else if worker_result != "success" || !digest(archive_sha) {
        return Err("selected integration workers did not all succeed".into());
    }
    let mut actual = BTreeSet::new();
    if receipts.exists() {
        for entry in fs::read_dir(receipts).map_err(fail)? {
            let entry = entry.map_err(fail)?;
            actual.insert(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "non-UTF-8 receipt filename")?,
            );
        }
    }
    let expected: BTreeSet<_> = context
        .shards
        .iter()
        .map(|s| format!("{s}.receipt"))
        .collect();
    if actual != expected {
        return Err("missing, duplicate, or unexpected integration receipts".into());
    }
    for shard in &context.shards {
        if read_record(&receipts.join(format!("{shard}.receipt")))?
            != receipt(context, archive_sha, shard)?
        {
            return Err(
                "integration receipt does not match measured context/shard/archive success".into(),
            );
        }
    }
    Ok(())
}
/// Bounded source-produced description of the large compile-once archive.
/// Bytes become authority only through an authenticated successful preparation
/// invocation. Consumers independently supply the immutable context/selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDependencies {
    pub context: Context,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub helper_sha256: String,
    pub bootstrap_sha256: String,
    pub executor_sha256: String,
    pub worker_script_sha256: String,
    pub status_script_sha256: String,
    pub inventory: BTreeMap<String, (u64, String)>,
}
impl Context {
    pub fn from_bindings(values: &BTreeMap<String, String>) -> Check<Self> {
        let field = |key: &str| {
            values
                .get(key)
                .cloned()
                .ok_or_else(|| format!("missing {key}"))
        };
        let value = Self {
            event: field("CUSTODY_EVENT")?,
            base: field("CUSTODY_BASE")?,
            head: field("CUSTODY_HEAD")?,
            merge_base: field("CUSTODY_MERGE_BASE")?,
            validation_head: field("CUSTODY_VALIDATION_HEAD")?,
            tree: field("CUSTODY_TREE")?,
            shards: json_shards(&field("CUSTODY_SHARDS")?)?,
        };
        value.validate()?;
        Ok(value)
    }
}
impl PreparedDependencies {
    pub fn encode(&self) -> Check<String> {
        self.context.validate()?;
        if [
            &self.helper_sha256,
            &self.bootstrap_sha256,
            &self.executor_sha256,
            &self.worker_script_sha256,
            &self.status_script_sha256,
        ]
        .iter()
        .any(|h| !digest(h))
            || if self.context.shards.is_empty() {
                !self.archive_sha256.is_empty()
                    || self.archive_bytes != 0
                    || !self.inventory.is_empty()
            } else {
                !digest(&self.archive_sha256)
                    || self.archive_bytes == 0
                    || self.inventory.is_empty()
            }
            || self.inventory.len() > 128
        {
            return Err("invalid integration preparation dependencies".into());
        }
        let mut out = format!(
            "decapod.integration-preparation.v1\ncontext={}\narchive_sha256={}\narchive_bytes={}\nhelper_sha256={}\nbootstrap_sha256={}\nexecutor_sha256={}\nworker_script_sha256={}\nstatus_script_sha256={}\n",
            self.context.encode()?,
            self.archive_sha256,
            self.archive_bytes,
            self.helper_sha256,
            self.bootstrap_sha256,
            self.executor_sha256,
            self.worker_script_sha256,
            self.status_script_sha256
        );
        for (path, (size, hash)) in &self.inventory {
            if path.is_empty()
                || !path
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                || !digest(hash)
            {
                return Err("invalid integration inventory entry".into());
            }
            out.push_str(&format!("file|{path}|{size}|{hash}\n"));
        }
        Ok(out)
    }
    pub fn parse(bytes: &[u8]) -> Check<Self> {
        if bytes.len() > 64 * 1024 {
            return Err("integration preparation exceeds bound".into());
        }
        let text = std::str::from_utf8(bytes).map_err(fail)?;
        let mut lines = text.lines();
        if lines.next() != Some("decapod.integration-preparation.v1") {
            return Err("invalid integration preparation schema".into());
        }
        let mut field = |name: &str| -> Check<String> {
            lines
                .next()
                .and_then(|s| s.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
                .ok_or_else(|| format!("missing ordered preparation field {name}"))
        };
        let context = Context::parse(&field("context")?)?;
        let archive_sha256 = field("archive_sha256")?;
        let archive_bytes = field("archive_bytes")?.parse().map_err(fail)?;
        let helper_sha256 = field("helper_sha256")?;
        let bootstrap_sha256 = field("bootstrap_sha256")?;
        let executor_sha256 = field("executor_sha256")?;
        let worker_script_sha256 = field("worker_script_sha256")?;
        let status_script_sha256 = field("status_script_sha256")?;
        let mut inventory = BTreeMap::new();
        for line in lines {
            let fields: Vec<_> = line.split('|').collect();
            if fields.len() != 4
                || fields[0] != "file"
                || inventory
                    .insert(
                        fields[1].into(),
                        (fields[2].parse().map_err(fail)?, fields[3].into()),
                    )
                    .is_some()
            {
                return Err("duplicate or malformed preparation inventory".into());
            }
        }
        let value = Self {
            context,
            archive_sha256,
            archive_bytes,
            helper_sha256,
            bootstrap_sha256,
            executor_sha256,
            worker_script_sha256,
            status_script_sha256,
            inventory,
        };
        if value.encode()?.as_bytes() != bytes {
            return Err("noncanonical preparation dependency manifest".into());
        }
        Ok(value)
    }
    pub fn verify_selection(&self, expected: &Context, targets: &[&str]) -> Check<()> {
        self.encode()?;
        if &self.context != expected {
            return Err("preparation immutable context/selection differs".into());
        }
        let mut names = BTreeSet::new();
        if !expected.shards.is_empty() {
            names.extend([
                "build_lifecycle",
                "integration-shards",
                "integration-custody",
                "integration-context.v1",
                "changed-paths.txt",
                "matrix.json",
            ]);
            names.extend(targets.iter().copied());
            if targets.contains(&"gatling") {
                names.insert("gatling-inventory.txt");
            }
        } else if !targets.is_empty() {
            return Err("empty selection has compile targets".into());
        }
        if self
            .inventory
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != names
        {
            return Err(
                "prepared executable/control inventory differs from source selection".into(),
            );
        }
        if !expected.shards.is_empty() {
            let (_, manifest) = &self.inventory["integration-context.v1"];
            if manifest != &sha256((expected.encode()? + "\n").as_bytes())?
                || self.inventory["integration-custody"].1 != self.helper_sha256
            {
                return Err("prepared archive context/helper is not bound".into());
            }
        }
        Ok(())
    }
    pub fn verify_worker_input(
        &self,
        archive_sha: &str,
        archive_bytes: u64,
        helper_sha: &str,
        executor_sha: &str,
    ) -> Check<()> {
        self.encode()?;
        if self.context.shards.is_empty()
            || archive_sha != self.archive_sha256
            || archive_bytes != self.archive_bytes
            || helper_sha != self.helper_sha256
            || executor_sha != self.executor_sha256
        {
            return Err(
                "worker did not consume exact authenticated preparation dependencies".into(),
            );
        }
        Ok(())
    }
}
/// Pure counterpart of the existing exact archive context check.
pub fn verify_manifest_bytes(context: &Context, bytes: &[u8]) -> Check<()> {
    if bytes.len() > 4096 || bytes != (context.encode()? + "\n").as_bytes() {
        return Err("integration artifact context mismatch".into());
    }
    Ok(())
}
/// Pure counterpart of the existing receipt parser; never creates a receipt.
pub fn verify_receipt_bytes(
    context: &Context,
    archive_sha: &str,
    shard: &str,
    bytes: &[u8],
) -> Check<()> {
    if bytes.len() > 4096 || bytes != receipt(context, archive_sha, shard)?.as_bytes() {
        return Err("integration receipt context/shard/archive mismatch".into());
    }
    Ok(())
}
pub fn verify_receipt_set(
    context: &Context,
    archive_sha: &str,
    receipts: &BTreeMap<String, Vec<u8>>,
) -> Check<()> {
    context.validate()?;
    if context.shards.is_empty() && !archive_sha.is_empty() {
        return Err("empty selection has archive identity".into());
    }
    let expected: BTreeSet<_> = context
        .shards
        .iter()
        .map(|s| format!("{s}.receipt"))
        .collect();
    if receipts.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err("missing or extra integration receipts".into());
    }
    for shard in &context.shards {
        verify_receipt_bytes(
            context,
            archive_sha,
            shard,
            &receipts[&format!("{shard}.receipt")],
        )?;
    }
    Ok(())
}
fn describe_preparation(context: &Context) -> Check<PreparedDependencies> {
    let temp = PathBuf::from(variable("RUNNER_TEMP")?);
    let archive = Path::new("target/integration-artifacts/integration-tests.tar");
    let (archive_sha256, archive_bytes) = if context.shards.is_empty() {
        (String::new(), 0)
    } else {
        (
            file_sha256(archive)?,
            fs::metadata(archive).map_err(fail)?.len(),
        )
    };
    let mut inventory = BTreeMap::new();
    if !context.shards.is_empty() {
        for entry in fs::read_dir(temp.join("integration-tests")).map_err(fail)? {
            let entry = entry.map_err(fail)?;
            let metadata = entry.metadata().map_err(fail)?;
            if entry.file_type().map_err(fail)?.is_symlink() {
                return Err("unbound top-level package link".into());
            }
            if metadata.is_file() {
                inventory.insert(
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| "invalid inventory name")?,
                    (metadata.len(), file_sha256(&entry.path())?),
                );
            }
        }
    }
    let bootstrap = temp.join("integration-bootstrap-package");
    let value = PreparedDependencies {
        context: context.clone(),
        archive_sha256,
        archive_bytes,
        helper_sha256: file_sha256(Path::new(
            "target/integration-custody-artifacts/integration-custody",
        ))?,
        bootstrap_sha256: file_sha256(Path::new(
            "target/integration-bootstrap-artifacts/integration-bootstrap.tar",
        ))?,
        executor_sha256: file_sha256(&bootstrap.join("integration-bootstrap/source-ci-profile"))?,
        worker_script_sha256: file_sha256(
            &bootstrap.join("assets/tools/source_ci/integration-worker.sh"),
        )?,
        status_script_sha256: file_sha256(
            &bootstrap.join("assets/tools/source_ci/integration-status.sh"),
        )?,
        inventory,
    };
    value.encode()?;
    Ok(value)
}

fn main_result() -> Check<()> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or("missing custody operation")?;
    let mut opts = BTreeMap::new();
    while let Some(key) = args.next() {
        if !["--artifact", "--archive", "--shard", "--receipts", "--file"].contains(&key.as_str()) {
            return Err("unknown custody option".into());
        }
        let value = args.next().ok_or("missing custody option value")?;
        if opts.insert(key, value).is_some() {
            return Err("duplicate custody option".into());
        }
    }
    let expected: &[&str] = match mode.as_str() {
        "context" | "describe-preparation" => &[],
        "digest" => &["--file"],
        "prepare" => &["--artifact"],
        "verify" => &["--artifact", "--archive"],
        "record" => &["--artifact", "--archive", "--shard", "--receipts"],
        "aggregate" => &["--receipts"],
        _ => return Err("unknown custody operation".into()),
    };
    if opts.len() != expected.len() || expected.iter().any(|key| !opts.contains_key(*key)) {
        return Err("invalid custody operation arguments".into());
    }
    let path = |key: &str| PathBuf::from(&opts[key]);
    if mode == "digest" {
        println!("{}", file_sha256(&path("--file"))?);
        return Ok(());
    }
    let context = Context::from_env()?;
    match mode.as_str() {
        "describe-preparation" => print!("{}", describe_preparation(&context)?.encode()?),
        "context" => {
            println!(
                "context={}\nhelper_sha256={}",
                context.encode()?,
                file_sha256(&env::current_exe().map_err(fail)?)?
            );
        }
        "prepare" => prepare(
            &path("--artifact"),
            &env::current_exe().map_err(fail)?,
            &context,
        )?,
        "verify" | "record" => {
            let sha = variable("CUSTODY_ARCHIVE_SHA256")?;
            verify(
                &path("--artifact"),
                &path("--archive"),
                &context,
                &sha,
                &variable("CUSTODY_HELPER_SHA256")?,
            )?;
            if mode == "record" {
                record(&path("--receipts"), &context, &sha, &opts["--shard"])?;
            }
        }
        "aggregate" => {
            if file_sha256(&env::current_exe().map_err(fail)?)?
                != variable("CUSTODY_HELPER_SHA256")?
            {
                return Err("aggregate verifier digest mismatch".into());
            }
            aggregate(
                &path("--receipts"),
                &context,
                &variable("CUSTODY_PREPARED_CONTEXT")?,
                &variable("CUSTODY_ARCHIVE_SHA256")?,
                &variable("CUSTODY_COUNT")?,
                &variable("CUSTODY_PREPARE_RESULT")?,
                &variable("CUSTODY_WORKER_RESULT")?,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn main() {
    if let Err(error) = main_result() {
        eprintln!("integration custody: {error}");
        std::process::exit(1);
    }
}
