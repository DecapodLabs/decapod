//! Mapping of CLI commands to canonical constitution documents.
//!
//! This provides deterministic routing from the interface surface to the law.

use std::collections::BTreeMap;

pub fn get_governance_map() -> BTreeMap<&'static str, Vec<&'static str>> {
    let mut m = BTreeMap::new();

    m.insert("todo", vec!["plugins/TODO.md"]);
    m.insert(
        "todo.add",
        vec!["plugins/TODO.md#lifecycle-management", "specs/INTENT.md"],
    );
    m.insert(
        "todo.claim",
        vec![
            "plugins/TODO.md#claims-and-exclusive-mode",
            "interfaces/CLAIMS.md",
        ],
    );
    m.insert(
        "todo.done",
        vec![
            "plugins/TODO.md#completion-and-verification",
            "specs/SYSTEM.md#proof-doctrine",
        ],
    );

    m.insert(
        "docs",
        vec![
            "core/DECAPOD.md#navigation-charter",
            "interfaces/DOC_RULES.md",
        ],
    );
    m.insert(
        "docs.show",
        vec!["core/DECAPOD.md#topic-specific-navigation"],
    );

    m.insert(
        "validate",
        vec!["plugins/VERIFY.md", "specs/SYSTEM.md#validation-gates"],
    );

    m.insert(
        "govern.policy",
        vec!["plugins/POLICY.md", "specs/SECURITY.md#policy-gates"],
    );
    m.insert(
        "govern.health",
        vec!["plugins/HEALTH.md", "interfaces/CLAIMS.md"],
    );
    m.insert(
        "govern.proof",
        vec!["specs/SYSTEM.md#proof-doctrine", "interfaces/TESTING.md"],
    );

    m.insert(
        "agent.init",
        vec![
            "core/DECAPOD.md#mandatory-session-start-protocol",
            "AGENTS.md",
        ],
    );
    m.insert(
        "exec",
        vec![
            "core/DECAPOD.md#the-thin-waist",
            "interfaces/CONTROL_PLANE.md",
        ],
    );
    m.insert(
        "fs",
        vec![
            "interfaces/STORE_MODEL.md",
            "specs/SYSTEM.md#weights-and-balances",
        ],
    );
    m.insert(
        "fs.write",
        vec![
            "interfaces/STORE_MODEL.md#mutation-rules",
            "core/DECAPOD.md#weights-and-balances",
        ],
    );
    m.insert(
        "fs.read",
        vec!["interfaces/STORE_MODEL.md#access-patterns"],
    );

    m.insert(
        "data.schema",
        vec!["core/PLUGINS.md", "interfaces/STORE_MODEL.md"],
    );
    m.insert(
        "data.broker",
        vec![
            "core/DECAPOD.md#the-thin-waist",
            "interfaces/STORE_MODEL.md",
        ],
    );

    m
}

pub fn related_docs(cmd_path: &str) -> Vec<&'static str> {
    let map = get_governance_map();

    // Exact match first
    if let Some(docs) = map.get(cmd_path) {
        return docs.clone();
    }

    // Prefix match (e.g. todo.add -> todo)
    let parts: Vec<&str> = cmd_path.split('.').collect();
    if parts.len() > 1 {
        if let Some(docs) = map.get(parts[0]) {
            return docs.clone();
        }
    }

    vec!["core/DECAPOD.md"]
}
