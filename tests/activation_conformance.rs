//! Clean-checkout activation contract for representative coding-agent hosts.
//!
//! This test does not emulate a model or vendor SDK. It exercises the same
//! repository-native path a host must discover from an entrypoint, and emits a
//! structured observation report when a contract boundary is missed.

use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const AGENT_ID: &str = "activation-fixture-agent";
const INTENT: &str =
    "Update the fixture greeting while preserving the repository governance contract.";

#[derive(Debug, Serialize)]
struct Observation {
    id: &'static str,
    passed: bool,
    evidence: String,
}

#[derive(Debug, Serialize)]
struct ActivationReport {
    schema_version: &'static str,
    fixture: &'static str,
    intent: &'static str,
    observations: Vec<Observation>,
}

fn run_decapod(dir: &Path, args: &[&str], password: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
    command
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_AGENT_ID", AGENT_ID)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1")
        .env("DECAPOD_VALIDATE_TIMEOUT_SECONDS", "30");
    if let Some(password) = password {
        command.env("DECAPOD_SESSION_PASSWORD", password);
    }
    command.output().expect("run decapod")
}

fn run_git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git")
}

fn combined(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn require_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed:\n{}",
        combined(output)
    );
}

fn parse_json(output: &Output, operation: &str) -> Value {
    require_success(output, operation);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{operation} did not return JSON: {error}\n{}",
            combined(output)
        )
    })
}

fn session_password(output: &Output) -> String {
    require_success(output, "session acquire");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::trim))
        .map(ToString::to_string)
        .expect("session acquire should return a password")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let temp = TempDir::new().expect("create fixture repository");
    let root = temp.path().to_path_buf();

    require_success(&run_git(&root, &["init", "-b", "master"]), "git init");
    require_success(
        &run_git(
            &root,
            &["config", "user.name", "Decapod Activation Fixture"],
        ),
        "git config user.name",
    );
    require_success(
        &run_git(&root, &["config", "user.email", "activation@decapod.local"]),
        "git config user.email",
    );
    fs::write(
        root.join("README.md"),
        "# Activation fixture\n\nHello from Decapod.\n",
    )
    .expect("write fixture README");
    fs::write(
        root.join("activation-task.md"),
        "# Activation task\n\nUpdate the fixture greeting while preserving the repository governance contract.\n",
    )
    .expect("write fixture task");
    require_success(
        &run_git(&root, &["add", "README.md", "activation-task.md"]),
        "git add fixture",
    );
    require_success(
        &run_git(&root, &["commit", "-m", "fixture task"]),
        "git commit fixture",
    );

    require_success(
        &run_decapod(&root, &["init", "--force"], None),
        "decapod init",
    );
    require_success(&run_git(&root, &["add", "-A"]), "git add decapod substrate");
    require_success(
        &run_git(&root, &["commit", "-m", "initialize decapod governance"]),
        "git commit decapod substrate",
    );

    let password = session_password(&run_decapod(&root, &["session", "acquire"], None));
    (temp, root, password)
}

fn rpc(dir: &Path, password: &str, op: &str, params: Value) -> Value {
    parse_json(
        &run_decapod(
            dir,
            &["rpc", "--op", op, "--params", &params.to_string()],
            Some(password),
        ),
        op,
    )
}

#[test]
fn clean_checkout_activation_path_is_bounded_and_proof_gated() {
    let (_temp, root, password) = setup_repo();
    let mut observations = Vec::new();

    let entrypoints = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];
    let required_entrypoint_markers = [
        "decapod docs ingest",
        "decapod rpc --op context.resolve",
        "decapod todo add",
        "decapod workspace ensure",
        "decapod validate",
    ];
    for entrypoint in entrypoints {
        let content = fs::read_to_string(root.join(entrypoint)).expect("read generated entrypoint");
        let missing = required_entrypoint_markers
            .iter()
            .filter(|marker| !content.contains(**marker))
            .copied()
            .collect::<Vec<_>>();
        observations.push(Observation {
            id: "entrypoint_discovery",
            passed: missing.is_empty(),
            evidence: format!("{entrypoint}: missing={missing:?}"),
        });
    }

    let todo = parse_json(
        &run_decapod(
            &root,
            &["todo", "add", INTENT, "--format", "json"],
            Some(&password),
        ),
        "todo add",
    );
    let task_id = todo["id"].as_str().expect("todo id").to_string();
    require_success(
        &run_decapod(
            &root,
            &["todo", "claim", "--id", &task_id, "--format", "json"],
            Some(&password),
        ),
        "todo claim",
    );
    observations.push(Observation {
        id: "task_custody",
        passed: true,
        evidence: format!("claimed todo {task_id}"),
    });

    let orientation = parse_json(
        &run_decapod(
            &root,
            &[
                "infer",
                "orientation",
                "--intent",
                INTENT,
                "--task-id",
                &task_id,
            ],
            Some(&password),
        ),
        "infer orientation",
    );
    let proof_required = orientation["proof_required"]
        .as_array()
        .expect("proof_required array");
    observations.push(Observation {
        id: "orientation_before_mutation",
        passed: proof_required.iter().any(|item| {
            item.as_str()
                .map(|value| value.contains("decapod validate"))
                .unwrap_or(false)
        }),
        evidence: format!("proof_required={proof_required:?}"),
    });

    let workspace = parse_json(
        &run_decapod(
            &root,
            &[
                "workspace",
                "ensure",
                "--branch",
                &format!("agent/{AGENT_ID}/{task_id}"),
            ],
            Some(&password),
        ),
        "workspace ensure",
    );
    let workspace_path =
        PathBuf::from(workspace["worktree_path"].as_str().expect("workspace path"));
    observations.push(Observation {
        id: "workspace_custody",
        passed: workspace["status"] == "ok"
            && workspace_path.starts_with(root.join(".decapod").join("workspaces")),
        evidence: workspace_path.display().to_string(),
    });

    let init = rpc(
        &workspace_path,
        &password,
        "agent.init",
        serde_json::json!({}),
    );
    observations.push(Observation {
        id: "agent_initialization",
        passed: init["success"] == true,
        evidence: init.to_string(),
    });

    let context = rpc(
        &workspace_path,
        &password,
        "context.resolve",
        serde_json::json!({}),
    );
    observations.push(Observation {
        id: "context_resolution",
        passed: context["success"] == true,
        evidence: context.to_string(),
    });

    let workunit_init = run_decapod(
        &workspace_path,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            &task_id,
            "--intent-ref",
            &format!("todo://{task_id}"),
        ],
        Some(&password),
    );
    require_success(&workunit_init, "workunit init");
    require_success(
        &run_decapod(
            &workspace_path,
            &[
                "govern",
                "workunit",
                "set-proof-plan",
                "--task-id",
                &task_id,
                "--gate",
                "validate_passes",
            ],
            Some(&password),
        ),
        "workunit set-proof-plan",
    );

    for (to, operation) in [
        ("executing", "workunit enter"),
        ("claimed", "workunit claim"),
    ] {
        require_success(
            &run_decapod(
                &workspace_path,
                &[
                    "govern",
                    "workunit",
                    "transition",
                    "--task-id",
                    &task_id,
                    "--to",
                    to,
                ],
                Some(&password),
            ),
            operation,
        );
    }

    let premature_verify = run_decapod(
        &workspace_path,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
        Some(&password),
    );
    let premature_output = combined(&premature_verify);
    observations.push(Observation {
        id: "proof_prerequisite_refusal",
        passed: !premature_verify.status.success()
            && (premature_output.contains("missing passing proof result")
                || premature_output.contains("proof result")),
        evidence: premature_output,
    });

    let capsule = rpc(
        &workspace_path,
        &password,
        "context.capsule.query",
        serde_json::json!({
            "topic": "activation",
            "scope": "interfaces",
            "task_id": task_id,
            "limit": 4,
            "write": true
        }),
    );
    observations.push(Observation {
        id: "state_bound_context_capsule",
        passed: capsule["success"] == true
            && workspace_path
                .join(".decapod/generated/context")
                .join(format!("{task_id}.json"))
                .exists(),
        evidence: capsule.to_string(),
    });

    let validate = run_decapod(&workspace_path, &["validate"], Some(&password));
    observations.push(Observation {
        id: "normal_validation",
        passed: validate.status.success(),
        evidence: combined(&validate),
    });
    require_success(
        &run_decapod(
            &workspace_path,
            &[
                "govern",
                "workunit",
                "record-proof",
                "--task-id",
                &task_id,
                "--gate",
                "validate_passes",
                "--status",
                "pass",
                "--artifact",
                "decapod validate",
            ],
            Some(&password),
        ),
        "workunit record-proof",
    );
    require_success(
        &run_decapod(
            &workspace_path,
            &[
                "govern",
                "workunit",
                "transition",
                "--task-id",
                &task_id,
                "--to",
                "verified",
            ],
            Some(&password),
        ),
        "workunit verified transition",
    );
    observations.push(Observation {
        id: "proof_backed_completion",
        passed: true,
        evidence: format!("workunit {task_id} reached VERIFIED after validate_passes"),
    });

    let report = ActivationReport {
        schema_version: "1.0.0",
        fixture: "activation-conformance-v1",
        intent: INTENT,
        observations,
    };
    let failed = report
        .observations
        .iter()
        .filter(|item| !item.passed)
        .count();
    assert_eq!(
        failed,
        0,
        "activation conformance report:\n{}",
        serde_json::to_string_pretty(&report).expect("serialize report")
    );
}
