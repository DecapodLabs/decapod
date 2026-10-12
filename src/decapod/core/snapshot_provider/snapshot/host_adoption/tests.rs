use super::*;
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn fixture_git(root: &Path, args: &[&str], input: Option<&[u8]>) -> String {
    let mut command = Command::new("/usr/bin/git");
    command
        .current_dir(root)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn local() -> OfficialCli {
    OfficialCli::git_only("/usr/bin/git".into(), |_, _| Ok(())).unwrap()
}
struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
    prior: PreparedSnapshot,
}
impl Fixture {
    fn new(budget: &Budget<'_>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("source");
        let remote = directory.path().join("remote.git");
        std::fs::create_dir(&root).unwrap();
        fixture_git(&root, &["init", "-b", "work"], None);
        std::fs::write(root.join("input"), b"base").unwrap();
        fixture_git(&root, &["add", "input"], None);
        fixture_git(&root, &["commit", "-m", "base"], None);
        let base = fixture_git(&root, &["rev-parse", "HEAD"], None)
            .trim()
            .into();
        std::fs::write(root.join("input"), b"candidate").unwrap();
        fixture_git(&root, &["commit", "-am", "candidate"], None);
        let request = SnapshotRequest {
            repository: Repository::new("DecapodLabs", "decapod").unwrap(),
            request_id: "11111111-2222-3333-4444-555555555555".into(),
            source_ref: "refs/heads/work".into(),
            source_head: fixture_git(&root, &["rev-parse", "HEAD"], None)
                .trim()
                .into(),
            source_tree: fixture_git(&root, &["rev-parse", "HEAD^{tree}"], None)
                .trim()
                .into(),
            accepted_base: base,
            accepted_base_ref: "refs/heads/master".into(),
            tags: BTreeMap::new(),
        };
        let prior = prepare(&mut local(), &root, request, budget).unwrap();
        fixture_git(
            directory.path(),
            &[
                "clone",
                "--bare",
                "--no-hardlinks",
                root.to_str().unwrap(),
                remote.to_str().unwrap(),
            ],
            None,
        );
        Self {
            _directory: directory,
            root,
            remote,
            prior,
        }
    }
    fn server_commit(&self, mutation: &str) -> (String, String) {
        let r = self.prior.request();
        let mut message = r.message().unwrap();
        let mut tree = r.source_tree.clone();
        let mut parents = vec![r.source_head.clone()];
        match mutation {
            "parent" => parents = vec![r.accepted_base.clone()],
            "parents" => parents.push(r.accepted_base.clone()),
            "tree" => {
                tree = fixture_git(
                    &self.remote,
                    &["rev-parse", &format!("{}^{{tree}}", r.accepted_base)],
                    None,
                )
                .trim()
                .into()
            }
            "source" => {
                message = message.replace(
                    &format!("source={}", r.source_head),
                    &format!("source={}", r.accepted_base),
                )
            }
            "base" => {
                message = message.replace(
                    &format!("base={}", r.accepted_base),
                    &format!("base={}", "e".repeat(40)),
                )
            }
            "uuid" => {
                message = message.replace(&r.request_id, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
            }
            "source_ref" => {
                message =
                    message.replace("source_ref=refs/heads/work", "source_ref=refs/heads/other")
            }
            "metadata" => message.push_str("extra=forbidden\n"),
            "missing_lf" => {
                message.pop();
            }
            "" => {}
            _ => panic!("unknown fixture mutation"),
        }
        let raw = format!(
            "tree {tree}\n{}author Host <host@example.invalid> 1234567890 +0000\ncommitter Host <host@example.invalid> 1234567891 +0000\n\n{message}",
            parents
                .iter()
                .map(|p| format!("parent {p}\n"))
                .collect::<String>()
        );
        let id = fixture_git(
            &self.remote,
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(raw.as_bytes()),
        )
        .trim()
        .into();
        (id, raw)
    }
}
struct Host {
    local: OfficialCli,
    remote: PathBuf,
    request: SnapshotRequest,
    server_s: String,
    raw: String,
    fault: &'static str,
    fetches: usize,
    metadata_reads: usize,
}
impl Host {
    fn new(f: &Fixture, s: String, raw: String) -> Self {
        Self {
            local: local(),
            remote: f.remote.clone(),
            request: f.prior.request().clone(),
            server_s: s,
            raw,
            fault: "",
            fetches: 0,
            metadata_reads: 0,
        }
    }
    fn commit(&self, id: &str) -> Value {
        let raw = if id == self.server_s {
            self.raw.clone()
        } else {
            fixture_git(&self.remote, &["cat-file", "commit", id], None)
        };
        let (headers, message) = raw.split_once("\n\n").unwrap();
        json!({"sha":id,"tree":{"sha":headers.lines().find_map(|l| l.strip_prefix("tree ")).unwrap()},"parents":headers.lines().filter_map(|l| l.strip_prefix("parent ")).map(|id| json!({"sha":id})).collect::<Vec<_>>(),"message":message.trim_end_matches('\n')})
    }
}
impl Runner for Host {
    fn execute(&mut self, spec: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
        if spec.program == Program::GitLocal {
            return self.local.execute(spec, b);
        }
        if let Some(ProviderRequest::GithubGet { path }) = spec.provider_request() {
            let prefix = format!("{}/git/", self.request.repository.api());
            let path = path.strip_prefix(&prefix).unwrap();
            let value = if let Some(id) = path.strip_prefix("commits/") {
                self.metadata_reads += 1;
                self.commit(id)
            } else {
                let reference = format!("refs/{}", path.strip_prefix("ref/").unwrap());
                let mut actual_ref = reference.clone();
                let mut id = if reference == self.request.accepted_base_ref {
                    self.request.accepted_base.clone()
                } else {
                    assert_eq!(reference, self.request.transport_ref());
                    self.server_s.clone()
                };
                match self.fault {
                    "ref" if reference == self.request.transport_ref() => {
                        actual_ref = "refs/heads/other".into()
                    }
                    "remote_s" if reference == self.request.transport_ref() => id = "f".repeat(40),
                    "remote_base" if reference == self.request.accepted_base_ref => {
                        id = "f".repeat(40)
                    }
                    "race"
                        if self.metadata_reads > 1 && reference == self.request.transport_ref() =>
                    {
                        id = "f".repeat(40)
                    }
                    _ => {}
                }
                json!({"ref":actual_ref,"object":{"sha":id,"type":"commit"}})
            };
            return Ok(Reply {
                exit_code: 0,
                stdout: serde_json::to_vec(&value).unwrap(),
            });
        }
        assert_eq!(spec.program, Program::GitTransport);
        if spec.args.iter().any(|a| a == "config") {
            return crate::core::snapshot_provider::fixture_config::execute(
                &mut self.local,
                spec,
                b,
            );
        }
        if spec.args.iter().any(|a| a == "ls-remote") {
            let stdout = if spec.args.iter().any(|a| a == "--tags") {
                if self.fault == "tags" {
                    format!("{}\trefs/tags/extra\n", self.request.source_head)
                } else {
                    String::new()
                }
            } else {
                let reference = spec.args.last().unwrap();
                let id = if self.fault == "git_ref" {
                    "f".repeat(40)
                } else {
                    self.server_s.clone()
                };
                format!("{id}\t{reference}\n")
            };
            return Ok(Reply {
                exit_code: 0,
                stdout: stdout.into_bytes(),
            });
        }
        let start = spec.args.iter().position(|a| a == "fetch").unwrap();
        let expected = [
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
            &self.request.repository.https_url(),
            &self.server_s,
        ]
        .map(str::to_string);
        assert_eq!(&spec.args[start..], expected.as_slice());
        if self.fault == "deny_fetch" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture transport authorization denied",
            ));
        }
        self.fetches += 1;
        // Real local Git object-only fetch; only the fixed public URL is swapped
        // for this disposable fixture. No network or credential path is enabled.
        let mut args = spec.args.clone();
        args[start + 10] = self.remote.to_str().unwrap().into();
        fixture_git(
            &spec.cwd,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
        );
        Ok(Reply {
            exit_code: 0,
            stdout: Vec::new(),
        })
    }
}
#[test]
fn adopts_only_exact_published_workspace_head_without_mutation_or_fetch() {
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(30), 1024 * 1024, &c).unwrap();
    let f = Fixture::new(&b);
    let s = f.prior.request().source_head.clone();
    let raw = fixture_git(&f.remote, &["cat-file", "commit", &s], None);
    let refs = fixture_git(&f.root, &["show-ref"], None);
    let index = std::fs::read(f.root.join(".git/index")).unwrap();
    let old = serde_json::to_vec(&f.prior).unwrap();
    let mut host = Host::new(&f, s.clone(), raw);
    let adopted = adopt_host_snapshot(&mut host, &f.prior, &s, &b).unwrap();
    assert_eq!(adopted.transport_head(), s);
    assert_eq!(adopted.request(), f.prior.request());
    assert_eq!(host.fetches, 0);
    assert_eq!(host.metadata_reads, 2);
    assert_eq!(refs, fixture_git(&f.root, &["show-ref"], None));
    assert_eq!(index, std::fs::read(f.root.join(".git/index")).unwrap());
    assert_eq!(old, serde_json::to_vec(&f.prior).unwrap());
}
#[test]
fn wrong_parent_tree_source_base_uuid_ref_and_metadata_never_adopted() {
    for mutation in [
        "parent",
        "parents",
        "tree",
        "source",
        "base",
        "uuid",
        "source_ref",
        "metadata",
        "missing_lf",
    ] {
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(30), 1024 * 1024, &c).unwrap();
        let f = Fixture::new(&b);
        let (s, raw) = f.server_commit(mutation);
        let mut host = Host::new(&f, s.clone(), raw);
        assert!(
            adopt_host_snapshot(&mut host, &f.prior, &s, &b).is_err(),
            "{mutation}"
        );
        assert_eq!(host.fetches, 0, "{mutation}");
    }
}
#[test]
fn wrong_remote_ref_base_tags_and_midflight_race_never_adopted() {
    for fault in ["ref", "remote_s", "remote_base", "git_ref", "tags", "race"] {
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(30), 1024 * 1024, &c).unwrap();
        let f = Fixture::new(&b);
        let s = f.prior.request().source_head.clone();
        let raw = fixture_git(&f.remote, &["cat-file", "commit", &s], None);
        let mut host = Host::new(&f, s.clone(), raw);
        host.fault = fault;
        assert!(
            adopt_host_snapshot(&mut host, &f.prior, &s, &b).is_err(),
            "{fault}"
        );
        assert_eq!(host.fetches, 0, "{fault}");
    }
}
#[test]
fn invalid_locator_dirty_source_and_cancellation_prevent_fetch() {
    let c = Cancellation::default();
    let b = Budget::new(Duration::from_secs(30), 1024 * 1024, &c).unwrap();
    let f = Fixture::new(&b);
    let (s, raw) = f.server_commit("");
    let mut host = Host::new(&f, s.clone(), raw);
    for invalid in [
        "--bad".into(),
        "f".repeat(64),
        "0".repeat(40),
        s.to_uppercase(),
    ] {
        assert!(adopt_host_snapshot(&mut host, &f.prior, &invalid, &b).is_err());
    }
    std::fs::write(f.root.join("input"), b"dirty").unwrap();
    assert!(adopt_host_snapshot(&mut host, &f.prior, &s, &b).is_err());
    c.cancel();
    assert_eq!(
        adopt_host_snapshot(&mut host, &f.prior, &s, &b)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(host.fetches, 0);
    assert_eq!(host.metadata_reads, 0);
}
