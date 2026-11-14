use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_cmd(repo_root: &Path, args: &[&str]) -> Output {
    // Resolve before changing the child cwd; Bazel supplies a runfiles-relative
    // binary while Cargo supplies an absolute executable.
    let exe = Path::new(env!("CARGO_BIN_EXE_decapod"))
        .canonicalize()
        .expect("decapod test binary");
    let output = Command::new(exe)
        .current_dir(repo_root)
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .unwrap_or_else(|e| panic!("failed to run decapod {args:?}: {e}"));
    println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        println!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
    }
    output
}

fn init_git_repo(path: &Path) {
    Command::new("git")
        .current_dir(path)
        .args(["init"])
        .output()
        .expect("failed to init git repo");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("failed to config git email");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.name", "test"])
        .output()
        .expect("failed to config git name");
}

fn extract_json(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).expect("Failed to parse JSON")
}

#[test]
fn test_infer_orientation_simple_task() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);
    run_cmd(root, &["init", "--force"]);

    // Create a dummy project structure
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }",
    )
    .unwrap();

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "add documentation to lib.rs",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert_eq!(json["user_goal"], "add documentation to lib.rs");
    assert_eq!(json["governed_plan"]["status"], "missing");
    assert!(json["decision_gates"].as_array().unwrap().is_empty());
    assert!(json["next_action"].as_str().unwrap().contains("research"));
    assert!(
        json.get("reflection").is_none(),
        "legacy orientation stays stateless"
    );
}

#[test]
fn test_orientation_and_mentor_share_the_reflection_continuation() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init_git_repo(root);
    assert!(run_cmd(root, &["init", "--force"]).status.success());
    assert!(run_cmd(root, &["session", "acquire"]).status.success());
    let added = run_cmd(root, &["todo", "add", "Accelerate fresh reads"]);
    let task = extract_json(&added)["id"].as_str().unwrap().to_string();
    assert!(
        run_cmd(root, &["todo", "claim", "--id", &task])
            .status
            .success()
    );
    assert!(
        run_cmd(
            root,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Fresh reads",
                "--intent",
                "Preserve concurrent mutation visibility",
                "--todo-id",
                &task,
                "--proof-hook",
                "visibility regression",
                "--forbidden-path",
                "src/mutations.rs"
            ]
        )
        .status
        .success()
    );
    assert!(
        run_cmd(root, &["govern", "plan", "approve"])
            .status
            .success()
    );
    let first = serde_json::json!({
        "task_id": task, "submission_id":"initial", "op":"change",
        "touched_paths":["src/mutations.rs","src/read_adapter.rs"],
        "rationale":"Cache at the mutation source"
    });
    let unbound = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "refactor the core interface",
            "--reflection",
            &first.to_string(),
        ],
    );
    assert!(!unbound.status.success());
    assert!(String::from_utf8_lossy(&unbound.stderr).contains("REFLECTION_TASK_REQUIRED"));
    let start = std::time::Instant::now();
    let baseline = run_cmd(root, &["infer", "orientation", "--task-id", &task]);
    let baseline_us = start.elapsed().as_micros();
    assert!(baseline.status.success());
    assert!(extract_json(&baseline).get("reflection").is_none());
    let start = std::time::Instant::now();
    let orientation = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task,
            "--reflection",
            &first.to_string(),
        ],
    );
    let reflection_us = start.elapsed().as_micros();
    assert!(orientation.status.success());
    let first = extract_json(&orientation)["reflection"].clone();
    assert_eq!(first["disposition"], "reflect");
    assert_eq!(
        first["round"], 1,
        "rejected outer binding must not consume a round"
    );
    assert_eq!(first["execution_blocked"], true);
    let revision = serde_json::json!({"reflection": {
        "task_id":task, "submission_id":"adapter-only", "expected_token":first["token"],
        "op":"change", "touched_paths":["src/read_adapter.rs"],
        "evidence":first["challenge"]["sources"],
        "rationale":"Keep the mutation source unchanged and independently test fresh reads"
    }});
    let rpc = run_cmd(
        root,
        &[
            "rpc",
            "--op",
            "mentor.obligations",
            "--params",
            &revision.to_string(),
        ],
    );
    assert!(rpc.status.success());
    let rpc = extract_json(&rpc);
    assert_eq!(rpc["result"]["reflection"]["disposition"], "proceed");
    assert_eq!(rpc["result"]["reflection"]["completion_proven"], false);
    assert_eq!(rpc["result"]["reflection"]["round"], 2);
    assert!(
        rpc.get("blocked_by")
            .is_none_or(|blockers| blockers.as_array().is_some_and(Vec::is_empty))
    );
    writeln!(
        std::io::stderr().lock(),
        "reflection_eval {}",
        serde_json::json!({"case":"cli_orientation", "baseline_latency_us":baseline_us,
            "reflection_latency_us":reflection_us, "baseline_payload_bytes":baseline.stdout.len(),
            "reflection_payload_bytes":orientation.stdout.len()})
    )
    .unwrap();
}

#[test]
fn deterministic_escalation_harness_investigates_before_asking_and_preserves_publication_gate() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init_git_repo(root);
    assert!(run_cmd(root, &["init", "--force"]).status.success());
    assert!(run_cmd(root, &["session", "acquire"]).status.success());
    fs::write(
        root.join("README.md"),
        "The existing adapter uses fresh reads.\n",
    )
    .unwrap();
    let added = run_cmd(root, &["todo", "add", "Choose the adapter architecture"]);
    let task = extract_json(&added)["id"].as_str().unwrap().to_string();
    assert!(
        run_cmd(root, &["todo", "claim", "--id", &task])
            .status
            .success()
    );
    let intent = "Preserve concurrent read visibility";
    assert!(
        run_cmd(
            root,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Read convention",
                "--intent",
                intent,
                "--todo-id",
                &task
            ]
        )
        .status
        .success()
    );
    assert!(
        run_cmd(root, &["govern", "plan", "approve"])
            .status
            .success()
    );
    let mut request = serde_json::json!({
        "task_id":task, "submission_id":"question", "op":"escalate", "touched_paths":[],
        "evidence":[{"ref_path":"README.md"}],
        "escalation":{"intent":intent, "question":"Which read convention should I choose?",
            "missing":"Choose the repository read convention", "uncertainty":"inferable_decision",
            "investigation":[], "risks":[], "constraints":[]}
    });
    let start = std::time::Instant::now();
    let first = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task,
            "--reflection",
            &request.to_string(),
        ],
    );
    assert!(first.status.success());
    let packet = extract_json(&first);
    assert!(packet["decision_gates"].as_array().unwrap().is_empty());
    let first = packet["reflection"].clone();
    assert_eq!(first["disposition"], "reflect");
    assert_eq!(first["escalation"]["human_escalation_permitted"], false);
    assert_eq!(first["challenge"]["sources"][0]["ref_path"], "README.md");
    let retry = run_cmd(
        root,
        &[
            "rpc",
            "--op",
            "mentor.obligations",
            "--params",
            &serde_json::json!({"reflection":request}).to_string(),
        ],
    );
    assert!(retry.status.success());
    assert_eq!(extract_json(&retry)["result"]["reflection"], first);
    // The deterministic mock agent actually inspects the repository source.
    let source = fs::read_to_string(root.join("README.md")).unwrap();
    assert!(source.contains("fresh reads"));
    request["submission_id"] = serde_json::json!("decision");
    request["expected_token"] = first["token"].clone();
    request["escalation"]["resolution"] = serde_json::json!({
        "decision":"Retain the established fresh-read convention",
        "facts":[{"source":{"ref_path":"README.md"}, "literal":"The existing adapter uses fresh reads."}]
    });
    let improved = run_cmd(
        root,
        &[
            "rpc",
            "--op",
            "mentor.obligations",
            "--params",
            &serde_json::json!({"reflection":request}).to_string(),
        ],
    );
    assert!(improved.status.success());
    let improved = extract_json(&improved)["result"]["reflection"].clone();
    assert_eq!(improved["disposition"], "proceed");
    assert_eq!(improved["progress"], "reduced_obligations");
    assert_eq!(improved["round"], 2);
    assert_eq!(improved["escalation"]["semantic_exhaustion_proven"], false);
    assert_eq!(improved["completion_proven"], false);
    // The helper's diagnostic Git exemption cannot become publication proof.
    let readiness = run_cmd(root, &["validate"]);
    assert!(!readiness.status.success());
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&readiness.stdout),
        String::from_utf8_lossy(&readiness.stderr)
    );
    assert!(
        diagnostic.contains("DIAGNOSTIC_OVERRIDES_ACTIVE"),
        "{diagnostic}"
    );
    assert!(
        run_cmd(
            root,
            &[
                "govern",
                "plan",
                "update",
                "--question",
                "Which external contractual retention requirement applies?"
            ]
        )
        .status
        .success()
    );
    request["submission_id"] = serde_json::json!("business-dependency");
    request["escalation"]["question"] =
        serde_json::json!("Which external contractual retention requirement applies?");
    request["expected_token"] = improved["token"].clone();
    let human = run_cmd(
        root,
        &[
            "rpc",
            "--op",
            "mentor.obligations",
            "--params",
            &serde_json::json!({"reflection":request}).to_string(),
        ],
    );
    assert!(human.status.success());
    let human_response = extract_json(&human);
    assert!(
        human_response
            .get("allowed_next_ops")
            .is_none_or(|ops| ops.as_array().is_some_and(Vec::is_empty))
    );
    let human = human_response["result"]["reflection"].clone();
    assert_eq!(human["disposition"], "human_decision");
    assert_eq!(human["escalation"]["human_escalation_permitted"], true);
    assert_eq!(human["execution_blocked"], true);
    writeln!(
        std::io::stderr().lock(),
        "escalation_harness {}",
        serde_json::json!({
            "routine_escalation_attempts":1,"unnecessary_questions_admitted":0,
            "correct_human_dependencies_preserved":1,"convergence_rounds":2,
            "elapsed_us":start.elapsed().as_micros(),"semantic_improvement_proven":false
        })
    )
    .unwrap();
}

#[test]
fn architecture_keywords_require_agent_investigation_without_manufacturing_human_authority() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);
    run_cmd(root, &["init", "--force"]);

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "refactor the core rpc interface",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert!(json["decision_gates"].as_array().unwrap().is_empty());
    assert!(json["constraints"].as_array().unwrap().iter().any(|c| {
        c.as_str()
            .is_some_and(|s| s.contains("Inspect governing intent"))
    }));
    assert!(!json["next_action"].as_str().unwrap().contains("STOP"));
}

#[test]
fn generated_agent_entrypoints_route_to_the_embedded_escalation_contract() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init_git_repo(root);
    assert!(
        run_cmd(root, &["init", "--force", "--all", "--proof"])
            .status
            .success()
    );
    for name in ["AGENTS.md", "CODEX.md", "CLAUDE.md", "GEMINI.md"] {
        let generated = fs::read_to_string(root.join(name)).unwrap();
        assert!(generated.contains("decapod docs ingest"), "{name}");
        assert!(generated.contains("decapod infer orientation"), "{name}");
    }
    // The shared generated entrypoints require ingestion; the build-time docs
    // owner embeds this canonical API corpus for every supported agent.
    let api = run_cmd(root, &["docs", "show", "docs/agent/api-index.md"]);
    assert!(api.status.success());
    let embedded = String::from_utf8_lossy(&api.stdout);
    assert!(embedded.contains("Do What Must Be Done"));
    assert!(embedded.contains("reflection.escalation"));
    assert!(embedded.contains("human_escalation_permitted"));
}

#[test]
fn test_infer_orientation_with_task_id() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);

    // Setup decapod project
    run_cmd(root, &["init", "--force"]);
    run_cmd(root, &["session", "acquire"]);

    // Create a task
    let add_output = run_cmd(
        root,
        &[
            "todo",
            "add",
            "fix the broken tests",
            "--scope",
            "tests/core",
        ],
    );
    assert!(add_output.status.success());

    let list_output = run_cmd(root, &["todo", "list", "--format", "json"]);
    assert!(list_output.status.success());
    let list_json = extract_json(&list_output);
    println!("LIST JSON: {list_json}");
    let task_id = list_json["items"]
        .as_array()
        .expect("items should be an array")[0]["id"]
        .as_str()
        .expect("Task should have an ID");

    let output = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--task-id",
            task_id,
            "--format",
            "json",
        ],
    );
    assert!(output.status.success());

    let json = extract_json(&output);
    assert_eq!(json["user_goal"], "fix the broken tests");
    assert_eq!(json["task_id"].as_str().unwrap(), task_id);
    assert!(
        json["allowed_scope"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "tests/core")
    );
    assert!(
        json["proof_required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("Reproduction"))
    );
}

#[test]
fn test_infer_surfaces_governed_plan_context_before_mutation() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    init_git_repo(root);
    run_cmd(root, &["init", "--force"]);
    run_cmd(root, &["session", "acquire"]);

    run_cmd(
        root,
        &["todo", "add", "integrate plan context into inference"],
    );
    let todos = run_cmd(root, &["todo", "list", "--format", "json"]);
    assert!(todos.status.success());
    let todo_id = extract_json(&todos)["items"][0]["id"]
        .as_str()
        .expect("todo id")
        .to_string();

    let plan = run_cmd(
        root,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "Inference plan",
            "--intent",
            "Expose governed plan state before implementation",
            "--todo-id",
            &todo_id,
            "--proof-hook",
            "cargo test",
            "--unknown",
            "Whether the plan needs another phase",
        ],
    );
    assert!(
        plan.status.success(),
        "plan init failed: {}",
        String::from_utf8_lossy(&plan.stderr)
    );

    let orientation = run_cmd(
        root,
        &[
            "infer",
            "orientation",
            "--intent",
            "integrate plan context into inference",
            "--task-id",
            &todo_id,
            "--format",
            "json",
        ],
    );
    assert!(orientation.status.success());
    let orientation_json = extract_json(&orientation);
    assert_eq!(orientation_json["governed_plan"]["status"], "present");
    assert_eq!(orientation_json["governed_plan"]["state"], "DRAFT");
    assert_eq!(orientation_json["governed_plan"]["task_binding"], "bound");
    assert_eq!(
        orientation_json["governed_plan"]["proof_hooks"][0],
        "cargo test"
    );
    assert!(
        orientation_json["known_unknowns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.as_str().unwrap().contains("another phase"))
    );

    let inference = run_cmd(
        root,
        &[
            "infer",
            "init",
            "--intent",
            "integrate plan context into inference",
            "--format",
            "json",
        ],
    );
    assert!(inference.status.success());
    let inference_json = extract_json(&inference);
    assert_eq!(inference_json["governed_plan"]["status"], "present");
    assert_eq!(
        inference_json["intent_convergence"]["status"],
        "needs_human_convergence"
    );
    assert_eq!(inference_json["intent_sources"][1], "governed-plan");
    assert_eq!(
        inference_json["plan_intent"],
        "Expose governed plan state before implementation"
    );
    assert!(
        inference_json["selected_context"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == ".decapod/governance.json#/sections/plan")
    );
    assert!(
        inference_json["selected_policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|policy| policy == "governed-plan")
    );
}
