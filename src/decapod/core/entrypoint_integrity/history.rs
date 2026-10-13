//! Commit-scoped source templates for Decapod's own immutable Git lineage.
//! This is a bounded source reader, never a Rust evaluator or a hash supplied by
//! the entrypoint being checked. Unrelated consumers retain binary templates.
use crate::core::bounded_process::BoundedCommand;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use syn::{Expr, Item, ItemFn, Lit, Stmt};

const SOURCE_ROOT: &str = "82a4997c9cb21794e3c8b3952bfe27482f1eaaee";
const ASSETS: &str = "src/decapod/core/assets.rs";
const MAX_BLOB: usize = 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
const HISTORY_TIMEOUT: Duration = Duration::from_secs(120);
const NAMED: &str = r##"fn template_named_agent(file_stem: &str) -> String {
    format!("# {}.md - Agent Entrypoint\n\n{}", file_stem, agent_entrypoint_body())
}"##;
const DISPATCH: &str = r##"fn canonical_template(name: &str) -> Option<String> {
    match name {
        "AGENTS.md" => Some(template_agents()),
        "CLAUDE.md" => Some(template_named_agent("CLAUDE")),
        "GEMINI.md" => Some(template_named_agent("GEMINI")),
        "CODEX.md" => Some(template_named_agent("CODEX")),
        _ => None,
    }
}"##;

pub(crate) fn git(repo: &Path, args: &[&str]) -> Option<std::process::Output> {
    let mut command = Command::new("git");
    // Local immutable object reads must not inherit replacement, graft,
    // alternate-object, or repository-redirection environment from the caller.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .bounded_output(CONTROL_TIMEOUT)
        .ok()
}
type GitRunner<'a> = dyn FnMut(&[&str]) -> Result<Output, String> + 'a;

pub(crate) fn ensure_ungrafted(repo: &Path) -> Result<(), String> {
    ensure_ungrafted_with(repo, &mut |args| {
        git(repo, args).ok_or_else(|| "bounded immutable Git inspection failed".into())
    })
}

fn ensure_ungrafted_with(repo: &Path, run_git: &mut GitRunner<'_>) -> Result<(), String> {
    let output = run_git(&["rev-parse", "--git-path", "info/grafts"])?;
    if !output.status.success() {
        return Err("cannot resolve historical Git graft path".into());
    }
    let path = String::from_utf8(output.stdout).map_err(|_| "invalid historical Git graft path")?;
    match std::fs::symlink_metadata(repo.join(path.trim())) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Git grafts are not immutable entrypoint history evidence".into()),
    }
}
fn oid_with(rev: &str, run_git: &mut GitRunner<'_>) -> Result<Option<String>, String> {
    let output = run_git(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])?;
    if !output.status.success() {
        return Ok(None);
    }
    let value =
        String::from_utf8(output.stdout).map_err(|_| "invalid immutable commit identity")?;
    let value = value.trim();
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid immutable commit identity".into());
    }
    Ok(Some(value.into()))
}
fn ancestor_with(base: &str, head: &str, run_git: &mut GitRunner<'_>) -> Result<bool, String> {
    let output = run_git(&["merge-base", "--is-ancestor", base, head])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("unable to verify immutable Git ancestry".into()),
    }
}

fn blob_with(commit: &str, path: &str, run_git: &mut GitRunner<'_>) -> Result<String, String> {
    let object = format!("{commit}:{path}");
    let size = run_git(&["cat-file", "-s", &object])?;
    if !size.status.success()
        || String::from_utf8_lossy(&size.stdout)
            .trim()
            .parse::<usize>()
            .ok()
            .is_none_or(|size| size > MAX_BLOB)
    {
        return Err(format!("missing or oversized historical source {path}"));
    }
    let output = run_git(&["cat-file", "blob", &object])?;
    if !output.status.success() {
        return Err(format!("cannot read historical source {path}"));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("historical source {path} is not UTF-8"))
}
fn source_blob_with(
    commit: &str,
    path: &str,
    run_git: &mut GitRunner<'_>,
) -> Result<String, String> {
    let output = run_git(&["ls-tree", commit, "--", path])?;
    if !output.status.success()
        || !String::from_utf8_lossy(&output.stdout).starts_with("100644 blob ")
    {
        return Err(format!(
            "historical source {path} must be a regular non-executable file"
        ));
    }
    blob_with(commit, path, run_git)
}

fn version(manifest: &str) -> Result<String, String> {
    let value: toml::Value =
        toml::from_str(manifest).map_err(|_| "invalid historical Cargo.toml")?;
    let package = value.get("package").ok_or("missing historical package")?;
    if package.get("name").and_then(toml::Value::as_str) != Some("decapod")
        || package.get("repository").and_then(toml::Value::as_str)
            != Some("https://github.com/DecapodLabs/decapod")
    {
        return Err("historical package is not Decapod source".into());
    }
    let version = package
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or("missing historical package version")?;
    // The bounded reader supports Decapod's stable release versions. Future
    // source/version encodings need an explicit reviewed reader extension.
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || p.len() > 1 && p.starts_with('0')
                || !p.bytes().all(|b| b.is_ascii_digit())
                || p.parse::<u64>().is_err()
        })
    {
        return Err("unsupported historical package version".into());
    }
    Ok(version.into())
}
fn function<'a>(file: &'a syn::File, name: &str) -> Result<&'a ItemFn, String> {
    let matches: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(function) if function.sig.ident == name => Some(function),
            _ => None,
        })
        .collect();
    if matches.len() != 1 || !matches[0].attrs.is_empty() {
        return Err(format!(
            "missing, conditional or duplicate template function {name}"
        ));
    }
    Ok(matches[0])
}
fn position(source: &str, span: proc_macro2::LineColumn) -> Result<usize, String> {
    let start: usize = source
        .split_inclusive('\n')
        .take(span.line.saturating_sub(1))
        .map(str::len)
        .sum();
    let position = start + span.column;
    if source.is_char_boundary(position) {
        Ok(position)
    } else {
        Err("invalid source span".into())
    }
}
fn tokens(source: &str) -> Result<String, String> {
    source
        .parse::<proc_macro2::TokenStream>()
        .map(|t| t.to_string())
        .map_err(|_| "unsupported template syntax".into())
}
fn exact_function(
    source: &str,
    file: &syn::File,
    name: &str,
    expected: &str,
) -> Result<(), String> {
    let function = function(file, name)?;
    let start = position(source, function.sig.fn_token.span.start())?;
    let end = position(source, function.block.brace_token.span.close().end())?;
    if tokens(&source[start..end])? != tokens(expected)? {
        return Err(format!("unsupported historical function {name}"));
    }
    Ok(())
}
fn literal(
    source: &str,
    file: &syn::File,
    name: &str,
    signature: &str,
    owned: bool,
) -> Result<String, String> {
    let function = function(file, name)?;
    let start = position(source, function.sig.fn_token.span.start())?;
    let end = position(source, function.block.brace_token.span.open().start())?;
    if tokens(&source[start..end])? != tokens(signature)? {
        return Err(format!("unsupported signature for {name}"));
    }
    let [Stmt::Expr(expr, None)] = function.block.stmts.as_slice() else {
        return Err(format!("unsupported body for {name}"));
    };
    let expr = if owned {
        let Expr::MethodCall(call) = expr else {
            return Err(format!("unsupported owned literal for {name}"));
        };
        if call.method != "to_string"
            || !call.args.is_empty()
            || call.turbofish.is_some()
            || !call.attrs.is_empty()
        {
            return Err(format!("unsupported literal conversion for {name}"));
        }
        call.receiver.as_ref()
    } else {
        expr
    };
    let Expr::Lit(value) = expr else {
        return Err(format!("nonliteral historical template {name}"));
    };
    let Lit::Str(value) = &value.lit else {
        return Err(format!("nonstring historical template {name}"));
    };
    Ok(value.value())
}
fn payload(source: &str, surface: &str) -> Result<String, String> {
    let file = syn::parse_file(source).map_err(|_| "invalid historical template source")?;
    exact_function(source, &file, "canonical_template", DISPATCH)?;
    exact_function(source, &file, "template_named_agent", NAMED)?;
    match surface {
        "AGENTS.md" => literal(
            source,
            &file,
            "template_agents",
            "fn template_agents() -> String",
            true,
        ),
        "CLAUDE.md" | "CODEX.md" | "GEMINI.md" => Ok(format!(
            "# {} - Agent Entrypoint\n\n{}",
            surface,
            literal(
                source,
                &file,
                "agent_entrypoint_body",
                "fn agent_entrypoint_body() -> &'static str",
                false
            )?
        )),
        _ => Err("unknown entrypoint".into()),
    }
}
fn historical_at(
    repo: &Path,
    commit: &str,
    surface: &str,
    head: &str,
    run_git: &mut GitRunner<'_>,
) -> Result<Option<String>, String> {
    ensure_ungrafted_with(repo, run_git)?;
    if commit == head || oid_with(SOURCE_ROOT, run_git)?.is_none() {
        return Ok(None);
    }
    if !ancestor_with(SOURCE_ROOT, head, run_git)? {
        return Ok(None);
    }
    if !ancestor_with(SOURCE_ROOT, commit, run_git)? || !ancestor_with(commit, head, run_git)? {
        return Err("historical commit is outside the Decapod source lineage".into());
    }
    // Neither origin text nor copied Cargo/assets files establish this identity.
    // The actual root commit and immutable ancestry are required first.
    let release = version(&source_blob_with(commit, "Cargo.toml", run_git)?)?;
    let body = payload(&source_blob_with(commit, ASSETS, run_git)?, surface)?;
    let hash = super::fingerprint_for_payload(surface, &release, &body);
    Ok(Some(format!(
        "<!-- decapod-release: {release} -->\n<!-- decapod-fingerprint: {hash} -->\n{body}"
    )))
}
fn normalize(content: &str) -> String {
    format!("{}\n", content.trim_end_matches(['\r', '\n']))
}

fn commit_path_violation_at(
    repo: &Path,
    commit: &str,
    status: char,
    surface: &str,
    head: &str,
    run_git: &mut GitRunner<'_>,
) -> Result<Option<String>, String> {
    if !super::ENTRYPOINT_FILES.contains(&surface) {
        return Ok(None);
    }
    ensure_ungrafted_with(repo, run_git)?;
    if status == 'D' {
        return Ok(Some(format!(
            "{commit}: deleted Decapod-owned entrypoint {surface} (not allowed)"
        )));
    }
    let commit = oid_with(commit, run_git)?.ok_or("unable to resolve entrypoint commit")?;
    let content = blob_with(&commit, surface, run_git)?;
    let parent = format!("{commit}^");
    let parent_object = format!("{parent}:{surface}");
    let parent_exists = run_git(&["cat-file", "-e", &parent_object])?;
    if parent_exists.status.success() && blob_with(&parent, surface, run_git)? == content {
        return Ok(Some(format!(
            "{commit}: non-content touch of entrypoint {surface} (mode/metadata-only changes are forbidden; leave Decapod-owned entrypoints alone)"
        )));
    }
    let historical = historical_at(repo, &commit, surface, head, run_git).map_err(|error| {
        format!("{commit}: cannot verify historical entrypoint {surface}: {error}")
    })?;
    let canonical = historical
        .clone()
        .or_else(|| super::render_entrypoint(surface))
        .ok_or("missing canonical entrypoint template")?;
    if normalize(&content) == normalize(&canonical) {
        return Ok(None);
    }
    // Preserve the existing binary-body rule for ordinary consumer histories
    // and current HEAD. A source-history mismatch never gets this fallback.
    if historical.is_none() {
        let prior = content
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("<!-- decapod-release:"))
            .and_then(|v| v.strip_suffix("-->"))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .and_then(|release| super::render_entrypoint_for_release(surface, release));
        if prior.is_some_and(|prior| normalize(&prior) == normalize(&content)) {
            return Ok(None);
        }
    }
    Ok(Some(format!(
        "{commit}: entrypoint {surface} is not its canonical Decapod template (hand edit or stale body). Regenerate via Decapod; do not hand-edit."
    )))
}

/// Validate every entrypoint transition in the exact candidate's history.
///
/// This publication invariant is independent of project-check/CI skip flags.
/// Every commit is visited, including merged side branches; merge resolutions
/// are inspected against their first parent instead of silently omitted by
/// the single-commit default of `git diff-tree`.
pub(crate) fn validate_committed_history(
    repo: &Path,
    accepted_base: &str,
    candidate_head: &str,
) -> Result<usize, String> {
    let started = Instant::now();
    validate_committed_history_with(repo, accepted_base, candidate_head, &mut |args| {
        if started.elapsed() >= HISTORY_TIMEOUT {
            return Err("immutable entrypoint history inspection exceeded its deadline".into());
        }
        git(repo, args).ok_or_else(|| "bounded immutable Git inspection failed".into())
    })
}

/// Use the controller's trusted executable, deadline, cancellation and output
/// budget for every read. Runner failures never become optional missing objects.
pub(crate) fn validate_committed_history_with(
    repo: &Path,
    accepted_base: &str,
    candidate_head: &str,
    run_git: &mut dyn FnMut(&[&str]) -> Result<Output, String>,
) -> Result<usize, String> {
    committed_history(repo, accepted_base, candidate_head, run_git)
        .map_err(|error| format!("ENTRYPOINT_COMMIT_DISCIPLINE: {error}"))
}

fn committed_history(
    repo: &Path,
    accepted_base: &str,
    candidate_head: &str,
    run_git: &mut GitRunner<'_>,
) -> Result<usize, String> {
    ensure_ungrafted_with(repo, run_git)?;
    let base = oid_with(accepted_base, run_git)?.ok_or("cannot resolve accepted base commit")?;
    let head = oid_with(candidate_head, run_git)?.ok_or("cannot resolve candidate commit")?;
    if !ancestor_with(&base, &head, run_git)? {
        return Err("accepted base is not an ancestor of the candidate".into());
    }
    let revisions = run_git(&[
        "rev-list",
        "--reverse",
        "--parents",
        &format!("{base}..{head}"),
    ])?;
    if !revisions.status.success() {
        return Err("unable to enumerate immutable candidate commits".into());
    }
    let revisions =
        String::from_utf8(revisions.stdout).map_err(|_| "invalid immutable commit enumeration")?;
    let mut failures = Vec::new();
    let mut canonical_rewrites = 0;
    for revision in revisions.lines() {
        let mut ids = revision.split_whitespace();
        let commit = ids.next().ok_or("missing enumerated commit")?;
        let parent = ids.next();
        let mut args = vec![
            "diff-tree",
            "--no-commit-id",
            "--name-status",
            "--no-renames",
            "-r",
            "-z",
        ];
        if let Some(parent) = parent {
            args.push(parent);
        } else {
            args.push("--root");
        }
        args.push(commit);
        args.push("--");
        let changed = run_git(&args)?;
        if !changed.status.success() {
            return Err(format!("{commit}: unable to inspect changed paths"));
        }
        let mut parts = changed.stdout.split(|byte| *byte == 0);
        while let Some(status) = parts.next() {
            if status.is_empty() {
                if parts.any(|part| !part.is_empty()) {
                    return Err(format!(
                        "{commit}: malformed immutable changed-path inspection"
                    ));
                }
                break;
            }
            let path = parts.next().ok_or("missing immutable changed path")?;
            let status =
                std::str::from_utf8(status).map_err(|_| "invalid immutable path status")?;
            if status.len() != 1 || !matches!(status.as_bytes()[0], b'A' | b'D' | b'M' | b'T') {
                return Err(format!(
                    "{commit}: unsupported immutable path status {status}"
                ));
            }
            let Some(surface) = super::ENTRYPOINT_FILES
                .iter()
                .find(|name| name.as_bytes() == path)
            else {
                continue;
            };
            if let Some(error) = commit_path_violation_at(
                repo,
                commit,
                status.as_bytes()[0] as char,
                surface,
                &head,
                run_git,
            )? {
                failures.push(error);
            } else {
                canonical_rewrites += 1;
            }
        }
    }
    if failures.is_empty() {
        Ok(canonical_rewrites)
    } else {
        Err(failures.join("; "))
    }
}

#[cfg(test)]
fn oid(repo: &Path, rev: &str) -> Option<String> {
    oid_with(rev, &mut |args| {
        git(repo, args).ok_or_else(|| "Git read failed".into())
    })
    .ok()
    .flatten()
}

#[cfg(test)]
fn historical(repo: &Path, commit: &str, surface: &str) -> Result<Option<String>, String> {
    let head = oid(repo, "HEAD").ok_or("cannot resolve current commit")?;
    historical_at(repo, commit, surface, &head, &mut |args| {
        git(repo, args).ok_or_else(|| "Git read failed".into())
    })
}

#[cfg(test)]
fn commit_path_violation(repo: &Path, commit: &str, status: char, surface: &str) -> Option<String> {
    let Some(head) = oid(repo, "HEAD") else {
        return Some("cannot resolve current commit".into());
    };
    commit_path_violation_at(repo, commit, status, surface, &head, &mut |args| {
        git(repo, args).ok_or_else(|| "Git read failed".into())
    })
    .unwrap_or_else(Some)
}

#[cfg(test)]
#[path = "history/tests.rs"]
mod tests;
