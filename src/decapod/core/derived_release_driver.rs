//! Nonpublishing derivative execution in a second owned standalone clone.
//!
//! This is an execution adapter, not an authorization boundary or OS sandbox.
//! Caller supplies reviewed tool paths and independently authenticated production
//! tool observations. Repository build scripts execute and require that review.
use super::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use super::derived_release_profile as release_profile;
use super::derived_release_profile::{
    DistInventory, FileMaterial, GeneratedRelease, GenerationBinding, ReleaseProfile, Snapshot,
    Tool, ToolResolution,
};
use super::release_archive::{self, Member};
use super::release_toolchain::ReleaseToolchain;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn checked<T>(value: Result<T, String>) -> io::Result<T> {
    value.map_err(invalid)
}
fn text(bytes: Vec<u8>) -> io::Result<String> {
    String::from_utf8(bytes).map_err(|_| invalid("non-UTF-8 command output"))
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64)
        && s.bytes().all(|b| b.is_ascii_hexdigit())
        && s.bytes().any(|b| b != b'0')
}
fn relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return Err(invalid("expected bounded non-symlink file"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut input = options.open(path)?;
    if !input.metadata()?.is_file() {
        return Err(invalid("opened material is not a regular file"));
    }
    let mut bytes = Vec::new();
    input.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("file exceeds byte budget"));
    }
    Ok(bytes)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverTools {
    pub git: PathBuf,
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub cargo_dist: PathBuf,
    pub release_plz: PathBuf,
    pub cargo_semver_checks: PathBuf,
    pub curl: PathBuf,
    pub xz: PathBuf,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverInputs {
    pub source_root: PathBuf,
    pub source_head: String,
    pub accepted_base: String,
    pub release_date: String,
    pub tools: DriverTools,
    /// Authenticated observations of all actual applicable production runtimes.
    pub production_tools: BTreeMap<Tool, ToolResolution>,
    pub semver_checks_production_version: String,
    pub semver_checks_installer_definition_sha256: String,
    /// Only compiler/linker/SDK variables accepted below; no ambient environment.
    pub build_environment: BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug)]
pub struct DriverLimits {
    pub timeout: Duration,
    pub command_timeout: Duration,
    pub max_output_bytes: usize,
    pub max_material_bytes: u64,
    pub max_members: usize,
}
impl Default for DriverLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3600),
            command_timeout: Duration::from_secs(1800),
            max_output_bytes: 16 * 1024 * 1024,
            max_material_bytes: 512 * 1024 * 1024,
            max_members: 100_000,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRecord {
    pub program_sha256: String,
    pub arguments: Vec<String>,
    pub cwd: String,
    pub environment_sha256: String,
    pub stdout_sha256: String,
    pub output_policy: String,
    pub stderr_sha256: String,
    pub exit_code: i32,
    pub duration_ms: u128,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stage {
    Plan,
    Package,
    Candidate,
    Local { target: String },
    Global,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSetup {
    pub tools: BTreeMap<Tool, ToolResolution>,
    pub semver_checks_version: String,
    pub semver_checks_sha256: String,
    pub installer_definition_sha256: String,
    pub build_environment: BTreeMap<String, String>,
    pub curl_sha256: String,
    pub xz_sha256: String,
}
fn worker_setup(
    inputs: &DriverInputs,
    tools: BTreeMap<Tool, ToolResolution>,
    semver_checks_version: String,
    semver_checks_sha256: String,
    limit: u64,
) -> io::Result<WorkerSetup> {
    Ok(WorkerSetup {
        tools,
        semver_checks_version,
        semver_checks_sha256,
        installer_definition_sha256: inputs.semver_checks_installer_definition_sha256.clone(),
        build_environment: inputs.build_environment.clone(),
        curl_sha256: release_profile::sha256(&read_bounded(&inputs.tools.curl, limit)?),
        xz_sha256: release_profile::sha256(&read_bounded(&inputs.tools.xz, limit)?),
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageReport {
    pub stage: Stage,
    pub source_commit: String,
    pub derived_commit: String,
    pub binding_sha256: String,
    pub command_records: Vec<CommandRecord>,
    pub artifacts: BTreeMap<String, String>,
    pub tool_setup_sha256: String,
    pub worker_setup: WorkerSetup,
}

struct Context<'a> {
    inputs: &'a DriverInputs,
    limits: DriverLimits,
    cancel: &'a Cancellation,
    deadline: Instant,
    environment: BTreeMap<String, String>,
    records: Vec<CommandRecord>,
    observed_tools: BTreeMap<PathBuf, String>,
    logs: PathBuf,
    log_bytes: u64,
}
impl Context<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "derivative proof cancelled",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "derivative proof deadline exceeded",
                )
            })
    }
    fn run(&mut self, program: &Path, args: &[String], cwd: &Path) -> io::Result<Vec<u8>> {
        let timeout = self.remaining()?.min(self.limits.command_timeout);
        if !program.is_absolute() || program.canonicalize()? != program {
            return Err(invalid("tool path must be absolute and canonical"));
        }
        let hash = release_profile::sha256(&read_bounded(program, self.limits.max_material_bytes)?);
        if self
            .observed_tools
            .get(program)
            .is_some_and(|prior| prior != &hash)
        {
            return Err(invalid("tool executable changed during derivative proof"));
        }
        self.observed_tools.insert(program.to_owned(), hash.clone());
        let mut result = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(&self.environment)
            .bounded_capture(
                ProcessLimits {
                    timeout,
                    max_output_bytes: self.limits.max_output_bytes,
                },
                self.cancel,
            )?;
        // Session status prints a live token. Retain only its public status line,
        // explicitly tagged as filtered output; never persist/export the token.
        let output_policy = if args == ["session", "status"] {
            let first = result
                .output
                .stdout
                .split(|b| *b == b'\n')
                .next()
                .unwrap_or_default()
                .to_vec();
            result.output.stdout.fill(0);
            result.output.stdout = first;
            result.output.stdout.push(b'\n');
            "session-status-first-line"
        } else {
            "raw"
        };
        self.log_bytes += (result.output.stdout.len() + result.output.stderr.len()) as u64;
        if self.log_bytes > self.limits.max_material_bytes {
            return Err(invalid("aggregate command-log budget exceeded"));
        }
        for (suffix, bytes) in [
            ("stdout", &result.output.stdout),
            ("stderr", &result.output.stderr),
        ] {
            fs::write(
                self.logs
                    .join(format!("{}.{}", release_profile::sha256(bytes), suffix)),
                bytes,
            )?;
        }
        self.records.push(CommandRecord {
            program_sha256: hash,
            arguments: args.to_vec(),
            cwd: cwd.to_string_lossy().into_owned(),
            environment_sha256: release_profile::sha256(&serde_json::to_vec(&self.environment)?),
            stdout_sha256: release_profile::sha256(&result.output.stdout),
            output_policy: output_policy.into(),
            stderr_sha256: release_profile::sha256(&result.output.stderr),
            exit_code: result.output.status.code().unwrap_or(-1),
            duration_ms: result.duration.as_millis(),
        });
        if result.termination != Termination::Exited || !result.output.status.success() {
            return Err(invalid(format!(
                "{} {:?} failed ({:?}): {}",
                program.file_name().unwrap_or_default().to_string_lossy(),
                args,
                result.termination,
                String::from_utf8_lossy(&result.output.stderr)
            )));
        }
        Ok(result.output.stdout)
    }
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<String> {
        let git = self.inputs.tools.git.clone();
        text(self.run(
            &git,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            root,
        )?)
    }
    fn git_raw(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        let git = self.inputs.tools.git.clone();
        self.run(
            &git,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            root,
        )
    }
    fn curl(&mut self, url: &str, destination: Option<&Path>, cwd: &Path) -> io::Result<Vec<u8>> {
        if !url.starts_with("https://crates.io/api/v1/crates/decapod")
            && !url.starts_with("https://static.crates.io/crates/decapod/")
        {
            return Err(invalid("unsupported registry endpoint"));
        }
        let mut args = [
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--connect-timeout",
            "15",
            "--max-time",
            "120",
            "--user-agent",
            "decapod-derived-release-proof",
        ]
        .map(str::to_owned)
        .to_vec();
        if let Some(path) = destination {
            args.extend(["--output".into(), path.to_string_lossy().into_owned()]);
        }
        args.extend([
            "--max-filesize".into(),
            self.limits.max_material_bytes.to_string(),
            "--url".into(),
            url.into(),
        ]);
        let curl = self.inputs.tools.curl.clone();
        self.run(&curl, &args, cwd)
    }
}

/// Owns the derivative clone and prevents it escaping the caller's owned scratch.
/// Dropping it removes only this newly allocated directory.
pub struct PreparedRelease<'a> {
    storage: tempfile::TempDir,
    root: PathBuf,
    context: Context<'a>,
    pub profile: ReleaseProfile,
    pub binding: GenerationBinding,
    pub generated: GeneratedRelease,
    pub semver_checks_version: String,
    pub semver_checks_sha256: String,
    pub installer_definition_sha256: String,
    pub preparation_records: Vec<CommandRecord>,
    pub tool_setup_sha256: String,
    generation_tools: BTreeMap<Tool, ToolResolution>,
    worker_setup: WorkerSetup,
}
impl PreparedRelease<'_> {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn owned_directory(&self) -> &Path {
        self.storage.path()
    }
}

fn profile_at(root: &Path, limit: u64) -> io::Result<ReleaseProfile> {
    let read = |name: &str| -> io::Result<String> { text(read_bounded(&root.join(name), limit)?) };
    checked(release_profile::profile(
        &read("Cargo.toml")?,
        &read(".github/release.toml")?,
        &read("dist-workspace.toml")?,
        &read("dist.toml")?,
    ))
}
fn snapshot(ctx: &mut Context<'_>, root: &Path) -> io::Result<Snapshot> {
    let listing = ctx.git_raw(root, &["ls-files", "--stage", "-z"])?;
    let mut result = Snapshot::new();
    let mut total = 0u64;
    for entry in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let entry = std::str::from_utf8(entry).map_err(|_| invalid("non-UTF-8 tracked path"))?;
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or_else(|| invalid("invalid index entry"))?;
        if !relative(path) {
            return Err(invalid("unsafe tracked path"));
        }
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() != 3 || fields[2] != "0" {
            return Err(invalid("unmerged index entry"));
        }
        let mut mode =
            u32::from_str_radix(fields[0], 8).map_err(|_| invalid("invalid index mode"))?;
        let physical = root.join(path);
        #[cfg(unix)]
        if matches!(mode, 0o100644 | 0o100755) {
            use std::os::unix::fs::PermissionsExt;
            mode = if fs::symlink_metadata(&physical)?.permissions().mode() & 0o111 == 0 {
                0o100644
            } else {
                0o100755
            };
        }
        let bytes = match mode {
            0o100644 | 0o100755 => read_bounded(
                &physical,
                ctx.limits.max_material_bytes.saturating_sub(total),
            )?,
            0o120000 => fs::read_link(&physical)?
                .to_str()
                .ok_or_else(|| invalid("non-UTF-8 symlink"))?
                .as_bytes()
                .to_vec(),
            _ => return Err(invalid("submodule/special tracked entry unsupported")),
        };
        total += bytes.len() as u64;
        if total > ctx.limits.max_material_bytes || result.len() >= ctx.limits.max_members {
            return Err(invalid("snapshot material budget exceeded"));
        }
        if result
            .insert(path.into(), FileMaterial { mode, bytes })
            .is_some()
        {
            return Err(invalid("duplicate index path"));
        }
    }
    let untracked = ctx.git_raw(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if !untracked.is_empty() {
        return Err(invalid("unexpected untracked generated files"));
    }
    Ok(result)
}
fn version(raw: &[u8], tool: &str) -> io::Result<String> {
    let raw = std::str::from_utf8(raw).map_err(|_| invalid("non-UTF-8 tool version"))?;
    let fields: Vec<_> = raw
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if fields.first().copied() != Some(tool) || fields.len() < 2 {
        return Err(invalid(format!("unexpected {tool} version output")));
    }
    Ok(fields[1].into())
}
fn tool_observations(
    ctx: &mut Context<'_>,
    root: &Path,
    pins: &ReleaseToolchain,
) -> io::Result<(BTreeMap<Tool, ToolResolution>, String, String)> {
    let mut observed = ctx.inputs.production_tools.clone();
    for (key, path, label, expected) in [
        (Tool::Git, ctx.inputs.tools.git.clone(), "git", None),
        (
            Tool::Cargo,
            ctx.inputs.tools.cargo.clone(),
            "cargo",
            Some(pins.rust.as_str()),
        ),
        (
            Tool::Rustc,
            ctx.inputs.tools.rustc.clone(),
            "rustc",
            Some(pins.rust.as_str()),
        ),
        (
            Tool::ReleasePlz,
            ctx.inputs.tools.release_plz.clone(),
            "release-plz",
            Some(pins.release_plz.as_str()),
        ),
        (
            Tool::CargoDist,
            ctx.inputs.tools.cargo_dist.clone(),
            "cargo-dist",
            Some(pins.cargo_dist.as_str()),
        ),
    ] {
        let version_args = if key == Tool::CargoDist {
            vec!["dist".into(), "--version".into()]
        } else {
            vec!["--version".into()]
        };
        let raw = ctx.run(&path, &version_args, root)?;
        let measured = if key == Tool::Git {
            let raw = text(raw)?;
            raw.strip_prefix("git version ")
                .and_then(|s| s.split_whitespace().next())
                .ok_or_else(|| invalid("invalid git version"))?
                .to_owned()
        } else {
            version(&raw, label)?
        };
        if expected.is_some_and(|v| v != measured) {
            return Err(invalid(format!("{label} differs from shared release pin")));
        }
        let evidence = observed
            .get_mut(&key)
            .ok_or_else(|| invalid(format!("missing production tool observation: {key:?}")))?;
        if evidence.actual_version != measured {
            return Err(invalid(format!(
                "{label} differs from authenticated production resolution"
            )));
        }
        evidence.executable_sha256 =
            release_profile::sha256(&read_bounded(&path, ctx.limits.max_material_bytes)?);
    }
    let semver = ctx.inputs.tools.cargo_semver_checks.clone();
    let measured = version(
        &ctx.run(&semver, &["--version".into()], root)?,
        "cargo-semver-checks",
    )?;
    if !pins.accepts_semver_checks(&measured)
        || measured != ctx.inputs.semver_checks_production_version
    {
        return Err(invalid(
            "semver-checks runtime parity unresolved or mismatched",
        ));
    }
    let definition = &ctx.inputs.semver_checks_installer_definition_sha256;
    if definition.len() != 64 || !definition.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("missing reviewed installer definition digest"));
    }
    let digest = release_profile::sha256(&read_bounded(&semver, ctx.limits.max_material_bytes)?);
    Ok((observed, measured, digest))
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct RegistryBaseline {
    version: String,
    checksum: String,
}
fn registry(ctx: &mut Context<'_>, root: &Path) -> io::Result<RegistryBaseline> {
    let raw = ctx.curl("https://crates.io/api/v1/crates/decapod", None, root)?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| invalid(e.to_string()))?;
    let current = value
        .pointer("/crate/max_stable_version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("unresolved published registry version"))?;
    if !current.split('.').count().eq(&3)
        || !current
            .split('.')
            .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid("unsupported registry release version"));
    }
    let versions = value
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("registry versions missing"))?;
    let selected: Vec<_> = versions
        .iter()
        .filter(|v| v.get("num").and_then(|n| n.as_str()) == Some(current))
        .collect();
    if selected.len() != 1 || selected[0].get("yanked").and_then(|v| v.as_bool()) != Some(false) {
        return Err(invalid("registry release missing, duplicated or yanked"));
    }
    let checksum = selected[0]
        .get("checksum")
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("registry checksum missing"))?;
    if checksum.len() != 64 || !checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("registry checksum malformed"));
    }
    Ok(RegistryBaseline {
        version: current.into(),
        checksum: checksum.into(),
    })
}
fn observed_registry_crate(
    home: &Path,
    baseline: &RegistryBaseline,
    limit: u64,
) -> io::Result<String> {
    // release-plz's default Cloner uses Cargo's package cache. A fresh CARGO_HOME
    // makes the selected crate observable without changing its production path.
    let cache = home.join("cargo/registry/cache");
    let mut found = Vec::new();
    for directory in fs::read_dir(&cache)? {
        let directory = directory?;
        if !directory.file_type()?.is_dir() {
            return Err(invalid("unexpected registry cache directory entry"));
        }
        for entry in fs::read_dir(directory.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| invalid("invalid registry cache filename"))?;
            if name.starts_with("decapod-") && name.ends_with(".crate") {
                if name != format!("decapod-{}.crate", baseline.version) {
                    return Err(invalid(
                        "generator consumed a different published registry version",
                    ));
                }
                let bytes = read_bounded(&entry.path(), limit)?;
                if release_profile::sha256(&bytes) != baseline.checksum {
                    return Err(invalid("generator registry crate checksum mismatch"));
                }
                found.push(release_profile::sha256(&bytes));
            }
        }
    }
    if found.len() != 1 {
        return Err(invalid(
            "generator registry input was not uniquely observed in its fresh Cargo cache",
        ));
    }
    Ok(found.remove(0))
}

fn extract_regular(members: BTreeMap<String, Member>, parent: &Path) -> io::Result<()> {
    for (path, member) in members {
        if !relative(&path) {
            return Err(invalid("unsafe extraction path"));
        }
        let output = parent.join(path);
        let dir = output
            .parent()
            .ok_or_else(|| invalid("no extraction parent"))?;
        fs::create_dir_all(dir)?;
        for ancestor in dir.ancestors().take_while(|p| p.starts_with(parent)) {
            if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
                return Err(invalid("symlink extraction ancestor"));
            }
        }
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        f.write_all(&member.bytes)?;
    }
    Ok(())
}

fn prepare_inner<'a>(
    inputs: &'a DriverInputs,
    owned_parent: &Path,
    limits: DriverLimits,
    cancel: &'a Cancellation,
    imported: Option<(&Path, &ReleaseTransport)>,
) -> io::Result<PreparedRelease<'a>> {
    if !oid(&inputs.source_head)
        || !oid(&inputs.accepted_base)
        || !inputs.source_root.is_absolute()
        || inputs.source_root.canonicalize()? != inputs.source_root
    {
        return Err(invalid("immutable canonical source/base required"));
    }
    if limits.timeout.is_zero()
        || limits.command_timeout.is_zero()
        || limits.max_output_bytes == 0
        || limits.max_output_bytes > 64 * 1024 * 1024
        || limits.max_material_bytes == 0
    {
        return Err(invalid("invalid derivative limits"));
    }
    let owned_parent = owned_parent.canonicalize()?;
    if owned_parent.starts_with(&inputs.source_root)
        || inputs.source_root.starts_with(&owned_parent)
    {
        return Err(invalid(
            "derivative scratch must be disjoint from source snapshot",
        ));
    }
    let storage = tempfile::Builder::new()
        .prefix("derived-release-")
        .tempdir_in(&owned_parent)?;
    let home = storage.path().join("home");
    let bin = storage.path().join("bin");
    let hooks = storage.path().join("no-hooks");
    let temporary = storage.path().join("tmp");
    let logs = storage.path().join("logs");
    for path in [&home, &bin, &hooks, &temporary, &logs] {
        fs::create_dir(path)?;
    }
    let mut environment = BTreeMap::from([
        ("HOME".into(), home.to_string_lossy().into_owned()),
        (
            "CARGO_HOME".into(),
            home.join("cargo").to_string_lossy().into_owned(),
        ),
        ("PATH".into(), format!("{}:/usr/bin:/bin", bin.display())),
        (
            "RUSTC".into(),
            inputs.tools.rustc.to_string_lossy().into_owned(),
        ),
        ("GIT_NO_REPLACE_OBJECTS".into(), "1".into()),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("GIT_CONFIG_COUNT".into(), "3".into()),
        ("GIT_CONFIG_KEY_0".into(), "core.hooksPath".into()),
        (
            "GIT_CONFIG_VALUE_0".into(),
            hooks.to_string_lossy().into_owned(),
        ),
        ("GIT_CONFIG_KEY_1".into(), "commit.gpgsign".into()),
        ("GIT_CONFIG_VALUE_1".into(), "false".into()),
        ("GIT_CONFIG_KEY_2".into(), "protocol.allow".into()),
        ("GIT_CONFIG_VALUE_2".into(), "never".into()),
        ("LANG".into(), "C.UTF-8".into()),
        ("LC_ALL".into(), "C".into()),
        ("TZ".into(), "UTC".into()),
    ]);
    for key in ["TMPDIR", "TMP", "TEMP"] {
        environment.insert(key.into(), temporary.to_string_lossy().into_owned());
    }
    for (key, value) in &inputs.build_environment {
        if !matches!(
            key.as_str(),
            "SDKROOT"
                | "MACOSX_DEPLOYMENT_TARGET"
                | "DEVELOPER_DIR"
                | "CC"
                | "CXX"
                | "AR"
                | "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"
        ) {
            return Err(invalid("unreviewed build environment variable"));
        }
        if key == "MACOSX_DEPLOYMENT_TARGET" {
            if !value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                return Err(invalid("invalid deployment target"));
            }
        } else if !Path::new(value).is_absolute()
            || !Path::new(value).exists()
            || value.chars().any(char::is_whitespace)
        {
            return Err(invalid(
                "compiler/SDK environment requires reviewed absolute paths",
            ));
        }
        environment.insert(key.clone(), value.clone());
    }
    #[cfg(unix)]
    for (name, path) in [
        ("git", &inputs.tools.git),
        ("cargo", &inputs.tools.cargo),
        ("rustc", &inputs.tools.rustc),
        ("cargo-dist", &inputs.tools.cargo_dist),
        ("release-plz", &inputs.tools.release_plz),
        ("cargo-semver-checks", &inputs.tools.cargo_semver_checks),
    ] {
        if path.canonicalize()? != *path {
            return Err(invalid("tool closure path not canonical"));
        }
        std::os::unix::fs::symlink(path, bin.join(name))?;
    }
    let mut context = Context {
        inputs,
        limits,
        cancel,
        deadline: Instant::now()
            .checked_add(limits.timeout)
            .ok_or_else(|| invalid("deadline overflow"))?,
        environment,
        records: Vec::new(),
        observed_tools: BTreeMap::new(),
        logs,
        log_bytes: 0,
    };
    let source = &inputs.source_root;
    if context.git(source, &["rev-parse", "HEAD"])?.trim() != inputs.source_head
        || !context
            .git(
                source,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?
            .is_empty()
    {
        return Err(invalid("source must be clean at exact immutable head"));
    }
    if context
        .git(source, &["rev-parse", "--is-shallow-repository"])?
        .trim()
        != "false"
    {
        return Err(invalid(
            "release generation requires complete source history",
        ));
    }
    context.git(
        source,
        &[
            "merge-base",
            "--is-ancestor",
            &inputs.accepted_base,
            &inputs.source_head,
        ],
    )?;
    let profile = profile_at(source, limits.max_material_bytes)?;
    checked(profile.generation_command(&inputs.release_date))?;
    let pins = checked(ReleaseToolchain::parse(&text(read_bounded(
        &source.join(".github/release-tools.toml"),
        limits.max_material_bytes,
    )?)?))?;
    if pins.rust != profile.rust_version || pins.cargo_dist != profile.dist_version {
        return Err(invalid("shared pins differ from source profile"));
    }
    let (tools, semver_checks_version, semver_checks_sha256) =
        tool_observations(&mut context, source, &pins)?;
    checked(release_profile::verify_tools(&profile, &tools))?;
    let before_source = snapshot(&mut context, source)?;
    let root = storage.path().join("candidate");
    let git = inputs.tools.git.clone();
    context.run(
        &git,
        &[
            "-c".into(),
            "protocol.file.allow=always".into(),
            "clone".into(),
            "--no-local".into(),
            "--no-hardlinks".into(),
            "--no-checkout".into(),
            "--".into(),
            source.to_string_lossy().into_owned(),
            root.to_string_lossy().into_owned(),
        ],
        storage.path(),
    )?;
    if !root.join(".git").is_dir() || root.join(".git/objects/info/alternates").exists() {
        return Err(invalid("derivative clone shares Git state"));
    }
    context.git(&root, &["remote", "remove", "origin"])?;
    context.git(&root, &["checkout", "--detach", &inputs.source_head])?;
    let before = snapshot(&mut context, &root)?;
    if before != before_source {
        return Err(invalid("derivative source material mismatch"));
    }
    if let Some((bundle, receipt)) = imported {
        return import_prepared(
            inputs,
            storage,
            root,
            context,
            profile,
            before,
            tools,
            semver_checks_version,
            semver_checks_sha256,
            bundle,
            receipt,
        );
    }
    let baseline = registry(&mut context, &root)?;
    let baseline_tag = format!("v{}", baseline.version);
    let baseline_commit = context
        .git(
            &root,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/tags/{baseline_tag}^{{commit}}"),
            ],
        )?
        .trim()
        .to_owned();
    context.git(
        &root,
        &[
            "merge-base",
            "--is-ancestor",
            &baseline_commit,
            &inputs.source_head,
        ],
    )?;
    let history = context.git(&root, &["log", "--format=raw", &inputs.source_head])?;
    let tags = context.git(
        &root,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/tags",
        ],
    )?;
    let crate_path = storage.path().join("baseline.crate");
    context.curl(
        &format!(
            "https://static.crates.io/crates/decapod/decapod-{}.crate",
            baseline.version
        ),
        Some(&crate_path),
        &root,
    )?;
    let baseline_bytes = read_bounded(&crate_path, limits.max_material_bytes)?;
    if release_profile::sha256(&baseline_bytes) != baseline.checksum {
        return Err(invalid("published baseline checksum mismatch"));
    }
    let baseline_members = release_archive::inspect(
        flate2::read::GzDecoder::new(baseline_bytes.as_slice()),
        limits.max_material_bytes,
        limits.max_members,
    )?;
    let prefix = format!("decapod-{}/", baseline.version);
    if baseline_members.is_empty() || baseline_members.keys().any(|p| !p.starts_with(&prefix)) {
        return Err(invalid("published crate has unexpected root"));
    }
    let registry_dir = storage.path().join("registry");
    fs::create_dir(&registry_dir)?;
    extract_regular(baseline_members, &registry_dir)?;
    let vcs: serde_json::Value = serde_json::from_slice(&read_bounded(
        &registry_dir.join(&prefix).join(".cargo_vcs_info.json"),
        4096,
    )?)
    .map_err(|e| invalid(e.to_string()))?;
    if vcs.pointer("/git/sha1").and_then(|v| v.as_str()) != Some(&baseline_commit)
        || vcs.pointer("/git/dirty").and_then(|v| v.as_bool()) == Some(true)
    {
        return Err(invalid(
            "registry baseline Git provenance differs from source release tag",
        ));
    }
    let tagged_manifest =
        context.git(&root, &["show", &format!("{baseline_commit}:Cargo.toml")])?;
    let tagged: toml::Value =
        toml::from_str(&tagged_manifest).map_err(|e| invalid(e.to_string()))?;
    if tagged
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        != Some(&baseline.version)
    {
        return Err(invalid(
            "source release tag version differs from registry baseline",
        ));
    }
    let baseline_manifest = registry_dir.join(&prefix).join("Cargo.toml");
    let baseline_cargo: toml::Value = toml::from_str(&text(read_bounded(
        &baseline_manifest,
        limits.max_material_bytes,
    )?)?)
    .map_err(|e| invalid(e.to_string()))?;
    if baseline_cargo
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        != Some(&baseline.version)
    {
        return Err(invalid("published crate manifest version mismatch"));
    }
    let source_git_tree = context
        .git(&root, &["rev-parse", "HEAD^{tree}"])?
        .trim()
        .to_owned();
    let cargo: toml::Value = toml::from_str(&text(read_bounded(
        &root.join("Cargo.toml"),
        limits.max_material_bytes,
    )?)?)
    .map_err(|e| invalid(e.to_string()))?;
    let repository = cargo
        .get("package")
        .and_then(|p| p.get("repository"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("repository URL missing"))?;
    let mut update = checked(profile.generation_command(&inputs.release_date))?.args;
    // Keep the default registry code path used by production. The supported
    // --registry-manifest-path path drops published SHA metadata in 0.3.171
    // and is therefore not a valid parity shortcut.
    update.extend(["--repo-url".into(), repository.into()]);
    let generator = inputs.tools.release_plz.clone();
    context.run(&generator, &update, &root)?;
    if registry(&mut context, &root)? != baseline {
        return Err(invalid(
            "published registry baseline moved during generation",
        ));
    }
    let consumed_registry_crate =
        observed_registry_crate(&home, &baseline, limits.max_material_bytes)?;
    let after = snapshot(&mut context, &root)?;
    checked(release_profile::verify_delta(
        &profile,
        &before,
        &after,
        &inputs.release_date,
    ))?;
    // Git index and commit mutation stay entirely inside the second owned clone.
    context.git(
        &root,
        &["add", "--", "Cargo.toml", "Cargo.lock", "CHANGELOG.md"],
    )?;
    let derived_git_tree = context.git(&root, &["write-tree"])?.trim().to_owned();
    let epoch = context
        .git(&root, &["show", "-s", "--format=%ct", &inputs.source_head])?
        .trim()
        .to_owned();
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
        context
            .environment
            .insert(key.into(), format!("@{epoch} +0000"));
    }
    for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
        context
            .environment
            .insert(key.into(), "Decapod Derivative Proof".into());
    }
    for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
        context
            .environment
            .insert(key.into(), "derived-proof@invalid".into());
    }
    let derived_commit = context
        .git(
            &root,
            &[
                "commit-tree",
                &derived_git_tree,
                "-p",
                &inputs.source_head,
                "-m",
                "chore: prepare local derived release proof",
            ],
        )?
        .trim()
        .to_owned();
    let binding = GenerationBinding {
        source_commit: inputs.source_head.clone(),
        source_git_tree,
        source_material_sha256: checked(release_profile::snapshot_digest(&before))?,
        derived_commit: derived_commit.clone(),
        derived_git_tree,
        derived_material_sha256: checked(release_profile::snapshot_digest(&after))?,
        accepted_base_commit: inputs.accepted_base.clone(),
        history_sha256: release_profile::sha256(format!("{history}\n{tags}").as_bytes()),
        baseline_tag,
        baseline_commit,
        baseline_version: baseline.version.clone(),
        registry_inputs_sha256: release_profile::sha256(&serde_json::to_vec(&(
            baseline.clone(),
            consumed_registry_crate,
        ))?),
        registry_identity: "https://index.crates.io/".into(),
        registry_observed_at: chrono::Utc::now().to_rfc3339(),
        registry_version: baseline.version,
        registry_crate_sha256: baseline.checksum,
        configuration_sha256: profile.configuration_digest.clone(),
        tools_sha256: release_profile::sha256(&serde_json::to_vec(&tools)?),
        release_date: inputs.release_date.clone(),
    };
    let generated = checked(release_profile::verify_generation(
        &profile, &before, &after, &binding, &tools,
    ))?;
    context.git(&root, &["checkout", "--detach", &derived_commit])?;
    if !context
        .git(
            &root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?
        .is_empty()
    {
        return Err(invalid("derived candidate not clean"));
    }
    if snapshot(&mut context, source)? != before_source
        || context.git(source, &["rev-parse", "HEAD"])?.trim() != inputs.source_head
    {
        return Err(invalid("source changed during derivative preparation"));
    }
    let worker_setup = worker_setup(
        inputs,
        tools.clone(),
        semver_checks_version.clone(),
        semver_checks_sha256.clone(),
        limits.max_material_bytes,
    )?;
    let tool_setup_sha256 = release_profile::sha256(&serde_json::to_vec(&worker_setup)?);
    let preparation_records = context.records.clone();
    Ok(PreparedRelease {
        storage,
        root,
        context,
        profile,
        binding,
        generated,
        semver_checks_version,
        semver_checks_sha256,
        installer_definition_sha256: inputs.semver_checks_installer_definition_sha256.clone(),
        preparation_records,
        tool_setup_sha256,
        generation_tools: tools,
        worker_setup,
    })
}

/// This is the single artifact-build argv authority used by both the production
/// build helper and the nonpublishing derivative driver. No shell arguments.
pub fn artifact_build_arguments(
    profile: &ReleaseProfile,
    stage: &Stage,
    tag: Option<&str>,
) -> io::Result<Vec<String>> {
    match stage {
        Stage::Local { target } => checked(profile.artifact_build_arguments(Some(target), tag)),
        Stage::Global => checked(profile.artifact_build_arguments(None, tag)),
        _ => Err(invalid("artifact helper only supports local/global builds")),
    }
}

fn verify_xz_size(listing: &str, limit: u64) -> io::Result<()> {
    let totals: Vec<_> = listing
        .lines()
        .filter(|line| line.starts_with("totals\t"))
        .collect();
    if totals.len() != 1 {
        return Err(invalid("missing or duplicate xz total"));
    }
    let fields: Vec<_> = totals[0].split('\t').collect();
    let size = fields
        .get(4)
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| invalid("invalid xz uncompressed size"))?;
    if size == 0 || size > limit || fields.get(8) != Some(&"1") {
        return Err(invalid("xz archive exceeds output budget or file count"));
    }
    Ok(())
}

fn binary_architecture(bytes: &[u8], target: &str) -> io::Result<()> {
    if target.ends_with("-unknown-linux-gnu") {
        if bytes.len() < 64 || &bytes[..4] != b"\x7fELF" || bytes[4] != 2 || bytes[5] != 1 {
            return Err(invalid("expected 64-bit little-endian ELF executable"));
        }
        let expected = if target.starts_with("x86_64-") {
            62
        } else if target.starts_with("aarch64-") {
            183
        } else {
            return Err(invalid("unsupported Linux architecture"));
        };
        if u16::from_le_bytes([bytes[18], bytes[19]]) != expected {
            return Err(invalid("ELF architecture mismatch"));
        }
    } else if target.ends_with("-apple-darwin") {
        if bytes.len() < 32 || bytes[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
            return Err(invalid("expected 64-bit little-endian Mach-O executable"));
        }
        let expected = if target.starts_with("x86_64-") {
            0x0100_0007
        } else if target.starts_with("aarch64-") {
            0x0100_000c
        } else {
            return Err(invalid("unsupported Darwin architecture"));
        };
        if u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != expected {
            return Err(invalid("Mach-O architecture mismatch"));
        }
    } else {
        return Err(invalid("unsupported release executable format"));
    }
    Ok(())
}
impl PreparedRelease<'_> {
    fn unchanged(&mut self) -> io::Result<()> {
        if self.context.git(&self.root, &["rev-parse", "HEAD"])?.trim()
            != self.binding.derived_commit
            || checked(release_profile::snapshot_digest(&snapshot(
                &mut self.context,
                &self.root,
            )?))?
                != self.binding.derived_material_sha256
        {
            return Err(invalid("derived commit or material changed during proof"));
        }
        checked(release_profile::verify_lock_unchanged(
            &self.generated,
            &read_bounded(
                &self.root.join("Cargo.lock"),
                self.context.limits.max_material_bytes,
            )?,
        ))
    }
    fn plan(&mut self) -> io::Result<DistInventory> {
        let args = vec![
            "dist".into(),
            "plan".into(),
            "--tag".into(),
            self.generated.tag.clone(),
            "--output-format=json".into(),
        ];
        let cargo = self.context.inputs.tools.cargo.clone();
        let raw = self.context.run(&cargo, &args, &self.root)?;
        self.unchanged()?;
        let inventory = checked(release_profile::verify_dist_plan(
            &self.profile,
            &self.generated,
            &raw,
        ))?;
        fs::write(self.storage.path().join("plan-dist-manifest.json"), raw)?;
        Ok(inventory)
    }
    fn validate_platform(&mut self, target: &str) -> io::Result<()> {
        let os = std::env::consts::OS;
        if !(os == "linux" && target.ends_with("-unknown-linux-gnu")
            || os == "macos" && target.ends_with("-apple-darwin"))
        {
            return Err(invalid(
                "missing release platform; cross-OS builds are unsupported",
            ));
        }
        if !self.profile.targets.contains(target) {
            return Err(invalid("target absent from production dist config"));
        }
        let rustc = self.context.inputs.tools.rustc.clone();
        let libdir = text(self.context.run(
            &rustc,
            &[
                "--print".into(),
                "target-libdir".into(),
                "--target".into(),
                target.into(),
            ],
            &self.root,
        )?)?;
        if !Path::new(libdir.trim()).is_dir() {
            return Err(invalid("Rust standard library missing for release target"));
        }
        if os == "linux" && std::env::consts::ARCH == "x86_64" && target.starts_with("aarch64-") {
            let linker = self
                .context
                .inputs
                .build_environment
                .get("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER")
                .ok_or_else(|| invalid("aarch64 Linux linker missing from reviewed toolchain"))?;
            if !Path::new(linker).is_absolute() || !Path::new(linker).is_file() {
                return Err(invalid("aarch64 Linux linker unavailable"));
            }
        }
        Ok(())
    }
    fn archive_files(&mut self, path: &Path) -> io::Result<BTreeMap<String, Member>> {
        let bytes = read_bounded(path, self.context.limits.max_material_bytes)?;
        if path.extension().and_then(|s| s.to_str()) == Some("gz") {
            release_archive::inspect(
                flate2::read::GzDecoder::new(bytes.as_slice()),
                self.context.limits.max_material_bytes,
                self.context.limits.max_members,
            )
        } else if path.extension().and_then(|s| s.to_str()) == Some("xz") {
            let temp = tempfile::Builder::new()
                .prefix("inspect-archive-")
                .tempdir_in(self.storage.path())?;
            let archive = temp.path().join("archive.tar.xz");
            fs::write(&archive, bytes)?;
            let xz = self.context.inputs.tools.xz.clone();
            let listing = text(self.context.run(
                &xz,
                &[
                    "--robot".into(),
                    "--list".into(),
                    "--".into(),
                    archive.to_string_lossy().into_owned(),
                ],
                temp.path(),
            )?)?;
            verify_xz_size(&listing, self.context.limits.max_material_bytes)?;
            self.context.run(
                &xz,
                &[
                    "--memlimit-decompress=256MiB".into(),
                    "--decompress".into(),
                    "--keep".into(),
                    "--".into(),
                    archive.to_string_lossy().into_owned(),
                ],
                temp.path(),
            )?;
            release_archive::inspect(
                File::open(temp.path().join("archive.tar"))?,
                self.context.limits.max_material_bytes,
                self.context.limits.max_members,
            )
        } else {
            Err(invalid("unsupported archive compression"))
        }
    }
    fn verify_source_archive(&mut self, path: &Path) -> io::Result<()> {
        let actual = self.archive_files(path)?;
        let reference = self.storage.path().join("expected-source.tar");
        let prefix = format!("{}-{}/", self.profile.package, self.generated.version);
        let git = self.context.inputs.tools.git.clone();
        self.context.run(
            &git,
            &[
                "archive".into(),
                "--format=tar".into(),
                format!("--prefix={prefix}"),
                format!("--output={}", reference.display()),
                self.binding.derived_commit.clone(),
            ],
            &self.root,
        )?;
        let expected = release_archive::inspect(
            File::open(reference)?,
            self.context.limits.max_material_bytes,
            self.context.limits.max_members,
        )?;
        if actual != expected {
            return Err(invalid(
                "source archive differs from exact derived git archive",
            ));
        }
        Ok(())
    }
    fn verify_local_archive(&mut self, path: &Path, target: &str) -> io::Result<()> {
        let members = self.archive_files(path)?;
        let root = format!("{}-{target}/", self.profile.package);
        if members.is_empty() || members.keys().any(|p| !p.starts_with(&root)) {
            return Err(invalid("distribution archive root mismatch"));
        }
        let mut executable_names = BTreeSet::new();
        for (path, member) in &members {
            let name = path.strip_prefix(&root).unwrap();
            if member.mode & 0o111 != 0 {
                if name.contains('/') || !self.profile.binaries.contains(name) {
                    return Err(invalid("unexpected executable in archive"));
                }
                binary_architecture(&member.bytes, target)?;
                executable_names.insert(name.to_owned());
            } else if self.profile.binaries.contains(name) {
                return Err(invalid("runtime archive binary is not executable"));
            }
        }
        if executable_names != self.profile.binaries {
            return Err(invalid("runtime binaries absent from archive"));
        }
        Ok(())
    }
    /// Actual stage execution. Returned records cover commands which really ran.
    /// The surrounding planner must keep all other obligations outstanding.
    pub fn run(&mut self, stage: Stage) -> io::Result<StageReport> {
        self.unchanged()?;
        let record_start = self.context.records.len();
        if let Stage::Local { target } = &stage {
            self.validate_platform(target)?;
        }
        let inventory = self.plan()?;
        let mut artifacts = BTreeMap::new();
        match &stage {
            Stage::Plan => {}
            Stage::Candidate => {
                artifacts = self.candidate_health()?;
            }
            Stage::Package => {
                let cargo = self.context.inputs.tools.cargo.clone();
                self.context.run(
                    &cargo,
                    &[
                        "package".into(),
                        "--locked".into(),
                        "--package".into(),
                        self.profile.package.clone(),
                    ],
                    &self.root,
                )?;
                let crate_name =
                    format!("{}-{}.crate", self.profile.package, self.generated.version);
                let package = self.root.join("target/package").join(&crate_name);
                let raw = read_bounded(&package, self.context.limits.max_material_bytes)?;
                let members = release_archive::inspect(
                    flate2::read::GzDecoder::new(raw.as_slice()),
                    self.context.limits.max_material_bytes,
                    self.context.limits.max_members,
                )?;
                let cargo_path = format!(
                    "{}-{}/Cargo.toml",
                    self.profile.package, self.generated.version
                );
                let file = members
                    .get(&cargo_path)
                    .ok_or_else(|| invalid("verified package missing Cargo.toml"))?;
                let cargo: toml::Value = toml::from_str(
                    std::str::from_utf8(&file.bytes)
                        .map_err(|_| invalid("package manifest not UTF-8"))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                if cargo
                    .get("package")
                    .and_then(|p| p.get("version"))
                    .and_then(|v| v.as_str())
                    != Some(&self.generated.version)
                {
                    return Err(invalid("verified package version mismatch"));
                }
                artifacts.insert(crate_name, release_profile::sha256(&raw));
            }
            Stage::Local { .. } | Stage::Global => {
                let cargo = self.context.inputs.tools.cargo.clone();
                let manifest = build_artifacts(
                    &self.root,
                    &self.profile,
                    &stage,
                    Some(&self.generated.tag),
                    |args| self.context.run(&cargo, args, &self.root),
                )?;
                let value: serde_json::Value = serde_json::from_slice(&manifest)
                    .map_err(|e| invalid(format!("invalid built artifact manifest: {e}")))?;
                if value.get("announcement_tag").and_then(|v| v.as_str())
                    != Some(&self.generated.tag)
                {
                    return Err(invalid("built artifact tag mismatch"));
                }
                fs::write(
                    self.storage.path().join("built-dist-manifest.json"),
                    &manifest,
                )?;
                let expected: BTreeSet<String> = match &stage {
                    Stage::Local { target } => {
                        let archive = inventory
                            .target_archives
                            .get(target)
                            .ok_or_else(|| invalid("planned target archive absent"))?;
                        BTreeSet::from([archive.clone(), format!("{archive}.sha256")])
                    }
                    Stage::Global => BTreeSet::from([
                        inventory.source_archive.clone(),
                        format!("{}.sha256", inventory.source_archive),
                    ]),
                    _ => unreachable!(),
                };
                let uploads = value
                    .get("upload_files")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| invalid("build manifest upload inventory absent"))?;
                let dist = self.root.join("target/distrib").canonicalize()?;
                let mut listed = BTreeSet::new();
                for item in uploads {
                    let p = Path::new(
                        item.as_str()
                            .ok_or_else(|| invalid("non-path artifact upload entry"))?,
                    );
                    let p = if p.is_absolute() {
                        p.to_owned()
                    } else {
                        self.root.join(p)
                    };
                    if p.parent().and_then(|p| p.canonicalize().ok()).as_ref() != Some(&dist)
                        || fs::symlink_metadata(&p)?.file_type().is_symlink()
                    {
                        return Err(invalid("artifact escaped owned distribution directory"));
                    }
                    let name = p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| invalid("non-UTF-8 artifact name"))?;
                    if !listed.insert(name.to_owned()) {
                        return Err(invalid("duplicate physical artifact"));
                    }
                }
                if listed != expected {
                    return Err(invalid(
                        "actual build artifact inventory differs from planned stage",
                    ));
                }
                for name in &expected {
                    let bytes =
                        read_bounded(&dist.join(name), self.context.limits.max_material_bytes)?;
                    if bytes.is_empty() {
                        return Err(invalid("empty built artifact"));
                    }
                    artifacts.insert(name.clone(), release_profile::sha256(&bytes));
                }
                for name in &expected {
                    if let Some(archive) = name.strip_suffix(".sha256") {
                        let expected_line = format!(
                            "{} *{archive}\n",
                            artifacts
                                .get(archive)
                                .ok_or_else(|| invalid("checksum archive absent"))?
                        );
                        if read_bounded(&dist.join(name), 1024)? != expected_line.as_bytes() {
                            return Err(invalid("built archive checksum mismatch"));
                        }
                    }
                }
                match &stage {
                    Stage::Local { target } => self.verify_local_archive(
                        &dist.join(&inventory.target_archives[target]),
                        target,
                    )?,
                    Stage::Global => {
                        self.verify_source_archive(&dist.join(&inventory.source_archive))?
                    }
                    _ => unreachable!(),
                }
            }
        }
        self.unchanged()?;
        let source = self.context.inputs.source_root.clone();
        if self.context.git(&source, &["rev-parse", "HEAD"])?.trim() != self.binding.source_commit
            || checked(release_profile::snapshot_digest(&snapshot(
                &mut self.context,
                &source,
            )?))?
                != self.binding.source_material_sha256
        {
            return Err(invalid("source changed during release stage"));
        }
        Ok(StageReport {
            stage,
            source_commit: self.binding.source_commit.clone(),
            derived_commit: self.binding.derived_commit.clone(),
            binding_sha256: self.generated.binding_sha256.clone(),
            command_records: self.context.records[record_start..].to_vec(),
            artifacts,
            tool_setup_sha256: self.tool_setup_sha256.clone(),
            worker_setup: self.worker_setup.clone(),
        })
    }
}

/// Real shared build helper: production and derivative execution both call this
/// exact function. The supplied runner owns process bounds/environment/capture.
pub fn build_artifacts(
    root: &Path,
    profile: &ReleaseProfile,
    stage: &Stage,
    tag: Option<&str>,
    mut execute: impl FnMut(&[String]) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    let lock = read_bounded(&root.join("Cargo.lock"), 64 * 1024 * 1024)?;
    let args = artifact_build_arguments(profile, stage, tag)?;
    if let Some(tag) = tag {
        let cargo: toml::Value =
            toml::from_str(&text(read_bounded(&root.join("Cargo.toml"), 1024 * 1024)?)?)
                .map_err(|e| invalid(e.to_string()))?;
        if cargo
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            != tag.strip_prefix('v')
        {
            return Err(invalid("artifact tag differs from actual Cargo version"));
        }
    }
    let result = execute(&args);
    if read_bounded(&root.join("Cargo.lock"), 64 * 1024 * 1024)? != lock {
        return Err(invalid("cargo-dist changed Cargo.lock"));
    }
    result
}

/// Complete-inventory check after the caller authenticates the reports' custody.
/// This does not turn user-supplied JSON reports into execution evidence.
pub fn verify_complete_reports(
    profile: &ReleaseProfile,
    generated: &GeneratedRelease,
    inventory: &DistInventory,
    source_commit: &str,
    derived_commit: &str,
    reports: &[StageReport],
) -> io::Result<BTreeMap<String, String>> {
    let mut seen = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut local = BTreeSet::new();
    for report in reports {
        if report.source_commit != source_commit
            || report.derived_commit != derived_commit
            || report.command_records.is_empty()
            || report.command_records.iter().any(|c| c.exit_code != 0)
            || report.binding_sha256 != generated.binding_sha256
            || report.tool_setup_sha256.len() != 64
        {
            return Err(invalid(
                "stage report identity, setup or execution mismatch",
            ));
        }
        let key = match &report.stage {
            Stage::Plan => "plan".to_owned(),
            Stage::Package => "package".to_owned(),
            Stage::Candidate => "candidate".to_owned(),
            Stage::Global => "global".to_owned(),
            Stage::Local { target } => {
                if !profile.targets.contains(target) || !local.insert(target.clone()) {
                    return Err(invalid("unknown/duplicate built target"));
                }
                format!("local:{target}")
            }
        };
        if !seen.insert(key) {
            return Err(invalid("duplicate release stage report"));
        }
        for (name, digest) in &report.artifacts {
            if digest.len() != 64
                || !digest.bytes().all(|b| b.is_ascii_hexdigit())
                || files.insert(name.clone(), digest.clone()).is_some()
            {
                return Err(invalid("invalid or duplicate aggregated artifact"));
            }
        }
    }
    if local != profile.targets
        || !["plan", "package", "global", "candidate"]
            .iter()
            .all(|key| seen.contains(*key))
    {
        return Err(invalid("release proof missing required stage/platform"));
    }
    let package = format!("{}-{}.crate", profile.package, generated.version);
    if files.remove(&package).is_none() {
        return Err(invalid("verified Cargo package missing"));
    }
    if files.remove("candidate-health.json").is_none() {
        return Err(invalid("candidate health artifact missing"));
    }
    checked(release_profile::verify_built_artifacts(inventory, &files))?;
    Ok(files)
}

#[cfg(test)]
#[path = "derived_release_driver_tests.rs"]
mod tests;

impl PreparedRelease<'_> {
    /// Persist verified output files in a fresh sibling directory owned by the
    /// surrounding runner. This never writes into the source worktree.
    pub fn export_artifacts(&self, report: &StageReport, destination: &Path) -> io::Result<()> {
        let parent = destination
            .parent()
            .ok_or_else(|| invalid("artifact destination has no parent"))?
            .canonicalize()?;
        let owned_parent = self
            .storage
            .path()
            .parent()
            .ok_or_else(|| invalid("missing owned parent"))?
            .canonicalize()?;
        if parent != owned_parent || destination.exists() {
            return Err(invalid(
                "artifact output must be a fresh owned sibling directory",
            ));
        }
        fs::create_dir(destination)?;
        let source_dir = match report.stage {
            Stage::Package => self.root.join("target/package"),
            Stage::Candidate => self.storage.path().to_owned(),
            _ => self.root.join("target/distrib"),
        };
        for (name, expected) in &report.artifacts {
            if !relative(name) || Path::new(name).components().count() != 1 {
                return Err(invalid("invalid export name"));
            }
            let bytes = read_bounded(
                &source_dir.join(name),
                self.context.limits.max_material_bytes,
            )?;
            if release_profile::sha256(&bytes) != *expected {
                return Err(invalid("artifact changed before export"));
            }
            use std::io::Write;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(name))?;
            output.write_all(&bytes)?;
        }
        Ok(())
    }
}

#[path = "derived_release_transport.rs"]
mod transport;
use transport::import_prepared;
pub use transport::{ReleaseTransport, import_release, prepare};

impl PreparedRelease<'_> {
    /// Actual captured bytes, addressed by their recorded digest. Only fresh
    /// scratch may receive them; no expected/log digest is copied as content.
    pub fn export_evidence(&self, destination: &Path) -> io::Result<()> {
        if destination.exists() || destination.starts_with(&self.context.inputs.source_root) {
            return Err(invalid("evidence destination must be fresh scratch"));
        }
        fs::create_dir(destination)?;
        let logs = destination.join("logs");
        fs::create_dir(&logs)?;
        for entry in fs::read_dir(&self.context.logs)? {
            let entry = entry?;
            let bytes = read_bounded(&entry.path(), self.context.limits.max_material_bytes)?;
            fs::write(logs.join(entry.file_name()), bytes)?;
        }
        for name in ["plan-dist-manifest.json", "built-dist-manifest.json"] {
            let path = self.storage.path().join(name);
            if path.exists() {
                fs::write(
                    destination.join(name),
                    read_bounded(&path, self.context.limits.max_output_bytes as u64)?,
                )?;
            }
        }
        Ok(())
    }
}

#[path = "derived_release_candidate.rs"]
mod candidate;

#[path = "derived_release_evidence.rs"]
mod evidence;
pub use evidence::{ExpectedTransport, verify_exported_stage, verify_exported_transport};
