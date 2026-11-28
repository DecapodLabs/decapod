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
            "Documented root command '{root_cmd}' not found in decapod --help"
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
                "Documented sub-command '{root_cmd} {sub_cmd}' not found in decapod {root_cmd} --help"
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
            "Documented config key '{section}.{key}' not found in decapod capabilities config"
        );
    }
}

#[test]
fn test_mcp_docs_match_current_adapter_boundary() {
    let agent_mcp = fs::read_to_string("docs/agent/mcp.md").expect("read agent MCP docs");
    let book_mcp = fs::read_to_string("docs/book/src/concepts/mcp.md").expect("read book MCP docs");
    let agent_first =
        fs::read_to_string("docs/book/src/concepts/agent-first.md").expect("read agent-first docs");

    for docs in [&agent_mcp, &book_mcp, &agent_first] {
        assert!(
            docs.contains("adapter") || docs.contains("adapter boundary"),
            "MCP documentation must identify the adapter boundary"
        );
        assert!(
            docs.contains("does not") || docs.contains("not part"),
            "MCP documentation must state the current implementation boundary"
        );
    }

    assert!(agent_mcp.contains("tamper-evident"));
    assert!(agent_mcp.contains("does not authenticate a model provider"));
    assert!(!agent_first.contains("By supporting the **Model Context Protocol (MCP)**"));
}

#[test]
fn test_public_docs_do_not_overclaim_protocol_or_identity_boundaries() {
    let docs = [
        "docs/agent/command-contracts.md",
        "docs/agent/payload-examples.md",
        "docs/agent/mcp.md",
        "docs/book/src/concepts/agent-first.md",
        "docs/book/src/concepts/mcp.md",
        "docs/book/src/mental-model.md",
    ];

    for path in docs {
        let content = fs::read_to_string(path).expect("read public documentation");
        assert!(
            !content.contains("Structured JSON-RPC interface")
                && !content.contains("JSON-RPC payloads")
                && !content.contains("structured CLI or JSON-RPC interface"),
            "{path} must describe Decapod's structured RPC boundary without claiming JSON-RPC 2.0"
        );
    }

    let agent_mcp = fs::read_to_string("docs/agent/mcp.md").expect("read agent MCP docs");
    for issue in [870, 871, 872, 873, 874, 876] {
        assert!(
            agent_mcp.contains(&format!("github.com/DecapodLabs/decapod/issues/{issue}")),
            "agent MCP documentation must link future boundary issue #{issue}"
        );
    }
    assert!(agent_mcp.contains("does not authenticate a model provider"));
    assert!(agent_mcp.contains("does not currently implement an MCP server"));
}

#[test]
fn test_docs_landing_page_links_local_and_cloud_offerings() {
    let index = fs::read_to_string("docs/book/src/index.md").expect("read docs/book/src/index.md");
    assert!(!index.contains("https://decapodlabs.ai/pricing"));
    assert!(index.contains("reference/cloud-projects.html"));
    assert!(index.contains("Cloud project docs"));
    assert!(index.contains("https://decapodlabs.ai/signup"));
    assert!(index.contains("https://decapodlabs.ai/admin"));
    assert!(!index.contains("https://decapodlabs.github.io/"));
    assert!(!index.contains("offerings.html"));
    assert!(index.contains("Decapod Local"));
    assert!(index.contains("Free · Offline · No signup"));
    assert!(index.contains("Decapod Cloud"));
    assert!(!index.contains("$50/contributor/repo/mo"));
    assert!(!index.contains("$0 surprise overages"));
    assert!(!index.contains("Coming soon"));
    assert!(!index.contains("no public pricing or active purchase route"));
    assert!(index.contains("quickstart.html"));
    assert!(index.contains("HARNESS AI"));

    let quickstart = fs::read_to_string("docs/book/src/quickstart.md")
        .expect("read docs/book/src/quickstart.md");
    assert!(!quickstart.contains("https://decapodlabs.ai/pricing"));
    assert!(!quickstart.contains("$50/contributor/repo/mo"));
    assert!(quickstart.contains("https://decapodlabs.ai/signup"));
    assert!(quickstart.contains("https://decapodlabs.ai/admin"));

    for path in [
        "docs/book/src/introduction.md",
        "docs/book/src/concepts/faq.md",
        "docs/book/src/reference/cloud-projects.md",
        "docs/book/src/reference/propodus.md",
        "docs/book/src/workflows/multi-agent.md",
    ] {
        let content = fs::read_to_string(path).expect("read book page");
        assert!(
            !content.contains("https://decapodlabs.ai/pricing"),
            "{path} must not link to hidden /pricing route"
        );
        assert!(
            !content.contains("$50/contributor/repo/mo"),
            "{path} must not include hidden $50 pricing copy"
        );
    }

    let hbs =
        fs::read_to_string("docs/book/theme/index.hbs").expect("read docs/book/theme/index.hbs");
    assert!(!hbs.contains("https://decapodlabs.ai/pricing"));
    assert!(hbs.contains("https://decapodlabs.ai/admin"));
    assert!(!hbs.contains("https://decapodlabs.github.io/"));

    let css =
        fs::read_to_string("docs/book/src/custom.css").expect("read docs/book/src/custom.css");
    assert!(css.contains(".decapod-docs-offerings"));
    assert!(css.contains(".decapod-offerings-grid"));
    assert!(css.contains(".decapod-offering-badge.badge-cloud"));
    assert!(css.contains(".decapod-offering-actions"));
    assert!(css.contains("@media(max-width:760px)"));
    assert!(css.contains(":focus-visible"));
    assert!(css.contains("prefers-reduced-motion:reduce"));
}
