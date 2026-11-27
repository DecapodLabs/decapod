//! Structural admission for the existing global Constitution build path.
//! This does not authenticate a reviewer or interpret prose as executable policy.
use serde_json::Value;

// Promotion changes require reviewing executable admission code as well as node
// metadata. An ordinary corpus-only contribution cannot promote itself. These
// are the binding identities in accepted source 6ab6de3134286a790d78d370ca3fb43c0d92f834.
const BINDING_NODES: &[&str] = &[
    "core/DECAPOD",
    "core/SCAFFOLDING",
    "core/ARCHITECTURE",
    "core/DOCS",
    "core/INTERFACES",
    "core/METHODOLOGY",
    "core/PLUGINS",
    "core/SPECS",
    "core/METADATA",
    "core/DEMANDS",
    "core/DEPRECATION",
    "core/EMERGENCY_PROTOCOL",
    "core/DATA",
    "core/RESEARCH",
];

pub fn validate(graph: &Value) -> Result<(), String> {
    let nodes = graph["nodes"]
        .as_object()
        .ok_or("CONSTITUTION_NODES_MISSING")?;
    for (id, node) in nodes {
        let category = node["category"].as_str().unwrap_or("");
        if node["id"].as_str() != Some(id) || id.split('/').next() != Some(category) {
            return Err(format!(
                "CONSTITUTION_ID_CATEGORY: {id}: map key, id and category must agree"
            ));
        }
        let authority = node["authority"].as_str().unwrap_or("");
        let expected_authority = if id == "core/DECAPOD" {
            "root-router"
        } else if BINDING_NODES.contains(&id.as_str())
            && !matches!(
                id.as_str(),
                "core/DEMANDS" | "core/DEPRECATION" | "core/EMERGENCY_PROTOCOL"
            )
        {
            "namespace-router"
        } else {
            "doctrine"
        };
        if authority != expected_authority {
            return Err(format!(
                "CONSTITUTION_ROUTER_AUTHORITY: {id}: expected {expected_authority}; corpus metadata cannot grant router authority"
            ));
        }
        let binding = node["binding"].as_str().unwrap_or("");
        if binding == "binding" {
            if !BINDING_NODES.contains(&id.as_str()) {
                return Err(format!(
                    "CONSTITUTION_PROMOTION_REVIEW_REQUIRED: {id}: advisory by default; promotion requires reviewing the admission registry and owning control, not only corpus metadata"
                ));
            }
            let review = &node["authority_review"];
            if review["control_id"] != "core/SPECS"
                || review["required_review"] != "repository_pr_review"
                || review["enforcement"] != "not_machine_enforced"
                || review["basis"].as_str().is_none_or(|s| {
                    s.strip_prefix("https://github.com/DecapodLabs/decapod/commit/")
                        .is_none_or(|sha| {
                            sha.len() != 40
                                || !sha
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        })
                })
            {
                return Err(format!(
                    "CONSTITUTION_AUTHORITY_REVIEW: {id}: name core/SPECS, repository PR review, accepted-source basis and truthful non-machine-enforced guidance; native gates have their own execution proof"
                ));
            }
        } else if binding == "advisory" {
            if !BINDING_NODES.contains(&id.as_str()) && node.get("authority_review").is_some() {
                return Err(format!(
                    "CONSTITUTION_AUTHORITY_REVIEW: {id}: advisory nodes cannot carry binding authority_review metadata"
                ));
            }
        } else {
            return Err(format!("CONSTITUTION_BINDING_INVALID: {id}"));
        }
        // 'proof' prose describes expected checks. It is not an execution ledger.
        if node.get("enforcement").is_some()
            || node.get("claims").is_some()
            || node["sections"]
                .as_object()
                .is_some_and(|secs| secs.contains_key("enforcement") || secs.contains_key("claims"))
        {
            return Err(format!(
                "CONSTITUTION_UNSUPPORTED_ENFORCEMENT: {id}: source assertions are not executed gate evidence"
            ));
        }
        for key in ["references", "referenced_by"] {
            for reference in node["links"][key]
                .as_array()
                .ok_or_else(|| format!("CONSTITUTION_LINKS_INVALID: {id}"))?
            {
                let reference = reference.as_str().ok_or("CONSTITUTION_LINK_INVALID")?;
                if !nodes.contains_key(reference) {
                    return Err(format!(
                        "CONSTITUTION_REFERENCE_UNRESOLVED: {id}: {reference}"
                    ));
                }
            }
        }
        if let Some(research) = node.get("research_doctrine") {
            for reference in research["related_nodes"]
                .as_array()
                .ok_or("CONSTITUTION_RESEARCH_LINKS_INVALID")?
            {
                let reference = reference
                    .as_str()
                    .ok_or("CONSTITUTION_RESEARCH_LINK_INVALID")?;
                if !nodes.contains_key(reference) {
                    return Err(format!(
                        "CONSTITUTION_REFERENCE_UNRESOLVED: {id}: {reference}"
                    ));
                }
            }
        }
    }
    for id in BINDING_NODES {
        if nodes
            .get(*id)
            .is_none_or(|node| node["binding"] != "binding")
        {
            return Err(format!(
                "CONSTITUTION_BINDING_CONTROL_REMOVED: {id}: accepted control requires an explicit admission-code review"
            ));
        }
    }
    let specs_apply_guarded = nodes
        .get("core/SPECS")
        .and_then(|node| node["sections"]["apply"].as_array())
        .is_some_and(|lines| {
            lines.iter().any(|line| {
                line.as_str().is_some_and(|text| {
                    text.contains("admission registry") && text.contains("advisory")
                })
            })
        });
    if !specs_apply_guarded {
        return Err(
            "CONSTITUTION_BINDING_CONTROL_REMOVED: core/SPECS.sections.apply must retain the non-self-promotion advisory admission rule".to_string(),
        );
    }
    for references in graph["lookup"]
        .as_object()
        .ok_or("CONSTITUTION_LOOKUP_MISSING")?
        .values()
    {
        for reference in references.as_array().ok_or("CONSTITUTION_LOOKUP_INVALID")? {
            let reference = reference.as_str().ok_or("CONSTITUTION_LOOKUP_INVALID")?;
            if !nodes.contains_key(reference) {
                return Err(format!("CONSTITUTION_LOOKUP_UNRESOLVED: {reference}"));
            }
        }
    }
    Ok(())
}
