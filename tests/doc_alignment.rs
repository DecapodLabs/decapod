use std::fs;
use std::process::Command;

#[test]
fn test_agent_docs_command_contracts_alignment() {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .arg("--help")
        .output()
        .expect("failed to execute decapod --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Read the command contracts
    let contracts_path = "docs/agent/command-contracts.md";
    let contracts_content =
        fs::read_to_string(contracts_path).expect("failed to read docs/agent/command-contracts.md");

    // Extract documented commands from ## decapod <command> headers
    // Note: The docs use `decapod todo claim` format
    let re = regex::Regex::new(r"## `decapod ([\w\s]+)`").unwrap();

    for cap in re.captures_iter(&contracts_content) {
        let full_cmd = &cap[1];
        let parts: Vec<&str> = full_cmd.split_whitespace().collect();

        if parts.is_empty() {
            continue;
        }

        let root_cmd = parts[0];

        // Verify root command exists in top-level help
        assert!(
            help_text.contains(root_cmd),
            "Documented root command '{}' not found in decapod --help",
            root_cmd
        );

        // If it's a sub-command (e.g. todo claim), verify it exists in sub-help
        if parts.len() > 1 {
            let sub_cmd = parts[1];
            let sub_help = Command::new(env!("CARGO_BIN_EXE_decapod"))
                .arg(root_cmd)
                .arg("--help")
                .output()
                .expect("failed to execute sub-command help");

            let sub_help_text = String::from_utf8_lossy(&sub_help.stdout);
            assert!(
                sub_help_text.contains(sub_cmd),
                "Documented sub-command '{} {}' not found in decapod {} --help",
                root_cmd,
                sub_cmd,
                root_cmd
            );
        }
    }
}

#[test]
fn test_config_schema_alignment() {
    // Verify that keys documented in docs/agent/config-schema.md exist in the project config
    let config_docs = fs::read_to_string("docs/agent/config-schema.md")
        .expect("failed to read docs/agent/config-schema.md");

    // Documented keys are in ### repo.key or ### init.key format
    let re = regex::Regex::new(r"### `(repo|init)\.(\w+)`").unwrap();

    // Use capabilities --format json to get the full project config
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["capabilities", "--format", "json"])
        .output()
        .expect("failed to get capabilities");

    let caps_json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("failed to parse capabilities JSON");

    let config = &caps_json["config"];

    for cap in re.captures_iter(&config_docs) {
        let section = &cap[1];
        let key = &cap[2];

        // Check if the key exists in the corresponding section of the config
        let section_val = &config[section];
        assert!(
            !section_val[key].is_null(),
            "Documented config key '{}.{}' not found in decapod capabilities config",
            section,
            key
        );
    }
}
