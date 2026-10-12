//! Sealed read-only API argv validation. The official CLIs own their own HTTP
//! redirect behavior; this seal does not claim their redirects are host-pinned.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderReadKind {
    GithubObjects,
    GithubArtifactMetadata,
    GithubArtifactZip,
    GithubCommitChecks,
    GithubRuns,
    GithubJobs,
    GithubJobLog,
    BuildkiteBuilds,
    BuildkiteJobLog,
}
#[derive(Debug)]
pub struct AuthorizedProviderRead {
    kind: ProviderReadKind,
    url: String,
    scope: String,
}
impl AuthorizedProviderRead {
    pub fn kind(&self) -> ProviderReadKind {
        self.kind
    }
    pub fn method(&self) -> &'static str {
        "GET"
    }
    pub fn initial_url(&self) -> &str {
        &self.url
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    /// Callers must not silently grant a strict host-pinned HTTP policy to a CLI
    /// whose own redirect settings are not controllable by this bounded argv.
    pub fn official_cli_redirects_are_host_pinned(&self) -> bool {
        false
    }
}
fn number(s: &str, max: u64) -> bool {
    !s.starts_with('0')
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u64>().is_ok_and(|n| n > 0 && n <= max)
}
fn decode_segment(s: &str) -> Result<String> {
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let high = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or_else(|| invalid("invalid API path encoding"))?;
            let low = bytes
                .next()
                .and_then(|b| (b as char).to_digit(16))
                .ok_or_else(|| invalid("invalid API path encoding"))?;
            out.push((high * 16 + low) as u8);
        } else {
            ensure(
                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'),
                "unexpected API path character",
            )?;
            out.push(b);
        }
    }
    String::from_utf8(out).map_err(|_| invalid("non-UTF8 API path"))
}
fn source_branch(s: &str) -> bool {
    decode_segment(s).is_ok_and(|s| branch(&format!("refs/heads/{s}")))
}
fn query<'a>(s: &'a str, names: &[&str]) -> Result<Vec<&'a str>> {
    let parts: Vec<_> = s.split('&').collect();
    ensure(parts.len() == names.len(), "unexpected API query fields")?;
    parts
        .iter()
        .zip(names)
        .map(|(part, name)| {
            part.strip_prefix(&format!("{name}="))
                .ok_or_else(|| invalid("unexpected API query order/name"))
        })
        .collect()
}
impl CommandSpec {
    pub fn authorize_provider_read(&self) -> Result<AuthorizedProviderRead> {
        let (path, kind, scope, url) = match self.provider_request() {
            Some(ProviderRequest::GithubGet { path }) => {
                let p: Vec<_> = path.split('/').collect();
                ensure(
                    p.len() >= 5 && p[0] == "repos" && slug(p[1]) && slug(p[2]),
                    "invalid scoped GitHub API path",
                )?;
                let kind = match &p[3..] {
                    ["actions", "artifacts", id] if number(id, u64::MAX) => {
                        ProviderReadKind::GithubArtifactMetadata
                    }
                    ["git", "commits", sha] if oid(sha) => ProviderReadKind::GithubObjects,
                    ["git", "ref", references @ ..]
                        if !references.is_empty()
                            && references
                                .iter()
                                .map(|s| decode_segment(s))
                                .collect::<Result<Vec<_>>>()
                                .is_ok_and(|r| branch(&format!("refs/{}", r.join("/")))) =>
                    {
                        ProviderReadKind::GithubObjects
                    }
                    ["commits", sha, endpoint] if oid(sha) => {
                        let (name, suffix) = endpoint
                            .split_once('?')
                            .ok_or_else(|| invalid("missing commit inventory pagination"))?;
                        let names = if name == "check-runs" {
                            vec!["filter", "per_page", "page"]
                        } else {
                            vec!["per_page", "page"]
                        };
                        ensure(
                            matches!(name, "statuses" | "check-runs"),
                            "unsupported commit inventory",
                        )?;
                        let q = query(suffix, &names)?;
                        let page = if name == "check-runs" {
                            ensure(q[0] == "all" && q[1] == "100", "unbounded checks inventory")?;
                            q[2]
                        } else {
                            ensure(q[0] == "100", "unbounded status inventory")?;
                            q[1]
                        };
                        ensure(number(page, 20), "unbounded commit inventory pages")?;
                        ProviderReadKind::GithubCommitChecks
                    }
                    ["actions", "runs", id] if number(id, u64::MAX) => ProviderReadKind::GithubRuns,
                    ["actions", "workflows", filename]
                        if filename.len() <= 200
                            && filename.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
                            })
                            && !filename.contains("..")
                            && (filename.ends_with(".yml") || filename.ends_with(".yaml")) =>
                    {
                        ProviderReadKind::GithubRuns
                    }
                    ["actions", "runs", run, artifacts] if number(run, u64::MAX) => {
                        let q = query(
                            artifacts
                                .strip_prefix("artifacts?")
                                .ok_or_else(|| invalid("invalid run artifact path"))?,
                            &["per_page", "page"],
                        )?;
                        ensure(
                            q[0] == "100" && number(q[1], 20),
                            "unbounded GitHub artifacts query",
                        )?;
                        ProviderReadKind::GithubArtifactMetadata
                    }
                    ["actions", "workflows", id, runs] if number(id, u64::MAX) => {
                        let q = query(
                            runs.strip_prefix("runs?")
                                .ok_or_else(|| invalid("invalid workflow runs path"))?,
                            &["head_sha", "event", "branch", "per_page", "page"],
                        )?;
                        ensure(
                            oid(q[0])
                                && q[1] == "push"
                                && source_branch(q[2])
                                && q[3] == "100"
                                && number(q[4], 10),
                            "unbounded/mismatched GitHub run query",
                        )?;
                        ProviderReadKind::GithubRuns
                    }
                    ["actions", "runs", run, "attempts", attempt, jobs]
                        if number(run, u64::MAX) && number(attempt, u32::MAX.into()) =>
                    {
                        let q = query(
                            jobs.strip_prefix("jobs?")
                                .ok_or_else(|| invalid("invalid attempt jobs path"))?,
                            &["per_page", "page"],
                        )?;
                        ensure(
                            q[0] == "100" && number(q[1], 20),
                            "unbounded GitHub jobs query",
                        )?;
                        ProviderReadKind::GithubJobs
                    }
                    _ => return Err(invalid("unsupported GitHub read endpoint")),
                };
                (
                    path.clone(),
                    kind,
                    format!("github:{}/{}", p[1], p[2]),
                    format!("https://api.github.com/{path}"),
                )
            }
            Some(ProviderRequest::GithubArtifactZip {
                repository,
                artifact_id,
                expected_bytes,
            }) => {
                ensure(
                    *artifact_id > 0 && *expected_bytes > 0 && *expected_bytes <= 256 * 1024 * 1024,
                    "invalid declared GitHub archive",
                )?;
                let path = format!("{}/actions/artifacts/{artifact_id}/zip", repository.api());
                (
                    path.clone(),
                    ProviderReadKind::GithubArtifactZip,
                    format!("github:{}", repository.full_name()),
                    format!("https://api.github.com/{path}"),
                )
            }
            Some(ProviderRequest::GithubJobLog { repository, job_id }) => {
                ensure(*job_id > 0, "invalid GitHub log job")?;
                let path = format!("{}/actions/jobs/{job_id}/logs", repository.api());
                (
                    path.clone(),
                    ProviderReadKind::GithubJobLog,
                    format!("github:{}", repository.full_name()),
                    format!("https://api.github.com/{path}"),
                )
            }
            Some(ProviderRequest::BuildkiteGet { organization, path }) => {
                ensure(
                    slug(organization) && self.organization.as_deref() == Some(organization),
                    "invalid Buildkite organization",
                )?;
                let p: Vec<_> = path.split('/').collect();
                ensure(
                    p.len() >= 4 && p[0].is_empty() && p[1] == "pipelines" && slug(p[2]),
                    "invalid scoped Buildkite path",
                )?;
                let kind = match &p[3..] {
                    [builds] if builds.starts_with("builds?") => {
                        let q = query(&builds[7..], &["commit", "branch", "per_page", "page"])?;
                        ensure(
                            oid(q[0]) && source_branch(q[1]) && q[2] == "100" && number(q[3], 10),
                            "unbounded/mismatched Buildkite build query",
                        )?;
                        ProviderReadKind::BuildkiteBuilds
                    }
                    ["builds", number_query] => {
                        let (n, q) = number_query
                            .split_once('?')
                            .ok_or_else(|| invalid("missing Buildkite retry policy"))?;
                        ensure(
                            number(n, u64::MAX) && q == "include_retried_jobs=false",
                            "unexpected Buildkite build policy",
                        )?;
                        ProviderReadKind::BuildkiteBuilds
                    }
                    ["builds", n, "jobs", id, "log"] if number(n, u64::MAX) && uuid(id) => {
                        ProviderReadKind::BuildkiteJobLog
                    }
                    _ => return Err(invalid("unsupported Buildkite read endpoint")),
                };
                (
                    path.clone(),
                    kind,
                    format!("buildkite:{organization}/{}", p[2]),
                    format!("https://api.buildkite.com/v2/organizations/{organization}{path}"),
                )
            }
            _ => return Err(invalid("request is not a sealed provider read")),
        };
        let expected: Vec<String> = match kind {
            ProviderReadKind::GithubObjects
            | ProviderReadKind::GithubArtifactMetadata
            | ProviderReadKind::GithubArtifactZip
            | ProviderReadKind::GithubCommitChecks
            | ProviderReadKind::GithubRuns
            | ProviderReadKind::GithubJobs
            | ProviderReadKind::GithubJobLog => {
                ensure(
                    self.program == Program::Github && self.organization.is_none(),
                    "GitHub program mismatch",
                )?;
                [
                    "api",
                    "--hostname",
                    "github.com",
                    "--method",
                    "GET",
                    "--header",
                    "Accept: application/vnd.github+json",
                    &path,
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            }
            ProviderReadKind::BuildkiteBuilds | ProviderReadKind::BuildkiteJobLog => {
                ensure(
                    self.program == Program::Buildkite,
                    "Buildkite program mismatch",
                )?;
                ["api", "--no-input", "--method", "GET", &path]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            }
        };
        ensure(
            self.args == expected,
            "provider argv differs from exact read contract",
        )?;
        Ok(AuthorizedProviderRead { kind, url, scope })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn spec(path: &str) -> CommandSpec {
        CommandSpec {
            program: Program::Github,
            args: [
                "api",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "--header",
                "Accept: application/vnd.github+json",
                path,
            ]
            .map(str::to_owned)
            .to_vec(),
            cwd: "/tmp".into(),
            organization: None,
            provider_request: Some(ProviderRequest::GithubGet { path: path.into() }),
        }
    }
    #[test]
    fn nested_git_refs_are_scoped_and_traversal_is_rejected() {
        for path in [
            "repos/org/repo/git/ref/heads/master",
            "repos/org/repo/git/ref/heads/decapod/preflight/11111111-2222-3333-4444-555555555555",
            "repos/org/repo/git/ref/heads%2Fmaster",
        ] {
            assert_eq!(
                spec(path).authorize_provider_read().unwrap().kind(),
                ProviderReadKind::GithubObjects
            );
        }
        for path in [
            "repos/org/repo/git/ref/heads/../master",
            "repos/org/repo/git/ref/heads/%2e%2e/master",
            "repos/org/repo/git/ref/heads//master",
            "repos/org/repo/git/ref/heads/master?method=DELETE",
            "repos/org/repo/git/ref/heads/master.lock",
            "repos/org/repo/git/ref/heads/%00master",
            "repos/org/repo/git/ref/heads/master@%7B1%7D",
        ] {
            assert!(spec(path).authorize_provider_read().is_err(), "{path}");
        }
    }
    #[test]
    fn exact_gh_get_is_sealed_but_redirect_policy_not_invented() {
        let s = spec("repos/org/repo/actions/runs/17");
        let a = s.authorize_provider_read().unwrap();
        assert_eq!(a.method(), "GET");
        assert!(!a.official_cli_redirects_are_host_pinned());
    }
    #[test]
    fn mismatched_argv_and_unbounded_queries_reject() {
        let mut s = spec("repos/org/repo/actions/runs/17");
        s.args[4] = "POST".into();
        assert!(s.authorize_provider_read().is_err());
        for p in [
            "https://evil.invalid/repos/org/repo/actions/runs/17",
            "repos/org/repo/actions/runs/17/cancel",
            "repos/org/repo/actions/runs/1/attempts/1/jobs?per_page=100&page=99",
        ] {
            assert!(spec(p).authorize_provider_read().is_err());
        }
    }
    #[test]
    fn original_workbranch_run_query_is_bounded_and_strict() {
        let sha = "a".repeat(40);
        for branch in ["work", "agent%2Fworker%2Ftask"] {
            let path = format!(
                "repos/org/repo/actions/workflows/17/runs?head_sha={sha}&event=push&branch={branch}&per_page=100&page=1"
            );
            assert!(spec(&path).authorize_provider_read().is_ok());
        }
        for branch in [
            "%2Fwork",
            "%2E%2E",
            "work%3Fbad",
            "work%00",
            "work.lock",
            "a%2F%2Fb",
        ] {
            let path = format!(
                "repos/org/repo/actions/workflows/17/runs?head_sha={sha}&event=push&branch={branch}&per_page=100&page=1"
            );
            assert!(spec(&path).authorize_provider_read().is_err(), "{branch}");
        }
    }
}
