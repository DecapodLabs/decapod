//! Immutable, credential-free documentation drift proof shared by PR and pre-PR CI.
//! This std-only tool also builds with rustc, before compiling the candidate crate.
//! Inputs are captured event identities, never branch names. No remote is contacted.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

const GENERATED_DOCS: [&str; 2] = [
    "docs/agent/command-contracts.md",
    "docs/agent/config-schema.md",
];
type Check<T> = Result<T, String>;

fn git(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(repo).args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "submodule.recurse=false",
    ]);
    command
}

fn output(command: &mut Command, label: &str) -> Check<Output> {
    let result = command
        .output()
        .map_err(|error| format!("{label}: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "{label} failed ({}): {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(result)
}

fn git_text(repo: &Path, args: &[&str]) -> Check<String> {
    let result = output(git(repo).args(args), "local Git identity check")?;
    String::from_utf8(result.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "Git identity is not UTF-8".to_string())
}

fn validate_sha(sha: &str) -> Check<()> {
    if !matches!(sha.len(), 40 | 64)
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || sha.bytes().all(|byte| byte == b'0')
    {
        return Err("base and head must be full, nonzero lowercase commit SHAs".to_string());
    }
    Ok(())
}

fn validate_range(repo: &Path, base: &str, head: &str) -> Check<()> {
    for sha in [base, head] {
        validate_sha(sha)?;
        let actual = git_text(
            repo,
            &["rev-parse", "--verify", &format!("{sha}^{{commit}}")],
        )?;
        if actual != sha {
            return Err("supplied SHA is not the exact commit object".to_string());
        }
    }
    if git_text(repo, &["rev-parse", "--is-shallow-repository"])? != "false" {
        return Err("documentation proof requires the full local Git history".to_string());
    }
    output(
        git(repo).args(["merge-base", base, head]),
        "base/head shared history",
    )?;
    Ok(())
}

fn require_clean_tracked(repo: &Path) -> Check<()> {
    for cached in [false, true] {
        let mut command = git(repo);
        command.args(["diff", "--no-ext-diff", "--no-textconv", "--exit-code"]);
        if cached {
            command.arg("--cached");
        }
        command.args(["HEAD", "--"]);
        output(
            &mut command,
            "tracked input must match HEAD, including the index",
        )?;
    }
    Ok(())
}

fn require_exact_head(repo: &Path, head: &str) -> Check<()> {
    if git_text(repo, &["rev-parse", "--verify", "HEAD^{commit}"])? != head {
        return Err("checked-out HEAD does not match the captured head SHA".to_string());
    }
    let status = git(repo)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(|error| error.to_string())?
        .status;
    if status.code() != Some(1) {
        return Err("documentation proof requires a detached immutable HEAD".to_string());
    }
    Ok(())
}

/// Accept the event head itself, or the exact two-parent event merge that the
/// default PR checkout supplies. A different head/merge must fail, never fetch.
/// The caller owns the provenance of base/head (PR payload or verified pre-PR context).
fn select(repo: &Path, base: &str, head: &str) -> Check<()> {
    validate_range(repo, base, head)?;
    require_clean_tracked(repo)?;
    let current = git_text(repo, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    if current != head {
        let parents = git_text(repo, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
        let expected = format!("{current} {base} {head}");
        if parents != expected {
            return Err(
                "default checkout is neither the captured head nor its exact base/head merge"
                    .to_string(),
            );
        }
    }
    output(
        git(repo).args(["checkout", "--detach", head, "--"]),
        "detach captured head",
    )?;
    require_exact_head(repo, head)
}

#[cfg(unix)]
fn path_argument(bytes: &[u8]) -> Check<OsString> {
    use std::os::unix::ffi::OsStringExt;
    let mut path = b"./".to_vec();
    path.extend_from_slice(bytes);
    Ok(OsString::from_vec(path))
}

#[cfg(not(unix))]
fn path_argument(bytes: &[u8]) -> Check<OsString> {
    let path = std::str::from_utf8(bytes)
        .map_err(|_| "changed filename cannot be represented on this platform".to_string())?;
    Ok(OsString::from(format!("./{path}")))
}

fn changed_paths(repo: &Path, base: &str, head: &str) -> Check<Vec<OsString>> {
    let result = output(
        git(repo).args([
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            base,
            head,
            "--",
        ]),
        "immutable changed paths",
    )?;
    if result.stdout.is_empty() {
        return Ok(Vec::new());
    }
    if result.stdout.last() != Some(&0) {
        return Err("Git returned a non-NUL-terminated path list".to_string());
    }
    result.stdout[..result.stdout.len() - 1]
        .split(|byte| *byte == 0)
        .map(|path| {
            if path.is_empty() {
                return Err("Git returned an empty changed filename".to_string());
            }
            // Prefix prevents option-like paths from becoming CLI flags. The
            // actual CLI owns generation selection; no copy of its rules here.
            path_argument(path)
        })
        .collect()
}

fn verify(repo: &Path, base: &str, head: &str, decapod: &Path) -> Check<()> {
    validate_range(repo, base, head)?;
    require_exact_head(repo, head)?;
    require_clean_tracked(repo)?;
    for path in GENERATED_DOCS {
        output(
            git(repo).args(["cat-file", "-e", &format!("{head}:{path}")]),
            "generated documentation must exist in the captured commit",
        )?;
    }
    let paths = changed_paths(repo, base, head)?;
    let binary = if decapod.is_absolute() {
        decapod.to_path_buf()
    } else {
        repo.join(decapod)
    };
    let binary = binary
        .canonicalize()
        .map_err(|error| format!("docs binary: {error}"))?;
    let mut command = Command::new(binary);
    command.current_dir(repo).args(["docs", "build"]);
    if !paths.is_empty() {
        command.arg("--touched").args(paths);
    }
    // This is deliberately the only non-Git child process: no shell, eval,
    // pipeline, arbitrary command, or argument template is accepted.
    let status = command
        .status()
        .map_err(|error| format!("docs build: {error}"))?;
    if !status.success() {
        return Err(format!("source-built decapod docs build failed ({status})"));
    }
    require_exact_head(repo, head)?;
    for cached in [false, true] {
        let mut command = git(repo);
        command.args(["diff", "--no-ext-diff", "--no-textconv", "--exit-code"]);
        if cached {
            command.arg("--cached");
        }
        command.args(["HEAD", "--"]).args(GENERATED_DOCS);
        output(
            &mut command,
            "generated documentation differs from the reviewed commit",
        )?;
    }
    println!("Generated documentation matches the captured head {head}.");
    Ok(())
}

fn run(args: &[OsString]) -> Check<()> {
    let value = |index: usize| {
        args[index]
            .to_str()
            .ok_or_else(|| "SHA or mode is not UTF-8".to_string())
    };
    if !matches!(args.len(), 8 | 10)
        || args[2] != "--repo"
        || args[4] != "--base"
        || args[6] != "--head"
    {
        return Err("usage: decapod-docs-sync select --repo PATH --base SHA --head SHA | verify --repo PATH --base SHA --head SHA --decapod PATH".to_string());
    }
    let repo = Path::new(&args[3]);
    match value(1)? {
        "select" if args.len() == 8 => select(repo, value(5)?, value(7)?),
        "verify" if args.len() == 10 && args[8] == "--decapod" => {
            verify(repo, value(5)?, value(7)?, Path::new(&args[9]))
        }
        _ => Err("expected select or verify with its exact owned arguments".to_string()),
    }
}

fn main() {
    if let Err(error) = run(&std::env::args_os().collect::<Vec<_>>()) {
        eprintln!("::error::Documentation proof rejected: {error}");
        eprintln!(
            "Run decapod docs build with the source-built binary and review the generated changes."
        );
        eprintln!(
            "Update the governed trajectory and run decapod validate against the final material."
        );
        eprintln!(
            "Stage authored/generated changes, record a unique decapod govern artifacts checkpoint,"
        );
        eprintln!(
            "then stage .decapod/governance.json and create a governed commit before retrying CI."
        );
        std::process::exit(1);
    }
}

#[cfg(all(test, unix))]
#[path = "../../tests/support/docs_sync_cases.rs"]
mod tests;
