use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Binding contract for the Decapod Constitution Graph
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstitutionGraph {
    /// Map of node ID to node definition
    pub nodes: HashMap<String, ConstitutionNode>,
    /// Categorized index of node IDs
    #[serde(default)]
    pub index: HashMap<String, Vec<String>>,
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
    /// Detailed node description
    pub description: String,
    /// High-level topic context
    pub topic_context: TopicContext,
    /// Authority level for this node
    pub authority: String,
    /// Binding status (binding, advisory, etc.)
    #[serde(default)]
    pub binding: String,
    /// Operational scope
    #[serde(default)]
    pub scope: String,
    /// Responsibility statement
    #[serde(default)]
    pub responsibility: String,
    /// Node-level cross-reference metadata.
    pub links: Links,
}

/// Structured content for a constitution node.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstitutionContent {
    /// Short node summary.
    pub summary: String,
    /// Named subsections for sectional retrieval.
    pub sections: HashMap<String, Value>,
    /// Detailed content description
    pub description: String,
    /// Depth standard for the domain brief
    #[serde(rename = "domain brief_depth", default)]
    pub domain_brief_depth: String,
    /// Authority statement for the content
    pub authority: String,
    /// Topic context for the content
    pub topic_context: TopicContext,
    /// Content-level cross-reference metadata.
    pub links: Links,
}

/// High-level topic context for a node or content block.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TopicContext {
    /// Subject domain
    pub domain: String,
    /// Summary of the topic
    pub summary: String,
    /// Core ideas within this topic
    pub core_ideas: Vec<String>,
    /// Keywords for retrieval
    pub concept_keywords: Vec<String>,
}

/// Bidirectional cross-reference metadata for a constitution node.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Links {
    /// Outbound references to other nodes.
    #[serde(default)]
    pub references: Vec<String>,
    /// Inbound references from other nodes.
    #[serde(default)]
    pub referenced_by: Vec<String>,
    /// Catch-all for additional metadata.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}
