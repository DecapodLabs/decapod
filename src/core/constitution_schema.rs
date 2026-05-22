use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Binding contract for the Decapod Constitution Graph
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstitutionGraph {
    /// Map of node ID to node definition
    pub nodes: HashMap<String, ConstitutionNode>,
}

/// A single node in the constitution graph
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstitutionNode {
    /// Human-readable title
    pub title: String,
    /// Category (e.g., core, architecture, plugins)
    pub category: String,
    /// IDs of nodes this node depends on
    pub dependencies: Vec<String>,
    /// Structured section content rendered from the embedded constitution asset.
    pub content: ConstitutionContent,
}

/// Structured content for a constitution node.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstitutionContent {
    /// Short node summary.
    pub summary: String,
    /// Named subsections for sectional retrieval.
    pub sections: HashMap<String, Value>,
    /// Optional cross-reference metadata for this node.
    #[serde(default)]
    pub links: HashMap<String, Value>,
}
