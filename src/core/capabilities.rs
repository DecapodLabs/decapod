//! Capability registry and definitions for Decapod.
//!
//! Capabilities are governed architectural responsibilities that refine
//! the repo-type baseline and materially affect generated specs,
//! scaffolding, validation gates, and proof requirements.

use crate::core::error;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A capability definition describing its effects on the project contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    /// Unique identifier for the capability (e.g., "public-api").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Purpose/description of the capability.
    pub purpose: String,
    /// Affected generated spec files.
    pub affected_specs: Vec<String>,
    /// Architectural decisions this capability requires.
    pub required_decisions: Vec<String>,
    /// Validation/proof obligations activated by this capability.
    pub proof_obligations: Vec<String>,
    /// Scaffolding recommendations.
    pub scaffolding_recommendations: Vec<String>,
    /// Evidence signals for inference.
    pub evidence_signals: Vec<String>,
    /// Capabilities that conflict with this one.
    pub conflicts: Vec<String>,
    /// Capabilities required by this one.
    pub requires: Vec<String>,
}

/// Registry of all supported capabilities.
pub struct CapabilityRegistry {
    capabilities: HashMap<String, CapabilityDefinition>,
}

impl CapabilityRegistry {
    /// Create a new registry with built-in capabilities.
    pub fn new() -> Self {
        let mut registry = Self {
            capabilities: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register built-in capabilities.
    fn register_builtins(&mut self) {
        self.register(CapabilityDefinition {
            id: "public-api".to_string(),
            name: "Public API".to_string(),
            purpose: "The repository exposes a public API with versioned contracts, authentication, and compatibility guarantees.".to_string(),
            affected_specs: vec![
                "INTERFACES.md".to_string(),
                "SECURITY.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "API versioning strategy".to_string(),
                "Authentication mechanism".to_string(),
                "Compatibility policy".to_string(),
                "Rate limiting and pagination".to_string(),
            ],
            proof_obligations: vec![
                "Interface contract tests".to_string(),
                "Compatibility regression tests".to_string(),
                "Malformed input handling tests".to_string(),
                "Authentication/authorization tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "api/ directory for route handlers".to_string(),
                "contracts/ directory for API schemas".to_string(),
                "tests/contract/ for contract tests".to_string(),
            ],
            evidence_signals: vec![
                "HTTP route handlers or API controllers".to_string(),
                "OpenAPI/Swagger schema files".to_string(),
                "API versioning in routes or headers".to_string(),
                "Authentication middleware or guards".to_string(),
            ],
            conflicts: vec!["internal-only".to_string()],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "persistent-state".to_string(),
            name: "Persistent State".to_string(),
            purpose: "The repository owns and manages durable state with transactions, migrations, and recovery procedures.".to_string(),
            affected_specs: vec![
                "ARCHITECTURE.md".to_string(),
                "SEMANTICS.md".to_string(),
                "OPERATIONS.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "State ownership and boundaries".to_string(),
                "Consistency model (strong/eventual)".to_string(),
                "Migration strategy and tooling".to_string(),
                "Backup and recovery procedures".to_string(),
            ],
            proof_obligations: vec![
                "Migration integration tests".to_string(),
                "Persistence integration tests".to_string(),
                "Rollback and recovery checks".to_string(),
                "Concurrency and conflict resolution tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "migrations/ directory for schema migrations".to_string(),
                "storage/ or repository/ abstraction layer".to_string(),
                "test fixtures for database integration tests".to_string(),
                ".decapod/generated/artifacts/custody/ directory for epistemic custody artifacts".to_string(),
            ],
            evidence_signals: vec![
                "Database migration files (e.g., migrations/*.sql)".to_string(),
                "ORM models or repository abstractions".to_string(),
                "Database connection pool configuration".to_string(),
                "Transaction boundary annotations".to_string(),
            ],
            conflicts: vec!["stateless".to_string()],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "stateless".to_string(),
            name: "Stateless".to_string(),
            purpose: "The repository does not maintain durable state between requests; all state is external or ephemeral.".to_string(),
            affected_specs: vec![
                "ARCHITECTURE.md".to_string(),
                "SEMANTICS.md".to_string(),
                "OPERATIONS.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "External state store".to_string(),
                "Session management strategy".to_string(),
            ],
            proof_obligations: vec![
                "No durable state in repository".to_string(),
                "External state store tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "No migrations directory needed".to_string(),
                "Stateless architecture documentation".to_string(),
            ],
            evidence_signals: vec![
                "No database migration files".to_string(),
                "No ORM models".to_string(),
                "No database connection pools".to_string(),
            ],
            conflicts: vec!["persistent-state".to_string()],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "background-processing".to_string(),
            name: "Background Processing".to_string(),
            purpose: "The repository runs background jobs with retries, idempotency, and poison-work handling.".to_string(),
            affected_specs: vec![
                "SEMANTICS.md".to_string(),
                "OPERATIONS.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Queue/broker technology".to_string(),
                "Retry policy and backoff strategy".to_string(),
                "Idempotency key design".to_string(),
                "Poison message handling".to_string(),
                "Graceful shutdown behavior".to_string(),
            ],
            proof_obligations: vec![
                "Duplicate delivery tests".to_string(),
                "Retry and backoff tests".to_string(),
                "Shutdown and recovery tests".to_string(),
                "Idempotency verification".to_string(),
            ],
            scaffolding_recommendations: vec![
                "workers/ or jobs/ directory for job definitions".to_string(),
                "Deterministic worker test harness".to_string(),
                "Queue configuration and health checks".to_string(),
            ],
            evidence_signals: vec![
                "Queue consumer/worker code".to_string(),
                "Retry middleware or configuration".to_string(),
                "Idempotency key handling".to_string(),
                "Dead letter queue handling".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "authentication".to_string(),
            name: "Authentication".to_string(),
            purpose: "The repository implements authentication with identity providers, token lifecycle, and credential handling.".to_string(),
            affected_specs: vec![
                "SECURITY.md".to_string(),
                "INTERFACES.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Identity provider selection".to_string(),
                "Token/session lifecycle".to_string(),
                "Credential handling and storage".to_string(),
                "Revocation and impersonation boundaries".to_string(),
            ],
            proof_obligations: vec![
                "Expired token tests".to_string(),
                "Revoked token tests".to_string(),
                "Malformed token tests".to_string(),
                "Unauthorized access tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "auth/ boundary module".to_string(),
                "Test identities and authorization fixtures".to_string(),
            ],
            evidence_signals: vec![
                "Authentication middleware or guards".to_string(),
                "Token validation logic".to_string(),
                "Identity provider configuration".to_string(),
                "Session/token store".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "authorization".to_string(),
            name: "Authorization".to_string(),
            purpose:
                "The repository enforces authorization policies with fine-grained access control."
                    .to_string(),
            affected_specs: vec![
                "SECURITY.md".to_string(),
                "INTERFACES.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Authorization model (RBAC/ABAC/etc.)".to_string(),
                "Policy decision point".to_string(),
                "Permission inheritance".to_string(),
            ],
            proof_obligations: vec![
                "Permission escalation tests".to_string(),
                "Access denial tests".to_string(),
                "Policy evaluation tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "authz/ policy module".to_string(),
                "Authorization test fixtures".to_string(),
            ],
            evidence_signals: vec![
                "Authorization middleware or guards".to_string(),
                "Policy decision point integration".to_string(),
                "Permission/role definitions".to_string(),
            ],
            conflicts: vec![],
            requires: vec!["authentication".to_string()],
        });

        self.register(CapabilityDefinition {
            id: "scheduled-jobs".to_string(),
            name: "Scheduled Jobs".to_string(),
            purpose: "The repository runs scheduled/cron jobs with deterministic execution and monitoring.".to_string(),
            affected_specs: vec![
                "OPERATIONS.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Scheduler technology".to_string(),
                "Schedule definition format".to_string(),
                "Overlap and concurrency policy".to_string(),
            ],
            proof_obligations: vec![
                "Schedule execution tests".to_string(),
                "Overlap handling tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "schedules/ or cron/ directory".to_string(),
                "Schedule test fixtures".to_string(),
            ],
            evidence_signals: vec![
                "Cron/scheduler configuration".to_string(),
                "Scheduled job definitions".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "event-driven".to_string(),
            name: "Event-Driven Architecture".to_string(),
            purpose: "The repository uses event-driven patterns with event sourcing, CQRS, or message brokers.".to_string(),
            affected_specs: vec![
                "ARCHITECTURE.md".to_string(),
                "SEMANTICS.md".to_string(),
                "INTERFACES.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Event broker selection".to_string(),
                "Event schema registry".to_string(),
                "Event ordering guarantees".to_string(),
                "Consumer idempotency".to_string(),
            ],
            proof_obligations: vec![
                "Event ordering tests".to_string(),
                "Consumer idempotency tests".to_string(),
                "Schema compatibility tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "events/ directory for event definitions".to_string(),
                "Event schema registry".to_string(),
                "Consumer test harness".to_string(),
            ],
            evidence_signals: vec![
                "Event publisher code".to_string(),
                "Event consumer/handler code".to_string(),
                "Event schema definitions".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "external-integrations".to_string(),
            name: "External Integrations".to_string(),
            purpose: "The repository integrates with external systems with contracts, resilience, and observability.".to_string(),
            affected_specs: vec![
                "INTERFACES.md".to_string(),
                "ARCHITECTURE.md".to_string(),
                "VALIDATION.md".to_string(),
                "OPERATIONS.md".to_string(),
            ],
            required_decisions: vec![
                "Integration contract format".to_string(),
                "Resilience patterns (circuit breaker, timeout)".to_string(),
                "Observability and tracing".to_string(),
            ],
            proof_obligations: vec![
                "Contract compatibility tests".to_string(),
                "Resilience pattern tests".to_string(),
                "Integration contract tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "integrations/ or clients/ directory".to_string(),
                "Contract test fixtures".to_string(),
                "Mock/stub implementations".to_string(),
            ],
            evidence_signals: vec![
                "HTTP/gRPC client code for external services".to_string(),
                "Circuit breaker configuration".to_string(),
                "External API schema definitions".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "multi-tenant".to_string(),
            name: "Multi-Tenant".to_string(),
            purpose: "The repository serves multiple tenants with isolation and per-tenant configuration.".to_string(),
            affected_specs: vec![
                "ARCHITECTURE.md".to_string(),
                "SECURITY.md".to_string(),
                "SEMANTICS.md".to_string(),
                "OPERATIONS.md".to_string(),
            ],
            required_decisions: vec![
                "Tenant isolation model (shared/db-per-tenant)".to_string(),
                "Tenant identification and routing".to_string(),
                "Per-tenant configuration and limits".to_string(),
            ],
            proof_obligations: vec![
                "Tenant isolation tests".to_string(),
                "Cross-tenant access prevention tests".to_string(),
                "Per-tenant resource limit tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "tenant/ context module".to_string(),
                "Tenant fixture builders".to_string(),
            ],
            evidence_signals: vec![
                "Tenant context/resolution code".to_string(),
                "Per-tenant configuration".to_string(),
            ],
            conflicts: vec![],
            requires: vec!["authentication".to_string(), "authorization".to_string()],
        });

        self.register(CapabilityDefinition {
            id: "secrets-handling".to_string(),
            name: "Secrets Handling".to_string(),
            purpose:
                "The repository manages secrets with rotation, encryption, and access control."
                    .to_string(),
            affected_specs: vec![
                "SECURITY.md".to_string(),
                "OPERATIONS.md".to_string(),
                "VALIDATION.md".to_string(),
            ],
            required_decisions: vec![
                "Secrets store/provider".to_string(),
                "Rotation schedule and automation".to_string(),
                "Encryption at rest and in transit".to_string(),
            ],
            proof_obligations: vec![
                "Secret rotation tests".to_string(),
                "Encryption verification tests".to_string(),
                "Access audit tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "secrets/ module".to_string(),
                "Secret rotation scripts".to_string(),
            ],
            evidence_signals: vec![
                "Secrets manager integration".to_string(),
                "Encryption configuration".to_string(),
                "Rotation scripts or automation".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });

        self.register(CapabilityDefinition {
            id: "infrastructure-management".to_string(),
            name: "Infrastructure Management".to_string(),
            purpose: "The repository manages infrastructure as code with plan-before-apply, drift detection, and rollback.".to_string(),
            affected_specs: vec![
                "ARCHITECTURE.md".to_string(),
                "SECURITY.md".to_string(),
                "VALIDATION.md".to_string(),
                "OPERATIONS.md".to_string(),
            ],
            required_decisions: vec![
                "IaC tool/format (Terraform, Pulumi, etc.)".to_string(),
                "Environment promotion strategy".to_string(),
                "Drift detection and remediation".to_string(),
            ],
            proof_obligations: vec![
                "Plan-before-apply checks".to_string(),
                "Drift detection tests".to_string(),
                "Rollback validation tests".to_string(),
            ],
            scaffolding_recommendations: vec![
                "infrastructure/ or iac/ directory".to_string(),
                "Environment separation".to_string(),
                "Policy check integration".to_string(),
            ],
            evidence_signals: vec![
                "Terraform/Pulumi/CloudFormation files".to_string(),
                "Plan/apply automation".to_string(),
                "Drift detection configuration".to_string(),
            ],
            conflicts: vec![],
            requires: vec![],
        });
    }

    /// Register a capability definition.
    pub fn register(&mut self, def: CapabilityDefinition) {
        let id = def.id.clone();
        assert!(
            self.capabilities.insert(id.clone(), def).is_none(),
            "duplicate built-in capability registration: {id}"
        );
    }

    /// Get a capability by ID.
    pub fn get(&self, id: &str) -> Option<&CapabilityDefinition> {
        self.capabilities.get(id)
    }

    /// Get all registered capabilities.
    pub fn all(&self) -> Vec<&CapabilityDefinition> {
        self.capabilities.values().collect()
    }

    /// Get all capability IDs.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.capabilities.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Canonicalize capabilities: sort and deduplicate.
    pub fn canonicalize_capabilities(capabilities: &[String]) -> Vec<String> {
        let caps: HashSet<_> = capabilities.iter().cloned().collect();
        let mut sorted: Vec<_> = caps.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Validate a set of capabilities, returning conflicts and missing requirements.
    /// Unknown capabilities are allowed (open vocabulary), but known capabilities
    /// are checked for conflicts and missing requirements.
    pub fn validate_capabilities(&self, capabilities: &[String]) -> Result<(), String> {
        let canonical = Self::canonicalize_capabilities(capabilities);
        let caps: HashSet<_> = canonical.iter().cloned().collect();

        // Check for duplicates (canonicalize handles this, but we can double-check)
        if capabilities.len() != caps.len() {
            return Err("Duplicate capabilities declared".to_string());
        }

        // Check conflicts - only for known capabilities
        for cap_id in &canonical {
            if let Some(def) = self.capabilities.get(cap_id) {
                for conflict in &def.conflicts {
                    if caps.contains(conflict) {
                        return Err(format!(
                            "Capability '{}' conflicts with '{}'",
                            cap_id, conflict
                        ));
                    }
                }
            }
        }

        // Check requirements - only for known capabilities
        for cap_id in &canonical {
            if let Some(def) = self.capabilities.get(cap_id) {
                for req in &def.requires {
                    if !caps.contains(req) {
                        return Err(format!(
                            "Capability '{}' requires '{}' to be declared",
                            cap_id, req
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Generate capability overlays for spec generation.
    pub fn generate_overlays(&self, capabilities: &[String]) -> Vec<CapabilityOverlay> {
        let canonical = Self::canonicalize_capabilities(capabilities);
        canonical
            .iter()
            .filter_map(|id| self.capabilities.get(id))
            .map(|def| CapabilityOverlay {
                capability_id: def.id.clone(),
                affected_specs: def.affected_specs.clone(),
                required_decisions: def.required_decisions.clone(),
                proof_obligations: def.proof_obligations.clone(),
            })
            .collect()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A capability overlay for spec generation.
#[derive(Debug, Clone)]
pub struct CapabilityOverlay {
    pub capability_id: String,
    pub affected_specs: Vec<String>,
    pub required_decisions: Vec<String>,
    pub proof_obligations: Vec<String>,
}

/// Inference result for a capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInference {
    pub capability_id: String,
    pub evidence: Vec<String>,
    pub confidence: f32,
    pub generation_impact: Vec<String>,
    pub recommended_action: String,
}

/// Detect capability evidence in the repository.
pub fn infer_capabilities(
    repo_root: &Path,
) -> Result<Vec<CapabilityInference>, error::DecapodError> {
    let mut inferences = Vec::new();
    let registry = CapabilityRegistry::new();

    for def in registry.all() {
        let mut evidence = Vec::new();

        for signal in &def.evidence_signals {
            if check_evidence_signal(repo_root, signal)? {
                evidence.push(signal.clone());
            }
        }

        if !evidence.is_empty() {
            let confidence = (evidence.len() as f32) / (def.evidence_signals.len() as f32);
            let generation_impact = def.affected_specs.clone();
            let recommended_action = if confidence > 0.5 {
                "Review and declare capability".to_string()
            } else {
                "Monitor for additional evidence".to_string()
            };

            inferences.push(CapabilityInference {
                capability_id: def.id.clone(),
                evidence,
                confidence,
                generation_impact,
                recommended_action,
            });
        }
    }

    Ok(inferences)
}

/// Check a single evidence signal against the repository.
fn check_evidence_signal(repo_root: &Path, signal: &str) -> Result<bool, error::DecapodError> {
    match signal {
        "HTTP route handlers or API controllers" => check_file_patterns(
            repo_root,
            &[
                "**/*handler*.rs",
                "**/*controller*.rs",
                "**/routes.rs",
                "**/api/*.rs",
            ],
        ),
        "OpenAPI/Swagger schema files" => check_file_patterns(
            repo_root,
            &[
                "**/openapi*.yaml",
                "**/openapi*.json",
                "**/swagger*.yaml",
                "**/swagger*.json",
            ],
        ),
        "API versioning in routes or headers" => {
            check_content_patterns(repo_root, &["v1/", "v2/", "api-version", "version"])
        }
        "Authentication middleware or guards" => check_content_patterns(
            repo_root,
            &["auth", "middleware", "guard", "jwt", "token", "bearer"],
        ),
        "Database migration files (e.g., migrations/*.sql)" => check_file_patterns(
            repo_root,
            &[
                "migrations/*.sql",
                "migrations/*.rs",
                "db/migrate/*.rb",
                "migrations/*.py",
                "migrations/",
                "db/migrate/",
                "alembic.ini",
                "prisma/migrations/",
            ],
        ),
        "ORM models or repository abstractions" => check_content_patterns(
            repo_root,
            &[
                "entity",
                "model",
                "repository",
                "orm",
                "diesel",
                "sqlx",
                "sea-orm",
                "prisma",
            ],
        ),
        "Database connection pool configuration" => check_content_patterns(
            repo_root,
            &["pool", "connection", "database_url", "DATABASE_URL"],
        ),
        "Transaction boundary annotations" => check_content_patterns(
            repo_root,
            &["transaction", "txn", "begin", "commit", "rollback"],
        ),
        "Queue consumer/worker code" => check_content_patterns(
            repo_root,
            &["consumer", "worker", "queue", "job", "handler"],
        ),
        "Retry middleware or configuration" => {
            check_content_patterns(repo_root, &["retry", "backoff", "attempts", "max_retries"])
        }
        "Idempotency key handling" => {
            check_content_patterns(repo_root, &["idempotent", "idempotency", "dedup"])
        }
        "Dead letter queue handling" => {
            check_file_patterns(repo_root, &["dead_letter", "dlq", "poison"])
        }
        "Cron/scheduler configuration" => {
            check_content_patterns(repo_root, &["cron", "schedule", "cronjob", "@cron"])
        }
        "Scheduled job definitions" => check_file_patterns(
            repo_root,
            &["**/jobs/*.rs", "**/cron/*.rs", "**/schedules/*.rs"],
        ),
        "Event publisher code" => {
            check_content_patterns(repo_root, &["publish", "emit", "event", "dispatch"])
        }
        "Event consumer/handler code" => {
            check_content_patterns(repo_root, &["consume", "handler", "subscriber", "listener"])
        }
        "Event schema definitions" => check_file_patterns(
            repo_root,
            &["**/events/*.json", "**/events/*.proto", "**/events/*.avsc"],
        ),
        "HTTP/gRPC client code for external services" => check_content_patterns(
            repo_root,
            &["client", "http", "grpc", "reqwest", "tonic", "grpcio"],
        ),
        "Circuit breaker configuration" => {
            check_content_patterns(repo_root, &["circuit_breaker", "circuitbreaker", "breaker"])
        }
        "External API schema definitions" => check_file_patterns(
            repo_root,
            &["**/proto/*.proto", "**/openapi/*.yaml", "**/schemas/*.json"],
        ),
        "Terraform/Pulumi/CloudFormation files" => check_file_patterns(
            repo_root,
            &[
                "**/*.tf",
                "**/*.tfvars",
                "**/Pulumi.yaml",
                "**/pulumi/**",
                "**/template.yaml",
                "**/cloudformation/**",
            ],
        ),
        "Plan/apply automation" => {
            check_content_patterns(repo_root, &["plan", "apply", "terraform", "pulumi"])
        }
        "Drift detection configuration" => {
            check_content_patterns(repo_root, &["drift", "driftctl", "detect"])
        }
        "Tenant context/resolution code" => {
            check_content_patterns(repo_root, &["tenant", "org_id", "organization_id"])
        }
        "Per-tenant configuration" => {
            check_content_patterns(repo_root, &["tenant_config", "per_tenant", "tenant_config"])
        }
        "Secrets manager integration" => check_content_patterns(
            repo_root,
            &[
                "vault",
                "secrets",
                "secret_manager",
                "aws_secrets",
                "azure_keyvault",
                "gcp_secret",
            ],
        ),
        "Encryption configuration" => {
            check_content_patterns(repo_root, &["encrypt", "decrypt", "kms", "aes", "rsa"])
        }
        "Rotation scripts or automation" => {
            check_content_patterns(repo_root, &["rotate", "rotation", "renew"])
        }
        _ => Ok(false),
    }
}

/// Check if any file matching patterns exists.
fn check_file_patterns(repo_root: &Path, patterns: &[&str]) -> Result<bool, error::DecapodError> {
    for pattern in patterns {
        if glob_match(repo_root, pattern)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check if any file contains any of the patterns.
fn check_content_patterns(
    repo_root: &Path,
    patterns: &[&str],
) -> Result<bool, error::DecapodError> {
    let walker = walkdir::WalkDir::new(repo_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let ext = e.path().extension().and_then(|s| s.to_str()).unwrap_or("");
            matches!(
                ext,
                "rs" | "py"
                    | "js"
                    | "ts"
                    | "go"
                    | "java"
                    | "kt"
                    | "rb"
                    | "php"
                    | "cs"
                    | "cpp"
                    | "cc"
                    | "h"
                    | "hpp"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
                    | "sql"
            )
        });

    for entry in walker {
        let path = entry.path();
        if let Ok(content) = std::fs::read_to_string(path) {
            let lower = content.to_lowercase();
            for pattern in patterns {
                if lower.contains(&pattern.to_lowercase()) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Simple glob matching for file patterns.
fn glob_match(repo_root: &Path, pattern: &str) -> Result<bool, error::DecapodError> {
    let pattern = pattern.replace("**/", "");
    let walker = walkdir::WalkDir::new(repo_root)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if entry.file_type().is_file() {
            let path_str = entry.path().to_string_lossy();
            if path_str.contains(&pattern.replace("*", "")) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Apply capability overlays to generated spec content.
pub fn apply_capability_overlays(
    spec_path: &str,
    mut content: String,
    capabilities: &[String],
) -> String {
    let canonical = CapabilityRegistry::canonicalize_capabilities(capabilities);
    if canonical.is_empty() {
        return content;
    }

    let registry = CapabilityRegistry::new();

    if spec_path_ends_with("INTENT.md", spec_path) {
        content = apply_capability_declaration(content, &canonical);
    }

    for cap_id in &canonical {
        if let Some(def) = registry.get(cap_id.as_str()) {
            // Check if this capability affects this spec
            if def
                .affected_specs
                .iter()
                .any(|s| spec_path_ends_with(s, spec_path))
            {
                content = apply_overlay(content, cap_id, spec_path);
            }
        }
    }
    content
}

/// Reconcile capability-owned sections while preserving authored content outside
/// the explicit marker ranges. This is the controlled regeneration path used by
/// `specs.refresh` after a capability declaration changes.
pub fn reconcile_capability_overlays(
    spec_path: &str,
    mut content: String,
    capabilities: &[String],
) -> String {
    for definition in CapabilityRegistry::new().all() {
        let start = format!(
            "<!-- decapod:capability-overlay:{}:start -->",
            definition.id
        );
        let end = format!("<!-- decapod:capability-overlay:{}:end -->", definition.id);
        while let Some(start_pos) = content.find(&start) {
            let Some(end_offset) = content[start_pos..].find(&end) else {
                break;
            };
            let end_pos = start_pos + end_offset + end.len();
            content = remove_marked_block(content, start_pos, end_pos);
        }
    }

    apply_capability_overlays(spec_path, content, capabilities)
}

fn spec_path_ends_with(spec_name: &str, path: &str) -> bool {
    path.ends_with(spec_name) || path.contains(&format!("/{}", spec_name))
}

fn apply_overlay(content: String, capability_id: &str, spec_path: &str) -> String {
    let start_marker = format!(
        "<!-- decapod:capability-overlay:{}:start -->",
        capability_id
    );
    let end_marker = format!("<!-- decapod:capability-overlay:{}:end -->", capability_id);

    // Check if overlay already applied
    if content.contains(&start_marker) {
        return content;
    }

    let overlay = match (spec_path, capability_id) {
        (s, "public-api") if s.contains("INTERFACES") => Some(public_api_interfaces_overlay()),
        (s, "public-api") if s.contains("SECURITY") => Some(public_api_security_overlay()),
        (s, "public-api") if s.contains("VALIDATION") => Some(public_api_validation_overlay()),
        (s, "persistent-state") if s.contains("ARCHITECTURE") => {
            Some(persistent_state_architecture_overlay())
        }
        (s, "persistent-state") if s.contains("SEMANTICS") => {
            Some(persistent_state_semantics_overlay())
        }
        (s, "persistent-state") if s.contains("OPERATIONS") => {
            Some(persistent_state_operations_overlay())
        }
        (s, "persistent-state") if s.contains("VALIDATION") => {
            Some(persistent_state_validation_overlay())
        }
        (s, "background-processing") if s.contains("SEMANTICS") => {
            Some(background_processing_semantics_overlay())
        }
        (s, "background-processing") if s.contains("OPERATIONS") => {
            Some(background_processing_operations_overlay())
        }
        (s, "background-processing") if s.contains("VALIDATION") => {
            Some(background_processing_validation_overlay())
        }
        _ => None,
    };

    if let Some(overlay) = overlay {
        let overlay = normalize_overlay_language(overlay);
        let overlay = format!("{start_marker}\n{overlay}\n{end_marker}");
        // Insert overlay before the first major section after the title
        if let Some(pos) = content.find("\n## ") {
            let mut result = content[..pos].to_string();
            result.push_str(&format!("\n\n{}", overlay));
            result.push_str(&content[pos..]);
            return result;
        }
    }
    content
}

/// Built-in packs transfer obligations without silently choosing local architecture,
/// service levels, or delivery guarantees for the project.
fn normalize_overlay_language(mut overlay: String) -> String {
    let replacements = [
        (
            "Deprecation window: minimum 90 days before removal",
            "Deprecation and removal policy MUST be selected for this project and proven against its consumers",
        ),
        (
            "Rate limiting MUST be enforced at the API gateway level",
            "Abuse-control enforcement point MUST be a documented project decision",
        ),
        (
            "Per-client rate limits MUST be enforced",
            "Limits and enforcement boundaries MUST be selected for this deployment",
        ),
        (
            "Distributed rate limiting for clustered deployments",
            "Clustered enforcement behavior MUST be documented when applicable",
        ),
        (
            "Rate limit headers MUST be returned (Retry-After, X-RateLimit-*)",
            "Client-visible throttling behavior MUST be part of the contract when applicable",
        ),
        (
            "Storage layer MUST be abstracted behind repository interfaces",
            "Storage ownership, consistency behavior, and access boundaries MUST be explicit",
        ),
        (
            "Repository implementations MUST be swappable",
            "Portability or swappable implementations are project decisions, not universal requirements",
        ),
        (
            "Migration path for storage changes MUST be documented",
            "Migration and rollback treatment MUST match the selected storage technology",
        ),
        (
            "Automated backup schedule: daily incremental, weekly full",
            "Backup scope, schedule, retention, and restore evidence MUST be selected for the project",
        ),
        (
            "Recovery point objective (RPO): < 1 hour",
            "Recovery point objectives MUST be explicit project decisions, not assumed values",
        ),
        (
            "Recovery time objective (RTO): < 4 hours",
            "Recovery time objectives MUST be explicit project decisions, not assumed values",
        ),
        (
            "Backup restoration tested quarterly",
            "Restore verification cadence MUST be recorded with the operational proof plan",
        ),
        (
            "RPO/RTO targets documented",
            "Recovery objectives MUST be selected for the project and recorded as proof obligations",
        ),
        (
            "Recovery procedures tested quarterly",
            "Recovery test cadence MUST be selected for the project and recorded as a proof obligation",
        ),
        (
            "Exponential backoff with jitter (base: 2s, max: 5min, max attempts: 5)",
            "Retry and backoff behavior MUST be selected and documented for each work class",
        ),
        (
            "Dead letter queue for messages exceeding max attempts",
            "Poison-work handling MUST be selected and documented for each work class",
        ),
        (
            "Retry MUST NOT cause duplicate side effects (idempotency required)",
            "Retry MUST preserve the declared side-effect and idempotency semantics",
        ),
        (
            "All background jobs MUST be idempotent",
            "Each job MUST declare whether it is idempotent, transactional, compensating, or otherwise duplicate-safe",
        ),
        (
            "Idempotency keys REQUIRED for all mutating operations",
            "Deduplication or compensation mechanisms are project decisions and require proof",
        ),
        (
            "Duplicate execution MUST return original result",
            "Duplicate execution MUST follow the job's declared duplicate-handling semantics",
        ),
        (
            "Maximum drain time: 30 seconds",
            "Drain behavior and timeout MUST be selected for the deployment",
        ),
        (
            "Forceful termination after timeout with job requeue",
            "Termination and requeue behavior MUST be selected and proven for the deployment",
        ),
        (
            "Exactly-once processing verification",
            "Verify the declared delivery guarantee; do not claim exactly-once behavior without proof",
        ),
        (
            "Exponential backoff with jitter verified",
            "Configured retry/backoff policy verified",
        ),
        (
            "Max retry attempts enforced",
            "Configured retry bound or unbounded policy verified",
        ),
        (
            "Dead letter queue population verified",
            "Poison-work handling verified when the project declares it",
        ),
    ];
    for (from, to) in replacements {
        overlay = overlay.replace(from, to);
    }
    overlay
}

fn apply_capability_declaration(mut content: String, capabilities: &[String]) -> String {
    let start = "<!-- decapod:declared-capabilities:start -->";
    let end = "<!-- decapod:declared-capabilities:end -->";
    while let Some(start_pos) = content.find(start) {
        let Some(end_offset) = content[start_pos..].find(end) else {
            break;
        };
        let end_pos = start_pos + end_offset + end.len();
        content = remove_marked_block(content, start_pos, end_pos);
    }
    if capabilities.is_empty() {
        return content;
    }
    let body = capabilities
        .iter()
        .map(|capability| format!("- `{capability}`"))
        .collect::<Vec<_>>()
        .join("\n");
    let section = format!("{start}\n\n## Declared Capability Surfaces\n\n{body}\n\n{end}");
    if let Some(pos) = content.find("\n## ") {
        let mut result = content[..pos].to_string();
        result.push_str("\n\n");
        result.push_str(&section);
        result.push_str(&content[pos..]);
        result
    } else {
        format!("{}\n\n{}\n", content.trim_end(), section)
    }
}

fn remove_marked_block(mut content: String, start_pos: usize, end_pos: usize) -> String {
    let line_start = content[..start_pos]
        .rfind('\n')
        .map(|position| position + 1)
        .unwrap_or(start_pos);
    let line_end = content[end_pos..]
        .find('\n')
        .map(|position| end_pos + position + 1)
        .unwrap_or(end_pos);
    let remove_start = if line_start > 0 && content[..line_start].ends_with('\n') {
        line_start - 1
    } else {
        line_start
    };
    let remove_end = if content[line_end..].starts_with('\n') {
        line_end + 1
    } else {
        line_end
    };
    content.replace_range(remove_start..remove_end, "");
    content
}

// Overlay content functions - using regular strings to avoid raw string delimiter issues
fn public_api_interfaces_overlay() -> String {
    "\n\n## Public API Capability Overlay\n\n\
### API Contract Requirements\n\
- All public endpoints MUST define explicit request/response schemas\n\
- Versioning strategy MUST be documented (URL path or header-based)\n\
- All public endpoints MUST implement idempotency for mutating operations\n\
- Rate limiting and pagination MUST be implemented for list endpoints\n\
\n\
### Compatibility Guarantees\n\
- Backward-compatible changes ONLY within a version\n\
- Breaking changes require new version (v1, v2, etc.)\n\
- Deprecation window: minimum 90 days before removal\n\
\n\
### Security Requirements\n\
- All public endpoints MUST implement authentication\n\
- Rate limiting MUST be enforced at the API gateway level\n\
- Input validation MUST reject malformed requests with typed errors"
        .to_string()
}

fn public_api_security_overlay() -> String {
    "\n\n## Public API Security Overlay\n\n\
### Authentication Requirements\n\
- All public endpoints MUST validate authentication tokens\n\
- Token validation MUST include expiry, revocation, and scope checks\n\
- Anonymous access MUST be explicitly documented and justified\n\
\n\
### Input Validation\n\
- All request bodies MUST be validated against schemas\n\
- Reject requests with unknown fields (strict schema validation)\n\
- Size limits MUST be enforced on all request bodies\n\
\n\
### Rate Limiting\n\
- Per-client rate limits MUST be enforced\n\
- Distributed rate limiting for clustered deployments\n\
- Rate limit headers MUST be returned (Retry-After, X-RateLimit-*)"
        .to_string()
}

fn public_api_validation_overlay() -> String {
    "\n\n## Public API Validation Overlay\n\n\
### Contract Tests\n\
- All public endpoints MUST have contract tests\n\
- Request/response schema validation on every request\n\
- Compatibility regression tests for each version\n\
\n\
### Security Tests\n\
- Authentication bypass tests\n\
- Malformed input handling tests\n\
- Rate limit enforcement tests\n\
- Token expiry/revocation tests"
        .to_string()
}

fn persistent_state_architecture_overlay() -> String {
    "\n\n## Persistent State Architecture Overlay\n\n\
### State Ownership\n\
- Each entity type MUST have a designated state owner\n\
- State ownership boundaries MUST be explicitly documented\n\
- Cross-boundary state access MUST go through defined interfaces\n\
\n\
### Transaction Boundaries\n\
- All multi-entity mutations MUST occur within explicit transactions\n\
- Transaction boundaries MUST be documented in ARCHITECTURE.md\n\
- Compensating transactions for distributed operations\n\
\n\
### Storage Abstraction\n\
- Storage layer MUST be abstracted behind repository interfaces\n\
- Repository implementations MUST be swappable\n\
- Migration path for storage changes MUST be documented"
        .to_string()
}

fn persistent_state_semantics_overlay() -> String {
    "\n\n## Persistent State Semantics Overlay\n\n\
### Transaction Semantics\n\
- All multi-entity operations MUST be atomic\n\
- Read-after-write consistency within transaction boundaries\n\
- Eventual consistency windows MUST be documented\n\
\n\
### Migration Semantics\n\
- Schema migrations MUST be backward-compatible\n\
- Migration rollback procedures MUST be documented\n\
- Data integrity checks post-migration\n\
\n\
### Recovery Semantics\n\
- Point-in-time recovery capability\n\
- RPO/RTO targets documented\n\
- Recovery procedures tested quarterly"
        .to_string()
}

fn persistent_state_operations_overlay() -> String {
    "\n\n## Persistent State Operations Overlay\n\n\
### Backup & Recovery\n\
- Automated backup schedule: daily incremental, weekly full\n\
- Recovery point objective (RPO): < 1 hour\n\
- Recovery time objective (RTO): < 4 hours\n\
- Backup restoration tested quarterly\n\
\n\
### Migration Operations\n\
- All schema changes via migration files\n\
- Migration rollback procedures documented\n\
- Zero-downtime migration strategy for production\n\
- Migration health checks and rollback triggers"
        .to_string()
}

fn persistent_state_validation_overlay() -> String {
    "\n\n## Persistent State Validation Overlay\n\n\
### Migration Tests\n\
- All migrations MUST have integration tests\n\
- Rollback procedures MUST be tested\n\
- Data integrity checks post-migration\n\
\n\
### Persistence Integration Tests\n\
- Repository abstraction tested against real database\n\
- Transaction boundary tests\n\
- Concurrency conflict tests\n\
- Data integrity validation after recovery"
        .replace(
            "### Migration Tests",
            "### Migration Proof Command\n- Configure `repo.migration_validation.command` and its arguments as the executable migration proof; file presence is not proof\n- The configured command MUST define its working directory, timeout, expected exit code, and evidence output\n\n### Migration Tests",
        )
}

fn background_processing_semantics_overlay() -> String {
    "\n\n## Background Processing Semantics Overlay\n\n\
### Retry Semantics\n\
- Exponential backoff with jitter (base: 2s, max: 5min, max attempts: 5)\n\
- Dead letter queue for messages exceeding max attempts\n\
- Retry MUST NOT cause duplicate side effects (idempotency required)\n\
\n\
### Idempotency\n\
- All background jobs MUST be idempotent\n\
- Idempotency keys REQUIRED for all mutating operations\n\
- Duplicate execution MUST return original result\n\
\n\
### Poison Message Handling\n\
- Messages failing after max retries go to dead letter queue\n\
- DLQ MUST be monitored and alerted\n\
- Manual replay capability for DLQ messages"
        .to_string()
}

fn background_processing_operations_overlay() -> String {
    "\n\n## Background Processing Operations Overlay\n\n\
### Queue Visibility\n\
- Queue depth, processing rate, and latency MUST be monitored\n\
- Dead letter queue MUST be visible and alerted\n\
- Worker health and processing rate metrics required\n\
\n\
### Shutdown Behavior\n\
- Graceful shutdown: stop accepting new work, finish current job\n\
- Maximum drain time: 30 seconds\n\
- Forceful termination after timeout with job requeue\n\
\n\
### Worker Health\n\
- Worker liveness and readiness probes\n\
- Queue depth alerts for backpressure detection\n\
- Processing latency percentiles (p50, p95, p99)"
        .to_string()
}

fn background_processing_validation_overlay() -> String {
    "\n\n## Background Processing Validation Overlay\n\n\
### Duplicate Delivery Tests\n\
- Same message delivered multiple times MUST produce same result\n\
- Idempotency key verification\n\
- Exactly-once processing verification\n\
\n\
### Retry Tests\n\
- Exponential backoff with jitter verified\n\
- Max retry attempts enforced\n\
- Dead letter queue population verified\n\
\n\
### Shutdown Tests\n\
- Graceful drain on signal\n\
- In-flight job completion or safe requeue\n\
- No data loss on forced termination"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_contains_builtins() {
        let registry = CapabilityRegistry::new();
        assert!(registry.get("public-api").is_some());
        assert!(registry.get("persistent-state").is_some());
        assert!(registry.get("background-processing").is_some());
        assert_eq!(registry.ids().len(), 12);
        assert_eq!(registry.ids().len(), registry.all().len());
    }

    #[test]
    #[should_panic(expected = "duplicate built-in capability registration")]
    fn duplicate_registry_ids_are_rejected() {
        let mut registry = CapabilityRegistry::new();
        registry.register(CapabilityDefinition {
            id: "public-api".to_string(),
            name: "duplicate".to_string(),
            purpose: String::new(),
            affected_specs: vec![],
            required_decisions: vec![],
            proof_obligations: vec![],
            scaffolding_recommendations: vec![],
            evidence_signals: vec![],
            conflicts: vec![],
            requires: vec![],
        });
    }

    #[test]
    fn test_validate_capabilities_valid() {
        let registry = CapabilityRegistry::new();
        let result = registry
            .validate_capabilities(&["public-api".to_string(), "persistent-state".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_capabilities_conflict() {
        let registry = CapabilityRegistry::new();
        // stateless conflicts with persistent-state
        let result = registry
            .validate_capabilities(&["stateless".to_string(), "persistent-state".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("conflicts"));
    }

    #[test]
    fn test_validate_capabilities_missing_requirement() {
        let registry = CapabilityRegistry::new();
        // authorization requires authentication
        let result = registry.validate_capabilities(&["authorization".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires"));
    }

    #[test]
    fn test_validate_capabilities_duplicate() {
        let registry = CapabilityRegistry::new();
        let result =
            registry.validate_capabilities(&["public-api".to_string(), "public-api".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn test_validate_unknown_capability() {
        let registry = CapabilityRegistry::new();
        let result = registry.validate_capabilities(&["unknown-capability".to_string()]);
        // Unknown capabilities are now allowed (open vocabulary)
        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_overlays() {
        let registry = CapabilityRegistry::new();
        let overlays =
            registry.generate_overlays(&["public-api".to_string(), "persistent-state".to_string()]);
        assert_eq!(overlays.len(), 2);
    }

    #[test]
    fn built_in_overlays_do_not_select_universal_service_levels() {
        let content = apply_capability_overlays(
            "VALIDATION.md",
            "# Validation\n\n## Gates\n".to_string(),
            &[
                "public-api".to_string(),
                "persistent-state".to_string(),
                "background-processing".to_string(),
            ],
        );
        for forbidden in [
            "90 days",
            "daily incremental",
            "< 1 hour",
            "< 4 hours",
            "30 seconds",
            "Exactly-once processing verification",
        ] {
            assert!(!content.contains(forbidden), "overlay contains {forbidden}");
        }
        assert!(content.contains("Migration Proof Command"));
        assert!(content.contains("project decision"));
    }
}
