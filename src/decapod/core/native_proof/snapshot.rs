//! Source-preserving, bounded preparation of an owned execution snapshot.
//!
//! Git history is imported from a freshly generated local bundle. No source Git
//! config, hooks, remotes or object-store links are copied. No fetch/clone or
//! networking is used. The caller must independently review input sensitivity;
//! filename/signature checks below are fail-closed defenses, not a universal
//! secret detector. Git history is already authorized repository data and may
//! contain data not visible in current files; it is never transmitted externally.
use super::*;
use crate::core::init_reconciliation::{TOOLCHAIN_ALIAS, TOOLCHAIN_TARGET, reviewed_source_alias};
use std::io::Write;

const FIXTURE_ALIAS: &str = "tests/fixtures/state_commit_repo/symlink_test";
const FIXTURE_TARGET: &str = "tests/fixtures/state_commit_repo/target.txt";

// These are exact, source-reviewed repository aliases, not a general permit
// for symlinks. In particular the immutable golden-vector fixture must retain
// its Git mode and link bytes; copying its referent as a regular file changes
// that proof. Targets must independently exist as regular repository material.
fn reviewed_alias(relative: &str) -> Option<(&'static str, &'static str)> {
    match relative {
        TOOLCHAIN_ALIAS => Some((TOOLCHAIN_TARGET, TOOLCHAIN_TARGET)),
        FIXTURE_ALIAS => Some(("target.txt", FIXTURE_TARGET)),
        _ => None,
    }
}

fn source_alias(root: &Path, relative: &str) -> io::Result<Vec<u8>> {
    if relative == TOOLCHAIN_ALIAS {
        return reviewed_source_alias(root, relative)
            .map_err(|error| invalid(error.to_string()))?
            .ok_or_else(|| invalid("snapshot requires the reviewed Rust toolchain alias"));
    }
    let (link, target) = reviewed_alias(relative)
        .ok_or_else(|| invalid("snapshot only supports exact reviewed source aliases"))?;
    let mut parent = root.to_path_buf();
    for component in Path::new(relative).parent().unwrap().components() {
        parent.push(component);
        let metadata = fs::symlink_metadata(&parent)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid("snapshot reviewed alias has an unsafe parent"));
        }
    }
    let metadata = fs::symlink_metadata(root.join(relative))?;
    let target_metadata = fs::symlink_metadata(root.join(target))?;
    if !metadata.file_type().is_symlink()
        || fs::read_link(root.join(relative))? != Path::new(link)
        || !target_metadata.is_file()
        || target_metadata.file_type().is_symlink()
    {
        return Err(invalid("snapshot reviewed alias or regular target differs"));
    }
    Ok(link.as_bytes().to_vec())
}
fn alias_blob(root: &Path, relative: &str, oid: &str, budget: &mut Budget<'_>) -> io::Result<()> {
    let (link, _) =
        reviewed_alias(relative).ok_or_else(|| invalid("snapshot unreviewed alias blob"))?;
    if budget.git(root, &["cat-file", "blob", oid])? != link.as_bytes() {
        return Err(invalid("snapshot committed/index alias target differs"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SnapshotReview {
    pub source_branch: String,
    pub head_revision: String,
    pub accepted_base_revision: String,
    pub content_digest: String,
    /// Additional exact paths the caller identified as sensitive. They cannot
    /// be overridden by a matching digest or a declaration as an input.
    pub denied_paths: BTreeSet<String>,
}
impl SnapshotReview {
    /// Freezes identity only. It does not itself authorize or classify inputs.
    pub fn frozen(identity: &MaterialIdentity, denied_paths: BTreeSet<String>) -> Self {
        Self {
            source_branch: identity.work_branch.clone(),
            head_revision: identity.head_revision.clone(),
            accepted_base_revision: identity.accepted_base_revision.clone(),
            content_digest: identity.tree_digest.clone(),
            denied_paths,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub timeout: Duration,
    /// Bundle + repository metadata + copied working material. Individual Git
    /// output files are additionally constrained by a child-local RLIMIT_FSIZE.
    pub max_snapshot_bytes: u64,
    pub max_files: usize,
    /// Total source/destination hashing and copying work, including repeated
    /// captures. Exceeding a budget fails rather than omitting any input.
    pub max_io_bytes: u64,
    pub git_output_limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndexEntry {
    mode: String,
    oid: String,
    path: String,
}

fn relative_input(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && p != ".git")
}
fn sensitive_path(path: &str, review: &SnapshotReview) -> bool {
    if review
        .denied_paths
        .iter()
        .any(|p| path == p || path.starts_with(&format!("{p}/")))
    {
        return true;
    }
    path.split('/').any(|part| {
        let name = part.to_ascii_lowercase();
        [
            ".ssh",
            ".aws",
            ".gnupg",
            ".azure",
            ".netrc",
            ".npmrc",
            ".pypirc",
            "credentials",
            "credentials.json",
            "service-account.json",
            "id_rsa",
            "id_ed25519",
            ".env",
        ]
        .contains(&name.as_str())
            || (name.starts_with(".env.")
                && ![".env.example", ".env.sample", ".env.template"].contains(&name.as_str()))
            || [".pem", ".p12", ".pfx", ".key"]
                .iter()
                .any(|suffix| name.ends_with(suffix))
    })
}
fn check_input(path: &str, review: &SnapshotReview) -> io::Result<()> {
    if !relative_input(path) {
        return Err(invalid("snapshot input has an unsafe path"));
    }
    if sensitive_path(path, review) {
        return Err(invalid(format!(
            "snapshot refuses sensitive input path: {path}"
        )));
    }
    Ok(())
}
fn secret_signature(bytes: &[u8]) -> bool {
    // No matched bytes are returned or logged. Deliberately conservative known
    // credential formats supplement the caller's explicit sensitivity review.
    const HEADERS: [&[u8]; 4] = [
        b"-----BEGIN PRIVATE KEY-----",
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
    ];
    if bytes.split(|b| *b == b'\n').any(|line| {
        let line = line.trim_ascii();
        HEADERS.iter().any(|header| line.starts_with(header))
    }) {
        return true;
    }
    for (prefix, minimum) in [
        (b"ghp_".as_slice(), 36),
        (b"github_pat_".as_slice(), 60),
        (b"sk-proj-".as_slice(), 24),
        (b"AKIA".as_slice(), 16),
        (b"ASIA".as_slice(), 16),
    ] {
        for start in 0..bytes.len().saturating_sub(prefix.len() + minimum - 1) {
            if bytes[start..].starts_with(prefix)
                && bytes[start + prefix.len()..start + prefix.len() + minimum]
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
            {
                return true;
            }
        }
    }
    false
}
fn no_secret(bytes: &[u8], path: &str) -> io::Result<()> {
    if secret_signature(bytes) {
        Err(invalid(format!(
            "snapshot refuses credential-like content in input: {path}"
        )))
    } else {
        Ok(())
    }
}
fn parse_index(bytes: &[u8], review: &SnapshotReview) -> io::Result<BTreeMap<String, IndexEntry>> {
    let mut entries = BTreeMap::new();
    for record in bytes.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let record = std::str::from_utf8(record)
            .map_err(|_| invalid("snapshot requires UTF-8 index paths"))?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or_else(|| invalid("invalid Git index record"))?;
        if material::is_bookkeeping(path) {
            continue;
        }
        check_input(path, review)?;
        let fields: Vec<_> = metadata.split_whitespace().collect();
        if fields.len() != 3
            || fields[2] != "0"
            || !(matches!(fields[0], "100644" | "100755")
                || (fields[0] == "120000" && reviewed_alias(path).is_some()))
        {
            return Err(invalid(
                "snapshot does not support index conflicts, unreviewed symlinks or submodules",
            ));
        }
        let entry = IndexEntry {
            mode: fields[0].into(),
            oid: fields[1].into(),
            path: path.into(),
        };
        if entries.insert(path.into(), entry).is_some() {
            return Err(invalid("duplicate snapshot index input"));
        }
    }
    Ok(entries)
}
fn validate_source(
    paths: &IdentityInputs,
    review: &SnapshotReview,
    budget: &mut Budget<'_>,
    max_files: usize,
) -> io::Result<(
    material::WorkingMaterial,
    BTreeMap<String, IndexEntry>,
    Vec<String>,
)> {
    // Inspect names before reading working/staged content, so well-known secret
    // files are rejected without copying them into the snapshot.
    let index = parse_index(
        &budget.git(&paths.root, &["ls-files", "--stage", "-z"])?,
        review,
    )?;
    for alias in index.values().filter(|entry| entry.mode == "120000") {
        alias_blob(&paths.root, &alias.path, &alias.oid, budget)?;
        let (_, target) = reviewed_alias(&alias.path).unwrap();
        if !index
            .get(target)
            .is_some_and(|entry| matches!(entry.mode.as_str(), "100644" | "100755"))
        {
            return Err(invalid(
                "snapshot alias destination is not a regular indexed input",
            ));
        }
    }
    let mut tree_alias_targets = BTreeSet::new();
    let mut tree_regular = BTreeSet::new();
    let tree = budget.git(&paths.root, &["ls-tree", "-rz", "--full-tree", "HEAD"])?;
    for record in tree.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let record = std::str::from_utf8(record)
            .map_err(|_| invalid("snapshot requires UTF-8 tree paths"))?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or_else(|| invalid("invalid Git tree record"))?;
        if material::is_bookkeeping(path) {
            continue;
        }
        check_input(path, review)?;
        if metadata.starts_with("100644 blob ") || metadata.starts_with("100755 blob ") {
            tree_regular.insert(path.to_owned());
        }
        if reviewed_alias(path).is_some() && metadata.starts_with("120000 blob ") {
            alias_blob(
                &paths.root,
                path,
                metadata.strip_prefix("120000 blob ").unwrap(),
                budget,
            )?;
            tree_alias_targets.insert(reviewed_alias(path).unwrap().1);
        } else if !metadata.starts_with("100644 blob ") && !metadata.starts_with("100755 blob ") {
            return Err(invalid(
                "snapshot does not support unreviewed tree symlinks or submodules",
            ));
        }
    }
    if tree_alias_targets
        .iter()
        .any(|target| !tree_regular.contains(*target))
    {
        return Err(invalid(
            "snapshot committed alias lacks its regular destination",
        ));
    }
    for path in &paths.declared_untracked {
        check_input(path, review)?;
    }
    let captured = material::capture_with_io(&paths.root, &paths.declared_untracked, budget)?;
    if captured.files.len() > max_files {
        return Err(invalid("snapshot file-count budget exceeded"));
    }
    for (path, entry) in &captured.files {
        check_input(path, review)?;
        if entry.mode == "120000" && reviewed_alias(path).is_some() {
            let link = source_alias(&paths.root, path)?;
            let (_, target) = reviewed_alias(path).unwrap();
            if entry.content_digest.as_deref() != Some(&bytes_digest(&link))
                || !captured
                    .files
                    .get(target)
                    .is_some_and(|target| matches!(target.mode.as_str(), "100644" | "100755"))
            {
                return Err(invalid(
                    "snapshot alias or its regular material destination changed",
                ));
            }
        } else if !matches!(entry.mode.as_str(), "100644" | "100755" | "000000") {
            return Err(invalid(
                "snapshot does not support unreviewed working symlinks or special files",
            ));
        }
    }
    let base = text(budget.git(
        &paths.root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", paths.accepted_base_ref),
        ],
    )?)?;
    if captured.work_branch != review.source_branch
        || captured.head_revision != review.head_revision
        || base != review.accepted_base_revision
        || captured.content_digest().map_err(io::Error::other)? != review.content_digest
    {
        return Err(invalid(
            "source changed from the independently frozen snapshot review",
        ));
    }
    let refs = budget.git(
        &paths.root,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )?;
    let refs = std::str::from_utf8(&refs)
        .map_err(|_| invalid("non-UTF-8 source refs"))?
        .lines()
        .map(str::to_owned)
        .collect();
    Ok((captured, index, refs))
}

fn git_path(
    inputs: &IdentityInputs,
    relative: &str,
    budget: &mut Budget<'_>,
) -> io::Result<PathBuf> {
    Ok(PathBuf::from(text(budget.git(
        &inputs.root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            relative,
        ],
    )?)?))
}
fn check_history(inputs: &IdentityInputs, budget: &mut Budget<'_>) -> io::Result<()> {
    if text(budget.git(&inputs.root, &["rev-parse", "--is-shallow-repository"])?)? != "false" {
        return Err(invalid(
            "snapshot requires complete, non-shallow local history",
        ));
    }
    if !budget
        .git(
            &inputs.root,
            &["for-each-ref", "--format=%(refname)", "refs/replace/"],
        )?
        .is_empty()
    {
        return Err(invalid("snapshot does not support Git replace refs"));
    }
    for name in ["objects/info/alternates", "info/grafts"] {
        if git_path(inputs, name, budget)?.exists() {
            return Err(invalid(
                "snapshot does not support alternate object stores or grafts",
            ));
        }
    }
    let partial = budget.git_accepted(
        &inputs.root,
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(extensions\.partialclone|remote\..*\.promisor)$",
        ],
        None,
        &[0, 1],
    )?;
    if !partial.is_empty() {
        return Err(invalid(
            "snapshot does not support partial/promisor repositories",
        ));
    }
    for entry in fs::read_dir(git_path(inputs, "objects/pack", budget)?)? {
        budget.remaining()?;
        if entry?
            .path()
            .extension()
            .is_some_and(|ext| ext == "promisor")
        {
            return Err(invalid("snapshot does not support promisor object packs"));
        }
    }
    Ok(())
}
fn tree_bytes(path: &Path, budget: &mut Budget<'_>, limit: u64) -> io::Result<u64> {
    let owned_checkout = path.join("checkout");
    let mut pending = vec![path.to_path_buf()];
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        budget.remaining()?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                let full = entry.path();
                let relative = full
                    .strip_prefix(&owned_checkout)
                    .ok()
                    .and_then(Path::to_str)
                    .filter(|relative| reviewed_alias(relative).is_some())
                    .ok_or_else(|| invalid("snapshot metadata contains an unsupported symlink"))?;
                let link = source_alias(&owned_checkout, relative)?;
                total = total
                    .checked_add(link.len() as u64)
                    .ok_or_else(|| invalid("snapshot size overflow"))?;
                if total > limit {
                    return Err(invalid("snapshot byte budget exceeded"));
                }
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| invalid("snapshot size overflow"))?;
                if total > limit {
                    return Err(invalid("snapshot byte budget exceeded"));
                }
            } else {
                return Err(invalid("snapshot contains an unsupported special file"));
            }
        }
    }
    Ok(total)
}
fn copy_material(
    source: &Path,
    destination: &Path,
    relative: &str,
    entry: &material::MaterialEntry,
    budget: &mut Budget<'_>,
    remaining: &mut u64,
) -> io::Result<()> {
    if entry.mode == "000000" {
        return Ok(());
    }
    if entry.mode == "120000" {
        let link = source_alias(source, relative)?;
        if entry.content_digest.as_deref() != Some(&bytes_digest(&link)) {
            return Err(invalid("source alias changed during snapshot copy"));
        }
        budget.remaining()?;
        budget.remaining_bytes = budget
            .remaining_bytes
            .checked_sub(link.len() as u64)
            .ok_or_else(|| invalid("snapshot I/O byte budget exceeded"))?;
        *remaining = remaining
            .checked_sub(link.len() as u64)
            .ok_or_else(|| invalid("snapshot byte budget exceeded"))?;
        #[cfg(unix)]
        {
            let (link, _) = reviewed_alias(relative).unwrap();
            let path = destination.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            std::os::unix::fs::symlink(link, path)?;
        }
        #[cfg(not(unix))]
        {
            return Err(invalid("reviewed source alias requires Unix"));
        }
        return Ok(());
    }
    let source = source.join(relative);
    let destination = destination.join(relative);
    let source_meta = fs::symlink_metadata(&source)?;
    if !source_meta.is_file()
        || source_meta.file_type().is_symlink()
        || source_meta.len() > *remaining
    {
        return Err(invalid(
            "snapshot source changed type or exceeds byte budget",
        ));
    }
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| invalid("snapshot file has no parent"))?,
    )?;
    let mut read = fs::OpenOptions::new();
    read.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        read.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut source = read.open(source)?;
    if !source.metadata()?.is_file() {
        return Err(invalid("snapshot source is not a regular file"));
    }
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    let mut carry = Vec::new();
    loop {
        budget.remaining()?;
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        budget.remaining_bytes = budget
            .remaining_bytes
            .checked_sub(n as u64)
            .ok_or_else(|| invalid("snapshot I/O byte budget exceeded"))?;
        *remaining = remaining
            .checked_sub(n as u64)
            .ok_or_else(|| invalid("snapshot byte budget exceeded"))?;
        carry.extend_from_slice(&buffer[..n]);
        no_secret(&carry, relative)?;
        if carry.len() > 128 {
            carry.drain(..carry.len() - 128);
        }
        hash.update(&buffer[..n]);
        destination.write_all(&buffer[..n])?;
    }
    if entry.content_digest.as_deref() != Some(&format!("sha256:{:x}", hash.finalize())) {
        return Err(invalid("source bytes changed during snapshot copy"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        destination.set_permissions(fs::Permissions::from_mode(if entry.mode == "100755" {
            0o755
        } else {
            0o644
        }))?;
    }
    Ok(())
}

/// Prepare a full, independently owned local Git snapshot. Any unsupported or
/// unreviewed input fails; files/platforms are never silently dropped.
pub fn prepare_snapshot(
    inputs: &IdentityInputs,
    review: &SnapshotReview,
    limits: SnapshotLimits,
    scratch_parent: &Path,
    cancel: &Cancellation,
) -> io::Result<OwnedExecutionSnapshot> {
    NativePlatform::current()?;
    native_executable(&inputs.git_program)?;
    if inputs.git_program.file_name().and_then(|n| n.to_str()) != Some("git")
        || inputs.root.canonicalize()? != inputs.root
        || !inputs.accepted_base_ref.starts_with("refs/")
        || limits.timeout.is_zero()
        || limits.max_snapshot_bytes == 0
        || limits.max_files == 0
        || limits.max_io_bytes == 0
        || limits.git_output_limit == 0
        || limits.git_output_limit > 64 * 1024 * 1024
    {
        return Err(invalid("invalid snapshot input/tool/budget"));
    }
    let mut budget = Budget {
        deadline: Instant::now()
            .checked_add(limits.timeout)
            .ok_or_else(|| invalid("snapshot deadline overflow"))?,
        cancel,
        remaining_bytes: limits.max_io_bytes,
        git_program: inputs.git_program.clone(),
        git_output_limit: limits.git_output_limit,
    };
    budget.remaining()?;
    let scratch_parent = scratch_parent.canonicalize()?;
    let common_git = PathBuf::from(text(budget.git(
        &inputs.root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?)?)
    .canonicalize()?;
    if scratch_parent.starts_with(&inputs.root) || scratch_parent.starts_with(common_git) {
        return Err(invalid(
            "snapshot scratch must not be inside source worktree or Git metadata",
        ));
    }
    check_history(inputs, &mut budget)?;
    let (before, index, refs) = validate_source(inputs, review, &mut budget, limits.max_files)?;
    let tags = tag_refs(&inputs.root, &mut budget)?;
    if tags.len() > limits.max_files {
        return Err(invalid("snapshot tag inventory exceeds count budget"));
    }
    // The index can retain credential-like bytes that are absent from the dirty
    // working copy. Inspect every unique meaningful staged blob before creating
    // a bundle, including unchanged HEAD/index blobs, not only added files.
    let mut checked_blobs = BTreeSet::new();
    for entry in index.values() {
        if checked_blobs.insert(entry.oid.clone()) {
            let blob = budget.git(&inputs.root, &["cat-file", "blob", &entry.oid])?;
            budget.remaining_bytes = budget
                .remaining_bytes
                .checked_sub(blob.len() as u64)
                .ok_or_else(|| invalid("snapshot staged sensitivity-scan budget exceeded"))?;
            no_secret(&blob, &entry.path)?;
        }
    }
    let mut builder = tempfile::Builder::new();
    builder.prefix("native-snapshot-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let storage = builder.tempdir_in(&scratch_parent)?;
    let bundle = storage.path().join("source.bundle");
    let root = storage.path().join("checkout");
    let mut directory = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(&root)?;
    // Source-side Git writes only this owned bundle; no source refs/index/object
    // writes, hooks, external transport, clone or fetch are invoked.
    budget.git_bounded(
        &inputs.root,
        &[
            "bundle",
            "create",
            "--version=2",
            bundle
                .to_str()
                .ok_or_else(|| invalid("non-UTF-8 snapshot path"))?,
            "HEAD",
            &inputs.accepted_base_ref,
            "--tags",
        ],
        Some(limits.max_snapshot_bytes),
    )?;
    let bundle_size = fs::metadata(&bundle)?.len();
    if bundle_size > limits.max_snapshot_bytes {
        return Err(invalid("snapshot bundle exceeds byte budget"));
    }
    let git_limit = Some(limits.max_snapshot_bytes.saturating_sub(bundle_size));
    if git_limit == Some(0) {
        return Err(invalid("snapshot has no remaining metadata budget"));
    }
    budget.git_bounded(
        &root,
        &["init", "--template=", "--initial-branch=snapshot-unused"],
        git_limit,
    )?;
    budget.git_bounded(
        &root,
        &["bundle", "unbundle", bundle.to_str().unwrap()],
        git_limit,
    )?;
    tree_bytes(storage.path(), &mut budget, limits.max_snapshot_bytes)?;
    fs::remove_file(&bundle)?;
    budget.git_bounded(
        &root,
        &[
            "update-ref",
            &format!("refs/heads/{}", review.source_branch),
            &review.head_revision,
        ],
        git_limit,
    )?;
    budget.git_bounded(
        &root,
        &[
            "update-ref",
            &inputs.accepted_base_ref,
            &review.accepted_base_revision,
        ],
        git_limit,
    )?;
    for (name, oid) in &tags {
        budget.git_bounded(&root, &["update-ref", name, oid], git_limit)?;
    }
    budget.git_bounded(
        &root,
        &["update-ref", "--no-deref", "HEAD", &review.head_revision],
        git_limit,
    )?;
    budget.git_bounded(&root, &["read-tree", &review.head_revision], git_limit)?;
    let snapshot_head_index =
        parse_index(&budget.git(&root, &["ls-files", "--stage", "-z"])?, review)?;
    // Preserve index modes/blob IDs, including staged additions and deletions.
    // Changed staged blobs may not be reachable from H and need explicit import.
    for (path, entry) in &index {
        if snapshot_head_index.get(path) == Some(entry) {
            continue;
        }
        let blob = budget.git(&inputs.root, &["cat-file", "blob", &entry.oid])?;
        no_secret(&blob, path)?;
        budget.remaining_bytes = budget
            .remaining_bytes
            .checked_sub(blob.len() as u64)
            .ok_or_else(|| invalid("snapshot staged I/O budget exceeded"))?;
        if tree_bytes(storage.path(), &mut budget, limits.max_snapshot_bytes)? + blob.len() as u64
            > limits.max_snapshot_bytes
        {
            return Err(invalid("snapshot staged blob exceeds byte budget"));
        }
        let staged = storage.path().join("staged-blob");
        fs::write(&staged, &blob)?;
        let object = text(budget.git_bounded(
            &root,
            &[
                "hash-object",
                "-w",
                "--no-filters",
                staged.to_str().unwrap(),
            ],
            git_limit,
        )?)?;
        fs::remove_file(staged)?;
        if object != entry.oid {
            return Err(invalid("snapshot staged object identity mismatch"));
        }
        budget.git_bounded(
            &root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("{},{},{}", entry.mode, entry.oid, path),
            ],
            git_limit,
        )?;
    }
    for path in snapshot_head_index
        .keys()
        .filter(|path| !index.contains_key(*path))
    {
        budget.git_bounded(
            &root,
            &["update-index", "--force-remove", "--", path],
            git_limit,
        )?;
    }
    let used = tree_bytes(storage.path(), &mut budget, limits.max_snapshot_bytes)?;
    let mut remaining = limits.max_snapshot_bytes - used;
    for (path, entry) in &before.files {
        copy_material(
            &inputs.root,
            &root,
            path,
            entry,
            &mut budget,
            &mut remaining,
        )?;
    }
    tree_bytes(storage.path(), &mut budget, limits.max_snapshot_bytes)?;
    let (after, after_index, after_refs) =
        validate_source(inputs, review, &mut budget, limits.max_files)?;
    if before != after || index != after_index || refs != after_refs {
        return Err(invalid("source changed during snapshot preparation"));
    }
    let snapshot = OwnedExecutionSnapshot::prepared(storage, Path::new("checkout"))?;
    let captured = material::capture_detached_with_io(
        snapshot.root(),
        &inputs.declared_untracked,
        &mut budget,
    )?;
    if captured.head_revision != review.head_revision
        || captured.content_digest().map_err(io::Error::other)? != review.content_digest
    {
        return Err(invalid(
            "prepared snapshot differs from frozen source material",
        ));
    }
    if parse_index(
        &budget.git(snapshot.root(), &["ls-files", "--stage", "-z"])?,
        review,
    )? != index
    {
        return Err(invalid(
            "prepared snapshot index differs from frozen source index",
        ));
    }
    if tag_refs(snapshot.root(), &mut budget)? != tags {
        return Err(invalid(
            "prepared snapshot tag inventory differs from original source",
        ));
    }
    if !budget.git(snapshot.root(), &["remote"])?.is_empty()
        || snapshot.root().join(".git/hooks").exists()
    {
        return Err(invalid("snapshot unexpectedly contains remotes or hooks"));
    }
    budget.remaining()?;
    Ok(snapshot)
}
