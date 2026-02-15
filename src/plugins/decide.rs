use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use crate::plugins::federation;
use clap::{Parser, Subcommand};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

// --- Decision Tree Data Model (compiled into binary) ---

#[derive(Debug, Clone, Serialize)]
pub struct DecisionTree {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub keywords: &'static [&'static str],
    pub questions: &'static [DecisionQuestion],
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionQuestion {
    pub id: &'static str,
    pub prompt: &'static str,
    pub context: &'static str,
    pub options: &'static [DecisionOption],
    pub depends_on: Option<&'static str>,
    pub depends_value: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionOption {
    pub value: &'static str,
    pub label: &'static str,
    pub rationale: &'static str,
}

// --- Embedded Decision Trees ---

static DECISION_TREES: [&DecisionTree; 4] = [
    &TREE_WEB_APP,
    &TREE_MICROSERVICE,
    &TREE_CLI_TOOL,
    &TREE_LIBRARY,
];

pub fn decision_trees() -> &'static [&'static DecisionTree] {
    &DECISION_TREES
}

static TREE_WEB_APP: DecisionTree = DecisionTree {
    id: "web-app",
    name: "Web Application",
    description: "Browser-based application (SPA, PWA, or traditional)",
    keywords: &["web", "app", "website", "frontend", "spa", "pwa", "ui", "dashboard", "page"],
    questions: &[
        DecisionQuestion {
            id: "runtime",
            prompt: "Which runtime target?",
            context: "Determines the core language and execution model for your web app",
            options: &[
                DecisionOption {
                    value: "typescript",
                    label: "TypeScript",
                    rationale: "Broadest ecosystem, fastest iteration, largest talent pool",
                },
                DecisionOption {
                    value: "wasm",
                    label: "WebAssembly",
                    rationale: "Near-native performance, write in Rust/Go/C++, compile to browser",
                },
                DecisionOption {
                    value: "both",
                    label: "Hybrid (TS + WASM)",
                    rationale: "TypeScript app shell with WASM compute-heavy modules",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "framework",
            prompt: "Which TypeScript framework?",
            context: "Determines component model, state management patterns, and ecosystem",
            options: &[
                DecisionOption {
                    value: "react",
                    label: "React",
                    rationale: "Largest ecosystem, most third-party libraries, widest hiring pool",
                },
                DecisionOption {
                    value: "svelte",
                    label: "Svelte",
                    rationale: "Compile-time reactivity, minimal boilerplate, excellent DX",
                },
                DecisionOption {
                    value: "solid",
                    label: "SolidJS",
                    rationale: "Fine-grained reactivity, React-like API, excellent performance",
                },
                DecisionOption {
                    value: "vue",
                    label: "Vue",
                    rationale: "Progressive framework, gentle learning curve, strong tooling",
                },
                DecisionOption {
                    value: "vanilla",
                    label: "Vanilla (no framework)",
                    rationale: "Zero dependencies, full control, web standards only",
                },
            ],
            depends_on: Some("runtime"),
            depends_value: Some("typescript"),
        },
        DecisionQuestion {
            id: "framework_wasm",
            prompt: "Which WASM framework?",
            context: "Determines the Rust web framework for your WebAssembly application",
            options: &[
                DecisionOption {
                    value: "leptos",
                    label: "Leptos",
                    rationale: "Fine-grained reactivity, SSR support, active community",
                },
                DecisionOption {
                    value: "yew",
                    label: "Yew",
                    rationale: "React-like component model, mature ecosystem",
                },
                DecisionOption {
                    value: "dioxus",
                    label: "Dioxus",
                    rationale: "Cross-platform (web, desktop, mobile), React-like API",
                },
            ],
            depends_on: Some("runtime"),
            depends_value: Some("wasm"),
        },
        DecisionQuestion {
            id: "bundler",
            prompt: "Which bundler?",
            context: "Build tool that processes and bundles your source code for the browser",
            options: &[
                DecisionOption {
                    value: "vite",
                    label: "Vite",
                    rationale: "Fast dev server, Rollup-based production builds, framework-agnostic",
                },
                DecisionOption {
                    value: "rspack",
                    label: "Rspack",
                    rationale: "Rust-based webpack-compatible bundler, extremely fast builds",
                },
                DecisionOption {
                    value: "esbuild",
                    label: "esbuild",
                    rationale: "Go-based, fastest pure bundler, minimal configuration",
                },
                DecisionOption {
                    value: "none",
                    label: "None / framework default",
                    rationale: "Use framework's built-in build system (e.g., trunk for WASM)",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "styling",
            prompt: "Styling approach?",
            context: "How CSS is authored and organized in the project",
            options: &[
                DecisionOption {
                    value: "tailwind",
                    label: "Tailwind CSS",
                    rationale: "Utility-first, rapid prototyping, consistent design system",
                },
                DecisionOption {
                    value: "css_modules",
                    label: "CSS Modules",
                    rationale: "Scoped CSS, no runtime cost, works with any framework",
                },
                DecisionOption {
                    value: "styled_components",
                    label: "CSS-in-JS",
                    rationale: "Co-located styles, dynamic theming, component-scoped",
                },
                DecisionOption {
                    value: "vanilla_css",
                    label: "Vanilla CSS",
                    rationale: "Standard CSS, no build step, maximum browser compatibility",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "state_mgmt",
            prompt: "State management approach?",
            context: "How application state is managed across components",
            options: &[
                DecisionOption {
                    value: "built_in",
                    label: "Framework built-in",
                    rationale: "Use the framework's native state primitives (signals, stores, context)",
                },
                DecisionOption {
                    value: "zustand",
                    label: "Zustand",
                    rationale: "Minimal, hooks-based, no boilerplate (React ecosystem)",
                },
                DecisionOption {
                    value: "redux",
                    label: "Redux Toolkit",
                    rationale: "Predictable state container, time-travel debugging, mature ecosystem",
                },
                DecisionOption {
                    value: "none",
                    label: "None (local state only)",
                    rationale: "Component-local state only, no global store needed",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
    ],
};

static TREE_MICROSERVICE: DecisionTree = DecisionTree {
    id: "microservice",
    name: "Microservice",
    description: "Backend service exposing an API (REST, gRPC, or GraphQL)",
    keywords: &["microservice", "service", "api", "backend", "server", "rest", "grpc", "endpoint"],
    questions: &[
        DecisionQuestion {
            id: "language",
            prompt: "Which language?",
            context: "Primary language for the service implementation",
            options: &[
                DecisionOption {
                    value: "rust",
                    label: "Rust",
                    rationale: "Memory safety, excellent performance, strong type system (axum, actix)",
                },
                DecisionOption {
                    value: "go",
                    label: "Go",
                    rationale: "Fast compilation, built-in concurrency, simple deployment",
                },
                DecisionOption {
                    value: "typescript",
                    label: "TypeScript (Node.js)",
                    rationale: "Huge ecosystem, shared frontend/backend types, rapid iteration",
                },
                DecisionOption {
                    value: "python",
                    label: "Python",
                    rationale: "Rich ecosystem (FastAPI, Django), ML/data integration, rapid prototyping",
                },
                DecisionOption {
                    value: "java",
                    label: "Java / Kotlin",
                    rationale: "Enterprise ecosystem (Spring Boot), JVM performance, strong typing",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "api_style",
            prompt: "API style?",
            context: "Communication protocol and data exchange format",
            options: &[
                DecisionOption {
                    value: "rest",
                    label: "REST (HTTP/JSON)",
                    rationale: "Widest tooling support, human-readable, browser-friendly",
                },
                DecisionOption {
                    value: "grpc",
                    label: "gRPC (Protocol Buffers)",
                    rationale: "Strong typing, efficient binary format, streaming, code generation",
                },
                DecisionOption {
                    value: "graphql",
                    label: "GraphQL",
                    rationale: "Flexible queries, schema-first design, client-driven data fetching",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "container",
            prompt: "Container runtime?",
            context: "How the service is packaged for deployment",
            options: &[
                DecisionOption {
                    value: "docker",
                    label: "Docker",
                    rationale: "Industry standard, widest tooling and registry support",
                },
                DecisionOption {
                    value: "podman",
                    label: "Podman",
                    rationale: "Daemonless, rootless by default, Docker-compatible CLI",
                },
                DecisionOption {
                    value: "none",
                    label: "No container",
                    rationale: "Direct binary/process deployment, simpler for single-host setups",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "orchestration",
            prompt: "Orchestration platform?",
            context: "How containers are scheduled, scaled, and managed in production",
            options: &[
                DecisionOption {
                    value: "kubernetes",
                    label: "Kubernetes",
                    rationale: "Production-grade orchestration, auto-scaling, self-healing, complex",
                },
                DecisionOption {
                    value: "compose",
                    label: "Docker Compose",
                    rationale: "Simple multi-container orchestration, good for small deployments",
                },
                DecisionOption {
                    value: "none",
                    label: "None",
                    rationale: "Single binary deployment, systemd, or serverless platform",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "database",
            prompt: "Primary database?",
            context: "Main data persistence layer for the service",
            options: &[
                DecisionOption {
                    value: "postgres",
                    label: "PostgreSQL",
                    rationale: "Feature-rich relational DB, JSONB support, excellent extensions",
                },
                DecisionOption {
                    value: "mysql",
                    label: "MySQL / MariaDB",
                    rationale: "Widely deployed, good replication, strong hosting support",
                },
                DecisionOption {
                    value: "sqlite",
                    label: "SQLite",
                    rationale: "Embedded, zero-config, perfect for single-node services",
                },
                DecisionOption {
                    value: "mongodb",
                    label: "MongoDB",
                    rationale: "Document store, flexible schema, horizontal scaling",
                },
                DecisionOption {
                    value: "none",
                    label: "None / external",
                    rationale: "Stateless service, or database managed by another service",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "observability",
            prompt: "Observability stack?",
            context: "How the service exposes metrics, traces, and logs",
            options: &[
                DecisionOption {
                    value: "opentelemetry",
                    label: "OpenTelemetry",
                    rationale: "Vendor-neutral, unified traces/metrics/logs, CNCF standard",
                },
                DecisionOption {
                    value: "prometheus_grafana",
                    label: "Prometheus + Grafana",
                    rationale: "Pull-based metrics, mature dashboarding, wide adoption",
                },
                DecisionOption {
                    value: "custom",
                    label: "Custom / structured logging",
                    rationale: "JSON structured logs, custom metrics endpoint",
                },
                DecisionOption {
                    value: "none",
                    label: "None",
                    rationale: "Defer observability to a later phase",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
    ],
};

static TREE_CLI_TOOL: DecisionTree = DecisionTree {
    id: "cli-tool",
    name: "CLI Tool",
    description: "Command-line tool or utility",
    keywords: &["cli", "command", "terminal", "shell", "tool", "utility", "script"],
    questions: &[
        DecisionQuestion {
            id: "language",
            prompt: "Which language?",
            context: "Primary language for the CLI tool",
            options: &[
                DecisionOption {
                    value: "rust",
                    label: "Rust",
                    rationale: "Fast single binary, clap ecosystem, excellent error handling",
                },
                DecisionOption {
                    value: "go",
                    label: "Go",
                    rationale: "Fast compilation, cobra/viper ecosystem, simple cross-compilation",
                },
                DecisionOption {
                    value: "typescript",
                    label: "TypeScript (Node.js)",
                    rationale: "Rich npm ecosystem, commander/yargs, rapid development",
                },
                DecisionOption {
                    value: "python",
                    label: "Python",
                    rationale: "argparse/click/typer, rapid prototyping, extensive standard library",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "distribution",
            prompt: "Distribution strategy?",
            context: "How users will install and update the tool",
            options: &[
                DecisionOption {
                    value: "binary",
                    label: "Static binary",
                    rationale: "Single file, no runtime deps (cargo install, go install, GH releases)",
                },
                DecisionOption {
                    value: "package_manager",
                    label: "Package manager",
                    rationale: "npm/pip/brew install, version management, dependency resolution",
                },
                DecisionOption {
                    value: "both",
                    label: "Both",
                    rationale: "Binary releases + package manager for maximum reach",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "config_format",
            prompt: "Configuration file format?",
            context: "Format for the tool's configuration files",
            options: &[
                DecisionOption {
                    value: "toml",
                    label: "TOML",
                    rationale: "Human-friendly, good for config files, Rust/cargo ecosystem standard",
                },
                DecisionOption {
                    value: "yaml",
                    label: "YAML",
                    rationale: "Widely used in DevOps, supports complex structures, comments",
                },
                DecisionOption {
                    value: "json",
                    label: "JSON",
                    rationale: "Universal, no ambiguity, easy machine generation",
                },
                DecisionOption {
                    value: "none",
                    label: "None (flags only)",
                    rationale: "All configuration via CLI flags and environment variables",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "output_format",
            prompt: "Output format support?",
            context: "How the tool presents results to users and other programs",
            options: &[
                DecisionOption {
                    value: "text_json",
                    label: "Text + JSON",
                    rationale: "Human-readable by default, --format json for machine consumption",
                },
                DecisionOption {
                    value: "text_only",
                    label: "Text only",
                    rationale: "Human-oriented output, simple implementation",
                },
                DecisionOption {
                    value: "json_only",
                    label: "JSON only",
                    rationale: "Machine-first tool, piped into other programs",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
    ],
};

static TREE_LIBRARY: DecisionTree = DecisionTree {
    id: "library",
    name: "Library / Package",
    description: "Reusable library, crate, or package published to a registry",
    keywords: &["library", "lib", "crate", "package", "module", "sdk", "framework"],
    questions: &[
        DecisionQuestion {
            id: "language",
            prompt: "Which language?",
            context: "Primary language for the library",
            options: &[
                DecisionOption {
                    value: "rust",
                    label: "Rust",
                    rationale: "Zero-cost abstractions, crates.io ecosystem, strong type system",
                },
                DecisionOption {
                    value: "typescript",
                    label: "TypeScript",
                    rationale: "Largest package ecosystem (npm), broad reach, type safety",
                },
                DecisionOption {
                    value: "python",
                    label: "Python",
                    rationale: "PyPI distribution, wide adoption in data/ML/scripting",
                },
                DecisionOption {
                    value: "go",
                    label: "Go",
                    rationale: "Go modules, simple dependency management, fast compilation",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "packaging",
            prompt: "Package registry?",
            context: "Where the library will be published for consumption",
            options: &[
                DecisionOption {
                    value: "crates_io",
                    label: "crates.io",
                    rationale: "Rust's official registry, cargo integration",
                },
                DecisionOption {
                    value: "npm",
                    label: "npm",
                    rationale: "JavaScript/TypeScript registry, largest package count",
                },
                DecisionOption {
                    value: "pypi",
                    label: "PyPI",
                    rationale: "Python's official registry, pip/poetry integration",
                },
                DecisionOption {
                    value: "go_modules",
                    label: "Go Modules",
                    rationale: "Go's built-in module system, proxy.golang.org",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "doc_tooling",
            prompt: "Documentation tooling?",
            context: "How API documentation is generated and published",
            options: &[
                DecisionOption {
                    value: "rustdoc",
                    label: "rustdoc",
                    rationale: "Built into cargo, docs.rs hosting, inline doc comments",
                },
                DecisionOption {
                    value: "typedoc",
                    label: "TypeDoc",
                    rationale: "TypeScript API docs from JSDoc/TSDoc comments",
                },
                DecisionOption {
                    value: "sphinx",
                    label: "Sphinx",
                    rationale: "Python standard, reStructuredText/Markdown, ReadTheDocs hosting",
                },
                DecisionOption {
                    value: "mdbook",
                    label: "mdBook",
                    rationale: "Markdown-based book format, great for guides and tutorials",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
        DecisionQuestion {
            id: "testing",
            prompt: "Testing strategy?",
            context: "Primary testing approach for the library",
            options: &[
                DecisionOption {
                    value: "unit_integration",
                    label: "Unit + Integration",
                    rationale: "Standard test pyramid: unit tests + integration tests",
                },
                DecisionOption {
                    value: "property_based",
                    label: "Property-based",
                    rationale: "Generative testing (proptest, hypothesis, rapid-check)",
                },
                DecisionOption {
                    value: "snapshot",
                    label: "Snapshot testing",
                    rationale: "Assert against recorded outputs (insta, jest snapshots)",
                },
                DecisionOption {
                    value: "all",
                    label: "All of the above",
                    rationale: "Comprehensive: unit + integration + property + snapshot",
                },
            ],
            depends_on: None,
            depends_value: None,
        },
    ],
};

// --- Data Types (DB rows) ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DecisionSession {
    pub id: String,
    pub tree_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub federation_node_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decisions: Option<Vec<Decision>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Decision {
    pub id: String,
    pub session_id: String,
    pub question_id: String,
    pub tree_id: String,
    pub question_text: String,
    pub chosen_value: String,
    pub chosen_label: String,
    pub rationale: String,
    pub user_note: String,
    pub federation_node_id: Option<String>,
    pub created_at: String,
    pub actor: String,
}

#[derive(Debug, Serialize)]
pub struct TreeSuggestion {
    pub tree_id: String,
    pub tree_name: String,
    pub score: f64,
    pub matched_keywords: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct NextQuestionResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<serde_json::Value>,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// --- CLI ---

#[derive(Parser, Debug)]
#[clap(
    name = "decide",
    about = "Architecture decision prompting — curated engineering questions for AI agents."
)]
pub struct DecideCli {
    #[clap(subcommand)]
    pub command: DecideCommand,
}

#[derive(Subcommand, Debug)]
pub enum DecideCommand {
    /// Start a new decision session for a domain.
    Start {
        /// Decision tree ID (web-app, microservice, cli-tool, library)
        #[clap(long)]
        tree: String,
        /// Session title
        #[clap(long)]
        title: String,
        /// Optional description
        #[clap(long, default_value = "")]
        description: String,
        /// Actor
        #[clap(long, default_value = "decapod")]
        actor: String,
    },
    /// Record a single decision answer within a session.
    Record {
        /// Session ID
        #[clap(long)]
        session: String,
        /// Question ID within the tree
        #[clap(long)]
        question: String,
        /// Chosen option value
        #[clap(long)]
        value: String,
        /// Optional user-provided rationale
        #[clap(long, default_value = "")]
        rationale: String,
        /// Actor
        #[clap(long, default_value = "decapod")]
        actor: String,
    },
    /// Complete a session (marks it finished).
    Complete {
        /// Session ID
        #[clap(long)]
        session: String,
    },
    /// List recorded decisions.
    List {
        /// Filter by session ID
        #[clap(long)]
        session: Option<String>,
        /// Filter by tree ID
        #[clap(long)]
        tree: Option<String>,
    },
    /// Get a specific decision by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// List or get decision sessions.
    Session {
        #[clap(subcommand)]
        command: SessionSubCommand,
    },
    /// List available decision trees.
    Trees,
    /// Suggest which decision tree to use for a given prompt.
    Suggest {
        /// User prompt describing the project
        #[clap(long)]
        prompt: String,
    },
    /// Show the next unanswered question for a session.
    Next {
        /// Session ID
        #[clap(long)]
        session: String,
    },
    /// Initialize decisions DB (no-op if exists).
    Init,
    /// Print JSON schema for the decide subsystem.
    Schema,
}

#[derive(Subcommand, Debug)]
pub enum SessionSubCommand {
    /// List all sessions
    List {
        /// Filter by status (active, completed)
        #[clap(long)]
        status: Option<String>,
    },
    /// Get a specific session with all its decisions
    Get {
        #[clap(long)]
        id: String,
    },
}

// --- Helpers ---

fn now_ts() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}Z", secs)
}

fn decide_db_path(root: &Path) -> PathBuf {
    root.join(schemas::DECIDE_DB_NAME)
}

pub fn initialize_decide_db(root: &Path) -> Result<(), error::DecapodError> {
    let db_path = decide_db_path(root);
    let conn = crate::core::db::db_connect(&db_path.to_string_lossy())?;

    conn.execute_batch(schemas::DECIDE_DB_SCHEMA_META)?;
    conn.execute_batch(schemas::DECIDE_DB_SCHEMA_SESSIONS)?;
    conn.execute_batch(schemas::DECIDE_DB_SCHEMA_DECISIONS)?;
    conn.execute_batch(schemas::DECIDE_DB_INDEX_DECISIONS_SESSION)?;
    conn.execute_batch(schemas::DECIDE_DB_INDEX_DECISIONS_TREE)?;
    conn.execute_batch(schemas::DECIDE_DB_INDEX_SESSIONS_TREE)?;
    conn.execute_batch(schemas::DECIDE_DB_INDEX_SESSIONS_STATUS)?;

    Ok(())
}

fn find_tree(tree_id: &str) -> Result<&'static DecisionTree, error::DecapodError> {
    decision_trees()
        .iter()
        .copied()
        .find(|t| t.id == tree_id)
        .ok_or_else(|| {
            let valid: Vec<&str> = decision_trees().iter().map(|t| t.id).collect();
            error::DecapodError::ValidationError(format!(
                "Unknown tree '{}'. Available: {}",
                tree_id,
                valid.join(", ")
            ))
        })
}

fn find_question<'a>(
    tree: &'a DecisionTree,
    question_id: &str,
) -> Result<&'a DecisionQuestion, error::DecapodError> {
    tree.questions
        .iter()
        .find(|q| q.id == question_id)
        .ok_or_else(|| {
            error::DecapodError::ValidationError(format!(
                "Unknown question '{}' in tree '{}'",
                question_id, tree.id
            ))
        })
}

fn find_option<'a>(
    question: &'a DecisionQuestion,
    value: &str,
) -> Result<&'a DecisionOption, error::DecapodError> {
    question
        .options
        .iter()
        .find(|o| o.value == value)
        .ok_or_else(|| {
            let valid: Vec<&str> = question.options.iter().map(|o| o.value).collect();
            error::DecapodError::ValidationError(format!(
                "Invalid value '{}' for question '{}'. Valid: {}",
                value,
                question.id,
                valid.join(", ")
            ))
        })
}

// --- Tree suggestion ---

pub fn suggest_trees(prompt: &str) -> Vec<TreeSuggestion> {
    let tokens: Vec<String> = prompt
        .to_lowercase()
        .split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|s| s.len() >= 3)
        .collect();

    let mut suggestions: Vec<TreeSuggestion> = decision_trees()
        .iter()
        .map(|tree| {
            let matched: Vec<String> = tree
                .keywords
                .iter()
                .filter(|kw| tokens.iter().any(|t| t.contains(*kw) || kw.contains(t.as_str())))
                .map(|s| s.to_string())
                .collect();
            let score = if tree.keywords.is_empty() {
                0.0
            } else {
                matched.len() as f64 / tree.keywords.len() as f64
            };
            TreeSuggestion {
                tree_id: tree.id.to_string(),
                tree_name: tree.name.to_string(),
                score,
                matched_keywords: matched,
            }
        })
        .collect();

    suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    suggestions.retain(|s| s.score > 0.0);
    suggestions
}

// --- Next question resolution ---

fn resolve_next_question<'a>(
    tree: &'a DecisionTree,
    answered: &std::collections::HashMap<String, String>,
) -> Option<&'a DecisionQuestion> {
    for question in tree.questions {
        // Skip already answered
        if answered.contains_key(question.id) {
            continue;
        }

        // Check dependency constraint
        if let (Some(dep_id), Some(dep_val)) = (question.depends_on, question.depends_value) {
            match answered.get(dep_id) {
                Some(val) if val == dep_val => {} // dependency satisfied
                _ => continue,                     // skip: dependency not met
            }
        }

        return Some(question);
    }
    None
}

// --- Federation integration ---

fn create_session_federation_node(
    store: &Store,
    session_id: &str,
    tree_id: &str,
    title: &str,
    actor: &str,
) -> Result<String, error::DecapodError> {
    federation::initialize_federation_db(&store.root)?;
    let node = federation::add_node(
        store,
        &format!("Decision Session: {}", title),
        "decision",
        "notable",
        "agent_inferred",
        &format!(
            "Architecture decision session for tree '{}'. Session ID: {}",
            tree_id, session_id
        ),
        &format!("event:decide.session.start:{}", session_id),
        &format!("decide,session,{}", tree_id),
        "repo",
        None,
        actor,
    )?;
    Ok(node.id)
}

fn create_decision_federation_node(
    store: &Store,
    decision_id: &str,
    session_fed_node_id: &str,
    question_text: &str,
    chosen_label: &str,
    tree_id: &str,
    actor: &str,
) -> Result<String, error::DecapodError> {
    federation::initialize_federation_db(&store.root)?;
    let node = federation::add_node(
        store,
        &format!("{} -> {}", question_text, chosen_label),
        "decision",
        "background",
        "agent_inferred",
        &format!(
            "Chose '{}' for '{}'. Decision ID: {}",
            chosen_label, question_text, decision_id
        ),
        &format!("event:decide.record:{}", decision_id),
        &format!("decide,answer,{}", tree_id),
        "repo",
        None,
        actor,
    )?;

    // Link decision node to session node
    federation::add_edge(store, &node.id, session_fed_node_id, "depends_on")?;

    Ok(node.id)
}

// --- Core operations ---

fn start_session(
    store: &Store,
    tree_id: &str,
    title: &str,
    description: &str,
    actor: &str,
) -> Result<DecisionSession, error::DecapodError> {
    let tree = find_tree(tree_id)?;
    let _ = tree; // validate tree exists

    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);
    let now = now_ts();
    let session_id = format!("DS_{}", Ulid::new());

    // Create federation cross-link
    let fed_node_id = create_session_federation_node(store, &session_id, tree_id, title, actor)?;

    let session = broker.with_conn(&db_path, actor, None, "decide.start", |conn| {
        conn.execute(
            "INSERT INTO sessions(id, tree_id, title, description, status, federation_node_id, created_at, updated_at, dir_path, scope, actor)
             VALUES(?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, ?8, 'repo', ?9)",
            params![
                session_id,
                tree_id,
                title,
                description,
                fed_node_id,
                now,
                now,
                store.root.to_string_lossy().to_string(),
                actor,
            ],
        )?;

        Ok(DecisionSession {
            id: session_id.clone(),
            tree_id: tree_id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: "active".to_string(),
            federation_node_id: Some(fed_node_id),
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
            actor: actor.to_string(),
            decisions: None,
        })
    })?;

    Ok(session)
}

fn record_decision(
    store: &Store,
    session_id: &str,
    question_id: &str,
    value: &str,
    rationale: &str,
    actor: &str,
) -> Result<Decision, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    // Look up session to get tree_id and federation_node_id
    let (tree_id, session_fed_node_id) = broker.with_conn(
        &db_path,
        actor,
        None,
        "decide.record.lookup",
        |conn| {
            let mut stmt = conn.prepare(
                "SELECT tree_id, federation_node_id, status FROM sessions WHERE id = ?1",
            )?;
            let row = stmt
                .query_row(params![session_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| {
                    error::DecapodError::NotFound(format!("Session '{}' not found", session_id))
                })?;

            if row.2 != "active" {
                return Err(error::DecapodError::ValidationError(format!(
                    "Session '{}' is '{}', not 'active'",
                    session_id, row.2
                )));
            }

            Ok((row.0, row.1))
        },
    )?;

    // Validate question and option against the tree
    let tree = find_tree(&tree_id)?;
    let question = find_question(tree, question_id)?;
    let option = find_option(question, value)?;

    let now = now_ts();
    let decision_id = format!("DD_{}", Ulid::new());

    // Create federation cross-link
    let fed_node_id = if let Some(ref session_fed_id) = session_fed_node_id {
        Some(create_decision_federation_node(
            store,
            &decision_id,
            session_fed_id,
            question.prompt,
            option.label,
            &tree_id,
            actor,
        )?)
    } else {
        None
    };

    let decision = broker.with_conn(&db_path, actor, None, "decide.record", |conn| {
        // Check for duplicate (same session + question)
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM decisions WHERE session_id = ?1 AND question_id = ?2",
                params![session_id, question_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)?;

        if exists {
            return Err(error::DecapodError::ValidationError(format!(
                "Question '{}' already answered in session '{}'",
                question_id, session_id
            )));
        }

        conn.execute(
            "INSERT INTO decisions(id, session_id, question_id, tree_id, question_text, chosen_value, chosen_label, rationale, federation_node_id, created_at, actor)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                decision_id,
                session_id,
                question_id,
                tree_id,
                question.prompt,
                value,
                option.label,
                rationale,
                fed_node_id,
                now,
                actor,
            ],
        )?;

        // Update session's updated_at
        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;

        Ok(Decision {
            id: decision_id,
            session_id: session_id.to_string(),
            question_id: question_id.to_string(),
            tree_id: tree_id.clone(),
            question_text: question.prompt.to_string(),
            chosen_value: value.to_string(),
            chosen_label: option.label.to_string(),
            rationale: rationale.to_string(),
            user_note: String::new(),
            federation_node_id: fed_node_id,
            created_at: now,
            actor: actor.to_string(),
        })
    })?;

    Ok(decision)
}

fn complete_session(store: &Store, session_id: &str) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);
    let now = now_ts();

    broker.with_conn(&db_path, "cli", None, "decide.complete", |conn| {
        let updated = conn.execute(
            "UPDATE sessions SET status = 'completed', completed_at = ?1, updated_at = ?1 WHERE id = ?2 AND status = 'active'",
            params![now, session_id],
        )?;

        if updated == 0 {
            return Err(error::DecapodError::NotFound(format!(
                "Active session '{}' not found",
                session_id
            )));
        }

        Ok(())
    })?;

    Ok(())
}

fn get_session(store: &Store, session_id: &str) -> Result<DecisionSession, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    broker.with_conn(&db_path, "cli", None, "decide.session.get", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, tree_id, title, description, status, federation_node_id, created_at, updated_at, completed_at, actor
             FROM sessions WHERE id = ?1",
        )?;
        let session = stmt
            .query_row(params![session_id], |row| {
                Ok(DecisionSession {
                    id: row.get(0)?,
                    tree_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    status: row.get(4)?,
                    federation_node_id: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    actor: row.get(9)?,
                    decisions: None,
                })
            })
            .map_err(|_| {
                error::DecapodError::NotFound(format!("Session '{}' not found", session_id))
            })?;

        // Load decisions
        let mut dstmt = conn.prepare(
            "SELECT id, session_id, question_id, tree_id, question_text, chosen_value, chosen_label, rationale, user_note, federation_node_id, created_at, actor
             FROM decisions WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let decisions: Vec<Decision> = dstmt
            .query_map(params![session_id], |row| {
                Ok(Decision {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    question_id: row.get(2)?,
                    tree_id: row.get(3)?,
                    question_text: row.get(4)?,
                    chosen_value: row.get(5)?,
                    chosen_label: row.get(6)?,
                    rationale: row.get(7)?,
                    user_note: row.get(8)?,
                    federation_node_id: row.get(9)?,
                    created_at: row.get(10)?,
                    actor: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DecisionSession {
            decisions: Some(decisions),
            ..session
        })
    })
}

fn list_sessions(
    store: &Store,
    status_filter: Option<&str>,
) -> Result<Vec<DecisionSession>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    broker.with_conn(&db_path, "cli", None, "decide.session.list", |conn| {
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(status) = status_filter {
                (
                    "SELECT id, tree_id, title, description, status, federation_node_id, created_at, updated_at, completed_at, actor
                     FROM sessions WHERE status = ?1 ORDER BY created_at DESC".to_string(),
                    vec![Box::new(status.to_string())],
                )
            } else {
                (
                    "SELECT id, tree_id, title, description, status, federation_node_id, created_at, updated_at, completed_at, actor
                     FROM sessions ORDER BY created_at DESC".to_string(),
                    vec![],
                )
            };

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let sessions: Vec<DecisionSession> = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(DecisionSession {
                    id: row.get(0)?,
                    tree_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    status: row.get(4)?,
                    federation_node_id: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    actor: row.get(9)?,
                    decisions: None,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(sessions)
    })
}

fn list_decisions(
    store: &Store,
    session_filter: Option<&str>,
    tree_filter: Option<&str>,
) -> Result<Vec<Decision>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    broker.with_conn(&db_path, "cli", None, "decide.list", |conn| {
        let mut conditions: Vec<String> = vec![];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if let Some(sid) = session_filter {
            conditions.push(format!("session_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(sid.to_string()));
        }
        if let Some(tid) = tree_filter {
            conditions.push(format!("tree_id = ?{}", param_values.len() + 1));
            param_values.push(Box::new(tid.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, session_id, question_id, tree_id, question_text, chosen_value, chosen_label, rationale, user_note, federation_node_id, created_at, actor
             FROM decisions{} ORDER BY created_at",
            where_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let decisions: Vec<Decision> = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(Decision {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    question_id: row.get(2)?,
                    tree_id: row.get(3)?,
                    question_text: row.get(4)?,
                    chosen_value: row.get(5)?,
                    chosen_label: row.get(6)?,
                    rationale: row.get(7)?,
                    user_note: row.get(8)?,
                    federation_node_id: row.get(9)?,
                    created_at: row.get(10)?,
                    actor: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(decisions)
    })
}

fn get_decision(store: &Store, decision_id: &str) -> Result<Decision, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    broker.with_conn(&db_path, "cli", None, "decide.get", |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, question_id, tree_id, question_text, chosen_value, chosen_label, rationale, user_note, federation_node_id, created_at, actor
             FROM decisions WHERE id = ?1",
        )?;
        stmt.query_row(params![decision_id], |row| {
            Ok(Decision {
                id: row.get(0)?,
                session_id: row.get(1)?,
                question_id: row.get(2)?,
                tree_id: row.get(3)?,
                question_text: row.get(4)?,
                chosen_value: row.get(5)?,
                chosen_label: row.get(6)?,
                rationale: row.get(7)?,
                user_note: row.get(8)?,
                federation_node_id: row.get(9)?,
                created_at: row.get(10)?,
                actor: row.get(11)?,
            })
        })
        .map_err(|_| {
            error::DecapodError::NotFound(format!("Decision '{}' not found", decision_id))
        })
    })
}

fn next_question(
    store: &Store,
    session_id: &str,
) -> Result<NextQuestionResult, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = decide_db_path(&store.root);

    // Get session tree_id and answered questions
    let (tree_id, answered) = broker.with_conn(
        &db_path,
        "cli",
        None,
        "decide.next.lookup",
        |conn| {
            let tree_id: String = conn
                .query_row(
                    "SELECT tree_id FROM sessions WHERE id = ?1 AND status = 'active'",
                    params![session_id],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    error::DecapodError::NotFound(format!(
                        "Active session '{}' not found",
                        session_id
                    ))
                })?;

            let mut stmt = conn.prepare(
                "SELECT question_id, chosen_value FROM decisions WHERE session_id = ?1",
            )?;
            let answered: std::collections::HashMap<String, String> = stmt
                .query_map(params![session_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<_, _>>()?;

            Ok((tree_id, answered))
        },
    )?;

    let tree = find_tree(&tree_id)?;

    match resolve_next_question(tree, &answered) {
        Some(question) => {
            let q_json = serde_json::json!({
                "id": question.id,
                "prompt": question.prompt,
                "context": question.context,
                "options": question.options.iter().map(|o| serde_json::json!({
                    "value": o.value,
                    "label": o.label,
                    "rationale": o.rationale,
                })).collect::<Vec<_>>(),
            });
            Ok(NextQuestionResult {
                question: Some(q_json),
                complete: false,
                message: None,
            })
        }
        None => Ok(NextQuestionResult {
            question: None,
            complete: true,
            message: Some("All questions answered for this session.".to_string()),
        }),
    }
}

// --- Schema export ---

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "decide",
        "version": "0.1.0",
        "description": "Architecture decision prompting with curated engineering questions",
        "commands": [
            { "name": "trees", "description": "List available decision trees" },
            { "name": "suggest", "description": "Suggest a tree for a given prompt" },
            { "name": "start", "description": "Start a new decision session" },
            { "name": "next", "description": "Get the next unanswered question" },
            { "name": "record", "description": "Record a decision answer" },
            { "name": "complete", "description": "Complete a decision session" },
            { "name": "list", "description": "List recorded decisions" },
            { "name": "get", "description": "Get a specific decision" },
            { "name": "session list", "description": "List decision sessions" },
            { "name": "session get", "description": "Get a session with all decisions" },
            { "name": "init", "description": "Initialize decisions database" },
            { "name": "schema", "description": "Print subsystem schema" }
        ],
        "storage": ["decisions.db"],
        "trees": decision_trees().iter().map(|t| serde_json::json!({
            "id": t.id,
            "name": t.name,
            "question_count": t.questions.len(),
        })).collect::<Vec<_>>(),
    })
}

// --- CLI dispatch ---

pub fn run_decide_cli(store: &Store, cli: DecideCli) -> Result<(), error::DecapodError> {
    initialize_decide_db(&store.root)?;

    match cli.command {
        DecideCommand::Trees => {
            let trees: Vec<serde_json::Value> = decision_trees()
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "name": t.name,
                        "description": t.description,
                        "keywords": t.keywords,
                        "question_count": t.questions.len(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&trees).unwrap());
        }

        DecideCommand::Suggest { prompt } => {
            let suggestions = suggest_trees(&prompt);
            println!(
                "{}",
                serde_json::to_string_pretty(&suggestions).unwrap()
            );
        }

        DecideCommand::Start {
            tree,
            title,
            description,
            actor,
        } => {
            let session = start_session(store, &tree, &title, &description, &actor)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&session).unwrap()
            );
        }

        DecideCommand::Next { session } => {
            let result = next_question(store, &session)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap()
            );
        }

        DecideCommand::Record {
            session,
            question,
            value,
            rationale,
            actor,
        } => {
            let decision =
                record_decision(store, &session, &question, &value, &rationale, &actor)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&decision).unwrap()
            );
        }

        DecideCommand::Complete { session } => {
            complete_session(store, &session)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "session_id": session,
                    "status": "completed",
                }))
                .unwrap()
            );
        }

        DecideCommand::List { session, tree } => {
            let decisions =
                list_decisions(store, session.as_deref(), tree.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&decisions).unwrap()
            );
        }

        DecideCommand::Get { id } => {
            let decision = get_decision(store, &id)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&decision).unwrap()
            );
        }

        DecideCommand::Session { command } => match command {
            SessionSubCommand::List { status } => {
                let sessions = list_sessions(store, status.as_deref())?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&sessions).unwrap()
                );
            }
            SessionSubCommand::Get { id } => {
                let session = get_session(store, &id)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&session).unwrap()
                );
            }
        },

        DecideCommand::Init => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "message": "Decisions database initialized",
                }))
                .unwrap()
            );
        }

        DecideCommand::Schema => {
            println!(
                "{}",
                serde_json::to_string_pretty(&schema()).unwrap()
            );
        }
    }

    Ok(())
}
