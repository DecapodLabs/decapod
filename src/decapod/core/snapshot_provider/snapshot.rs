//! Submit the exact existing local workspace branch. All mutations require
//! the caller's separately established authorization for this exact destination.
use super::*;
use std::collections::BTreeMap;

mod host_adoption;
pub use host_adoption::adopt_host_snapshot;

pub const PREFIX: &str = "refs/heads/decapod/preflight/";
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SnapshotRequest {
    pub repository: Repository,
    pub request_id: String,
    pub source_ref: String,
    pub source_head: String,
    pub source_tree: String,
    pub accepted_base: String,
    pub accepted_base_ref: String,
    /// Full frozen local tag ref to object-ID inventory, including annotated objects.
    pub tags: BTreeMap<String, String>,
}
impl SnapshotRequest {
    pub fn validate(&self) -> Result<()> {
        ensure(
            uuid(&self.request_id),
            "snapshot request must be a canonical lowercase nonzero UUID",
        )?;
        ensure(
            branch(&self.source_ref)
                && !self.source_ref.starts_with(PREFIX)
                && self.source_ref != self.accepted_base_ref,
            "invalid logical source branch",
        )?;
        ensure(
            branch(&self.accepted_base_ref) && !self.accepted_base_ref.starts_with(PREFIX),
            "accepted branch must be a full original heads ref",
        )?;
        ensure(
            oid(&self.source_head)
                && oid(&self.source_tree)
                && oid(&self.accepted_base)
                && self.source_head.len() == self.source_tree.len()
                && self.source_head.len() == self.accepted_base.len()
                && self.source_head != self.accepted_base,
            "invalid immutable snapshot object identity",
        )?;
        ensure(
            self.tags
                .iter()
                .all(|(reference, id)| valid_tag(reference) && oid(id)),
            "invalid frozen tag inventory",
        )?;
        Ok(())
    }
    pub fn tag_inventory_digest(&self) -> Result<String> {
        Ok(hash(&serde_json::to_vec(&self.tags).map_err(|_| {
            invalid("tag inventory serialization failed")
        })?))
    }
    pub fn transport_ref(&self) -> String {
        self.source_ref.clone()
    }
    /// Historical handoff description only. This text is never written as a
    /// commit or used to authorize source identity; the actual workspace H is
    /// submitted unchanged.
    pub fn message(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "decapod CI preflight snapshot\n\nschema=1\nrequest={}\nsource={}\nbase={}\nsource_ref={}\n",
            self.request_id, self.source_head, self.accepted_base, self.source_ref
        ))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PreparedSnapshot {
    request: SnapshotRequest,
    transport_head: String,
    root: PathBuf,
}
impl PreparedSnapshot {
    #[cfg(test)]
    pub(crate) fn fixture(request: SnapshotRequest, transport_head: String, root: PathBuf) -> Self {
        Self {
            request,
            transport_head,
            root,
        }
    }
    pub fn request(&self) -> &SnapshotRequest {
        &self.request
    }
    pub fn transport_head(&self) -> &str {
        &self.transport_head
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}
fn git(
    runner: &mut impl Runner,
    root: &Path,
    args: &[&str],
    budget: &Budget<'_>,
) -> Result<String> {
    text(checked(runner, git_spec(root, args, false), budget)?)
        .map(|s| s.trim_end_matches('\n').into())
}
fn valid_tag(reference: &str) -> bool {
    reference
        .strip_prefix("refs/tags/")
        .is_some_and(|suffix| branch(&format!("refs/heads/{suffix}")))
}
fn parse_tags(bytes: &[u8], allow_peeled: bool) -> Result<BTreeMap<String, String>> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("invalid tag inventory bytes"))?;
    let mut tags = BTreeMap::new();
    let mut peeled = vec![];
    for line in text.lines() {
        let (id, reference) = line
            .split_once('\t')
            .ok_or_else(|| invalid("invalid tag inventory line"))?;
        ensure(oid(id), "invalid tag object ID")?;
        if let Some(original) = reference.strip_suffix("^{}") {
            ensure(
                allow_peeled && valid_tag(original),
                "invalid peeled tag ref",
            )?;
            peeled.push(original.to_string());
            continue;
        }
        ensure(
            valid_tag(reference) && tags.insert(reference.into(), id.into()).is_none(),
            "invalid or duplicate tag ref",
        )?;
    }
    ensure(
        peeled.iter().all(|name| tags.contains_key(name)),
        "peeled tag missing its actual ref",
    )?;
    Ok(tags)
}
pub fn capture_local_tags(
    runner: &mut impl Runner,
    root: &Path,
    budget: &Budget<'_>,
) -> Result<BTreeMap<String, String>> {
    let bytes = checked(
        runner,
        git_spec(
            root,
            &[
                "for-each-ref",
                "--format=%(objectname)%09%(refname)",
                "refs/tags/",
            ],
            false,
        ),
        budget,
    )?;
    parse_tags(&bytes, false)
}
fn check_transport_configuration(
    runner: &mut impl Runner,
    root: &Path,
    budget: &Budget<'_>,
) -> Result<()> {
    let config = captured(
        runner,
        &git_spec(
            root,
            &[
                "config",
                "--name-only",
                "--get-regexp",
                "^url\\..*\\.(insteadof|pushinsteadof)$",
            ],
            true,
        ),
        budget,
    )?;
    ensure(
        config.exit_code == 1 && config.stdout.is_empty(),
        "Git transport URL rewrite configuration unsupported",
    )
}
pub fn verify_remote_tags(
    runner: &mut impl Runner,
    snapshot: &PreparedSnapshot,
    budget: &Budget<'_>,
) -> Result<()> {
    check_transport_configuration(runner, &snapshot.root, budget)?;
    let bytes = checked(
        runner,
        git_spec(
            &snapshot.root,
            &[
                "ls-remote",
                "--tags",
                "--",
                &snapshot.request.repository.https_url(),
            ],
            true,
        ),
        budget,
    )?;
    ensure(
        parse_tags(&bytes, true)? == snapshot.request.tags,
        "remote tags differ from frozen local release history; tag publication is not authorized",
    )
}
/// Hash actual bytes with no filters or index stat-cache shortcuts. The committed
/// tree must cover every declared build input; ignored build inputs remain
/// unsupported and must be rejected by the parent inventory planner.
fn verify_committed_bytes(
    runner: &mut impl Runner,
    root: &Path,
    request: &SnapshotRequest,
    budget: &Budget<'_>,
) -> Result<()> {
    use std::path::Component;
    let inventory = checked(
        runner,
        git_spec(root, &["ls-tree", "-r", "-z", &request.source_head], false),
        budget,
    )?;
    let mut total = 0_u64;
    for entry in inventory.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        budget.remaining()?;
        let entry =
            std::str::from_utf8(entry).map_err(|_| invalid("non-UTF8 source path unsupported"))?;
        let (metadata, relative) = entry
            .split_once('\t')
            .ok_or_else(|| invalid("invalid committed tree entry"))?;
        let parts = metadata.split(' ').collect::<Vec<_>>();
        ensure(
            parts.len() == 3 && parts[1] == "blob" && oid(parts[2]),
            "unsupported committed tree object",
        )?;
        let path = Path::new(relative);
        ensure(
            !path.is_absolute()
                && path.components().all(|c| matches!(c, Component::Normal(_)))
                && !relative.split('/').any(|c| c == ".git"),
            "unsafe committed tree path",
        )?;
        let mut ancestor = root.to_path_buf();
        let components = path.components().collect::<Vec<_>>();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            ancestor.push(component.as_os_str());
            let meta = std::fs::symlink_metadata(&ancestor)?;
            ensure(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "symlink source ancestor unsupported",
            )?;
        }
        let full = root.join(path);
        let meta = std::fs::symlink_metadata(&full)?;
        total = total
            .checked_add(meta.len())
            .ok_or_else(|| invalid("source byte overflow"))?;
        ensure(
            total <= 256 * 1024 * 1024,
            "source material exceeds snapshot byte bound",
        )?;
        match parts[0] {
            "100644" | "100755" => {
                ensure(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "source type differs from committed mode",
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    ensure(
                        (meta.permissions().mode() & 0o111 != 0) == (parts[0] == "100755"),
                        "source executable mode differs",
                    )?;
                }
                #[cfg(not(unix))]
                return Err(invalid("snapshot mode comparison requires Unix"));
                ensure(
                    git(
                        runner,
                        root,
                        &["hash-object", "--no-filters", "--", relative],
                        budget,
                    )? == parts[2],
                    "actual source bytes differ from committed H",
                )?;
            }
            "120000" => {
                ensure(meta.file_type().is_symlink(), "source symlink differs")?;
                let target = std::fs::read_link(&full)?;
                ensure(!target.is_absolute(), "absolute source symlink unsupported")?;
                let resolved = full.canonicalize()?;
                ensure(
                    resolved.starts_with(root)
                        && !resolved
                            .strip_prefix(root)
                            .unwrap()
                            .components()
                            .any(|p| p.as_os_str() == ".git"),
                    "escaping source symlink unsupported",
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    let expected = checked(
                        runner,
                        git_spec(root, &["cat-file", "blob", parts[2]], false),
                        budget,
                    )?;
                    ensure(
                        target.as_os_str().as_bytes() == expected,
                        "source symlink bytes differ",
                    )?;
                }
                #[cfg(not(unix))]
                return Err(invalid("source symlink requires Unix"));
            }
            _ => return Err(invalid("unsupported committed source mode")),
        }
    }
    Ok(())
}
/// Deliberately refuses dirty or undeclared material. The parent must checkpoint
/// it through the normal governed path. It must not silently fall back to HEAD.
pub fn verify_source(
    runner: &mut impl Runner,
    root: &Path,
    request: &SnapshotRequest,
    budget: &Budget<'_>,
) -> Result<()> {
    request.validate()?;
    ensure(
        capture_local_tags(runner, root, budget)? == request.tags,
        "local tag inventory changed",
    )?;
    ensure(
        root.is_absolute() && root.canonicalize()? == root,
        "source root must be canonical and absolute",
    )?;
    ensure(
        git(runner, root, &["rev-parse", "--show-toplevel"], budget)? == root.to_string_lossy(),
        "source root must be worktree root",
    )?;
    ensure(
        git(
            runner,
            root,
            &["rev-parse", "--is-shallow-repository"],
            budget,
        )? == "false",
        "snapshot needs complete history",
    )?;
    let grafts = git(
        runner,
        root,
        &["rev-parse", "--git-path", "info/grafts"],
        budget,
    )?;
    ensure(!root.join(grafts).exists(), "grafted history unsupported")?;
    ensure(
        git(runner, root, &["symbolic-ref", "--quiet", "HEAD"], budget)? == request.source_ref,
        "source logical branch changed",
    )?;
    ensure(
        git(runner, root, &["rev-parse", "--verify", "HEAD"], budget)? == request.source_head,
        "source HEAD changed",
    )?;
    ensure(
        git(
            runner,
            root,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{tree}}", request.source_head),
            ],
            budget,
        )? == request.source_tree,
        "source tree changed",
    )?;
    for id in [&request.source_head, &request.accepted_base] {
        ensure(
            git(runner, root, &["cat-file", "-t", id], budget)? == "commit",
            "snapshot requires commits",
        )?;
    }
    git(
        runner,
        root,
        &[
            "merge-base",
            "--is-ancestor",
            &request.accepted_base,
            &request.source_head,
        ],
        budget,
    )?;
    let filters = captured(
        runner,
        &git_spec(
            root,
            &["config", "--name-only", "--get-regexp", "^filter\\."],
            false,
        ),
        budget,
    )?;
    ensure(
        filters.exit_code == 1 && filters.stdout.is_empty(),
        "Git content filters unsupported for frozen snapshot",
    )?;
    let index = checked(
        runner,
        git_spec(root, &["ls-files", "-v", "-z"], false),
        budget,
    )?;
    ensure(
        index
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
            .all(|e| e.starts_with(b"H ")),
        "hidden/conflicted/sparse tracked files unsupported",
    )?;
    let entries = git(runner, root, &["ls-files", "--stage"], budget)?;
    ensure(
        !entries.lines().any(|l| l.starts_with("160000 ")),
        "submodule material unsupported",
    )?;
    git(
        runner,
        root,
        &[
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            &request.source_head,
            "--",
        ],
        budget,
    )?;
    ensure(
        checked(
            runner,
            git_spec(
                root,
                &["ls-files", "--others", "--exclude-standard", "-z"],
                false,
            ),
            budget,
        )?
        .is_empty(),
        "untracked material must be governed and committed before snapshot",
    )?;
    verify_committed_bytes(runner, root, request, budget)
}
/// Capture the exact clean, committed local workspace branch. This creates no
/// Git object or reference and never changes the index or worktree.
pub fn prepare(
    runner: &mut impl Runner,
    root: &Path,
    request: SnapshotRequest,
    budget: &Budget<'_>,
) -> Result<PreparedSnapshot> {
    let head = request.source_head.clone();
    resume(runner, root, request, &head, budget)
}
/// Reconstruct the exact workspace-branch submission after independently
/// recapturing current source bytes, branch, base and tags. Historical synthetic
/// transport objects cannot be adopted. Remote binding is verified separately.
pub fn resume(
    runner: &mut impl Runner,
    root: &Path,
    request: SnapshotRequest,
    observed_transport_head: &str,
    budget: &Budget<'_>,
) -> Result<PreparedSnapshot> {
    budget.remaining()?;
    ensure(
        oid(observed_transport_head) && observed_transport_head == request.source_head,
        "workspace transport must equal the exact source HEAD",
    )?;
    verify_source(runner, root, &request, budget)?;
    Ok(PreparedSnapshot {
        request,
        transport_head: observed_transport_head.into(),
        root: root.into(),
    })
}
/// Fresh read of one accepted target. No local/remote ref is created or updated.
pub fn accepted_target(
    runner: &mut impl Runner,
    root: &Path,
    repository: &Repository,
    reference: &str,
    budget: &Budget<'_>,
) -> Result<Option<String>> {
    ensure(
        branch(reference) && !reference.starts_with(PREFIX),
        "invalid accepted target branch",
    )?;
    check_transport_configuration(runner, root, budget)?;
    let bytes = checked(
        runner,
        git_spec(
            root,
            &[
                "ls-remote",
                "--refs",
                "--",
                &repository.https_url(),
                reference,
            ],
            true,
        ),
        budget,
    )?;
    parse_readback(&bytes, reference)
}
/// Compatibility getter: read only the exact original workspace branch.
pub fn temporary_ref(
    runner: &mut impl Runner,
    snapshot: &PreparedSnapshot,
    budget: &Budget<'_>,
) -> Result<Option<String>> {
    read_ref(runner, snapshot, &snapshot.request.transport_ref(), budget)
}

fn read_ref(
    runner: &mut impl Runner,
    snapshot: &PreparedSnapshot,
    reference: &str,
    budget: &Budget<'_>,
) -> Result<Option<String>> {
    check_transport_configuration(runner, &snapshot.root, budget)?;
    let bytes = checked(
        runner,
        git_spec(
            &snapshot.root,
            &[
                "ls-remote",
                "--refs",
                "--",
                &snapshot.request.repository.https_url(),
                reference,
            ],
            true,
        ),
        budget,
    )?;
    parse_readback(&bytes, reference)
}
pub fn parse_readback(bytes: &[u8], reference: &str) -> Result<Option<String>> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("non-UTF8 ref readback"))?;
    if text.is_empty() {
        return Ok(None);
    }
    let lines = text.lines().collect::<Vec<_>>();
    ensure(
        lines.len() == 1 && text.ends_with('\n'),
        "ambiguous ref readback",
    )?;
    let (id, name) = lines[0]
        .split_once('\t')
        .ok_or_else(|| invalid("invalid ref readback"))?;
    ensure(oid(id) && name == reference, "wrong ref readback")?;
    Ok(Some(id.into()))
}
/// A push can succeed remotely even when the local response fails. Retain the
/// exact source identity for readback/reuse; validation never deletes the branch.
#[derive(Debug)]
pub struct PublishAttempt {
    pub snapshot: PreparedSnapshot,
    pub outcome: Result<()>,
}
pub fn publish(
    runner: &mut impl Runner,
    snapshot: PreparedSnapshot,
    budget: &Budget<'_>,
) -> PublishAttempt {
    let outcome = (|| {
        verify_source(runner, &snapshot.root, &snapshot.request, budget)?;
        let reference = snapshot.request.transport_ref();
        verify_remote_tags(runner, &snapshot, budget)?;
        ensure(
            read_ref(
                runner,
                &snapshot,
                &snapshot.request.accepted_base_ref,
                budget,
            )?
            .as_deref()
                == Some(snapshot.request.accepted_base.as_str()),
            "accepted target moved before transport",
        )?;
        ensure(
            snapshot.transport_head == snapshot.request.source_head,
            "workspace transport must equal the exact source HEAD",
        )?;
        let previous = read_ref(runner, &snapshot, &reference, budget)?;
        if previous.as_deref() != Some(snapshot.transport_head.as_str()) {
            if let Some(previous) = previous {
                // Fetch only immutable object data, without updating any local ref.
                // The exact current remote commit must be an ancestor of H.
                checked(
                    runner,
                    git_spec(
                        &snapshot.root,
                        &[
                            "fetch",
                            "--no-tags",
                            "--no-write-fetch-head",
                            "--no-auto-maintenance",
                            "--no-write-commit-graph",
                            "--no-recurse-submodules",
                            "--no-prune",
                            "--no-prune-tags",
                            "--refmap=",
                            "--",
                            &snapshot.request.repository.https_url(),
                            &previous,
                        ],
                        true,
                    ),
                    budget,
                )?;
                git(
                    runner,
                    &snapshot.root,
                    &[
                        "merge-base",
                        "--is-ancestor",
                        &previous,
                        &snapshot.transport_head,
                    ],
                    budget,
                )?;
            }
            // Git itself rechecks fast-forward safety against the current remote
            // during this ordinary push. Never use force, leases, or a '+' refspec.
            checked(
                runner,
                git_spec(
                    &snapshot.root,
                    &[
                        "push",
                        "--porcelain",
                        "--no-verify",
                        "--no-follow-tags",
                        "--recurse-submodules=no",
                        "--",
                        &snapshot.request.repository.https_url(),
                        &format!("{}:{reference}", snapshot.transport_head),
                    ],
                    true,
                ),
                budget,
            )?;
        }
        ensure(
            read_ref(runner, &snapshot, &reference, budget)?.as_deref()
                == Some(snapshot.transport_head.as_str()),
            "transport exact readback failed",
        )?;
        verify_source(runner, &snapshot.root, &snapshot.request, budget)
    })();
    PublishAttempt { snapshot, outcome }
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Cleanup {
    RetainedWorkbranch,
    AlreadyAbsent,
    Removed,
    NeedsRecovery,
}
/// Real workspace branches are never deleted by validation or recovery.
/// Legacy result variants remain readable, but no deletion operation is emitted.
pub fn cleanup(
    _runner: &mut impl Runner,
    snapshot: &PreparedSnapshot,
    budget: &Budget<'_>,
) -> Result<Cleanup> {
    budget.remaining()?;
    snapshot.request.validate()?;
    ensure(
        snapshot.transport_head == snapshot.request.source_head,
        "workspace transport must equal the exact source HEAD",
    )?;
    Ok(Cleanup::RetainedWorkbranch)
}
#[cfg(test)]
mod tests;
