//! Obligations Engine - Mentor/Babysitter for Agents
//!
//! This module provides deterministic guidance that pushes agents back to:
//! - Prior decisions (ADRs)
//! - Specs/architecture/security docs
//! - Knowledge graph nodes
//! - Active todos/commitments
//!
//! # Design Principles
//!
//! - Deterministic: Same repo state + input = same obligations
//! - Immutable sources: Never modifies existing docs/KG
//! - Compact views: Max 5 items per obligations list
//! - Optional LLM: Only for ranking/phrasing, never adding obligations
//!
//! # For AI Agents
//!
//! Call `mentor.obligations` to get contextually relevant obligations before acting.
//! The engine will point you to prior decisions, specs, and commitments that may
//! affect your current work.

use crate::core::error::DecapodError;
use crate::core::rpc::{Blocker, BlockerKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An obligation item - a pointer to relevant context
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Obligation {
    /// Kind of obligation
    pub kind: ObligationKind,
    /// Reference path/ID
    pub ref_path: String,
    /// Human-readable title
    pub title: String,
    /// Brief explanation of why this matters now
    pub why_short: String,
    /// Evidence/provenance
    pub evidence: Evidence,
    /// Relevance score (0.0-1.0, higher = more relevant)
    pub relevance_score: f64,
}

/// Types of obligations
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    /// Reference to a doc section/anchor
    DocAnchor,
    /// Architecture Decision Record
    Adr,
    /// Knowledge graph node
    KgNode,
    /// Active todo/commitment
    Todo,
    /// Validation gate that must pass
    Gate,
    /// Container/Docker requirement - Silicon Valley hygiene
    Container,
}

/// Evidence/provenance for an obligation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Evidence {
    /// Source type
    pub source: String,
    /// ID within source
    pub id: String,
    /// Optional content hash
    pub hash: Option<String>,
    /// Timestamp of source
    pub timestamp: Option<String>,
}

/// Computed obligations for an operation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Obligations {
    /// Must-address obligations (<= 5 items)
    pub must: Vec<Obligation>,
    /// Recommended obligations (<= 5 items)
    pub recommended: Vec<Obligation>,
    /// Detected contradictions
    pub contradictions: Vec<Contradiction>,
}

/// A detected contradiction
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Contradiction {
    /// Description of the contradiction
    pub description: String,
    /// Current state/assumption
    pub current: String,
    /// Prior decision/spec that conflicts
    pub prior: String,
    /// Suggested resolution
    pub resolution_hint: String,
}

/// Input context for computing obligations
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObligationsContext {
    /// Operation being performed
    pub op: String,
    /// Operation parameters
    pub params: serde_json::Value,
    /// Paths touched by operation
    pub touched_paths: Vec<String>,
    /// Diff summary (if applicable)
    pub diff_summary: Option<String>,
    /// Project profile ID
    pub project_profile_id: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// High-risk operation flag
    pub high_risk: bool,
}

/// Candidate source for obligation retrieval
#[derive(Debug, Clone)]
enum CandidateSource {
    Adr {
        path: PathBuf,
        content: String,
    },
    Doc {
        path: PathBuf,
        section: String,
        content: String,
    },
    KgNode {
        id: String,
        title: String,
        node_type: String,
        tags: Vec<String>,
    },
    Todo {
        id: String,
        title: String,
        status: String,
        category: Option<String>,
    },
}

/// Mentor engine for computing obligations
pub struct MentorEngine {
    repo_root: PathBuf,
}

impl MentorEngine {
    /// Create a new mentor engine
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
        }
    }

    /// Compute obligations for a given operation context
    pub fn compute_obligations(
        &self,
        context: &ObligationsContext,
    ) -> Result<Obligations, DecapodError> {
        // Step 1: Get container obligations FIRST (Silicon Valley hygiene priority)
        let container_obligations = self.get_container_candidates()?;

        // Step 2: Retrieve candidates from all other sources
        let candidates = self.retrieve_candidates(context)?;

        // Step 3: Score candidates based on relevance
        let scored = self.score_candidates(&candidates, context);

        // Step 4: Check for contradictions
        let contradictions = self.detect_contradictions(&scored, context);

        // Step 5: Build obligations lists (capped at 5 each)
        let (mut must, mut recommended) = self.build_obligations(scored, context);

        // Step 6: Prepend container obligations (they take precedence)
        // Container obligations are MUST if Dockerfile missing or work not containerized
        for container_obligation in container_obligations {
            if container_obligation.relevance_score >= 0.9 {
                // High-priority container obligations go to must
                must.insert(0, container_obligation);
            } else {
                // Lower priority go to recommended
                recommended.insert(0, container_obligation);
            }
        }

        // Ensure caps are maintained after adding container obligations
        must.truncate(5);
        recommended.truncate(5);

        Ok(Obligations {
            must,
            recommended,
            contradictions,
        })
    }

    /// Retrieve candidates from all sources
    fn retrieve_candidates(
        &self,
        context: &ObligationsContext,
    ) -> Result<Vec<CandidateSource>, DecapodError> {
        let mut candidates = vec![];

        // Get ADRs
        candidates.extend(self.get_adr_candidates()?);

        // Get doc candidates
        candidates.extend(self.get_doc_candidates(context)?);

        // Get knowledge graph candidates
        candidates.extend(self.get_kg_candidates()?);

        // Get todo candidates
        candidates.extend(self.get_todo_candidates()?);

        Ok(candidates)
    }

    /// Get ADR candidates from docs/decisions/
    fn get_adr_candidates(&self) -> Result<Vec<CandidateSource>, DecapodError> {
        let mut candidates = vec![];
        let decisions_dir = self.repo_root.join("docs").join("decisions");

        if !decisions_dir.exists() {
            return Ok(candidates);
        }

        for entry in std::fs::read_dir(&decisions_dir).map_err(DecapodError::IoError)? {
            let entry = entry.map_err(DecapodError::IoError)?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                candidates.push(CandidateSource::Adr { path, content });
            }
        }

        Ok(candidates)
    }

    /// Get doc candidates from docs/*.md
    fn get_doc_candidates(
        &self,
        context: &ObligationsContext,
    ) -> Result<Vec<CandidateSource>, DecapodError> {
        let mut candidates = vec![];
        let docs_dir = self.repo_root.join("docs");

        if !docs_dir.exists() {
            return Ok(candidates);
        }

        let doc_files = ["spec.md", "architecture.md", "security.md", "ops.md"];

        for filename in &doc_files {
            let path = docs_dir.join(filename);
            if path.exists() {
                let content = std::fs::read_to_string(&path).unwrap_or_default();

                // Extract sections (simple markdown header parsing)
                let sections = self.extract_sections(&content);

                for (section, section_content) in sections {
                    // Check if section is relevant to operation
                    if self.is_section_relevant(&section, &section_content, context) {
                        candidates.push(CandidateSource::Doc {
                            path: path.clone(),
                            section,
                            content: section_content,
                        });
                    }
                }
            }
        }

        Ok(candidates)
    }

    /// Extract markdown sections (## headers)
    fn extract_sections(&self, content: &str) -> Vec<(String, String)> {
        let mut sections = vec![];
        let lines: Vec<&str> = content.lines().collect();
        let mut current_section = "Introduction".to_string();
        let mut current_content = String::new();

        for line in &lines {
            if let Some(stripped) = line.strip_prefix("## ") {
                // Save previous section
                if !current_content.is_empty() {
                    sections.push((current_section.clone(), current_content.clone()));
                }
                current_section = stripped.trim().to_string();
                current_content.clear();
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        // Save last section
        if !current_content.is_empty() {
            sections.push((current_section, current_content));
        }

        sections
    }

    /// Check if a section is relevant to the operation
    fn is_section_relevant(
        &self,
        section: &str,
        content: &str,
        context: &ObligationsContext,
    ) -> bool {
        let section_lower = section.to_lowercase();
        let content_lower = content.to_lowercase();

        // Check against touched paths
        for path in &context.touched_paths {
            let path_lower = path.to_lowercase();
            if section_lower.contains(&path_lower) || content_lower.contains(&path_lower) {
                return true;
            }
        }

        // Check against operation type
        let op_lower = context.op.to_lowercase();
        if section_lower.contains(&op_lower) || content_lower.contains(&op_lower) {
            return true;
        }

        // Check for high-risk keywords
        if context.high_risk {
            let risk_keywords = ["security", "auth", "network", "credential", "secret"];
            for keyword in &risk_keywords {
                if section_lower.contains(keyword) || content_lower.contains(keyword) {
                    return true;
                }
            }
        }

        false
    }

    /// Get knowledge graph candidates
    fn get_kg_candidates(&self) -> Result<Vec<CandidateSource>, DecapodError> {
        let mut candidates = vec![];

        // Query federation database for active nodes
        let db_path = self
            .repo_root
            .join(".decapod")
            .join("data")
            .join("federation.db");

        if !db_path.exists() {
            return Ok(candidates);
        }

        let conn = rusqlite::Connection::open(&db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, title, node_type, tags FROM nodes 
                 WHERE status = 'active' 
                 AND (node_type = 'decision' OR node_type = 'commitment')
                 ORDER BY created_at DESC
                 LIMIT 20",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        for row in rows {
            let (id, title, node_type, tags_str) = row?;
            let tags: Vec<String> = tags_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            candidates.push(CandidateSource::KgNode {
                id,
                title,
                node_type,
                tags,
            });
        }

        Ok(candidates)
    }

    /// Get todo candidates
    fn get_todo_candidates(&self) -> Result<Vec<CandidateSource>, DecapodError> {
        let mut candidates = vec![];

        // Query todo database for active tasks
        let db_path = self.repo_root.join(".decapod").join("data").join("todo.db");

        if !db_path.exists() {
            return Ok(candidates);
        }

        let conn = rusqlite::Connection::open(&db_path)?;

        let mut stmt = conn.prepare(
            "SELECT id, title, status, category FROM todos 
                 WHERE status IN ('open', 'claimed', 'in_progress')
                 ORDER BY priority DESC, created_at DESC
                 LIMIT 20",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        for row in rows {
            let (id, title, status, category) = row?;

            candidates.push(CandidateSource::Todo {
                id,
                title,
                status,
                category,
            });
        }

        Ok(candidates)
    }

    /// Score candidates by relevance
    fn score_candidates(
        &self,
        candidates: &[CandidateSource],
        context: &ObligationsContext,
    ) -> Vec<(CandidateSource, f64)> {
        let mut scored = vec![];

        for candidate in candidates {
            let score = self.compute_relevance_score(candidate, context);
            scored.push((candidate.clone(), score));
        }

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        scored
    }

    /// Compute relevance score for a candidate
    fn compute_relevance_score(
        &self,
        candidate: &CandidateSource,
        context: &ObligationsContext,
    ) -> f64 {
        let mut score = 0.0;

        match candidate {
            CandidateSource::Adr { path, content } => {
                // ADRs are high priority for architectural decisions
                score += 0.8;

                // Check for recency (newer ADRs slightly higher)
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if filename.starts_with("ADR-") {
                        // Parse ADR number (lower number = older, slight penalty)
                        if let Some(num_str) = filename.split('-').nth(1) {
                            if let Ok(num) = num_str.parse::<u32>() {
                                score += (100.0 - num as f64).max(0.0) * 0.001;
                            }
                        }
                    }
                }

                // Check content relevance
                let content_lower = content.to_lowercase();
                for path in &context.touched_paths {
                    if content_lower.contains(&path.to_lowercase()) {
                        score += 0.15;
                    }
                }
            }
            CandidateSource::Doc {
                path,
                section,
                content,
            } => {
                // Base score for docs
                score += 0.6;

                // Security docs get boost for high-risk ops
                if context.high_risk && path.to_string_lossy().contains("security") {
                    score += 0.2;
                }

                // Check section relevance
                let section_lower = section.to_lowercase();
                for touched in &context.touched_paths {
                    let touched_lower = touched.to_lowercase();
                    if section_lower.contains(&touched_lower) {
                        score += 0.2;
                    }
                }

                // Check content relevance
                let content_lower = content.to_lowercase();
                for touched in &context.touched_paths {
                    if content_lower.contains(&touched.to_lowercase()) {
                        score += 0.1;
                    }
                }
            }
            CandidateSource::KgNode {
                id: _,
                title,
                node_type,
                tags,
            } => {
                // Decisions and commitments are high priority
                match node_type.as_str() {
                    "decision" => score += 0.75,
                    "commitment" => score += 0.7,
                    _ => score += 0.5,
                }

                // Check tag relevance
                for tag in tags {
                    for path in &context.touched_paths {
                        if tag.to_lowercase().contains(&path.to_lowercase()) {
                            score += 0.15;
                        }
                    }
                }

                // Check title relevance
                let title_lower = title.to_lowercase();
                for path in &context.touched_paths {
                    if title_lower.contains(&path.to_lowercase()) {
                        score += 0.1;
                    }
                }
            }
            CandidateSource::Todo {
                id: _,
                title,
                status,
                category,
            } => {
                // Active todos are medium priority
                match status.as_str() {
                    "claimed" => score += 0.6,
                    "in_progress" => score += 0.55,
                    "open" => score += 0.5,
                    _ => score += 0.3,
                }

                // Category match
                if let Some(cat) = category {
                    for path in &context.touched_paths {
                        if cat.to_lowercase().contains(&path.to_lowercase()) {
                            score += 0.1;
                        }
                    }
                }

                // Title relevance
                let title_lower = title.to_lowercase();
                for path in &context.touched_paths {
                    if title_lower.contains(&path.to_lowercase()) {
                        score += 0.1;
                    }
                }
            }
        }

        // Cap at 1.0
        score.min(1.0)
    }

    /// Detect contradictions between candidates and context
    fn detect_contradictions(
        &self,
        scored: &[(CandidateSource, f64)],
        context: &ObligationsContext,
    ) -> Vec<Contradiction> {
        let mut contradictions = vec![];

        // Simple contradiction detection: if operation modifies X but ADR says Y about X
        for (candidate, score) in scored {
            if *score < 0.5 {
                continue; // Skip low-relevance candidates
            }

            if let CandidateSource::Adr { path, content } = candidate {
                // Check if ADR imposes constraints that operation might violate
                let content_lower = content.to_lowercase();

                for touched in &context.touched_paths {
                    let touched_lower = touched.to_lowercase();
                    if content_lower.contains(&format!("must use {}", touched_lower))
                        || content_lower.contains(&format!("shall use {}", touched_lower))
                        || content_lower.contains(&format!("decided: {}", touched_lower))
                    {
                        // Potential contradiction if operation changes this
                        if context.op.contains("change")
                            || context.op.contains("modify")
                            || context.op.contains("update")
                        {
                            contradictions.push(Contradiction {
                                description: format!(
                                    "Operation may contradict ADR decision regarding {}",
                                    touched
                                ),
                                current: format!("Operation: {}", context.op),
                                prior: format!(
                                    "ADR {} specifies requirements",
                                    path.file_name().unwrap_or_default().to_string_lossy()
                                ),
                                resolution_hint:
                                    "Review ADR or create new ADR if decision has changed"
                                        .to_string(),
                            });
                        }
                    }
                }
            }
        }

        contradictions
    }

    /// Build obligations lists (capped at 5 each)
    fn build_obligations(
        &self,
        scored: Vec<(CandidateSource, f64)>,
        context: &ObligationsContext,
    ) -> (Vec<Obligation>, Vec<Obligation>) {
        let mut must = vec![];
        let mut recommended = vec![];

        // Determine thresholds based on risk
        let must_threshold = if context.high_risk { 0.7 } else { 0.8 };
        let recommended_threshold = if context.high_risk { 0.5 } else { 0.6 };

        for (candidate, score) in scored {
            // Cap at 5 per list
            if must.len() >= 5 && recommended.len() >= 5 {
                break;
            }

            let obligation = self.candidate_to_obligation(&candidate, score);

            if score >= must_threshold && must.len() < 5 {
                must.push(obligation);
            } else if score >= recommended_threshold && recommended.len() < 5 {
                recommended.push(obligation);
            }
        }

        (must, recommended)
    }

    /// Convert candidate to obligation
    fn candidate_to_obligation(&self, candidate: &CandidateSource, score: f64) -> Obligation {
        match candidate {
            CandidateSource::Adr { path, content } => {
                let title = self.extract_title(content).unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("ADR")
                        .to_string()
                });

                Obligation {
                    kind: ObligationKind::Adr,
                    ref_path: path.to_string_lossy().to_string(),
                    title,
                    why_short: "Prior architectural decision may affect this work".to_string(),
                    evidence: Evidence {
                        source: "docs/decisions".to_string(),
                        id: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        hash: Some(self.compute_hash(content)),
                        timestamp: None,
                    },
                    relevance_score: score,
                }
            }
            CandidateSource::Doc {
                path,
                section,
                content: _,
            } => Obligation {
                kind: ObligationKind::DocAnchor,
                ref_path: format!("{}# {}", path.to_string_lossy(), section),
                title: section.clone(),
                why_short: "Relevant documentation section".to_string(),
                evidence: Evidence {
                    source: "docs".to_string(),
                    id: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    hash: None,
                    timestamp: None,
                },
                relevance_score: score,
            },
            CandidateSource::KgNode {
                id,
                title,
                node_type,
                tags: _,
            } => {
                let why = match node_type.as_str() {
                    "decision" => "Prior decision may constrain this work",
                    "commitment" => "Active commitment may affect approach",
                    _ => "Relevant knowledge graph entry",
                };

                Obligation {
                    kind: ObligationKind::KgNode,
                    ref_path: format!(".decapod/data/federation# {}", id),
                    title: title.clone(),
                    why_short: why.to_string(),
                    evidence: Evidence {
                        source: ".decapod/data".to_string(),
                        id: id.clone(),
                        hash: None,
                        timestamp: None,
                    },
                    relevance_score: score,
                }
            }
            CandidateSource::Todo {
                id,
                title,
                status,
                category: _,
            } => {
                let why = match status.as_str() {
                    "claimed" => "Claimed work may overlap or conflict",
                    "in_progress" => "In-progress work may be related",
                    _ => "Active task may be relevant",
                };

                Obligation {
                    kind: ObligationKind::Todo,
                    ref_path: format!(".decapod/data/todo# {}", id),
                    title: title.clone(),
                    why_short: why.to_string(),
                    evidence: Evidence {
                        source: ".decapod/data".to_string(),
                        id: id.clone(),
                        hash: None,
                        timestamp: None,
                    },
                    relevance_score: score,
                }
            }
        }
    }

    /// Extract title from markdown content
    fn extract_title(&self, content: &str) -> Option<String> {
        for line in content.lines() {
            if let Some(stripped) = line.strip_prefix("# ") {
                return Some(stripped.trim().to_string());
            }
        }
        None
    }

    /// Compute hash of content
    fn compute_hash(&self, content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get container/Docker obligation candidates
    fn get_container_candidates(&self) -> Result<Vec<Obligation>, DecapodError> {
        let mut obligations = vec![];

        // Check for Dockerfile
        let dockerfile_paths = [
            self.repo_root.join("Dockerfile"),
            self.repo_root.join(".devcontainer").join("Dockerfile"),
            self.repo_root.join("docker").join("Dockerfile"),
        ];

        let mut found_dockerfile = false;
        for path in &dockerfile_paths {
            if path.exists() {
                found_dockerfile = true;
                let content = std::fs::read_to_string(path).unwrap_or_default();
                let hash = self.compute_hash(&content);

                obligations.push(Obligation {
                    kind: ObligationKind::Container,
                    ref_path: path.to_string_lossy().to_string(),
                    title: "Dockerfile exists - Containerization Required".to_string(),
                    why_short: "Silicon Valley hygiene: All work must be containerized".to_string(),
                    evidence: Evidence {
                        source: "workspace".to_string(),
                        id: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        hash: Some(hash),
                        timestamp: None,
                    },
                    relevance_score: 0.95,
                });
                break;
            }
        }

        // If no Dockerfile, add obligation to create one
        if !found_dockerfile {
            obligations.push(Obligation {
                kind: ObligationKind::Container,
                ref_path: "Dockerfile".to_string(),
                title: "No Dockerfile - Containerization Required".to_string(),
                why_short: "Silicon Valley hygiene: Create Dockerfile before proceeding"
                    .to_string(),
                evidence: Evidence {
                    source: "workspace".to_string(),
                    id: "dockerfile_missing".to_string(),
                    hash: None,
                    timestamp: None,
                },
                relevance_score: 0.90,
            });
        }

        // Check for .dockerignore
        let dockerignore_path = self.repo_root.join(".dockerignore");
        if !dockerignore_path.exists() {
            obligations.push(Obligation {
                kind: ObligationKind::Container,
                ref_path: ".dockerignore".to_string(),
                title: "No .dockerignore - Add for efficient builds".to_string(),
                why_short: "Prevent unnecessary files from being copied to containers".to_string(),
                evidence: Evidence {
                    source: "workspace".to_string(),
                    id: "dockerignore_missing".to_string(),
                    hash: None,
                    timestamp: None,
                },
                relevance_score: 0.70,
            });
        }

        Ok(obligations)
    }

    /// Check if an operation is high-risk
    pub fn is_high_risk_op(&self, op: &str, touched_paths: &[String]) -> bool {
        let high_risk_ops = [
            "git", "push", "deploy", "release", "auth", "security", "network",
        ];

        // Check operation name
        for risk_op in &high_risk_ops {
            if op.to_lowercase().contains(risk_op) {
                return true;
            }
        }

        // Check paths
        let high_risk_paths = [
            "auth",
            "credential",
            "secret",
            "password",
            "key",
            "cert",
            "network",
            "firewall",
            "security",
            "permission",
            "role",
            "deploy",
            "production",
            "release",
        ];

        for path in touched_paths {
            let path_lower = path.to_lowercase();
            for risk_path in &high_risk_paths {
                if path_lower.contains(risk_path) {
                    return true;
                }
            }
        }

        false
    }
}

/// Convert obligations to RPC blockers (for contradictions)
pub fn contradictions_to_blockers(contradictions: &[Contradiction]) -> Vec<Blocker> {
    contradictions
        .iter()
        .map(|c| Blocker {
            kind: BlockerKind::Conflict,
            message: c.description.clone(),
            resolve_hint: c.resolution_hint.clone(),
        })
        .collect()
}
