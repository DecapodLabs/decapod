//! Fail-closed classification of immutable, version-only Decapod releases.
//!
//! This intentionally accepts the repository's canonical Cargo serialization,
//! not arbitrary TOML. An unfamiliar but valid representation runs normal CI.
//! Keep this dependency-free so CI can compile it with rustc before any build.

use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("git unavailable: {error}"))?;
    if !output.status.success() {
        return Err(format!("git {} failed", args.first().unwrap_or(&"command")));
    }
    String::from_utf8(output.stdout).map_err(|_| "non-UTF-8 Git output".to_string())
}

fn stable_version(value: &str) -> Option<[u64; 3]> {
    let components: Vec<_> = value.split('.').collect();
    if components.len() != 3 {
        return None;
    }
    let mut version = [0; 3];
    for (index, component) in components.iter().enumerate() {
        if component.is_empty()
            || !component.bytes().all(|byte| byte.is_ascii_digit())
            || (component.len() > 1 && component.starts_with('0'))
        {
            return None;
        }
        version[index] = component.parse().ok()?;
    }
    Some(version)
}

fn quoted_version(line: &str) -> Option<&str> {
    let value = line.strip_prefix("version = \"")?.strip_suffix('"')?;
    stable_version(value)?;
    Some(value)
}

fn manifest_version(text: &str) -> Option<&str> {
    let mut lines = text.lines();
    if lines.next()? != "[package]" || lines.next()? != "name = \"decapod\"" {
        return None;
    }
    let next = lines.next()?;
    quoted_version(if next == "default-run = \"decapod\"" {
        lines.next()?
    } else {
        next
    })
}

pub(crate) fn version_only_manifest(before: &str, after: &str) -> Option<(String, String)> {
    let old = manifest_version(before)?;
    let new = manifest_version(after)?;
    if stable_version(new)? <= stable_version(old)? {
        return None;
    }
    let old_line = format!("version = \"{old}\"");
    let new_line = format!("version = \"{new}\"");
    // Only the proven package.version line may change, including whitespace.
    if before.replacen(&old_line, &new_line, 1) != after {
        return None;
    }
    Some((old.to_string(), new.to_string()))
}

fn lock_root_version_line(text: &str, expected: &str) -> Option<usize> {
    // Cargo emits simple strings, not multiline TOML. Reject unusual syntax
    // instead of interpreting a fake package header inside a multiline string.
    if text.contains("\"\"\"") || text.contains("'''") || text.contains('\r') {
        return None;
    }
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_end_matches('\n') != "[[package]]" {
            continue;
        }
        if lines.get(index + 1)?.trim_end_matches('\n') != "name = \"decapod\"" {
            continue;
        }
        if found.is_some()
            || quoted_version(lines.get(index + 2)?.trim_end_matches('\n'))? != expected
        {
            return None;
        }
        let package_end = lines[index + 3..]
            .iter()
            .position(|line| line.trim_end_matches('\n') == "[[package]]")
            .map_or(lines.len(), |offset| index + 3 + offset);
        // The root package cannot be a registry/git dependency or an alias.
        if lines[index + 3..package_end].iter().any(|line| {
            let line = line.trim_start();
            line.starts_with("source")
                || line.starts_with("checksum")
                || line.starts_with("name")
                || line.starts_with("version")
                || line.starts_with('[')
        }) {
            return None;
        }
        found = Some(index + 2);
    }
    found
}

pub(crate) fn version_only_lock(before: &str, after: &str, old: &str, new: &str) -> bool {
    let Some(old_index) = lock_root_version_line(before, old) else {
        return false;
    };
    let Some(new_index) = lock_root_version_line(after, new) else {
        return false;
    };
    if old_index != new_index {
        return false;
    }
    let mut lines: Vec<_> = before.split_inclusive('\n').map(str::to_string).collect();
    let newline = if lines[old_index].ends_with('\n') {
        "\n"
    } else {
        ""
    };
    lines[old_index] = format!("version = \"{new}\"{newline}");
    lines.concat() == after
}

fn is_commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0')
}

/// Resolve a complete immutable CI range without granting a release exemption.
/// Ordinary PRs may execute a real two-parent merge of the supplied base/head;
/// only the release classifier below additionally requires equal candidate trees.
pub fn validation_change_base(
    repo: &Path,
    event: &str,
    base: &str,
    head: &str,
    accepted_base: &str,
) -> Result<String, String> {
    if !matches!(event, "push" | "pull_request")
        || !is_commit_id(base)
        || !is_commit_id(head)
        || base == head
    {
        return Err("expected distinct, nonzero immutable PR/push base and head IDs".into());
    }
    for revision in [base, head, accepted_base] {
        git(repo, &["cat-file", "-e", &format!("{revision}^{{commit}}")])?;
    }
    git(repo, &["merge-base", "--is-ancestor", base, accepted_base])?;
    let checkout = git(repo, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
    let revisions: Vec<_> = checkout.split_whitespace().collect();
    if revisions.first().copied() != Some(head)
        && !(event == "pull_request"
            && revisions.len() == 3
            && revisions[1] == base
            && revisions[2] == head)
    {
        return Err(
            "checkout is neither the immutable head nor its exact base/head PR merge".into(),
        );
    }
    if event == "pull_request" {
        Ok(git(repo, &["merge-base", base, head])?.trim().to_string())
    } else {
        git(repo, &["merge-base", "--is-ancestor", base, head])?;
        Ok(base.to_string())
    }
}

fn regular_blob(repo: &Path, commit: &str, path: &str) -> Result<(), String> {
    let entry = git(repo, &["ls-tree", commit, "--", path])?;
    if !entry.starts_with("100644 blob ") || !entry.ends_with(&format!("\t{path}\n")) {
        return Err(format!("{path} must remain a non-executable regular file"));
    }
    Ok(())
}

/// Return true only for a complete, immutable release-only change range.
/// Push ranges use the event's pre-push SHA, never only HEAD's final parent.
/// PR ranges use the immutable base/head SHAs and their merge base.
pub fn classify(repo: &Path, event: &str, base: &str, head: &str) -> Result<bool, String> {
    if !matches!(event, "push" | "pull_request") {
        return Ok(false);
    }
    if !is_commit_id(base) || !is_commit_id(head) || base == head {
        return Ok(false);
    }
    git(repo, &["cat-file", "-e", &format!("{base}^{{commit}}")])?;
    git(repo, &["cat-file", "-e", &format!("{head}^{{commit}}")])?;
    // PR checkouts may be synthetic merges with newer base code. Only slim a
    // candidate whose tree is exactly the immutable head we classified.
    if git(repo, &["rev-parse", &format!("{head}^{{tree}}")])?
        != git(repo, &["rev-parse", "HEAD^{tree}"])?
    {
        return Ok(false);
    }
    let comparison_base = if event == "pull_request" {
        git(repo, &["merge-base", base, head])?.trim().to_string()
    } else {
        // An unavailable or rewritten history is never evidence of a release.
        git(repo, &["merge-base", "--is-ancestor", base, head])?;
        base.to_string()
    };
    let changed = git(
        repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-status",
            "-z",
            &comparison_base,
            head,
            "--",
        ],
    )?;
    let fields: Vec<_> = changed.split_terminator('\0').collect();
    if fields.len() % 2 != 0 || fields.is_empty() {
        return Ok(false);
    }
    let mut paths = Vec::new();
    for change in fields.chunks_exact(2) {
        let [status, path] = change else {
            unreachable!()
        };
        if *status != "M" || !matches!(*path, "Cargo.toml" | "Cargo.lock" | "CHANGELOG.md") {
            return Ok(false);
        }
        regular_blob(repo, &comparison_base, path)?;
        regular_blob(repo, head, path)?;
        paths.push(*path);
    }
    if !paths.contains(&"Cargo.toml") || !paths.contains(&"Cargo.lock") {
        return Ok(false);
    }
    let blob = |commit: &str, path: &str| git(repo, &["show", &format!("{commit}:{path}")]);
    let Some((old, new)) = version_only_manifest(
        &blob(&comparison_base, "Cargo.toml")?,
        &blob(head, "Cargo.toml")?,
    ) else {
        return Ok(false);
    };
    Ok(version_only_lock(
        &blob(&comparison_base, "Cargo.lock")?,
        &blob(head, "Cargo.lock")?,
        &old,
        &new,
    ))
}
