//! Publication proof is read from the exact push target and GitHub PR, never
//! inferred from a local tracking branch or the exit status of `git push`.
use super::*;
use std::collections::BTreeSet;

pub(super) const NETWORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone)]
pub(super) struct RemoteProof {
    head: String,
    base: String,
    paths: BTreeSet<String>,
}

fn failure(message: impl Into<String>) -> DecapodError {
    DecapodError::ValidationError(message.into())
}

pub(super) fn after_push(error: DecapodError) -> DecapodError {
    failure(format!(
        "PUBLICATION_INCOMPLETE: branch was pushed, but publication is not verified. {error}\nRepair the reported condition and retry `decapod workspace publish`; existing matching PRs are reused. Do not force-push, delete the branch, or report the PR ready."
    ))
}

/// Never echo transport output: credentials can occur in arbitrary diagnostic
/// text as well as URL userinfo/query strings.
fn run(repo: &Path, program: &str, args: &[&str], action: &str) -> Result<Vec<u8>, DecapodError> {
    let output = Command::new(program).current_dir(repo).args(args).bounded_output(NETWORK_TIMEOUT)
        .map_err(|_| failure(format!("{action} did not complete within its deadline or could not start. Remote mutations may already have occurred; reconcile the exact remote state before retrying. Check {program} installation, authentication and network availability.")))?;
    if !output.status.success() {
        return Err(failure(format!(
            "{action} failed. Check {program} authentication, target access, and network availability, then retry. Raw transport diagnostics are withheld to protect credentials."
        )));
    }
    Ok(output.stdout)
}

pub(super) fn redact_remote(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
        format!("{scheme}://{rest}")
    } else {
        url.to_string()
    }
}

fn text(bytes: Vec<u8>) -> Result<String, DecapodError> {
    String::from_utf8(bytes).map_err(|_| failure("Publication proof contained invalid UTF-8."))
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn remote_heads(
    repo: &Path,
    target: &str,
    branch: &str,
    base: &str,
) -> Result<(String, String), DecapodError> {
    let head_ref = format!("refs/heads/{branch}");
    let base_ref = format!("refs/heads/{base}");
    let output = text(run(
        repo,
        "git",
        &["ls-remote", "--refs", "--", target, &head_ref, &base_ref],
        "Remote branch readback",
    )?)?;
    let mut head = None;
    let mut base_oid = None;
    for line in output.lines() {
        let Some((sha, reference)) = line.split_once('\t') else {
            continue;
        };
        if !oid(sha) {
            return Err(failure("Remote readback returned an invalid object ID."));
        }
        if reference == head_ref {
            head = Some(sha.to_string());
        }
        if reference == base_ref {
            base_oid = Some(sha.to_string());
        }
    }
    Ok((head.ok_or_else(|| failure("Pushed branch is absent from the exact push destination."))?,
        base_oid.ok_or_else(|| failure("Requested PR base is absent from the exact push destination. Verify the remote and base branch."))?))
}

pub(super) fn verify_remote_unchanged(
    repo: &Path,
    target: &str,
    branch: &str,
    base: &str,
    proof: &RemoteProof,
) -> Result<(), DecapodError> {
    let (head, base) = remote_heads(repo, target, branch, base)?;
    if head != proof.head || base != proof.base {
        return Err(failure(
            "REMOTE_PUBLICATION_RACE: remote head or base changed during verification; fetch, reconcile, validate, and retry.",
        ));
    }
    Ok(())
}

fn changed_paths(repo: &Path, base: &str, head: &str) -> Result<BTreeSet<String>, DecapodError> {
    let bytes = run(
        repo,
        "git",
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            &format!("{base}...{head}"),
            "--",
        ],
        "Remote commit diff inspection",
    )?;
    let output = text(bytes)?;
    Ok(output
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

fn require_bundle_paths(paths: &BTreeSet<String>) -> Result<(), DecapodError> {
    let missing: Vec<_> = REQUIRED_PR_GOVERNANCE_ARTIFACTS
        .iter()
        .filter(|p| !paths.contains(**p))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(failure(format!(
            "REMOTE_GOVERNANCE_DIFF_MISSING: {}. Refresh through the governed CLI, validate, commit all four artifacts, and retry.",
            missing.join(", ")
        )));
    }
    if !paths
        .iter()
        .any(|path| path.starts_with(".decapod/managed/specs/") && path.ends_with(".md"))
    {
        return Err(failure(
            "REMOTE_SPECS_DIFF_MISSING: commit the reviewed living specs and retry publication.",
        ));
    }
    Ok(())
}

pub(super) fn verify_committed_bundle(repo: &Path, head: &str) -> Result<(), DecapodError> {
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let committed = run(
            repo,
            "git",
            &["show", &format!("{head}:{path}")],
            "Committed governance artifact inspection",
        )?;
        let current = std::fs::read(repo.join(path)).map_err(|_| failure(format!("Missing governance artifact {path}; run `decapod govern artifacts inventory --repair`, then validate.")))?;
        if committed != current {
            return Err(failure(format!(
                "UNCOMMITTED_GOVERNANCE_ARTIFACT: {path} differs from the validated publication commit. Stage, commit, and revalidate before retrying."
            )));
        }
    }
    // A hook or parallel writer must not change HEAD/index/worktree after proof.
    let current = text(run(
        repo,
        "git",
        &["rev-parse", "HEAD"],
        "Local HEAD inspection",
    )?)?;
    if current.trim() != head {
        return Err(failure(
            "LOCAL_PUBLICATION_RACE: HEAD changed during publication.",
        ));
    }
    let status = run(
        repo,
        "git",
        &["status", "--porcelain", "--untracked-files=normal"],
        "Publication worktree inspection",
    )?;
    if !status.is_empty() {
        return Err(failure(
            "UNCOMMITTED_PUBLICATION_CONTENT: worktree or index changed after commit. Review, validate, and retry publication.",
        ));
    }
    Ok(())
}

pub(super) fn verify_remote(
    repo: &Path,
    target: &str,
    branch: &str,
    base_branch: &str,
    expected: &str,
) -> Result<RemoteProof, DecapodError> {
    let (head, base) = remote_heads(repo, target, branch, base_branch)?;
    if head != expected {
        return Err(failure(
            "REMOTE_HEAD_MISMATCH: the exact push destination does not contain the validated commit at the requested branch. Fetch and reconcile without force-pushing.",
        ));
    }
    // Fetch exact immutable IDs from the same endpoint; no shared tracking refs
    // or FETCH_HEAD are trusted. Equality of commit IDs proves complete tree bytes.
    run(
        repo,
        "git",
        &[
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
            target,
            &head,
            &base,
        ],
        "Remote publication object fetch",
    )?;
    verify_committed_bundle(repo, &head)?;
    let paths = changed_paths(repo, &base, &head)?;
    require_bundle_paths(&paths)?;
    let proof = RemoteProof { head, base, paths };
    // Reuse local semantic/material gates against the actual immutable base.
    if !project_specs::material_specs_change_vs_base(repo, &proof.base)?.has_material_change {
        return Err(failure(
            "REMOTE_FINGERPRINT_ONLY_SPECS: actual remote base-to-head diff lacks material authored specs. Review and update the relevant living spec, validate, and retry.",
        ));
    }
    verify_spec_reviews(repo, &proof.base)?;
    verify_remote_unchanged(repo, target, branch, base_branch, &proof)?;
    Ok(proof)
}

/// Require explicit, current-content review instead of demanding cosmetic
/// rewrites of every spec or treating an unrelated spec edit as API review.
pub(super) fn verify_spec_reviews(repo: &Path, base: &str) -> Result<(), DecapodError> {
    let base_ref = if oid(base) {
        base.to_string()
    } else {
        base_ref_for_branch(repo, base)
            .ok_or_else(|| failure("Cannot resolve publication base for spec review."))?
    };
    let changed = changed_paths(repo, &base_ref, "HEAD")?;
    let implementation_changed = changed
        .iter()
        .any(|path| project_specs::is_implementation_path(path));
    if !implementation_changed {
        return Ok(());
    }
    let plan = plan_governance::load_plan(repo)?.ok_or_else(|| failure("SPEC_REVIEW_REQUIRED: initialize the governed plan and review current interfaces, architecture, and security."))?;
    let fingerprint = project_specs::repo_signal_fingerprint(repo)?;
    for path in plan_governance::PUBLICATION_REVIEW_SPECS {
        let body = std::fs::read_to_string(repo.join(path)).map_err(|_| failure(format!("SPEC_REVIEW_REQUIRED: missing {path}. Refresh and explicitly review the current contract.")))?;
        let hash = project_specs::material_spec_body_hash(&body);
        let matching: Vec<_> = plan
            .spec_reviews
            .iter()
            .filter(|review| review.path == *path)
            .collect();
        if matching.len() != 1 {
            return Err(failure(format!(
                "SPEC_REVIEW_REQUIRED: explicitly review {path} against current code; use `decapod govern plan review-spec --path {path} --disposition updated|unchanged-with-reason|requires-decision --reason <review findings>`, then validate. Review does not require unnecessary prose changes."
            )));
        }
        let review = matching[0];
        if review.disposition == plan_governance::SpecReviewDisposition::Updated {
            let base_body = Command::new("git")
                .current_dir(repo)
                .args(["show", &format!("{base_ref}:{path}")])
                .bounded_output(CONTROL_TIMEOUT)
                .map_err(DecapodError::IoError)?;
            if base_body.status.success()
                && project_specs::material_spec_body_hash(&String::from_utf8_lossy(
                    &base_body.stdout,
                )) == hash
            {
                return Err(failure(format!(
                    "SPEC_REVIEW_DISPOSITION_MISMATCH: {path} has no material update. Record an honest unchanged-with-reason review instead."
                )));
            }
        }
        if review.disposition == plan_governance::SpecReviewDisposition::RequiresDecision {
            return Err(failure(format!(
                "SPEC_REVIEW_DECISION_REQUIRED: {path} records unresolved human judgment. Ask the human before publication; record accepted judgment using `decapod govern plan resolve-spec-review --path <path> --decision-ref <human decision reference> --reason <accepted rationale>`. Do not infer approval."
            )));
        }
        if review.spec_material_hash != hash
            || review.reviewed_code_fingerprint != fingerprint
            || review.reason.trim().is_empty()
        {
            return Err(failure(format!(
                "STALE_SPEC_REVIEW: {path} or code changed after review. Re-review current bytes through `decapod govern plan review-spec`, refresh projections, and validate."
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct PrRef {
    #[serde(rename = "ref")]
    branch: String,
    sha: String,
    repo: Option<PrRepo>,
}
#[derive(Debug, Deserialize)]
struct PrRepo {
    full_name: String,
}
#[derive(Debug, Deserialize)]
struct PullRequest {
    number: u64,
    html_url: String,
    state: String,
    head: PrRef,
    base: PrRef,
    changed_files: usize,
}

fn check_pr(
    pr: &PullRequest,
    slug: &str,
    branch: &str,
    base: &str,
    proof: &RemoteProof,
) -> Result<(), DecapodError> {
    if pr.state != "open"
        || pr.head.branch != branch
        || pr.base.branch != base
        || pr.head.sha != proof.head
        || pr.base.sha != proof.base
        || !pr
            .head
            .repo
            .as_ref()
            .is_some_and(|repo| repo.full_name.eq_ignore_ascii_case(slug))
        || !pr
            .base
            .repo
            .as_ref()
            .is_some_and(|repo| repo.full_name.eq_ignore_ascii_case(slug))
        || pr.html_url != format!("https://github.com/{slug}/pull/{}", pr.number)
    {
        return Err(failure(
            "PR_TARGET_MISMATCH: PR repository, open state, head, base, or commit differs from the verified publication. Inspect the PR target, reconcile, and retry.",
        ));
    }
    Ok(())
}

fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DecapodError> {
    serde_json::from_slice(bytes).map_err(|_| failure("GitHub returned invalid publication proof; retry after checking CLI/API compatibility."))
}

fn gh(repo: &Path, args: &[&str]) -> Result<Vec<u8>, DecapodError> {
    run(repo, "gh", args, "GitHub PR publication/readback")
}

fn find_pr(
    call: &mut impl FnMut(&[&str]) -> Result<Vec<u8>, DecapodError>,
    slug: &str,
    branch: &str,
) -> Result<Option<u64>, DecapodError> {
    let owner = slug.split('/').next().unwrap_or_default();
    let prs: Vec<Vec<serde_json::Value>> = json(&call(&[
        "api",
        "--method",
        "GET",
        &format!("repos/{slug}/pulls"),
        "-f",
        "state=open",
        "-f",
        &format!("head={owner}:{branch}"),
        "-f",
        "per_page=100",
        "--paginate",
        "--slurp",
    ])?)?;
    let prs: Vec<_> = prs.into_iter().flatten().collect();
    match prs.as_slice() {
        [] => Ok(None),
        [pr] => pr
            .get("number")
            .and_then(|n| n.as_u64())
            .map(Some)
            .ok_or_else(|| failure("GitHub returned a PR without a valid number.")),
        _ => Err(failure(
            "Multiple open PRs use this head; resolve the intended target before retrying publication.",
        )),
    }
}

pub(super) fn ensure_and_verify_pr(
    repo: &Path,
    slug: &str,
    branch: &str,
    base: &str,
    proof: &RemoteProof,
    title: &str,
    description: &str,
) -> Result<String, DecapodError> {
    ensure_and_verify_pr_with(
        &mut |args| gh(repo, args),
        slug,
        branch,
        base,
        proof,
        title,
        description,
    )
}

fn ensure_and_verify_pr_with(
    call: &mut impl FnMut(&[&str]) -> Result<Vec<u8>, DecapodError>,
    slug: &str,
    branch: &str,
    base: &str,
    proof: &RemoteProof,
    title: &str,
    description: &str,
) -> Result<String, DecapodError> {
    let number = match find_pr(call, slug, branch)? {
        Some(number) => number,
        None => {
            // Retry a failed create only by readback: GitHub may have accepted
            // the request before the connection failed. Never create duplicates.
            let created = call(&[
                "pr",
                "create",
                "--draft",
                "--repo",
                slug,
                "--head",
                branch,
                "--base",
                base,
                "--title",
                title,
                "--body",
                description,
            ]);
            match find_pr(call, slug, branch)? {
                Some(number) => number,
                None => {
                    created?;
                    return Err(failure(
                        "PR_CREATE_UNCONFIRMED: no open PR was found after creation. Branch remains pushed; check GitHub and retry.",
                    ));
                }
            }
        }
    };
    let endpoint = format!("repos/{slug}/pulls/{number}");
    let pr: PullRequest = json(&call(&["api", &endpoint])?)?;
    check_pr(&pr, slug, branch, base, proof)?;
    let pages: Vec<Vec<serde_json::Value>> = json(&call(&[
        "api",
        &format!("{endpoint}/files?per_page=100"),
        "--paginate",
        "--slurp",
    ])?)?;
    let files: Vec<_> = pages.into_iter().flatten().collect();
    let mut paths = BTreeSet::new();
    for file in &files {
        let path = file
            .get("filename")
            .and_then(|v| v.as_str())
            .ok_or_else(|| failure("PR diff readback contained an invalid path."))?;
        paths.insert(path.to_string());
        // GitHub represents renames as one file; local --no-renames lists both.
        if file.get("status").and_then(|v| v.as_str()) == Some("renamed") {
            let old = file
                .get("previous_filename")
                .and_then(|v| v.as_str())
                .ok_or_else(|| failure("PR rename proof omitted its previous path."))?;
            paths.insert(old.to_string());
        }
    }
    if files.len() != pr.changed_files || paths != proof.paths {
        return Err(failure(
            "PR_DIFF_MISMATCH: GitHub's complete PR diff differs from the verified remote commits or was truncated. Do not report the PR ready; fetch, inspect, and retry.",
        ));
    }
    require_bundle_paths(&paths)?;
    let final_pr: PullRequest = json(&call(&["api", &endpoint])?)?;
    check_pr(&final_pr, slug, branch, base, proof)?;
    if final_pr.changed_files != pr.changed_files {
        return Err(failure(
            "PR_PUBLICATION_RACE: PR diff changed during verification; retry.",
        ));
    }
    Ok(final_pr.html_url)
}

#[cfg(test)]
#[path = "../../../tests/unit/core/workspace_publication_tests.rs"]
mod tests;
