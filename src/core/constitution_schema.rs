use serde::{Deserialize, Serialize};
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
    /// Full Markdown content
    pub content: String,
}
