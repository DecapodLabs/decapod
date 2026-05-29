use clap::Parser;
use std::collections::HashMap;
use std::process::{Command, Stdio};

#[derive(Parser, Debug)]
#[command(name = "selective-test")]
#[command(about = "Selective test automation: runs tests against changed files only")]
struct Args {
    #[arg(help = "Files to test (comma or space separated)")]
    files: Option<String>,

    #[arg(long, help = "Run all tests")]
    all: bool,

    #[arg(long, help = "Run in reflex mode (post-commit hook style)")]
    reflex: bool,
}

fn get_changed_files() -> Vec<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

    match output {
        Some(out) => out
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let status = parts[0];
                    if status
                        .chars()
                        .any(|c| matches!(c, 'M' | 'A' | 'D' | 'R' | 'C'))
                    {
                        return Some(parts[1].to_string());
                    }
                }
                None
            })
            .collect(),
        None => Vec::new(),
    }
}

fn get_changed_files_from_arg(files: &str) -> Vec<String> {
    files
        .split([',', ' '])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_ignored_path(path: &str) -> bool {
    path.starts_with("target/")
        || path.starts_with("build/")
        || path.starts_with(".git/")
        || path == "Cargo.lock"
        || path == "flake.lock"
        || path.starts_with("docs/")
        || path.starts_with("constitution_embed")
        || path.starts_with(".decapod/")
        || path.starts_with("project/")
        || path.starts_with("tests/fixtures")
        || path.starts_with("tests/golden")
}

fn add_tests_for_file(file: &str, tests_to_run: &mut HashMap<String, bool>) {
    match file {
        "src/core/todo.rs" => {
            tests_to_run.insert("todo_enforcement".to_string(), true);
            tests_to_run.insert("todo_rebuild_compat".to_string(), true);
        }
        "src/core/validate.rs" => {
            tests_to_run.insert("validate_termination".to_string(), true);
            tests_to_run.insert("validate_optional_artifact_gates".to_string(), true);
        }
        "src/core/gatekeeper.rs" => {
            tests_to_run.insert("validate_termination".to_string(), true);
            tests_to_run.insert("validate_optional_artifact_gates".to_string(), true);
        }
        "src/core/workspace.rs" => {
            tests_to_run.insert("workspace_interlock".to_string(), true);
        }
        "src/core/workunit.rs" => {
            tests_to_run.insert("workunit_cli".to_string(), true);
            tests_to_run.insert("workunit_publish_gate".to_string(), true);
        }
        "src/core/obligation.rs" => {
            tests_to_run.insert("obligation".to_string(), true);
        }
        "src/core/docs.rs" => {
            tests_to_run.insert("context_capsule_cli".to_string(), true);
            tests_to_run.insert("context_capsule_rpc".to_string(), true);
            tests_to_run.insert("lcm_determinism".to_string(), true);
        }
        "src/core/context_capsule.rs" => {
            tests_to_run.insert("context_capsule_cli".to_string(), true);
            tests_to_run.insert("context_capsule_rpc".to_string(), true);
            tests_to_run.insert("context_capsule_schema".to_string(), true);
        }
        "src/core/rpc.rs" => {
            tests_to_run.insert("agent_rpc_suite".to_string(), true);
        }
        "src/migration.rs" => {
            tests_to_run.insert("core_tests".to_string(), true);
        }
        "src/lib.rs" => {
            tests_to_run.insert("entrypoint_correctness".to_string(), true);
            tests_to_run.insert("init_config_behavior".to_string(), true);
            tests_to_run.insert("init_validate_green_field".to_string(), true);
        }
        "src/cli.rs" => {
            tests_to_run.insert("cli_contract_enforcement".to_string(), true);
        }
        s if s.starts_with("src/plugins/") => {
            if let Some(rest) = s.strip_prefix("src/plugins/") {
                let plugin_name = rest.strip_suffix(".rs").unwrap_or(rest);
                let test_name = format!("plugins_{plugin_name}_tests");
                tests_to_run.insert(test_name, true);
            }
        }
        "Cargo.toml" | "AGENTS.md" | "CLAUDE.md" | "CODEX.md" | "GEMINI.md"
        | "constitution.json" => {
            tests_to_run.insert("entrypoint_correctness".to_string(), true);
            tests_to_run.insert("cli_contract_enforcement".to_string(), true);
        }
        s if s.ends_with(".sql") => {
            tests_to_run.insert("core_tests".to_string(), true);
        }
        s if s.starts_with("src/core/") && s.ends_with(".rs") => {
            if let Some(rest) = s.strip_prefix("src/core/") {
                let module = rest.strip_suffix(".rs").unwrap_or(rest);
                match module {
                    "todo" => {
                        tests_to_run.insert("todo_enforcement".to_string(), true);
                        tests_to_run.insert("todo_rebuild_compat".to_string(), true);
                    }
                    "validate" => {
                        tests_to_run.insert("validate_termination".to_string(), true);
                        tests_to_run.insert("validate_optional_artifact_gates".to_string(), true);
                    }
                    "gatekeeper" => {
                        tests_to_run.insert("validate_termination".to_string(), true);
                    }
                    "workspace" => {
                        tests_to_run.insert("workspace_interlock".to_string(), true);
                    }
                    "workunit" => {
                        tests_to_run.insert("workunit_cli".to_string(), true);
                        tests_to_run.insert("workunit_publish_gate".to_string(), true);
                    }
                    "obligation" => {
                        tests_to_run.insert("obligation".to_string(), true);
                    }
                    "docs" => {
                        tests_to_run.insert("context_capsule_cli".to_string(), true);
                        tests_to_run.insert("lcm_determinism".to_string(), true);
                    }
                    "capsule" => {
                        tests_to_run.insert("context_capsule_cli".to_string(), true);
                        tests_to_run.insert("context_capsule_rpc".to_string(), true);
                    }
                    "rpc" => {
                        tests_to_run.insert("agent_rpc_suite".to_string(), true);
                    }
                    "migration" => {
                        tests_to_run.insert("core_tests".to_string(), true);
                    }
                    "schema" => {
                        tests_to_run.insert("context_capsule_schema".to_string(), true);
                    }
                    _ => {
                        tests_to_run.insert("entrypoint_correctness".to_string(), true);
                    }
                }
            }
        }
        s if s.starts_with("tests/") && s.ends_with(".rs") => {
            if let Some(rest) = s.strip_prefix("tests/") {
                let test_name = rest.strip_suffix(".rs").unwrap_or(rest);
                tests_to_run.insert(test_name.to_string(), true);
            }
        }
        _ => {}
    }
}

fn run_cargo_test(test: &str, threads: &str) -> bool {
    println!(">>> Running: {test}");
    let status = Command::new("cargo")
        .args([
            "test",
            "--all-features",
            "--test",
            test,
            "--",
            "--test-threads",
            threads,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("✓ {test} passed");
            true
        }
        _ => {
            println!("✗ {test} FAILED");
            false
        }
    }
}

fn run_reflex_mode(changed_files: &[String]) -> bool {
    let mut failed = false;

    for file in changed_files {
        match file.as_str() {
            "src/core/todo.rs" => {
                println!("Testing: todo module");
                if !run_cargo_test("todo_enforcement", "2") {
                    failed = true;
                }
            }
            "src/core/validate.rs" => {
                println!("Testing: validate module");
                if !run_cargo_test("validate_termination", "2") {
                    failed = true;
                }
                if !run_cargo_test("validate_optional_artifact_gates", "2") {
                    failed = true;
                }
            }
            s if s.starts_with("src/plugins/") => {
                if let Some(rest) = s.strip_prefix("src/plugins/") {
                    let plugin_name = rest.strip_suffix(".rs").unwrap_or(rest);
                    println!("Testing: plugin {plugin_name}");
                    let test_name = format!("plugins_{plugin_name}_tests");
                    if !run_cargo_test(&test_name, "2") {
                        failed = true;
                    }
                }
            }
            "src/cli.rs" | "src/lib.rs" => {
                println!("Testing: CLI contracts");
                if !run_cargo_test("cli_contract_enforcement", "2") {
                    failed = true;
                }
                if !run_cargo_test("entrypoint_correctness", "2") {
                    failed = true;
                }
            }
            _ => {}
        }
    }

    failed
}

fn all_tests() -> Vec<&'static str> {
    vec![
        "todo_enforcement",
        "validate_termination",
        "workspace_interlock",
        "workunit_cli",
        "context_capsule_cli",
        "agent_rpc_suite",
        "entrypoint_correctness",
        "cli_contract_enforcement",
        "init_config_behavior",
        "init_validate_green_field",
        "plugins_todo_tests",
        "plugins_policy_tests",
        "plugins_health_tests",
        "plugins_aptitude_tests",
        "plugins_internalize_tests",
        "plugins_federation_tests",
        "plugins_decide_tests",
        "plugins_obligation_tests",
    ]
}

fn main() {
    let args = Args::parse();

    let changed_files: Vec<String> = if args.reflex {
        get_changed_files()
    } else if args.all {
        println!("Mode: all tests");
        for test in all_tests() {
            print!("{test} ");
        }
        println!();
        vec!["--all".to_string()]
    } else if let Some(ref files_arg) = args.files {
        get_changed_files_from_arg(files_arg)
    } else {
        get_changed_files()
    };

    if changed_files.is_empty()
        || (changed_files.len() == 1 && changed_files[0] == "--all") && !args.all
    {
        println!("No changed files detected. Use --all to run all tests.");
    }

    println!("Changed files: {changed_files:?}");

    if args.reflex {
        let failed = run_reflex_mode(&changed_files);
        if failed {
            println!("✗ Some tests failed");
            std::process::exit(1);
        } else {
            println!("✓ All affected tests passed");
        }
        return;
    }

    let mut tests_to_run: HashMap<String, bool> = HashMap::new();

    if changed_files.first().map(|s| s.as_str()) == Some("--all") {
        for test in all_tests() {
            tests_to_run.insert(test.to_string(), true);
        }
    } else {
        for file in &changed_files {
            if is_ignored_path(file) {
                continue;
            }
            add_tests_for_file(file, &mut tests_to_run);
        }
    }

    if tests_to_run.is_empty() {
        println!("(none determined - defaulting to entrypoint_correctness)");
        tests_to_run.insert("entrypoint_correctness".to_string(), true);
    }

    println!("\n=== Tests to run ===");
    let targets: Vec<String> = tests_to_run.keys().cloned().collect();
    println!("Target: {}", targets.join(" "));

    println!("\n=== Running selective tests ===");

    let mut failed = false;
    for test in &targets {
        if !run_cargo_test(test, "4") {
            failed = true;
        }
    }

    if failed {
        println!("\n=== Some selective tests failed ===");
        std::process::exit(1);
    } else {
        println!("\n=== All selective tests passed ===");
    }
}
