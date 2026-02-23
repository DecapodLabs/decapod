//! Interview engine for spec/architecture/security/ops generation
//!
//! The interview engine helps agents gather requirements from humans
//! through a structured question-and-answer process. It produces
//! industry-grade documentation with sensible defaults.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Current state of an interview
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InterviewState {
    /// Interview ID
    pub id: String,
    /// Project name
    pub project_name: String,
    /// Current section being interviewed
    pub current_section: String,
    /// Questions answered so far
    pub answers: HashMap<String, Answer>,
    /// Artifacts generated
    pub artifacts_generated: Vec<String>,
    /// Whether interview is complete
    pub is_complete: bool,
}

/// A question in the interview
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Question {
    /// Question ID
    pub id: String,
    /// Section this question belongs to
    pub section: String,
    /// The question text
    pub text: String,
    /// Why this question matters
    pub why_it_matters: String,
    /// Where the answer lands in docs
    pub lands_in: String,
    /// Expected answer type
    pub answer_type: AnswerType,
    /// Sensible default if available
    pub default_value: Option<String>,
    /// Options for choice answers
    pub options: Option<Vec<String>>,
    /// Whether this is a blocking question
    pub is_blocking: bool,
}

/// Answer types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerType {
    Text,
    Choice,
    MultiChoice,
    Boolean,
    Number,
}

/// An answer to a question
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Answer {
    /// Question ID
    pub question_id: String,
    /// The answer value
    pub value: serde_json::Value,
    /// Timestamp
    pub timestamp: String,
}

/// Generated artifact
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Artifact {
    /// Artifact type: spec, architecture, security, ops, adr
    pub artifact_type: String,
    /// File path
    pub path: PathBuf,
    /// Content
    pub content: String,
}

/// Interview sections
const SECTIONS: &[&str] = &[
    "overview",
    "purpose",
    "runtime",
    "architecture",
    "security",
    "operations",
    "done",
];

/// Get all interview questions
fn get_all_questions() -> Vec<Question> {
    vec![
        // Overview section
        Question {
            id: "project_name".to_string(),
            section: "overview".to_string(),
            text: "What is the name of this project?".to_string(),
            why_it_matters:
                "The project name appears in all documentation and identifies the work.".to_string(),
            lands_in: "docs/spec.md (title), docs/architecture.md".to_string(),
            answer_type: AnswerType::Text,
            default_value: None,
            options: None,
            is_blocking: true,
        },
        Question {
            id: "one_liner".to_string(),
            section: "overview".to_string(),
            text: "Describe this project in one sentence.".to_string(),
            why_it_matters:
                "A clear one-liner helps everyone quickly understand the project's purpose."
                    .to_string(),
            lands_in: "docs/spec.md (summary), README.md".to_string(),
            answer_type: AnswerType::Text,
            default_value: None,
            options: None,
            is_blocking: true,
        },
        // Purpose section
        Question {
            id: "problem".to_string(),
            section: "purpose".to_string(),
            text: "What problem does this project solve?".to_string(),
            why_it_matters: "Understanding the problem ensures the solution is fit for purpose."
                .to_string(),
            lands_in: "docs/spec.md (problem statement)".to_string(),
            answer_type: AnswerType::Text,
            default_value: None,
            options: None,
            is_blocking: true,
        },
        Question {
            id: "success".to_string(),
            section: "purpose".to_string(),
            text: "How will we know this project is successful?".to_string(),
            why_it_matters: "Success criteria define when the work is done and working."
                .to_string(),
            lands_in: "docs/spec.md (success criteria)".to_string(),
            answer_type: AnswerType::Text,
            default_value: None,
            options: None,
            is_blocking: false,
        },
        // Runtime section
        Question {
            id: "language".to_string(),
            section: "runtime".to_string(),
            text: "What programming language will you use?".to_string(),
            why_it_matters: "Language choice affects tooling, dependencies, and deployment."
                .to_string(),
            lands_in: "docs/architecture.md (runtime), docs/ops.md".to_string(),
            answer_type: AnswerType::Choice,
            default_value: Some("Rust".to_string()),
            options: Some(vec![
                "Rust".to_string(),
                "TypeScript".to_string(),
                "Python".to_string(),
                "Go".to_string(),
                "Other".to_string(),
            ]),
            is_blocking: true,
        },
        Question {
            id: "deployment".to_string(),
            section: "runtime".to_string(),
            text: "How will this be deployed?".to_string(),
            why_it_matters: "Deployment approach affects build configuration and operations."
                .to_string(),
            lands_in: "docs/ops.md (deployment), docs/architecture.md".to_string(),
            answer_type: AnswerType::Choice,
            default_value: Some("Docker container".to_string()),
            options: Some(vec![
                "Docker container".to_string(),
                "Binary/executable".to_string(),
                "Library/crate".to_string(),
                "Serverless function".to_string(),
                "Static site".to_string(),
                "Other".to_string(),
            ]),
            is_blocking: false,
        },
        // Architecture section
        Question {
            id: "core_components".to_string(),
            section: "architecture".to_string(),
            text: "What are the main components/modules?".to_string(),
            why_it_matters: "Component breakdown guides implementation structure.".to_string(),
            lands_in: "docs/architecture.md (components)".to_string(),
            answer_type: AnswerType::Text,
            default_value: None,
            options: None,
            is_blocking: false,
        },
        Question {
            id: "data_storage".to_string(),
            section: "architecture".to_string(),
            text: "How will data be stored?".to_string(),
            why_it_matters: "Storage choices affect reliability, performance, and operations."
                .to_string(),
            lands_in: "docs/architecture.md (data), docs/ops.md".to_string(),
            answer_type: AnswerType::Choice,
            default_value: Some("SQLite (local)".to_string()),
            options: Some(vec![
                "SQLite (local)".to_string(),
                "PostgreSQL".to_string(),
                "No document store".to_string(),
                "File-based".to_string(),
                "In-memory only".to_string(),
                "Other".to_string(),
            ]),
            is_blocking: false,
        },
        // Security section
        Question {
            id: "secrets".to_string(),
            section: "security".to_string(),
            text: "Will this handle secrets or credentials?".to_string(),
            why_it_matters: "Secret handling requires special care for security compliance."
                .to_string(),
            lands_in: "docs/security.md (secrets)".to_string(),
            answer_type: AnswerType::Boolean,
            default_value: Some("false".to_string()),
            options: None,
            is_blocking: false,
        },
        Question {
            id: "user_data".to_string(),
            section: "security".to_string(),
            text: "Will this process user data or PII?".to_string(),
            why_it_matters: "User data requires privacy considerations and compliance.".to_string(),
            lands_in: "docs/security.md (privacy)".to_string(),
            answer_type: AnswerType::Boolean,
            default_value: Some("false".to_string()),
            options: None,
            is_blocking: false,
        },
        Question {
            id: "network".to_string(),
            section: "security".to_string(),
            text: "Will this accept network connections?".to_string(),
            why_it_matters: "Network exposure increases attack surface and requires hardening."
                .to_string(),
            lands_in: "docs/security.md (network)".to_string(),
            answer_type: AnswerType::Boolean,
            default_value: Some("false".to_string()),
            options: None,
            is_blocking: false,
        },
        // Operations section
        Question {
            id: "logging".to_string(),
            section: "operations".to_string(),
            text: "What log level is appropriate for production?".to_string(),
            why_it_matters: "Log levels affect observability and storage costs.".to_string(),
            lands_in: "docs/ops.md (monitoring)".to_string(),
            answer_type: AnswerType::Choice,
            default_value: Some("info".to_string()),
            options: Some(vec![
                "error".to_string(),
                "warn".to_string(),
                "info".to_string(),
                "debug".to_string(),
            ]),
            is_blocking: false,
        },
        Question {
            id: "health_checks".to_string(),
            section: "operations".to_string(),
            text: "What health checks are needed?".to_string(),
            why_it_matters: "Health checks enable automated recovery and monitoring.".to_string(),
            lands_in: "docs/ops.md (health)".to_string(),
            answer_type: AnswerType::Text,
            default_value: Some("Basic liveness check".to_string()),
            options: None,
            is_blocking: false,
        },
    ]
}

/// Get the next question for the interview
pub fn next_question(state: &InterviewState) -> Option<Question> {
    let all_questions = get_all_questions();
    let current_section_idx = SECTIONS.iter().position(|&s| s == state.current_section)?;

    // Find first unanswered question in current or next sections
    for section in &SECTIONS[current_section_idx..] {
        for question in &all_questions {
            if question.section == *section && !state.answers.contains_key(&question.id) {
                return Some(question.clone());
            }
        }
    }

    None
}

/// Apply an answer to the interview state
pub fn apply_answer(
    state: &mut InterviewState,
    question_id: &str,
    value: serde_json::Value,
) -> Result<(), DecapodError> {
    let all_questions = get_all_questions();

    // Validate question exists
    let question = all_questions
        .iter()
        .find(|q| q.id == question_id)
        .ok_or_else(|| {
            DecapodError::ValidationError(format!("Unknown question: {}", question_id))
        })?;

    // Add answer
    state.answers.insert(
        question_id.to_string(),
        Answer {
            question_id: question_id.to_string(),
            value,
            timestamp: crate::core::time::now_epoch_z(),
        },
    );

    // Update current section
    state.current_section = question.section.clone();

    // Check if interview is complete (all blocking questions answered)
    let blocking_answered = all_questions
        .iter()
        .filter(|q| q.is_blocking)
        .all(|q| state.answers.contains_key(&q.id));

    if blocking_answered && state.current_section == "done" {
        state.is_complete = true;
    }

    Ok(())
}

/// Generate documentation artifacts from interview state
pub fn generate_artifacts(
    state: &InterviewState,
    output_dir: &Path,
) -> Result<Vec<Artifact>, DecapodError> {
    let mut artifacts = vec![
        // Generate spec.md
        generate_spec(state, output_dir)?,
        // Generate architecture.md
        generate_architecture(state, output_dir)?,
        // Generate security.md
        generate_security(state, output_dir)?,
        // Generate ops.md
        generate_ops(state, output_dir)?,
    ];

    // Generate ADR if significant decisions
    if has_significant_decisions(state) {
        artifacts.push(generate_adr(state, output_dir)?);
    }

    Ok(artifacts)
}

/// Generate spec.md
fn generate_spec(state: &InterviewState, output_dir: &Path) -> Result<Artifact, DecapodError> {
    let project_name =
        get_answer(state, "project_name").unwrap_or_else(|| "Untitled Project".to_string());
    let one_liner =
        get_answer(state, "one_liner").unwrap_or_else(|| "A software project".to_string());
    let problem = get_answer(state, "problem").unwrap_or_else(|| "To be determined".to_string());
    let success =
        get_answer(state, "success").unwrap_or_else(|| "System functions correctly".to_string());

    let content = format!(
        r#"# {project_name}

{one_liner}

## Problem Statement

{problem}

## Success Criteria

{success}

## Scope

This specification defines the functional and non-functional requirements for {project_name}.

## Non-Goals

- Out of scope for initial implementation

## Assumptions

- Standard development environment
- Access to required dependencies

---
*Generated by Decapod Interview Engine*
"#
    );

    Ok(Artifact {
        artifact_type: "spec".to_string(),
        path: output_dir.join("docs/spec.md"),
        content,
    })
}

/// Generate architecture.md
fn generate_architecture(
    state: &InterviewState,
    output_dir: &Path,
) -> Result<Artifact, DecapodError> {
    let project_name =
        get_answer(state, "project_name").unwrap_or_else(|| "Untitled Project".to_string());
    let language = get_answer(state, "language").unwrap_or_else(|| "Rust".to_string());
    let components =
        get_answer(state, "core_components").unwrap_or_else(|| "Core module".to_string());
    let data_storage =
        get_answer(state, "data_storage").unwrap_or_else(|| "File-based".to_string());

    let content = format!(
        r#"# Architecture: {project_name}

## Overview

{project_name} is implemented in {language} following a modular architecture.

## Components

{components}

## Data Storage

{data_storage}

## Dependencies

- Standard library
- Required crates TBD

## Design Principles

- Local-first: All state is local and auditable
- Deterministic: Behavior is predictable and reproducible
- Agent-native: Designed for programmatic access

---
*Generated by Decapod Interview Engine*
"#
    );

    Ok(Artifact {
        artifact_type: "architecture".to_string(),
        path: output_dir.join("docs/architecture.md"),
        content,
    })
}

/// Generate security.md
fn generate_security(state: &InterviewState, output_dir: &Path) -> Result<Artifact, DecapodError> {
    let project_name =
        get_answer(state, "project_name").unwrap_or_else(|| "Untitled Project".to_string());
    let handles_secrets = get_answer(state, "secrets")
        .map(|v| v == "true")
        .unwrap_or(false);
    let handles_pii = get_answer(state, "user_data")
        .map(|v| v == "true")
        .unwrap_or(false);
    let has_network = get_answer(state, "network")
        .map(|v| v == "true")
        .unwrap_or(false);

    let mut sections = vec![];

    if handles_secrets {
        sections.push(
            r#"## Secrets Management

- Secrets are never logged
- Secrets are never committed to version control
- Secrets are rotated regularly
- Use environment variables or dedicated secret stores
"#
            .to_string(),
        );
    }

    if handles_pii {
        sections.push(
            r#"## Privacy & Data Protection

- User data is handled according to privacy principles
- Data minimization: only collect what's necessary
- Access controls restrict who can view user data
"#
            .to_string(),
        );
    }

    if has_network {
        sections.push(
            r#"## Network Security

- Input validation on all network inputs
- Rate limiting to prevent abuse
- Use TLS for all connections
- Keep dependencies updated
"#
            .to_string(),
        );
    }

    let content = format!(
        r#"# Security: {project_name}

## Security Posture

{sections}
## General Security Practices

- Follow principle of least privilege
- Validate all inputs
- Keep dependencies updated
- Review code for security issues
- Test security controls

---
*Generated by Decapod Interview Engine*
"#,
        project_name = project_name,
        sections = sections.join("\n")
    );

    Ok(Artifact {
        artifact_type: "security".to_string(),
        path: output_dir.join("docs/security.md"),
        content,
    })
}

/// Generate ops.md
fn generate_ops(state: &InterviewState, output_dir: &Path) -> Result<Artifact, DecapodError> {
    let project_name =
        get_answer(state, "project_name").unwrap_or_else(|| "Untitled Project".to_string());
    let deployment = get_answer(state, "deployment").unwrap_or_else(|| "Binary".to_string());
    let log_level = get_answer(state, "logging").unwrap_or_else(|| "info".to_string());
    let health_checks =
        get_answer(state, "health_checks").unwrap_or_else(|| "Basic liveness".to_string());

    let content = format!(
        r#"# Operations: {project_name}

## Deployment

{deployment}

## Monitoring

- Log level: {log_level}
- Health checks: {health_checks}

## Backup/Recovery

- Back up .decapod/data directory
- Store backups in version-controlled location
- Test recovery procedures

## Troubleshooting

- Check logs for errors
- Verify file permissions
- Validate configuration

---
*Generated by Decapod Interview Engine*
"#
    );

    Ok(Artifact {
        artifact_type: "ops".to_string(),
        path: output_dir.join("docs/ops.md"),
        content,
    })
}

/// Generate ADR for significant decisions
fn generate_adr(state: &InterviewState, output_dir: &Path) -> Result<Artifact, DecapodError> {
    let project_name =
        get_answer(state, "project_name").unwrap_or_else(|| "Untitled Project".to_string());
    let language = get_answer(state, "language").unwrap_or_else(|| "Rust".to_string());
    let data_storage =
        get_answer(state, "data_storage").unwrap_or_else(|| "File-based".to_string());

    let content = format!(
        r#"# ADR-0001: Core Technology Choices for {project_name}

## Status

Accepted

## Context

Initial technology selection for {project_name}.

## Decision

- **Language**: {language}
- **Storage**: {data_storage}

## Consequences

### Positive

- Standard toolchain
- Maintainable codebase

### Negative

- Technology lock-in
- Learning curve

---
*Generated by Decapod Interview Engine*
"#
    );

    let adr_path = output_dir.join(format!(
        "docs/decisions/ADR-0001-{}-core-tech.md",
        project_name.to_lowercase().replace(" ", "-")
    ));

    Ok(Artifact {
        artifact_type: "adr".to_string(),
        path: adr_path,
        content,
    })
}

/// Check if there are significant decisions worth an ADR
fn has_significant_decisions(state: &InterviewState) -> bool {
    state.answers.contains_key("language") || state.answers.contains_key("data_storage")
}

/// Get an answer value as string
fn get_answer(state: &InterviewState, question_id: &str) -> Option<String> {
    state.answers.get(question_id).map(|a| match &a.value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => a.value.to_string(),
    })
}

/// Initialize a new interview
pub fn init_interview(project_name: String) -> InterviewState {
    InterviewState {
        id: ulid::Ulid::new().to_string(),
        project_name,
        current_section: "overview".to_string(),
        answers: HashMap::new(),
        artifacts_generated: vec![],
        is_complete: false,
    }
}
