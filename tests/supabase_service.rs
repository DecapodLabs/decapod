//! Opt-in production-command proof against a real authenticated PostgreSQL
//! service. The external fixture owner creates and destroys a disposable
//! database/namespace, provisions policy, and checks the event ledger. This
//! test never treats the local HTTP fixture as PostgreSQL proof.
#![cfg(feature = "supabase-cloud")]

use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    profile: String,
    endpoint: String,
    repository: String,
    allowed_tokens: [String; 2],
    denials: Vec<Denial>,
    // Explicit assertion by the operator: this is disposable state, with
    // cleanup owned by the harness even if this test fails or is interrupted.
    disposable: bool,
    receipt_path: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Denial {
    case: String,
    token: String,
    repository: String,
    expected_error: String,
}

fn parse_fixture(bytes: &[u8]) -> Result<Fixture, &'static str> {
    let fixture: Fixture =
        serde_json::from_slice(bytes).map_err(|_| "invalid protected fixture schema")?;
    if fixture.allowed_tokens[0] == fixture.allowed_tokens[1] {
        return Err("shared visibility requires independently authenticated clients");
    }
    Ok(fixture)
}

#[test]
fn protected_fixture_errors_do_not_reveal_credential_values() {
    let secret = "synthetic-private-fixture-value";
    let duplicate = json!({
        "profile": "local-postgres",
        "endpoint": "http://127.0.0.1:1",
        "repository": "fixture/proof",
        "allowed_tokens": [secret, secret],
        "denials": [],
        "disposable": true,
        "receipt_path": "/unused/receipt.json"
    });
    let mut malformed = duplicate.clone();
    malformed["allowed_tokens"] = json!(secret);
    for fixture in [duplicate, malformed] {
        let error = parse_fixture(&serde_json::to_vec(&fixture).unwrap())
            .err()
            .expect("invalid fixture must fail");
        assert!(!error.contains(secret));
    }
}

fn project(repository: &str) -> TempDir {
    // Parse through the same public repository identity contract as the CLI.
    let remote = format!("https://github.com/{repository}.git");
    let identity =
        decapod::core::repo_identity::resolve_repository_identity_from_remote(&remote).unwrap();
    assert_eq!(identity.canonical_name, repository);
    let root = tempfile::tempdir().unwrap();
    for args in [vec!["init", "-q"], vec!["remote", "add", "origin", &remote]] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::create_dir_all(root.path().join(".decapod/data")).unwrap();
    std::fs::write(root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n"
    ).unwrap();
    root
}

fn cli(fixture: &Fixture, root: &Path, actor: &str, token: &str, args: &[&str]) -> Command {
    let mut command = Command::new(
        std::fs::canonicalize(env!("CARGO_BIN_EXE_decapod"))
            .expect("resolve decapod binary before changing working directory"),
    );
    command
        .args(["todo"])
        .args(args)
        .args(["--format", "json"])
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("DECAPOD_HEADLESS", "1")
        .env_remove("DECAPOD_CLOUD_AUTH_MODE")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env("DECAPOD_AGENT_ID", actor)
        .env("DECAPOD_CLOUD_DATASTORE", "supabase")
        .env("DECAPOD_PROPODUS_API_URL", &fixture.endpoint)
        .env("DECAPOD_ACCESS_TOKEN", token);
    command
}

fn result(output: Output, fixture: &Fixture) -> Value {
    assert_safe(&output, fixture);
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI JSON result")
}

fn assert_safe(output: &Output, fixture: &Fixture) {
    let bytes = [output.stdout.as_slice(), output.stderr.as_slice()].concat();
    let text = String::from_utf8_lossy(&bytes);
    for token in fixture
        .allowed_tokens
        .iter()
        .chain(fixture.denials.iter().map(|denial| &denial.token))
    {
        assert!(!text.contains(token), "CLI exposed a fixture credential");
    }
}

fn assert_no_fallback(root: &Path) {
    for database in ["decapod.db", "todo.db"] {
        assert!(!root.join(".decapod/data").join(database).exists());
    }
}

fn rejected_transition(fixture: &Fixture, root: &Path, actor: &str, token: &str, args: &[&str]) {
    let output = cli(fixture, root, actor, token, args).output().unwrap();
    assert_safe(&output, fixture);
    assert!(
        !output.status.success(),
        "transition must fail: {}",
        args[0]
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("expected exactly one"),
        "failed transition must expose the zero-row conflict category"
    );
    assert_no_fallback(root);
}

fn run(profile: &str) {
    let path = std::env::var_os("DECAPOD_SUPABASE_FIXTURE").expect(
        "DECAPOD_SUPABASE_FIXTURE must name a protected fixture file; unavailable is not a pass",
    );
    let fixture = parse_fixture(&std::fs::read(path).expect("read protected fixture"))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        fixture.profile == profile,
        "unexpected protected fixture profile"
    );
    assert!(
        fixture.disposable,
        "proof requires externally managed disposable data and cleanup"
    );
    assert!(
        fixture
            .allowed_tokens
            .iter()
            .all(|token| !token.trim().is_empty())
    );
    assert!(
        fixture.receipt_path.is_absolute(),
        "receipt path must belong to external fixture workspace"
    );
    for case in [
        "cross_org",
        "wrong_group",
        "wrong_team",
        "unauthorized_user",
        "insufficient_access",
        "revoked_access",
        "expired_session",
    ] {
        assert!(
            fixture.denials.iter().any(|denial| denial.case == case),
            "missing required denial fixture {case}"
        );
    }
    if profile == "local-postgres" {
        assert!(
            fixture.endpoint.starts_with("http://127.0.0.1:")
                || fixture.endpoint.starts_with("http://[::1]:"),
            "local profile requires a literal loopback service"
        );
    } else {
        assert!(
            fixture.endpoint.starts_with("https://"),
            "hosted proof requires HTTPS"
        );
        assert_eq!(
            std::env::var("DECAPOD_SUPABASE_HOSTED_PROOF").as_deref(),
            Ok("1"),
            "explicit protected hosted opt-in required"
        );
    }
    let first = project(&fixture.repository);
    let second = project(&fixture.repository);
    let one = &fixture.allowed_tokens[0];
    let two = &fixture.allowed_tokens[1];
    let before = result(
        cli(
            &fixture,
            first.path(),
            "proof-one",
            one,
            &["list", "--status", "all"],
        )
        .output()
        .unwrap(),
        &fixture,
    );
    assert!(
        before["items"].as_array().unwrap().is_empty(),
        "fixture must start with fresh empty task state"
    );
    let add = result(
        cli(
            &fixture,
            first.path(),
            "proof-one",
            one,
            &["add", "Supabase disposable command proof"],
        )
        .output()
        .unwrap(),
        &fixture,
    );
    let id = add["id"]
        .as_str()
        .expect("explicit client-generated task ID");
    assert!(!id.is_empty());
    assert_eq!(add["item"]["version"], 1);
    assert_eq!(add["item"]["status"], "open");
    for args in [vec!["get", "--id", id], vec!["show", "--id", id]] {
        let shared = result(
            cli(&fixture, second.path(), "proof-two", two, &args)
                .output()
                .unwrap(),
            &fixture,
        );
        assert_eq!(
            shared["item"], add["item"],
            "second principal sees the same normalized row"
        );
    }
    let shared_list = result(
        cli(
            &fixture,
            second.path(),
            "proof-two",
            two,
            &["list", "--status", "all"],
        )
        .output()
        .unwrap(),
        &fixture,
    );
    assert_eq!(shared_list["items"], json!([add["item"].clone()]));
    for operation in ["get", "show"] {
        let missing = result(
            cli(
                &fixture,
                second.path(),
                "proof-two",
                two,
                &[operation, "--id", "missing-proof-task"],
            )
            .output()
            .unwrap(),
            &fixture,
        );
        assert_eq!(missing["status"], "not_found");
        assert!(missing["item"].is_null());
    }
    for operation in ["claim", "release", "done"] {
        rejected_transition(
            &fixture,
            second.path(),
            "proof-two",
            two,
            &[operation, "--id", "missing-proof-task"],
        );
    }

    // Concurrency uses independent CLI processes and independently authenticated
    // users. Exactly one task/event transition may win.
    let a = cli(
        &fixture,
        first.path(),
        "proof-one",
        one,
        &["claim", "--id", id],
    )
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
    let b = cli(
        &fixture,
        second.path(),
        "proof-two",
        two,
        &["claim", "--id", id],
    )
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
    let a = a.wait_with_output().unwrap();
    let b = b.wait_with_output().unwrap();
    assert_safe(&a, &fixture);
    assert_safe(&b, &fixture);
    assert_ne!(
        a.status.success(),
        b.status.success(),
        "concurrent claim must have exactly one owner"
    );
    let ((root, actor, token), (other_root, other_actor, other_token)) = if a.status.success() {
        (
            (first.path(), "proof-one", one),
            (second.path(), "proof-two", two),
        )
    } else {
        (
            (second.path(), "proof-two", two),
            (first.path(), "proof-one", one),
        )
    };
    // Repository access does not grant ownership of another principal's claim.
    for operation in ["release", "done"] {
        rejected_transition(
            &fixture,
            other_root,
            other_actor,
            other_token,
            &[operation, "--id", id],
        );
    }
    let stale = cli(&fixture, root, actor, token, &["claim", "--id", id])
        .output()
        .unwrap();
    assert_safe(&stale, &fixture);
    assert!(
        !stale.status.success(),
        "retry is a conflict, not an idempotent success claim"
    );
    let claimed = result(
        cli(&fixture, root, actor, token, &["get", "--id", id])
            .output()
            .unwrap(),
        &fixture,
    );
    assert_eq!(claimed["item"]["assignee"], actor);
    assert_eq!(claimed["item"]["version"], 2);
    let released = result(
        cli(&fixture, root, actor, token, &["release", "--id", id])
            .output()
            .unwrap(),
        &fixture,
    );
    assert_eq!(released["item"]["status"], "open");
    assert_eq!(released["item"]["version"], 3);
    result(
        cli(&fixture, root, actor, token, &["claim", "--id", id])
            .output()
            .unwrap(),
        &fixture,
    );
    let completed = result(
        cli(&fixture, root, actor, token, &["done", "--id", id])
            .output()
            .unwrap(),
        &fixture,
    );
    assert_eq!(completed["item"]["status"], "completed");
    assert_eq!(completed["item"]["version"], 5);
    // Observe after failed retries instead of interpreting them as idempotent
    // successes. The fixture owner still requires exactly five ledger events.
    for operation in ["claim", "release", "done"] {
        rejected_transition(&fixture, root, actor, token, &[operation, "--id", id]);
    }
    let observed = result(
        cli(
            &fixture,
            other_root,
            other_actor,
            other_token,
            &["get", "--id", id],
        )
        .output()
        .unwrap(),
        &fixture,
    );
    assert_eq!(observed["item"], completed["item"]);
    let validated = cli(
        &fixture,
        root,
        actor,
        token,
        &["done", "--id", id, "--validated"],
    )
    .output()
    .unwrap();
    assert!(
        !validated.status.success(),
        "remote proof-capture remains unsupported"
    );
    assert_safe(&validated, &fixture);

    for denial in &fixture.denials {
        let denied_repo = project(&denial.repository);
        let mut operations = vec![
            vec!["add", "denied mutation sentinel"],
            vec!["claim", "--id", id],
            vec!["release", "--id", id],
            vec!["done", "--id", id],
        ];
        if denial.case != "insufficient_access" {
            operations.extend([
                vec!["list"],
                vec!["get", "--id", id],
                vec!["show", "--id", id],
            ]);
        }
        for args in operations {
            let denied = cli(
                &fixture,
                denied_repo.path(),
                "proof-denied",
                &denial.token,
                &args,
            )
            .output()
            .unwrap();
            assert_safe(&denied, &fixture);
            assert!(!denied.status.success(), "{} must be denied", denial.case);
            let diagnostic = String::from_utf8_lossy(&denied.stderr);
            assert!(
                diagnostic.contains(&denial.expected_error),
                "{} returned wrong error category",
                denial.case
            );
            assert!(!diagnostic.contains("Supabase disposable command proof"));
            assert!(
                !String::from_utf8_lossy(&denied.stdout).contains(id),
                "denial must not expose protected task rows"
            );
        }
        assert_no_fallback(denied_repo.path());
    }
    let after = result(
        cli(
            &fixture,
            first.path(),
            "proof-one",
            one,
            &["list", "--status", "all"],
        )
        .output()
        .unwrap(),
        &fixture,
    );
    assert_eq!(
        after["items"],
        json!([completed["item"].clone()]),
        "denied mutations or failed claims changed task state"
    );
    for root in [first.path(), second.path()] {
        assert_no_fallback(root);
    }
    // The fixture owner verifies these state/event facts directly in actual
    // PostgreSQL before cleanup. No arbitrary SQL/admin route is added to the
    // client merely to inspect its own proof.
    std::fs::write(&fixture.receipt_path, serde_json::to_vec_pretty(&json!({
        "profile": profile, "task_id": id, "expected_task_count": 1,
        "expected_task_version": 5, "expected_event_types": ["task.add", "task.claim", "task.release", "task.claim", "task.done"],
        "client_result": "passed", "event_ledger_result": "requires_service_verification",
        "cleanup": "owned_by_external_disposable_fixture"
    })).unwrap()).unwrap();
}

#[test]
#[ignore = "requires authenticated disposable PostgreSQL service fixture and external cleanup"]
fn local_postgres_service_contract() {
    run("local-postgres");
}

#[test]
#[ignore = "requires protected opt-in hosted Supabase fixture and external disposable-data cleanup"]
fn protected_hosted_supabase_service_contract() {
    run("hosted-supabase");
}
