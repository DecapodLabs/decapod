use decapod::core::docs_cli::trajectory_record_contract;
use decapod::core::trajectory::{
    TrajectoryCheckStatus, TrajectoryGraderResult, TrajectoryLoopStatus, TrajectoryLoopType,
    TrajectoryMutationProposal, TrajectoryTrigger, parse_check_spec, parse_loop_json,
};
use serde::de::{DeserializeOwned, value::StrDeserializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

const CONTRACTS: &str = include_str!("../docs/agent/command-contracts.md");
const EXAMPLES: &str = include_str!("../docs/agent/payload-examples.md");
const SCHEMA: &str = include_str!("../assets/schemas/trajectory.schema.json");
const HEADING: &str = "### `decapod govern trajectory record`";

fn trajectory_section(text: &str) -> &str {
    let start = text.find(HEADING).expect("trajectory record contract");
    text[start..].split("\n## ").next().unwrap().trim()
}

fn example_script() -> &'static str {
    EXAMPLES
        .split("<!-- trajectory-record-example -->")
        .nth(1)
        .expect("marked trajectory example")
        .split("```bash\n")
        .nth(1)
        .expect("bash example")
        .split("\n```")
        .next()
        .unwrap()
}

fn example_loop() -> Value {
    let raw = example_script()
        .split("--loop-json '")
        .nth(1)
        .expect("documented loop input")
        .split('\'')
        .next()
        .unwrap();
    serde_json::from_str(raw).expect("valid documented JSON object")
}

fn strings(values: &Value) -> Vec<String> {
    serde_json::from_value(values.clone()).expect("schema string array")
}

// Capture the variant list supplied by the real derived Deserialize
// implementation, rather than repeating its values in a test or parsing
// human-readable serde error text. Added or removed parser variants must
// update the schema and generated documentation together.
#[derive(Debug)]
struct VariantNames {
    names: &'static [&'static str],
    message: String,
}

impl fmt::Display for VariantNames {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VariantNames {}

impl serde::de::Error for VariantNames {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self {
            names: &[],
            message: message.to_string(),
        }
    }

    fn unknown_variant(variant: &str, expected: &'static [&'static str]) -> Self {
        Self {
            names: expected,
            message: format!("unknown variant {variant}"),
        }
    }
}

fn parser_variants<T: DeserializeOwned + fmt::Debug>() -> Vec<String> {
    let error = T::deserialize(StrDeserializer::<VariantNames>::new(
        "__undocumented_trajectory_value__",
    ))
    .expect_err("unknown enum value must fail");
    assert!(!error.names.is_empty(), "missing enum variants: {error}");
    error.names.iter().map(|name| name.to_string()).collect()
}

fn loop_parser_variants(field: &str) -> Vec<String> {
    match field {
        "loop_type" => parser_variants::<TrajectoryLoopType>(),
        "trigger" => parser_variants::<TrajectoryTrigger>(),
        "grader_result" => parser_variants::<TrajectoryGraderResult>(),
        "mutation_proposal" => parser_variants::<TrajectoryMutationProposal>(),
        "status" => parser_variants::<TrajectoryLoopStatus>(),
        other => panic!("connect schema field {other} to its actual parser type"),
    }
}

#[test]
fn trajectory_contract_matches_schema_and_actual_parser_enums_exactly() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let mut expected_rows = BTreeMap::new();
    let loop_properties = schema["$defs"]["loop"]["properties"].as_object().unwrap();
    for (field, definition) in loop_properties {
        if let Some(values) = definition.get("enum") {
            let values = strings(values);
            assert_eq!(values, loop_parser_variants(field), "{field} parser drift");
            for value in &values {
                let mut input = example_loop();
                input[field] = Value::String(value.clone());
                let parsed = parse_loop_json(&input.to_string()).unwrap();
                assert_eq!(serde_json::to_value(parsed).unwrap()[field], *value);
            }
            expected_rows.insert(field.clone(), values);
        }
    }
    let check_values = strings(&schema["$defs"]["check"]["properties"]["status"]["enum"]);
    assert_eq!(check_values, parser_variants::<TrajectoryCheckStatus>());
    for value in &check_values {
        let check = parse_check_spec(&format!("documented={value}")).unwrap();
        assert_eq!(serde_json::to_value(check.status).unwrap(), *value);
    }
    expected_rows.insert("--check status".to_string(), check_values);

    let section = trajectory_section(CONTRACTS);
    let rows = section
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| {
            let columns = line.split('|').collect::<Vec<_>>();
            assert_eq!(columns.len(), 4, "malformed enum row {line}");
            let name = columns[1].trim().trim_matches('`').to_string();
            let values = columns[2]
                .trim()
                .split(", ")
                .map(|value| value.trim_matches('`').to_string())
                .collect::<Vec<_>>();
            (name, values)
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(rows, expected_rows, "docs/schema enum drift");
    assert_eq!(
        section
            .lines()
            .filter(|line| line.starts_with("| `"))
            .count(),
        rows.len(),
        "duplicate enum rows"
    );
    assert!(section.contains(&format!(
        "limited to {} bytes",
        decapod::core::trajectory::MAX_LOOP_FEEDBACK_BYTES
    )));
    assert_eq!(
        section,
        trajectory_record_contract().unwrap().trim(),
        "regenerate contracts with decapod docs build"
    );

    let mut required = strings(&schema["$defs"]["loop"]["required"]);
    required.sort();
    let fields = example_loop()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        fields, required,
        "example must contain exactly the required fields"
    );
}

#[test]
fn trajectory_parser_keeps_check_aliases_separate_from_exact_json_enums() {
    for (alias, canonical) in [("pass", "passed"), ("fail", "failed")] {
        for input in [alias.to_string(), format!(" {} ", alias.to_uppercase())] {
            let check = parse_check_spec(&format!(" documented = {input}")).unwrap();
            assert_eq!(check.name, "documented");
            assert_eq!(serde_json::to_value(check.status).unwrap(), canonical);
        }
        assert!(trajectory_section(CONTRACTS).contains(&format!("`{alias}` as `{canonical}`")));
    }
    for invalid in [
        "check=no_checks_run",
        "check=skipped",
        "check=open",
        "=passed",
        "passed",
    ] {
        assert!(
            parse_check_spec(invalid).is_err(),
            "unexpected check input {invalid}"
        );
    }
    for (field, invalid) in [
        ("trigger", "user_scope_confirmed"),
        ("trigger", "Human"),
        ("grader_result", "passed"),
        ("grader_result", "PASS"),
        ("status", "pass"),
        ("status", "fail"),
        ("loop_type", "verify"),
        ("mutation_proposal", "code"),
    ] {
        let mut input = example_loop();
        input[field] = Value::String(invalid.to_string());
        assert!(
            parse_loop_json(&input.to_string()).is_err(),
            "accepted {field}={invalid}"
        );
    }
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_AGENT_ID", "trajectory-docs-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1")
        .output()
        .expect("run decapod")
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn embedded_doc(dir: &Path, name: &str) -> String {
    let output = success(run(dir, &["docs", "show", name, "--source", "embedded"]));
    let node: Value = serde_json::from_str(&output).expect("embedded document node");
    strings(&node["sections"]["concepts"]).join("\n")
}

#[test]
fn trajectory_contract_is_embedded_and_schema_touch_regenerates_it() {
    let temp = TempDir::new().unwrap();
    success(run(temp.path(), &["init", "--force"]));
    let contract = trajectory_record_contract().unwrap();
    for output in [
        success(run(temp.path(), &["docs", "ingest"])),
        embedded_doc(temp.path(), "command-contracts"),
    ] {
        assert!(
            output.contains(contract.trim()),
            "missing embedded trajectory contract"
        );
    }
    let example = embedded_doc(temp.path(), "payload-examples");
    assert!(example.contains(example_script()));

    fs::create_dir_all(temp.path().join("docs/agent")).unwrap();
    let contracts = temp.path().join("docs/agent/command-contracts.md");
    fs::write(&contracts, "stale documentation").unwrap();
    success(run(
        temp.path(),
        &[
            "docs",
            "build",
            "--touched",
            "assets/schemas/trajectory.schema.json",
        ],
    ));
    let regenerated = fs::read_to_string(contracts).unwrap();
    assert_eq!(trajectory_section(&regenerated), contract.trim());
    assert!(regenerated.contains(decapod::core::docs_cli::specs_refresh_contract()));
}

#[test]
fn documented_minimal_trajectory_commands_execute_without_claiming_success() {
    let temp = TempDir::new().unwrap();
    success(run(temp.path(), &["init", "--force"]));
    let binary = fs::canonicalize(env!("CARGO_BIN_EXE_decapod")).unwrap();
    let output = Command::new("sh")
        .current_dir(temp.path())
        .args(["-eu", "-c"])
        .arg(format!(
            "decapod() {{ \"$TRAJECTORY_DOCS_TEST_BIN\" \"$@\"; }}\n{}",
            example_script()
        ))
        .env("TRAJECTORY_DOCS_TEST_BIN", binary)
        .env("DECAPOD_AGENT_ID", "trajectory-docs-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1")
        .output()
        .expect("execute documented commands");
    let stdout = success(output);
    let results = serde_json::Deserializer::from_str(&stdout)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .expect("documented commands return JSON");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["marker"], "TRAJECTORY_INITIALIZED");
    let record = &results[1];
    assert_eq!(record["checks"][0]["status"], "unavailable");
    assert_eq!(record["proof_status"], "unavailable");
    let mut recorded_loop = record["loops"][0].clone();
    assert!(recorded_loop["custody_event_id"].is_string());
    recorded_loop
        .as_object_mut()
        .unwrap()
        .remove("custody_event_id");
    assert_eq!(recorded_loop, example_loop());
    assert_ne!(record["verdicts"]["completion_proof"], "supported");
}
