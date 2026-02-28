//! CLI struct definitions for the Decapod command-line interface.
//!
//! All clap-derived types live here. Dispatch logic lives in `dispatch/`.

use crate::core::{docs_cli, flight_recorder, obligation, plan_governance, todo, workunit};
use crate::plugins::{
    aptitude, container, cron, decide, doctor, eval, federation, health, internalize, lcm, map_ops,
    policy, primitives, reflex, verify, workflow,
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(
    name = "decapod",
    version = env!("CARGO_PKG_VERSION"),
    about = "Decapod is the daemonless, local-first control plane that agents call on demand to turn intent into context, then context into explicit specifications before inference, enforce boundaries, and produce proof-backed completion across concurrent multi-agent work. 🦀",
    disable_version_flag = true
)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ValidateCli {
    /// Store to validate: 'user' (blank-slate semantics) or 'repo' (dogfood backlog).
    #[clap(long, default_value = "repo")]
    pub store: String,
    /// Output format: 'text' or 'json'.
    #[clap(long, default_value = "text")]
    pub format: String,
    /// Print per-gate timing information.
    #[clap(long, short = 'v')]
    pub verbose: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct CapabilitiesCli {
    /// Output format: 'json' or 'text'.
    #[clap(long, default_value = "text")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct WorkspaceCli {
    #[clap(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WorkspaceCommand {
    /// Ensure an isolated workspace exists (create if needed)
    Ensure {
        /// Branch name (auto-generated if not provided)
        #[clap(long)]
        branch: Option<String>,
        /// Use a container for the workspace
        #[clap(long)]
        container: bool,
    },
    /// Show current workspace status
    Status,
    /// Publish workspace changes as a patch/PR bundle
    Publish {
        /// Title for the change
        #[clap(long)]
        title: Option<String>,
        /// Description for the change
        #[clap(long)]
        description: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct RpcCli {
    /// Operation to perform
    #[clap(long)]
    pub op: Option<String>,
    /// JSON parameters
    #[clap(long)]
    pub params: Option<String>,
    /// Read request from stdin instead of command line
    #[clap(long)]
    pub stdin: bool,
}

// ===== Grouped Command Structures =====

#[derive(clap::Args, Debug)]
pub(crate) struct InitGroupCli {
    #[clap(subcommand)]
    pub command: Option<InitCommand>,
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    pub dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    pub force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    pub dry_run: bool,
    /// Generate project specs docs scaffolding under `.decapod/generated/specs/` (enabled by default).
    #[clap(long = "no-specs", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub specs: bool,
    /// Diagram style for generated `.decapod/generated/specs/ARCHITECTURE.md`.
    #[clap(long, value_enum, default_value_t = InitDiagramStyle::Ascii)]
    pub diagram_style: InitDiagramStyle,
    /// Force creation of all 3 entrypoint files (GEMINI.md, AGENTS.md, CLAUDE.md).
    #[clap(long)]
    pub all: bool,
    /// Create only CLAUDE.md entrypoint file.
    #[clap(long)]
    pub claude: bool,
    /// Create only GEMINI.md entrypoint file.
    #[clap(long)]
    pub gemini: bool,
    /// Create only AGENTS.md entrypoint file.
    #[clap(long)]
    pub agents: bool,
    /// Seed product name for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_name: Option<String>,
    /// Seed product summary/outcome for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_summary: Option<String>,
    /// Seed architecture direction for generated specs (non-interactive safe).
    #[clap(long)]
    pub architecture_direction: Option<String>,
    /// Seed product type for generated specs (e.g. service_or_library/application).
    #[clap(long)]
    pub product_type: Option<String>,
    /// Seed done criteria for generated specs (non-interactive safe).
    #[clap(long)]
    pub done_criteria: Option<String>,
    /// Seed primary languages (repeatable and/or comma-separated).
    #[clap(long = "primary-language", value_delimiter = ',')]
    pub primary_languages: Vec<String>,
    /// Seed detected surfaces (repeatable and/or comma-separated).
    #[clap(long = "surface", value_delimiter = ',')]
    pub detected_surfaces: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum InitCommand {
    /// Remove all Decapod files from repository
    Clean {
        /// Directory to clean (defaults to current working directory).
        #[clap(short, long)]
        dir: Option<PathBuf>,
    },
    /// Apply explicit init options (non-interactive).
    #[clap(alias = "wtih")]
    With(InitWithCli),
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct InitWithCli {
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    pub dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    pub force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    pub dry_run: bool,
    /// Force creation of all entrypoint files.
    #[clap(long)]
    pub all: bool,
    /// Create only CLAUDE.md entrypoint file.
    #[clap(long)]
    pub claude: bool,
    /// Create only GEMINI.md entrypoint file.
    #[clap(long)]
    pub gemini: bool,
    /// Create only AGENTS.md entrypoint file.
    #[clap(long)]
    pub agents: bool,
    /// Generate project specs docs scaffolding under `.decapod/generated/specs/` (enabled by default).
    #[clap(long = "no-specs", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub specs: bool,
    /// Diagram style for generated `.decapod/generated/specs/ARCHITECTURE.md`.
    #[clap(long, value_enum, default_value_t = InitDiagramStyle::Ascii)]
    pub diagram_style: InitDiagramStyle,
    /// Seed product name for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_name: Option<String>,
    /// Seed product summary/outcome for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_summary: Option<String>,
    /// Seed architecture direction for generated specs (non-interactive safe).
    #[clap(long)]
    pub architecture_direction: Option<String>,
    /// Seed product type for generated specs (e.g. service_or_library/application).
    #[clap(long)]
    pub product_type: Option<String>,
    /// Seed done criteria for generated specs (non-interactive safe).
    #[clap(long)]
    pub done_criteria: Option<String>,
    /// Seed primary languages (repeatable and/or comma-separated).
    #[clap(long = "primary-language", value_delimiter = ',')]
    pub primary_languages: Vec<String>,
    /// Seed detected surfaces (repeatable and/or comma-separated).
    #[clap(long = "surface", value_delimiter = ',')]
    pub detected_surfaces: Vec<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum InitDiagramStyle {
    Ascii,
    Mermaid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DecapodProjectConfig {
    pub schema_version: String,
    pub init: InitConfigSection,
    pub repo: RepoContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InitConfigSection {
    pub specs: bool,
    pub diagram_style: InitDiagramStyle,
    pub entrypoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RepoContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "architecture_direction", alias = "architecture_intent")]
    pub architecture_direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_criteria: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected_surfaces: Vec<String>,
}

impl Default for DecapodProjectConfig {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            init: InitConfigSection {
                specs: true,
                diagram_style: InitDiagramStyle::Ascii,
                entrypoints: vec![
                    "AGENTS.md".to_string(),
                    "CLAUDE.md".to_string(),
                    "GEMINI.md".to_string(),
                    "CODEX.md".to_string(),
                ],
            },
            repo: RepoContext::default(),
        }
    }
}

#[derive(clap::Args, Debug)]
pub(crate) struct SessionCli {
    #[clap(subcommand)]
    pub command: SessionCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SessionCommand {
    /// Acquire a new session token (required before using other commands)
    Acquire,
    /// Show current session status
    Status,
    /// Release the current session token
    Release,
    /// Bootstrap a governed work session with stubs and handshake artifact
    Init {
        /// Intended scope for this work session
        #[clap(long, default_value = "governed-work-session")]
        scope: String,
        /// Proof commands this session commits to run
        #[clap(long = "proof")]
        proofs: Vec<String>,
        /// Overwrite existing stubs if they already exist
        #[clap(long)]
        force: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct SetupCli {
    #[clap(subcommand)]
    pub command: SetupCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SetupCommand {
    /// Install or uninstall repository git hooks
    Hook {
        /// Install conventional commit message validation hook
        #[clap(long)]
        commit_msg: bool,
        /// Install Rust pre-commit hook (fmt + clippy)
        #[clap(long)]
        pre_commit: bool,
        /// Remove installed hooks
        #[clap(long)]
        uninstall: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct GovernCli {
    #[clap(subcommand)]
    pub command: GovernCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum GovernCommand {
    /// Risk classification and approvals
    Policy(policy::PolicyCli),

    /// Claims, proofs, and system health
    Health(health::HealthCli),

    /// Execute verification proofs
    Proof(ProofCommandCli),

    /// Run integrity watchlist checks
    Watcher(WatcherCli),

    /// Operator feedback and preferences
    Feedback(FeedbackCli),

    /// Workspace safety gates: path blocklist, diff size, secret scan, dangerous patterns
    Gatekeeper(GatekeeperCli),

    /// Plan-governed execution artifacts and gates
    Plan(PlanCli),

    /// Work unit manifest artifacts (intent/spec/state/proof chain)
    Workunit(WorkunitCli),

    /// Deterministic context capsule query over embedded constitution docs
    Capsule(CapsuleCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct PlanCli {
    #[clap(subcommand)]
    pub command: PlanCommand,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum PlanStateArg {
    Draft,
    Annotating,
    Approved,
    Executing,
    Done,
}

impl From<PlanStateArg> for plan_governance::PlanState {
    fn from(value: PlanStateArg) -> Self {
        match value {
            PlanStateArg::Draft => Self::Draft,
            PlanStateArg::Annotating => Self::Annotating,
            PlanStateArg::Approved => Self::Approved,
            PlanStateArg::Executing => Self::Executing,
            PlanStateArg::Done => Self::Done,
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum PlanCommand {
    /// Initialize governed PLAN artifact
    Init {
        #[clap(long)]
        title: String,
        #[clap(long)]
        intent: String,
        #[clap(long = "todo-id")]
        todo_ids: Vec<String>,
        #[clap(long = "proof-hook")]
        proof_hooks: Vec<String>,
        #[clap(long = "unknown")]
        unknowns: Vec<String>,
        #[clap(long = "question")]
        human_questions: Vec<String>,
        #[clap(long = "forbidden-path")]
        forbidden_paths: Vec<String>,
        #[clap(long)]
        file_touch_budget: Option<usize>,
    },
    /// Patch governed PLAN artifact
    Update {
        #[clap(long)]
        title: Option<String>,
        #[clap(long)]
        intent: Option<String>,
        #[clap(long = "todo-id")]
        todo_ids: Vec<String>,
        #[clap(long = "proof-hook")]
        proof_hooks: Vec<String>,
        #[clap(long = "unknown")]
        unknowns: Vec<String>,
        #[clap(long = "question")]
        human_questions: Vec<String>,
        #[clap(long, default_value_t = false)]
        clear_unknowns: bool,
        #[clap(long, default_value_t = false)]
        clear_questions: bool,
        #[clap(long = "forbidden-path")]
        forbidden_paths: Vec<String>,
        #[clap(long)]
        file_touch_budget: Option<usize>,
    },
    /// Set plan state
    SetState {
        #[clap(long, value_enum)]
        state: PlanStateArg,
    },
    /// Shortcut for setting plan state to APPROVED
    Approve,
    /// Display current plan artifact
    Status,
    /// Execute readiness check with typed pushback markers
    CheckExecute {
        #[clap(long)]
        todo_id: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct WorkunitCli {
    #[clap(subcommand)]
    pub command: WorkunitCommand,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum WorkunitStatusArg {
    Draft,
    Executing,
    Claimed,
    Verified,
}

impl From<WorkunitStatusArg> for workunit::WorkUnitStatus {
    fn from(value: WorkunitStatusArg) -> Self {
        match value {
            WorkunitStatusArg::Draft => Self::Draft,
            WorkunitStatusArg::Executing => Self::Executing,
            WorkunitStatusArg::Claimed => Self::Claimed,
            WorkunitStatusArg::Verified => Self::Verified,
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum WorkunitCommand {
    /// Initialize a work unit manifest for a task
    Init {
        #[clap(long)]
        task_id: String,
        #[clap(long)]
        intent_ref: String,
    },
    /// Get full work unit manifest JSON
    Get {
        #[clap(long)]
        task_id: String,
    },
    /// Show compact work unit status
    Status {
        #[clap(long)]
        task_id: String,
    },
    /// Attach a spec reference to a work unit
    AttachSpec {
        #[clap(long)]
        task_id: String,
        #[clap(long = "ref")]
        reference: String,
    },
    /// Attach a state reference to a work unit
    AttachState {
        #[clap(long)]
        task_id: String,
        #[clap(long = "ref")]
        reference: String,
    },
    /// Replace proof plan gates for a work unit
    SetProofPlan {
        #[clap(long)]
        task_id: String,
        #[clap(long = "gate")]
        gates: Vec<String>,
    },
    /// Record proof result for a gate
    RecordProof {
        #[clap(long)]
        task_id: String,
        #[clap(long)]
        gate: String,
        #[clap(long)]
        status: String,
        #[clap(long)]
        artifact: Option<String>,
    },
    /// Transition workunit status through governed state machine
    Transition {
        #[clap(long)]
        task_id: String,
        #[clap(long, value_enum)]
        to: WorkunitStatusArg,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct CapsuleCli {
    #[clap(subcommand)]
    pub command: CapsuleCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CapsuleCommand {
    /// Query a deterministic context capsule from embedded docs
    Query {
        #[clap(long)]
        topic: String,
        #[clap(long)]
        scope: String,
        #[clap(long)]
        risk_tier: Option<String>,
        #[clap(long)]
        task_id: Option<String>,
        #[clap(long)]
        workunit_id: Option<String>,
        #[clap(long, default_value_t = 6)]
        limit: usize,
        #[clap(long, default_value_t = false)]
        write: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct DataCli {
    #[clap(subcommand)]
    pub command: DataCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum DataCommand {
    /// Session archives (MOVE-not-TRIM)
    Archive(ArchiveCli),

    /// Repository knowledge base
    Knowledge(KnowledgeCli),

    /// Token budgets and context packing
    Context(ContextCli),

    /// Subsystem schemas and discovery
    Schema(SchemaCli),

    /// Repository structure and dependencies
    Repo(RepoCli),

    /// Audit log access (The Thin Waist)
    Broker(BrokerCli),

    /// Aptitude memory and preferences
    #[clap(aliases = ["memory"])]
    Aptitude(aptitude::AptitudeCli),

    /// Governed agent memory — typed knowledge graph
    Federation(federation::FederationCli),

    /// Markdown-native primitive layer
    Primitives(primitives::PrimitivesCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct AutoCli {
    #[clap(subcommand)]
    pub command: AutoCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum AutoCommand {
    /// Scheduled tasks (time-based)
    Cron(cron::CronCli),

    /// Event-driven automation
    Reflex(reflex::ReflexCli),

    /// Workflow automation and discovery
    Workflow(workflow::WorkflowCli),

    /// Ephemeral isolated container execution
    Container(container::ContainerCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct QaCli {
    #[clap(subcommand)]
    pub command: QaCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum QaCommand {
    /// Verify previously completed work (proof replay + drift checks)
    Verify(verify::VerifyCli),

    /// CI validation checks
    Check {
        /// Check crate description matches expected
        #[clap(long)]
        crate_description: bool,
        /// Smoke-check all discoverable command help surfaces
        #[clap(long)]
        commands: bool,
        /// Run all checks
        #[clap(long)]
        all: bool,
    },

    /// Run gatling regression test across all CLI code paths
    Gatling(crate::plugins::gatling::GatlingCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct HandshakeCli {
    /// Intended scope of work for this agent/session
    #[clap(long)]
    pub scope: String,
    /// Proof commands this agent commits to run
    #[clap(long = "proof")]
    pub proofs: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ReleaseCli {
    #[clap(subcommand)]
    pub command: ReleaseCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ReleaseCommand {
    /// Validate release readiness (versioning, changelog, manifests, lockfile)
    Check,
    /// Emit deterministic repository inventory JSON for CI artifacts
    Inventory,
    /// Normalize and stamp deterministic policy lineage across provenance manifests
    LineageSync,
}

// ===== Main Command Enum =====

#[derive(clap::Args, Debug)]
pub(crate) struct TraceCli {
    #[clap(subcommand)]
    pub command: TraceCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TraceCommand {
    /// Export local traces
    Export {
        /// Number of last traces to export
        #[clap(long, default_value = "10")]
        last: usize,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Activate local control plane state and run startup migrations
    #[clap(name = "activate")]
    Activate,

    /// Bootstrap system and manage lifecycle
    #[clap(name = "init", visible_alias = "i")]
    Init(InitGroupCli),

    /// Configure repository (hooks, settings)
    #[clap(name = "setup")]
    Setup(SetupCli),

    /// Session token management (required for agent operation)
    #[clap(name = "session", visible_alias = "s")]
    Session(SessionCli),

    /// Access methodology documentation
    #[clap(name = "docs", visible_alias = "d")]
    Docs(docs_cli::DocsCli),

    /// Track tasks and work items
    #[clap(name = "todo", visible_alias = "t")]
    Todo(todo::TodoCli),

    /// Governance-native obligation graph
    #[clap(name = "obligation", visible_alias = "o")]
    Obligation(obligation::ObligationCli),

    /// Validate methodology compliance
    #[clap(name = "validate", visible_alias = "v")]
    Validate(ValidateCli),

    /// Show version information
    #[clap(name = "version")]
    Version,

    /// Governance: policy, health, proofs, audits
    #[clap(name = "govern", visible_alias = "g")]
    Govern(GovernCli),

    /// Data: archives, knowledge, context, schemas
    #[clap(name = "data")]
    Data(DataCli),

    /// Automation: scheduled and event-driven
    #[clap(name = "auto", visible_alias = "a")]
    Auto(AutoCli),

    /// Quality assurance: verification and checks
    #[clap(name = "qa", visible_alias = "q")]
    Qa(QaCli),

    /// Architecture decision prompting
    #[clap(name = "decide")]
    Decide(decide::DecideCli),

    /// Agent workspace management
    #[clap(name = "workspace", visible_alias = "w")]
    Workspace(WorkspaceCli),

    /// Structured JSON-RPC interface for agents
    #[clap(name = "rpc")]
    Rpc(RpcCli),

    /// Deterministic agent handshake artifact (repo-native)
    #[clap(name = "handshake")]
    Handshake(HandshakeCli),

    /// Release lifecycle checks and guards
    #[clap(name = "release")]
    Release(ReleaseCli),

    /// Show Decapod capabilities (for agent discovery)
    #[clap(name = "capabilities")]
    Capabilities(CapabilitiesCli),

    /// Internalized context artifacts: create, attach, and inspect context adapters
    #[clap(name = "internalize")]
    Internalize(internalize::InternalizeCli),

    /// Preflight check: before any operation, predict what will fail
    #[clap(name = "preflight")]
    Preflight(PreflightCli),

    /// Impact analysis: predict validation outcomes for changed files
    #[clap(name = "impact")]
    Impact(ImpactCli),

    /// Local trace management
    #[clap(name = "trace")]
    Trace(TraceCli),

    /// Variance-aware evaluation artifacts and promotion gates
    #[clap(name = "eval")]
    Eval(eval::EvalCli),

    /// Governance Flight Recorder - render timeline from event logs
    #[clap(name = "flight-recorder")]
    FlightRecorder(flight_recorder::FlightRecorderCli),

    /// STATE_COMMIT: prove and verify cryptographic state commitments
    #[clap(name = "state-commit")]
    StateCommit(StateCommitCli),

    /// Preflight health checks for the workspace
    #[clap(name = "doctor")]
    Doctor(doctor::DoctorCli),

    /// Lossless Context Management — immutable originals + deterministic summaries
    #[clap(name = "lcm")]
    Lcm(lcm::LcmCli),

    /// Deterministic map operators — structured parallel processing
    #[clap(name = "map")]
    Map(map_ops::MapCli),

    /// Run demonstrations of Decapod features
    #[clap(name = "demo")]
    Demo(DemoCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct BrokerCli {
    #[clap(subcommand)]
    pub command: BrokerCommand,
}

#[derive(clap::Args, Debug)]
pub(crate) struct StateCommitCli {
    #[clap(subcommand)]
    pub command: StateCommitCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum StateCommitCommand {
    /// Compute STATE_COMMIT for the current workspace
    Prove {
        /// Base commit SHA (required)
        #[clap(long)]
        base: String,
        /// Head commit SHA (defaults to current HEAD)
        #[clap(long)]
        head: Option<String>,
        /// Output file for scope_record.cbor
        #[clap(long, default_value = "scope_record.cbor")]
        output: PathBuf,
    },
    /// Verify a STATE_COMMIT matches current workspace
    Verify {
        /// Path to scope_record.cbor
        #[clap(long)]
        scope_record: PathBuf,
        /// Expected state_commit_root
        #[clap(long)]
        expected_root: Option<String>,
    },
    /// Explain the contents of a scope_record.cbor file
    Explain {
        /// Path to scope_record.cbor
        #[clap(long)]
        scope_record: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum BrokerCommand {
    /// Show the audit log of brokered mutations.
    Audit,
    /// Verify audit log integrity and detect crash-induced divergence.
    Verify,
}

#[derive(clap::Args, Debug)]
pub(crate) struct KnowledgeCli {
    #[clap(subcommand)]
    pub command: KnowledgeCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum KnowledgeCommand {
    /// Add an entry to project knowledge
    Add {
        #[clap(long)]
        id: String,
        #[clap(long)]
        title: String,
        #[clap(long)]
        text: String,
        #[clap(long)]
        provenance: String,
        #[clap(long)]
        claim_id: Option<String>,
    },
    /// Search project knowledge
    Search {
        #[clap(long)]
        query: String,
    },
    /// Record explicit promotion of advisory/episodic knowledge into procedural class
    Promote {
        #[clap(long)]
        source_entry_id: String,
        #[clap(long = "evidence-ref")]
        evidence_refs: Vec<String>,
        #[clap(long)]
        approved_by: String,
        #[clap(long)]
        reason: String,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct RepoCli {
    #[clap(subcommand)]
    pub command: RepoCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum RepoCommand {
    /// Generate a deterministic summary of the repo
    Map,
    /// Generate a Markdown dependency graph (Mermaid format)
    Graph,
}

#[derive(clap::Args, Debug)]
pub(crate) struct WatcherCli {
    #[clap(subcommand)]
    pub command: WatcherCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WatcherCommand {
    /// Run all checks in the watchlist
    Run,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ArchiveCli {
    #[clap(subcommand)]
    pub command: ArchiveCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ArchiveCommand {
    /// List all session archives
    List,
    /// Verify archive integrity (hashes and presence)
    Verify,
}

#[derive(clap::Args, Debug)]
pub(crate) struct FeedbackCli {
    #[clap(subcommand)]
    pub command: FeedbackCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum FeedbackCommand {
    /// Add operator feedback to the ledger
    Add {
        #[clap(long)]
        source: String,
        #[clap(long)]
        text: String,
        #[clap(long)]
        links: Option<String>,
    },
    /// Propose preference updates based on feedback
    Propose,
}

#[derive(clap::Args, Debug)]
pub(crate) struct GatekeeperCli {
    #[clap(subcommand)]
    pub command: GatekeeperCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum GatekeeperCommand {
    /// Check staged/changed files against safety gates
    Check {
        /// Paths to check (defaults to git staged files)
        #[clap(long)]
        paths: Option<Vec<String>>,
        /// Maximum diff size in bytes (default 10MB)
        #[clap(long)]
        max_diff_bytes: Option<u64>,
        /// Disable secret scanning
        #[clap(long)]
        no_secrets: bool,
        /// Disable dangerous pattern scanning
        #[clap(long)]
        no_dangerous: bool,
    },
}

#[derive(clap::Args, Debug)]
pub struct ProofCommandCli {
    #[clap(subcommand)]
    pub command: ProofSubCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProofSubCommand {
    /// Run all configured proofs
    Run,
    /// Run a specific proof by name
    Test {
        #[clap(long)]
        name: String,
    },
    /// Show proof configuration and results
    List,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ContextCli {
    #[clap(subcommand)]
    pub command: ContextCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ContextCommand {
    /// Audit current session token usage against profiles.
    Audit {
        #[clap(long)]
        profile: String,
        #[clap(long)]
        files: Vec<PathBuf>,
    },
    /// Perform MOVE-not-TRIM archival of a session file.
    Pack {
        #[clap(long)]
        path: PathBuf,
        #[clap(long)]
        summary: String,
    },
    /// Restore content from an archive (budget-gated)
    Restore {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "main")]
        profile: String,
        #[clap(long)]
        current_files: Vec<PathBuf>,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct SchemaCli {
    /// Format: json | md
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Optional: filter by subsystem name
    #[clap(long)]
    pub subsystem: Option<String>,
    /// Force deterministic output (removes volatile timestamps)
    #[clap(long)]
    pub deterministic: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct PreflightCli {
    /// Operation to preflight (e.g., todo.add, validate, workspace.ensure)
    #[clap(long)]
    pub op: Option<String>,
    /// Output format: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Session ID to preflight for
    #[clap(long)]
    pub session: Option<String>,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ImpactCli {
    /// Comma-separated list of changed files
    #[clap(long)]
    pub changed_files: Option<String>,
    /// Output format: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Predict mode: don't actually run gates, just predict
    #[clap(long)]
    pub predict: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct DemoCli {
    /// Demo to run: interlock
    #[clap(long, default_value = "interlock")]
    pub demo: String,
}
