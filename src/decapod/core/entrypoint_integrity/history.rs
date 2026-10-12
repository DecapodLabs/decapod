//! Commit-scoped source templates for Decapod's own immutable Git lineage.
//! This is a bounded source reader, never a Rust evaluator or a hash supplied by
//! the entrypoint being checked. Unrelated consumers retain binary templates.
use std::path::Path;
use std::process::Command;
use syn::{Expr, Item, ItemFn, Lit, Stmt};

const SOURCE_ROOT: &str = "82a4997c9cb21794e3c8b3952bfe27482f1eaaee";
const ASSETS: &str = "src/decapod/core/assets.rs";
const MAX_BLOB: usize = 1024 * 1024;
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
        .output()
        .ok()
}
pub(crate) fn ensure_ungrafted(repo: &Path) -> Result<(), String> {
    let output = git(repo, &["rev-parse", "--git-path", "info/grafts"])
        .ok_or("cannot inspect historical Git graft state")?;
    if !output.status.success() {
        return Err("cannot resolve historical Git graft path".into());
    }
    let path = String::from_utf8(output.stdout).map_err(|_| "invalid historical Git graft path")?;
    match std::fs::symlink_metadata(repo.join(path.trim())) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Git grafts are not immutable entrypoint history evidence".into()),
    }
}
fn oid(repo: &Path, rev: &str) -> Option<String> {
    let output = git(
        repo,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn ancestor(repo: &Path, base: &str, head: &str) -> Result<bool, String> {
    let output = git(repo, &["merge-base", "--is-ancestor", base, head])
        .ok_or("cannot inspect immutable Git ancestry")?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("unable to verify immutable Git ancestry".into()),
    }
}

fn blob(repo: &Path, commit: &str, path: &str) -> Result<String, String> {
    let object = format!("{commit}:{path}");
    let size = git(repo, &["cat-file", "-s", &object]).ok_or("cannot inspect Git blob")?;
    if !size.status.success()
        || String::from_utf8_lossy(&size.stdout)
            .trim()
            .parse::<usize>()
            .ok()
            .is_none_or(|size| size > MAX_BLOB)
    {
        return Err(format!("missing or oversized historical source {path}"));
    }
    let output = git(repo, &["cat-file", "blob", &object]).ok_or("cannot read Git blob")?;
    if !output.status.success() {
        return Err(format!("cannot read historical source {path}"));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("historical source {path} is not UTF-8"))
}
fn source_blob(repo: &Path, commit: &str, path: &str) -> Result<String, String> {
    let output = git(repo, &["ls-tree", commit, "--", path]).ok_or("cannot inspect source mode")?;
    if !output.status.success()
        || !String::from_utf8_lossy(&output.stdout).starts_with("100644 blob ")
    {
        return Err(format!(
            "historical source {path} must be a regular non-executable file"
        ));
    }
    blob(repo, commit, path)
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
fn historical(repo: &Path, commit: &str, surface: &str) -> Result<Option<String>, String> {
    ensure_ungrafted(repo)?;
    let head = oid(repo, "HEAD").ok_or("cannot resolve current commit")?;
    if commit == head || oid(repo, SOURCE_ROOT).is_none() {
        return Ok(None);
    }
    if !ancestor(repo, SOURCE_ROOT, &head)? {
        return Ok(None);
    }
    if !ancestor(repo, SOURCE_ROOT, commit)? || !ancestor(repo, commit, &head)? {
        return Err("historical commit is outside the Decapod source lineage".into());
    }
    // Neither origin text nor copied Cargo/assets files establish this identity.
    // The actual root commit and immutable ancestry are required first.
    let release = version(&source_blob(repo, commit, "Cargo.toml")?)?;
    let body = payload(&source_blob(repo, commit, ASSETS)?, surface)?;
    let hash = super::fingerprint_for_payload(surface, &release, &body);
    Ok(Some(format!(
        "<!-- decapod-release: {release} -->\n<!-- decapod-fingerprint: {hash} -->\n{body}"
    )))
}
fn normalize(content: &str) -> String {
    format!("{}\n", content.trim_end_matches(['\r', '\n']))
}

pub(crate) fn commit_path_violation(
    repo: &Path,
    commit: &str,
    status: char,
    surface: &str,
) -> Option<String> {
    if !super::ENTRYPOINT_FILES.contains(&surface) {
        return None;
    }
    if let Err(error) = ensure_ungrafted(repo) {
        return Some(error);
    }
    if status == 'D' {
        return Some(format!(
            "{commit}: deleted Decapod-owned entrypoint {surface} (not allowed)"
        ));
    }
    let Some(commit) = oid(repo, commit) else {
        return Some("unable to resolve entrypoint commit".into());
    };
    let content = match blob(repo, &commit, surface) {
        Ok(content) => content,
        Err(error) => return Some(format!("{commit}: {error}")),
    };
    if blob(repo, &format!("{commit}^"), surface).ok().as_deref() == Some(&content) {
        return Some(format!(
            "{commit}: non-content touch of entrypoint {surface} (mode/metadata-only changes are forbidden; leave Decapod-owned entrypoints alone)"
        ));
    }
    let historical = match historical(repo, &commit, surface) {
        Ok(canonical) => canonical,
        Err(error) => {
            return Some(format!(
                "{commit}: cannot verify historical entrypoint {surface}: {error}"
            ));
        }
    };
    let canonical = historical
        .clone()
        .or_else(|| super::render_entrypoint(surface))?;
    if normalize(&content) == normalize(&canonical) {
        return None;
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
            return None;
        }
    }
    Some(format!(
        "{commit}: entrypoint {surface} is not its canonical Decapod template (hand edit or stale body). Regenerate via Decapod; do not hand-edit."
    ))
}

#[cfg(test)]
#[path = "history/tests.rs"]
mod tests;
