//! Dependency-free, read-only verification of the CI checkout and event range.
//!
//! Compile before building the project:
//! `rustc --edition=2024 assets/tools/ci_event_context.rs -o ci-event-context`
//!
//! The caller supplies IDs from the provider event, never a branch-name fallback.
//! Explicit pre-PR dispatches use PR semantics only after proving that the exact
//! candidate contains the captured accepted target tip. The accepted ref is local
//! evidence, NOT a provider freshness check. The orchestration layer must recheck
//! the provider's target tip before reporting readiness or creating a PR.
//!
//! Temporary transport adapter: `--event push --head TRANSPORT_SHA --base ""`
//! `--pre-pr true --snapshot true --trigger-ref refs/heads/decapod/preflight/UUID`
//! `--accepted-base-ref refs/remotes/origin/master`. The base comes from strict
//! snapshot metadata, never push.before. An optional nonempty --base must match.
//! The transport is a single-parent, byte-identical-tree child of the original
//! committed candidate. Its creator must first prove the local source is clean
//! and checkpointed; this remote verifier cannot attest unexported local state.
//! After success the workflow may detach to the emitted original `head`.
//! This is a repository-owned adapter, not literal `bk preflight` compatibility.
//!
//! This tool neither fetches nor executes a shell, and emits outputs only after
//! every check succeeds. It does not classify releases or grant CI exemptions.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

type Check<T> = Result<T, String>;

#[derive(Debug)]
pub struct Inputs {
    pub repo: PathBuf,
    pub event: String,
    pub base: String,
    pub head: String,
    pub pre_pr: bool,
    pub accepted_base_ref: Option<String>,
    pub snapshot: bool,
    pub trigger_ref: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Context {
    pub event: String,
    pub trigger_event: String,
    pub base: String,
    pub head: String,
    pub merge_base: String,
    pub checkout_head: String,
    pub checkout_tree: String,
    pub head_tree: String,
    pub accepted_base: String,
    pub pre_pr: bool,
    pub snapshot: Option<SnapshotMetadata>,
}

/// Exact transport message (all fields mandatory, in this order; one final LF):
/// ```text
/// decapod CI preflight snapshot
///
/// schema=1
/// request=<canonical lowercase UUID>
/// source=<full original candidate commit ID>
/// base=<full immutable accepted base commit ID>
/// source_ref=<original full refs/heads/... branch ref>
/// ```
/// No extra lines, trailers, whitespace normalization, or duplicate fields.
/// The source branch is an identity only; it is never resolved as a fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub request: String,
    pub source: String,
    pub base: String,
    pub source_ref: String,
}

const SNAPSHOT_PREFIX: &str = "refs/heads/decapod/preflight/";

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
        && value
            .bytes()
            .any(|byte| matches!(byte, b'1'..=b'9' | b'a'..=b'f'))
}

fn valid_source_ref(reference: &str) -> bool {
    reference.starts_with("refs/heads/")
        && !reference.starts_with(SNAPSHOT_PREFIX)
        && reference.len() <= 512
        && !reference.ends_with('.')
        && !reference.contains("..")
        && !reference.contains("@{")
        && reference.bytes().all(|byte| {
            byte.is_ascii_graphic()
                && !matches!(byte, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
        && reference
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

impl SnapshotMetadata {
    fn validate(&self) -> Check<()> {
        if !is_uuid(&self.request) {
            return Err("snapshot request must be a canonical nonzero lowercase UUID".into());
        }
        require_oid(&self.source)?;
        require_oid(&self.base)?;
        if self.source == self.base || !valid_source_ref(&self.source_ref) {
            return Err("snapshot requires distinct commits and a full original branch ref".into());
        }
        Ok(())
    }

    pub fn encode(&self) -> Check<String> {
        self.validate()?;
        Ok(format!(
            "decapod CI preflight snapshot\n\nschema=1\nrequest={}\nsource={}\nbase={}\nsource_ref={}\n",
            self.request, self.source, self.base, self.source_ref
        ))
    }

    pub fn parse(message: &str) -> Check<Self> {
        if message.len() > 1024 {
            return Err("snapshot metadata exceeds its bound".into());
        }
        let lines: Vec<_> = message.split('\n').collect();
        if lines.len() != 8
            || lines[0] != "decapod CI preflight snapshot"
            || !lines[1].is_empty()
            || lines[2] != "schema=1"
            || !lines[7].is_empty()
        {
            return Err("snapshot message must use the exact schema-1 encoding".into());
        }
        let field = |index: usize, prefix| {
            lines[index]
                .strip_prefix(prefix)
                .map(str::to_string)
                .ok_or_else(|| {
                    "snapshot fields are missing, duplicated, or out of order".to_string()
                })
        };
        let metadata = Self {
            request: field(3, "request=")?,
            source: field(4, "source=")?,
            base: field(5, "base=")?,
            source_ref: field(6, "source_ref=")?,
        };
        metadata.validate()?;
        Ok(metadata)
    }
}

fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
}

fn require_oid(value: &str) -> Check<()> {
    if is_oid(value) {
        Ok(())
    } else {
        Err("expected a full, lowercase, nonzero immutable Git object ID".into())
    }
}

fn git_output(repo: &Path, args: &[&str]) -> Check<Output> {
    let mut command = Command::new("git");
    // Never inherit an alternate worktree, index, graft file, replacement-object
    // view, command-scoped Git config, or credentials from the caller's process.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .arg("--no-replace-objects")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|_| "could not execute Git".into())
}

fn git(repo: &Path, args: &[&str]) -> Check<String> {
    let output = git_output(repo, args)?;
    if !output.status.success() {
        // Do not echo untrusted provider inputs, paths, or Git diagnostics into
        // the workflow command channel.
        return Err(format!("Git {} verification failed", args[0]));
    }
    if output.stdout.len() > 16 * 1024 {
        return Err("Git verification output exceeded its bound".into());
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim_end_matches('\n').to_string())
        .map_err(|_| "Git returned non-UTF-8 verification output".into())
}

fn commit(repo: &Path, id: &str) -> Check<()> {
    require_oid(id)?;
    if git(repo, &["cat-file", "-t", id])? != "commit" {
        return Err("immutable input must name a commit, not another Git object".into());
    }
    Ok(())
}

fn revision_oid(repo: &Path, revision: &str) -> Check<String> {
    let oid = git(
        repo,
        &["rev-parse", "--verify", "--end-of-options", revision],
    )?;
    require_oid(&oid)?;
    Ok(oid)
}

fn ancestor(repo: &Path, base: &str, head: &str) -> Check<()> {
    git(repo, &["merge-base", "--is-ancestor", base, head])?;
    Ok(())
}

fn accepted_tip(repo: &Path, reference: &str) -> Check<String> {
    // Require a named local capture of the accepted branch, not HEAD, a tag,
    // revision syntax, an ambiguous short name, or an option-like argument.
    if reference.len() > 512
        || !(reference.starts_with("refs/heads/") || reference.starts_with("refs/remotes/"))
        || reference.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("accepted base must be an explicit heads/remotes ref".into());
    }
    git(repo, &["check-ref-format", reference])?;
    let oid = git(repo, &["show-ref", "--verify", "--hash", reference])?;
    commit(repo, &oid)?;
    Ok(oid)
}

pub fn resolve(input: &Inputs) -> Check<Context> {
    if !matches!(
        input.event.as_str(),
        "pull_request" | "push" | "workflow_dispatch"
    ) {
        return Err("unsupported CI event".into());
    }
    if input.snapshot && (input.event != "push" || !input.pre_pr) {
        return Err("snapshot mode requires push and explicit pre-PR mode".into());
    }
    if input.pre_pr && input.event != "workflow_dispatch" && !input.snapshot {
        return Err("pre-PR mode requires explicit dispatch or snapshot mode".into());
    }
    if !input.snapshot && input.trigger_ref.is_some() {
        return Err("trigger-ref is reserved for snapshot mode".into());
    }
    if input.snapshot && input.trigger_ref.is_none() {
        return Err("snapshot mode requires the exact provider trigger-ref".into());
    }
    let manual = input.event == "workflow_dispatch" && !input.pre_pr;
    if manual && (!input.base.is_empty() || input.accepted_base_ref.is_some()) {
        return Err("ordinary manual dispatch cannot carry a PR base or accepted ref".into());
    }
    if !input.pre_pr && input.accepted_base_ref.is_some() {
        return Err("accepted-base-ref is reserved for explicit pre-PR mode".into());
    }
    require_oid(&input.head)?;
    if !manual && !input.snapshot {
        require_oid(&input.base)?;
        if input.base == input.head {
            return Err("base and head must be distinct immutable commits".into());
        }
    }
    if input.pre_pr && input.accepted_base_ref.is_none() {
        return Err("pre-PR mode requires an explicit accepted-base-ref".into());
    }
    let repo = input
        .repo
        .canonicalize()
        .map_err(|_| "repository path is unavailable")?;
    let top = PathBuf::from(git(&repo, &["rev-parse", "--show-toplevel"])?);
    if top
        .canonicalize()
        .map_err(|_| "checkout root is unavailable")?
        != repo
    {
        return Err("repo must identify the checkout root".into());
    }
    if git(&repo, &["rev-parse", "--is-shallow-repository"])? != "false" {
        return Err("complete Git history is required".into());
    }
    let grafts = PathBuf::from(git(&repo, &["rev-parse", "--git-path", "info/grafts"])?);
    if repo.join(grafts).exists() {
        return Err("legacy Git grafts are not immutable history evidence".into());
    }
    commit(&repo, &input.head)?;
    let checkout_head = revision_oid(&repo, "HEAD")?;
    let checkout_tree = revision_oid(&repo, &format!("{checkout_head}^{{tree}}"))?;
    let head_tree = revision_oid(&repo, &format!("{}^{{tree}}", input.head))?;
    // Git's normal worktree comparison can invoke a configured clean/process
    // filter. CI context verification never needs executable content filters.
    let filters = git_output(
        &repo,
        &["config", "--name-only", "--get-regexp", "^filter\\."],
    )?;
    if filters.status.code() != Some(1) {
        return Err("repository content filters are not supported during verification".into());
    }
    // Check index, tracked worktree, and non-ignored untracked files, without
    // allowing external diff/textconv programs or writing the index cache.
    git(
        &repo,
        &[
            "diff",
            "--quiet",
            "--no-ext-diff",
            "--no-textconv",
            &checkout_head,
            "--",
        ],
    )?;
    if !git(&repo, &["ls-files", "--others", "--exclude-standard", "-z"])?.is_empty() {
        return Err("checkout contains untracked files".into());
    }
    if input.snapshot {
        // An assume-unchanged or sparse/skip-worktree index could hide dirty
        // tracked source from the normal comparison. A snapshot must attest a
        // complete ordinary checkout, not silently ignore those paths.
        let index = git_output(&repo, &["ls-files", "-v", "-z"])?;
        if !index.status.success()
            || index.stdout.len() > 8 * 1024 * 1024
            || index
                .stdout
                .split(|byte| *byte == 0)
                .filter(|entry| !entry.is_empty())
                .any(|entry| !entry.starts_with(b"H "))
        {
            return Err("snapshot requires a complete index without hidden tracked paths".into());
        }
        return resolve_snapshot(&repo, input, checkout_head, checkout_tree);
    }
    let mut context = Context {
        event: if input.pre_pr {
            "pull_request".into()
        } else {
            input.event.clone()
        },
        trigger_event: input.event.clone(),
        base: input.base.clone(),
        head: input.head.clone(),
        merge_base: String::new(),
        checkout_head,
        checkout_tree,
        head_tree,
        accepted_base: String::new(),
        pre_pr: input.pre_pr,
        snapshot: None,
    };
    if manual {
        if context.checkout_head != input.head {
            return Err("manual dispatch head does not equal the current checkout".into());
        }
        verify_unchanged_head(&repo, &context.checkout_head)?;
        return Ok(context);
    }
    commit(&repo, &input.base)?;
    if context.checkout_head != input.head {
        if context.event != "pull_request" || input.pre_pr {
            return Err("candidate head does not equal the current checkout".into());
        }
        let parents = git(
            &repo,
            &["rev-list", "--parents", "-n", "1", &context.checkout_head],
        )?;
        let expected = format!("{} {} {}", context.checkout_head, input.base, input.head);
        if parents != expected {
            return Err("PR checkout is not the exact ordered base/head two-parent merge".into());
        }
    }
    if input.pre_pr {
        context.accepted_base = accepted_tip(&repo, input.accepted_base_ref.as_deref().unwrap())?;
        if context.accepted_base != input.base {
            return Err("captured accepted target tip differs from the immutable base".into());
        }
        ancestor(&repo, &input.base, &input.head)?;
        if context.checkout_tree != context.head_tree {
            return Err("pre-PR candidate tree differs from the checkout tree".into());
        }
    }
    context.merge_base = if context.event == "push" {
        ancestor(&repo, &input.base, &input.head)?;
        input.base.clone()
    } else {
        let base = git(&repo, &["merge-base", "--all", &input.base, &input.head])?;
        // Multiple merge bases are ambiguous: never silently select one.
        require_oid(&base)?;
        base
    };
    verify_unchanged_head(&repo, &context.checkout_head)?;
    if input.pre_pr
        && accepted_tip(&repo, input.accepted_base_ref.as_deref().unwrap())?
            != context.accepted_base
    {
        return Err("captured accepted target ref changed during verification".into());
    }
    Ok(context)
}

fn resolve_snapshot(
    repo: &Path,
    input: &Inputs,
    checkout_head: String,
    checkout_tree: String,
) -> Check<Context> {
    if checkout_head != input.head {
        return Err("snapshot event head does not equal the current checkout".into());
    }
    let request = input
        .trigger_ref
        .as_deref()
        .and_then(|reference| reference.strip_prefix(SNAPSHOT_PREFIX))
        .filter(|request| is_uuid(request))
        .ok_or("snapshot trigger-ref must be the exact reserved prefix and canonical UUID")?;
    // Ordinary context ignores replacements; snapshot proof explicitly rejects
    // any replacement view so its raw transport parent cannot be misrepresented.
    if !git(
        repo,
        &["for-each-ref", "--format=%(refname)", "refs/replace/"],
    )?
    .is_empty()
    {
        return Err("snapshot verification rejects replacement-object refs".into());
    }
    let raw = git_output(repo, &["cat-file", "commit", &checkout_head])?;
    if !raw.status.success() || raw.stdout.len() > 16 * 1024 {
        return Err("snapshot commit is unavailable or exceeds its bound".into());
    }
    let raw = String::from_utf8(raw.stdout).map_err(|_| "snapshot commit is not UTF-8")?;
    let (headers, message) = raw
        .split_once("\n\n")
        .ok_or("snapshot commit has no message")?;
    let metadata = SnapshotMetadata::parse(message)?;
    let parents: Vec<_> = headers
        .lines()
        .filter_map(|line| line.strip_prefix("parent "))
        .collect();
    if parents.as_slice() != [metadata.source.as_str()] {
        return Err("snapshot must have exactly one parent equal to its original source".into());
    }
    if metadata.request != request || (!input.base.is_empty() && input.base != metadata.base) {
        return Err(
            "snapshot request or explicit base does not match its immutable metadata".into(),
        );
    }
    commit(repo, &metadata.source)?;
    commit(repo, &metadata.base)?;
    git(repo, &["check-ref-format", &metadata.source_ref])?;
    let head_tree = revision_oid(repo, &format!("{}^{{tree}}", metadata.source))?;
    if checkout_tree != head_tree {
        return Err("snapshot tree differs from the original committed candidate".into());
    }
    let accepted_ref = input
        .accepted_base_ref
        .as_deref()
        .ok_or("snapshot requires accepted-base-ref")?;
    let accepted_base = accepted_tip(repo, accepted_ref)?;
    if accepted_base != metadata.base {
        return Err("captured accepted target tip differs from the snapshot base".into());
    }
    ancestor(repo, &metadata.base, &metadata.source)?;
    verify_unchanged_head(repo, &checkout_head)?;
    if accepted_tip(repo, accepted_ref)? != accepted_base {
        return Err("captured accepted target ref changed during verification".into());
    }
    Ok(Context {
        event: "pull_request".into(),
        trigger_event: "push".into(),
        base: metadata.base.clone(),
        head: metadata.source.clone(),
        merge_base: metadata.base.clone(),
        checkout_head,
        checkout_tree,
        head_tree,
        accepted_base,
        pre_pr: true,
        snapshot: Some(metadata),
    })
}

fn verify_unchanged_head(repo: &Path, expected: &str) -> Check<()> {
    if revision_oid(repo, "HEAD")? != expected {
        return Err("checkout HEAD changed during verification".into());
    }
    Ok(())
}

impl Context {
    /// Fixed keys and validated single-line values only. Keep one complete
    /// string so failures cannot leave partially valid GitHub outputs behind.
    pub fn github_outputs(&self) -> String {
        let mut outputs = format!(
            "event={}\ntrigger_event={}\nbase={}\nhead={}\nmerge_base={}\ncheckout_head={}\ncheckout_tree={}\nhead_tree={}\naccepted_base={}\npre_pr={}\npr_like={}\nlocal_base_verified={}\nprovider_freshness=not_checked\n",
            self.event,
            self.trigger_event,
            self.base,
            self.head,
            self.merge_base,
            self.checkout_head,
            self.checkout_tree,
            self.head_tree,
            self.accepted_base,
            self.pre_pr,
            self.event == "pull_request",
            self.pre_pr,
        );
        if let Some(metadata) = &self.snapshot {
            outputs.push_str(&format!(
                "snapshot=true\ntransport_head={}\nrequest_id={}\nsource_ref={}\n",
                self.checkout_head, metadata.request, metadata.source_ref
            ));
        }
        outputs
    }
}

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Check<Inputs> {
    let mut values = std::collections::BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if !matches!(
            flag.as_str(),
            "--repo"
                | "--event"
                | "--base"
                | "--head"
                | "--pre-pr"
                | "--accepted-base-ref"
                | "--snapshot"
                | "--trigger-ref"
        ) {
            return Err("unknown argument".into());
        }
        let value = args.next().ok_or("missing argument value")?;
        if values.insert(flag, value).is_some() {
            return Err("duplicate argument".into());
        }
    }
    let mut required = |key| values.remove(key).ok_or_else(|| format!("missing {key}"));
    Ok(Inputs {
        repo: PathBuf::from(required("--repo")?),
        event: required("--event")?,
        base: required("--base")?,
        head: required("--head")?,
        pre_pr: match required("--pre-pr")?.as_str() {
            "true" => true,
            "false" => false,
            _ => return Err("pre-pr must be exactly true or false".into()),
        },
        accepted_base_ref: values.remove("--accepted-base-ref"),
        snapshot: match values.remove("--snapshot").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            _ => return Err("snapshot must be exactly true or false".into()),
        },
        trigger_ref: values.remove("--trigger-ref"),
    })
}

fn main() {
    match parse_args(std::env::args().skip(1)).and_then(|input| resolve(&input)) {
        Ok(context) => print!("{}", context.github_outputs()),
        Err(error) => {
            eprintln!("CI event context rejected: {error}");
            std::process::exit(1);
        }
    }
}
