//! Documentation CLI for accessing embedded constitution.
//!
//! This module implements the `decapod docs` command family for querying
//! Decapod's embedded methodology documents.

use crate::core::{assets, docs, error};
use clap::Subcommand;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// CLI structure for `decapod docs` command
#[derive(clap::Args, Debug)]
pub struct DocsCli {
    #[clap(subcommand)]
    pub command: DocsCommand,
}

/// Document source selector for viewing constitution docs
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum DocumentSource {
    /// Show only the embedded content (from the binary)
    Embedded,
    /// Show only the override content (from .decapod/OVERRIDE.md sections)
    Override,
    /// Show merged content (embedded base + project override appended)
    Merged,
}

/// Subcommands for the `decapod docs` CLI
#[derive(Subcommand, Debug)]
pub enum DocsCommand {
    /// List all embedded Decapod methodology documents.
    List,
    /// Display the content of a specific embedded document.
    Show {
        #[clap(value_parser)]
        path: String,
        /// Source to display: embedded (binary), override (.decapod), or merged (default)
        #[clap(long, short, value_enum, default_value = "merged")]
        source: DocumentSource,
    },
    /// Dump all embedded constitution for agentic ingestion.
    Ingest,
    /// Return scoped constitution fragments relevant to a concrete query.
    Search {
        /// Problem/query text to scope against constitution docs.
        #[clap(long)]
        query: String,
        /// Optional operation context (e.g. workspace.ensure, store.upsert).
        #[clap(long)]
        op: Option<String>,
        /// Optional touched paths (repeatable).
        #[clap(long = "path")]
        path: Vec<String>,
        /// Optional intent tags (repeatable).
        #[clap(long = "tag")]
        tag: Vec<String>,
        /// Max fragments to return.
        #[clap(long, default_value_t = 5)]
        limit: usize,
        /// Output format: text or json.
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Validate and cache OVERRIDE.md checksum.
    Override {
        /// Force re-cache even if unchanged
        #[clap(long, short)]
        force: bool,
    },
    /// Autogenerate/Sync documentation from code implementation.
    Build {
        /// Only update docs for specific files that were touched.
        #[clap(long, num_args(1..))]
        touched: Option<Vec<PathBuf>>,
    },
}

#[derive(Debug, Default)]
pub struct DocsRunResult {
    pub ingested_core_constitution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideChecksumStatus {
    MissingOverride,
    Cached,
    Updated,
    Unchanged,
}

pub fn sync_override_checksum(
    repo_root: &Path,
    force: bool,
    dry_run: bool,
) -> Result<OverrideChecksumStatus, error::DecapodError> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return Ok(OverrideChecksumStatus::MissingOverride);
    }

    let current_checksum = calculate_sha256(&override_path)?;
    if force {
        if !dry_run {
            cache_checksum(repo_root, &current_checksum)?;
        }
        return Ok(OverrideChecksumStatus::Cached);
    }

    match get_cached_checksum(repo_root) {
        Some(cached_checksum) if cached_checksum == current_checksum => {
            Ok(OverrideChecksumStatus::Unchanged)
        }
        Some(_) => {
            if !dry_run {
                cache_checksum(repo_root, &current_checksum)?;
            }
            Ok(OverrideChecksumStatus::Updated)
        }
        None => {
            if !dry_run {
                cache_checksum(repo_root, &current_checksum)?;
            }
            Ok(OverrideChecksumStatus::Cached)
        }
    }
}

pub fn run_docs_cli(cli: DocsCli) -> Result<DocsRunResult, error::DecapodError> {
    match cli.command {
        DocsCommand::List => {
            let all_docs = assets::list_docs();
            let docs: Vec<_> = all_docs
                .into_iter()
                .filter(|d| d.starts_with("docs/agent/"))
                .collect();

            if !docs.is_empty() {
                println!("Decapod Agent Documentation:");
                for doc in docs {
                    println!("- embedded/{doc}");
                }
            } else {
                println!("No agent documentation found.");
            }
            Ok(DocsRunResult::default())
        }
        DocsCommand::Show { path, source } => {
            let normalized_path = normalize_agent_doc_path(&path);

            if !normalized_path.starts_with("docs/agent/") {
                return Err(error::DecapodError::ValidationError(format!(
                    "Access denied: 'decapod docs' is restricted to 'docs/agent/' paths. Use 'decapod constitution' for other sections. Invalid path: {path}"
                )));
            }

            // It's a direct document, not a constitution section
            let content = match source {
                DocumentSource::Embedded => assets::get_embedded_doc(&normalized_path),
                DocumentSource::Override => {
                    let current_dir =
                        std::env::current_dir().map_err(error::DecapodError::IoError)?;
                    let repo_root = find_repo_root(&current_dir)?;
                    assets::get_override_doc(&repo_root, &normalized_path)
                }
                DocumentSource::Merged => {
                    let current_dir =
                        std::env::current_dir().map_err(error::DecapodError::IoError)?;
                    let repo_root = find_repo_root(&current_dir)?;
                    assets::get_merged_doc(&repo_root, &normalized_path)
                }
            };

            match content {
                Some(content) => {
                    println!("{content}");
                    Ok(DocsRunResult::default())
                }
                None => Err(error::DecapodError::NotFound(format!(
                    "Document not found: {path}"
                ))),
            }
        }
        DocsCommand::Build { touched } => {
            build_docs(touched.unwrap_or_default())?;
            Ok(DocsRunResult::default())
        }

        DocsCommand::Ingest => {
            let docs = assets::list_docs();
            // Determine repo root for override merging
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let mut ingested_core_constitution = false;

            for doc_path in docs {
                if !doc_path.starts_with("docs/agent/") {
                    continue;
                }

                // Convert embedded path to relative path for override merging
                let relative_path = doc_path.strip_prefix("embedded/").unwrap_or(&doc_path);
                if relative_path.starts_with("core/") {
                    ingested_core_constitution = true;
                }

                if let Some(content) = assets::get_merged_doc(&repo_root, relative_path) {
                    println!("--- BEGIN embedded/constitution.json#{doc_path} ---");
                    println!("{content}");
                    println!("--- END embedded/constitution.json#{doc_path} ---");
                }
            }
            Ok(DocsRunResult {
                ingested_core_constitution,
            })
        }
        DocsCommand::Search {
            query,
            op,
            path,
            tag,
            limit,
            format,
        } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let all_fragments = docs::resolve_scoped_fragments(
                &repo_root,
                Some(&query),
                op.as_deref(),
                &path,
                &tag,
                limit * 2, // Get more to allow for filtering
            );

            // Filter for only docs/agent/
            let fragments: Vec<_> = all_fragments
                .into_iter()
                .filter(|f| f.r#ref.contains("docs/agent/"))
                .take(limit)
                .collect();

            if format.eq_ignore_ascii_case("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "query": query,
                        "op": op,
                        "paths": path,
                        "tags": tag,
                        "fragments": fragments,
                    }))
                    .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?
                );
            } else {
                println!("Scoped agent documentation context:");
                if fragments.is_empty() {
                    println!("No matching agent documentation found.");
                }
                for (idx, fragment) in fragments.iter().enumerate() {
                    println!("\n{}. {} ({})", idx + 1, fragment.title, fragment.r#ref);
                    println!("{}", fragment.excerpt);
                }
            }
            Ok(DocsRunResult::default())
        }
        DocsCommand::Override { force } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let override_path = repo_root.join(".decapod").join("OVERRIDE.md");
            match sync_override_checksum(&repo_root, force, false)? {
                OverrideChecksumStatus::MissingOverride => {
                    println!("ℹ No OVERRIDE.md found at {}", override_path.display());
                    println!("  Run `decapod init` to create one.");
                }
                OverrideChecksumStatus::Cached => {
                    println!("✓ OVERRIDE.md checksum cached");
                }
                OverrideChecksumStatus::Updated => {
                    println!("📝 OVERRIDE.md checksum refreshed");
                }
                OverrideChecksumStatus::Unchanged => {
                    println!("✓ OVERRIDE.md unchanged");
                }
            }

            Ok(DocsRunResult::default())
        }
    }
}

fn normalize_agent_doc_path(path: &str) -> String {
    let path = path.strip_prefix("embedded/").unwrap_or(path);
    if path.starts_with("docs/agent/") {
        return path.to_string();
    }

    let short_name = path.strip_suffix(".md").unwrap_or(path);
    if !short_name.contains('/') {
        return format!("docs/agent/{short_name}.md");
    }

    path.to_string()
}

/// Helper function to find the .decapod repo root
/// (This is a simplified version; a real implementation might be more robust)
fn find_repo_root(start_dir: &Path) -> Result<PathBuf, error::DecapodError> {
    // Check for developer override first
    let override_root = std::env::var("DECAPOD_DEV_OVERRIDE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| start_dir.to_path_buf());

    let mut current_dir = override_root;
    loop {
        if current_dir.join(".decapod").exists() {
            return Ok(current_dir);
        }
        if !current_dir.pop() {
            return Err(error::DecapodError::NotFound(
                "'.decapod' directory not found in current or parent directories.".to_string(),
            ));
        }
    }
}

/// Calculate SHA256 checksum of a file
use clap::CommandFactory;
use std::io::Write;

/// Shared by the generated command contracts and their regression tests.
pub fn readiness_contract() -> &'static str {
    r#"- **Subject:** The exact current work branch, its accepted target tip, source and governance material, reviewed CI/release definitions, toolchain and applicable complete check matrix.
- **Outcome:** Exit 0 only after all required evidence passes. Missing, empty, stale, running, skipped-required or failed proof returns nonzero. Any relevant branch or target change invalidates earlier success.
- **Publication loop:** Work branch pushes and iteration are allowed before preflight. Hold PR creation (including a draft) until intent is converged, the branch is mergeable into the accepted target, and full validation succeeds against the exact current candidate and accepted target. Follow the validation report's actionable instructions, repair reported root causes on the work branch, and rerun exact-candidate validation until it succeeds; never bypass a reported gate or fabricate evidence. `workspace publish` combines branch push and PR creation, so it remains held at this readiness boundary; earlier standalone branch pushes are allowed. RPC `validate.run` enforces the same validation contract.
- **CI context:** Inside GitHub Actions (`GITHUB_ACTIONS=true`) or Buildkite (`BUILDKITE=true`), public CLI/alias/RPC validation executes the real bounded local gates and ignores remote preflight configuration. It reports `scope=ci_local` and `publication_ready=false`, never writes a full-readiness receipt or waits for its own suite, and rejects snapshot transport arguments. Provider environment flags alone cannot authorize publication: the publisher independently invokes the complete workbranch readiness boundary.
- **Prerequisites:** `decapod govern project-check` runs bounded project/governance prerequisites for bootstrap and finite native/CI recipe execution. Its partial success never grants publication readiness. Complete workbranch validation runs outside CI and aggregates actual source checks without recursion.
- **Providers:** `proof.validation_buildkite_preflights = false` selects native execution; true selects the repository-owned temporary-snapshot provider. Both enforce the same complete required profile. Unknown repositories, missing capabilities and unreconciled contracts fail explicitly.
- **Automatic provider execution:** With Buildkite validation enabled outside CI, `validate` submits the reserved same-tree snapshot, waits for every required imported Buildkite workflow, collects authenticated execution artifacts once, and rechecks the candidate, accepted target and provider attempt identities before returning success. The required project checks execute on Buildkite; this route neither runs a duplicate local prerequisite suite nor requires a fresh local validation receipt before submission. It uses already configured official Git access, repository check URLs only to locate Buildkite runs, and the reviewed Buildkite reader; missing tools, access denial, timeout, cancellation, failed jobs or incomplete custody return nonzero. It never installs credentials or falls back after an authorization failure.
- **Snapshot handoff:** `validate --snapshot-prepare` prepares private transport metadata and returns nonzero while proof is pending. An authorized host may upload only that exact temporary snapshot. If its supported object API changes the commit identity, `validate --snapshot-resume <HANDOFF> --snapshot-adopt <SERVER_S>` independently verifies the exact tree, parent, source, target, metadata and owned remote ref, then writes a new private handoff and remains non-ready. Never edit a handoff to assert a different identity. `validate --snapshot-resume <HANDOFF>` downloads and independently verifies Buildkite identities, raw artifact digests and execution evidence; caller files cannot assert success. Temporary transport uses the repository's supported shared trigger without a skip directive. Existing GitHub workflows may run independently. GitHub workflow results and artifacts never satisfy or block this Buildkite validation profile. This is not a compatibility claim for a separate `bk preflight` command.
- **Boundaries:** Validation does not publish the work branch, open a PR, create a release/tag, upload a package/image or deploy. Remote CI submission needs existing authorized access and remains subject to external-action policy. The core source-PR profile requires actual compile, test and build execution. Release-only packaging is outside this profile and has no packaging-assurance claim; publish/deploy dispositions describe simulation only, never actual delivery.
- **Recovery:** Follow the validation report's actionable recovery instructions, preserve blockers requiring human input, repair root causes or missing proof, and rerun against the exact candidate. Do not mark unexecuted checks green, weaken the required profile to pass, or replace unavailable provider evidence with a local receipt."#
}

pub fn readiness_config_contract() -> &'static str {
    r#"## Required proof and execution provider

`[proof].validation_buildkite_preflights` is a boolean, default false. False runs
the native readiness backend; it does not disable compile, test or CI proof.
True submits an authorized temporary snapshot to Buildkite, waits for its real
required checks, and downloads verified Buildkite execution evidence without
publishing the work branch. It does not run or collect GitHub Actions; normal
PR workflows on that service remain independent.
`[proof.required_profile]` binds the reviewed complete repository CI/release
contract. A missing, unknown or changed contract fails readiness; an empty
`commands` list cannot prove completion. Init and initial agent execution
reconcile existing repository contracts, preserve user-authored choices, and
report missing or conflicting obligations for repair. Successful init is not
successful validation. Existing repositories must reconcile their contract and
provide the required capabilities before validate can authorize publication."#
}

pub fn specs_refresh_contract() -> &'static str {
    r#"## Living-spec refresh surfaces

- **RPC:** `decapod rpc --op specs.refresh` refreshes supported spec fingerprints,
  attestations, overlays, generated boundaries and the manifest.
- **Validation flag:** `decapod validate --refresh-specs` requests the same
  supported refresh while running validation. Validation remains authoritative;
  this flag does not make unrelated gates pass.
- **Preconditions:** Use an active session and the claimed, isolated workspace.
  Review and author the material living-spec prose before refreshing it.
- **Boundary:** Refresh does not rewrite authored intent or architecture and
  cannot substitute for the required material spec change. It does not approve
  a plan or mark a todo complete.
- **Recovery:** There is no top-level `decapod specs.refresh` command. Use one of
  the two invocations above, inspect the refreshed files, and rerun validation."#
}

/// Render the trajectory input contract from the existing artifact schema.
/// `docs build` and its regression tests share this renderer; enum values are
/// never independently maintained in the documentation generator.
pub fn trajectory_record_contract() -> Result<String, error::DecapodError> {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../assets/schemas/trajectory.schema.json"
    ))
    .map_err(|error| error::DecapodError::ValidationError(error.to_string()))?;
    let loop_schema = &schema["$defs"]["loop"];
    let properties = loop_schema["properties"].as_object().ok_or_else(|| {
        error::DecapodError::ValidationError(
            "trajectory schema is missing loop properties".to_string(),
        )
    })?;
    let mut contract = String::from(
        "### `decapod govern trajectory record`\n\n\
         - **Preconditions:** Use an active session and initialize the run with `decapod govern trajectory init --run-id <run-id> --original-intent \"...\" --derived-intent \"...\"`.\n\
         - **Inputs:** Repeat `--loop-json '<object>'` for typed loop attempts and `--check \"name=status\"` for named check results. Each loop input is one JSON object, not an array.\n\
         - **Schema:** The tables and required fields below are generated by `decapod docs build` from [trajectory.schema.json](../../assets/schemas/trajectory.schema.json), `$defs.loop` and `$defs.check`. They are embedded in `decapod docs ingest` and `decapod docs show command-contracts --source embedded`.\n",
    );
    contract.push_str(&format!(
        "- **Required loop fields:** {}.\n\n",
        schema_code_list(&loop_schema["required"])?
    ));
    contract.push_str("| Input | Canonical values |\n| --- | --- |\n");
    for (field, definition) in properties {
        if let Some(values) = definition.get("enum") {
            contract.push_str(&format!("| `{field}` | {} |\n", schema_code_list(values)?));
        }
    }
    contract.push_str(&format!(
        "| `--check status` | {} |\n\n",
        schema_code_list(&schema["$defs"]["check"]["properties"]["status"]["enum"])?
    ));
    contract.push_str(
        "- **Exact JSON values:** Loop enum strings are case-sensitive. In particular, `grader_result` uses `pass` / `fail`, while loop `status` uses `passed` / `failed`. Arbitrary trigger descriptions are not enum values.\n\
         - **Check aliases:** The check parser additionally accepts `pass` as `passed` and `fail` as `failed`, ignoring status case and surrounding whitespace. Check names must be non-empty. `no_checks_run` is a computed proof status, not an accepted check input.\n\
         - **Identity and attempts:** Non-empty `intent_id`, `trajectory_id`, and `loop_id` are required. `trajectory_id` must equal `--run-id`; `intent_id` must match the initialized intent (by default `intent:<run-id>`). Attempts for each `loop_id` start at 1 and remain contiguous, without duplicates.\n\
         - **Verification evidence:** A verification loop with `grader_result: \"pass\"` requires at least one `proof_refs` entry, and every entry must be non-empty; `grader_result: \"fail\"` requires non-empty `feedback`. Feedback on every loop is limited to 2048 bytes. A passing enum value is not evidence by itself.\n\
         - **Optional loop fields:** `parent_loop_id`, `tool_calls`, `observations`, `feedback`, `proof_refs`, and `custody_event_id`. Event and improvement loops record evidence; a mutation proposal does not apply a change.\n\
         - **Example:** See [the minimal trajectory record](payload-examples.md#trajectory-record) for an initialized run and an unverified inspection.\n",
    );
    Ok(contract)
}

fn schema_code_list(values: &serde_json::Value) -> Result<String, error::DecapodError> {
    let values: Vec<String> = serde_json::from_value(values.clone()).map_err(|error| {
        error::DecapodError::ValidationError(format!(
            "trajectory contract expects a schema string array: {error}"
        ))
    })?;
    Ok(values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", "))
}

fn build_docs(touched: Vec<PathBuf>) -> Result<(), error::DecapodError> {
    let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
    let repo_root = find_repo_root(&current_dir)?;

    let contracts_path = repo_root.join("docs/agent/command-contracts.md");
    let _schema_path = repo_root.join("docs/agent/config-schema.md");

    // Only update if relevant files are touched, or if touched is empty (full build)
    let update_contracts = touched.is_empty()
        || touched.iter().any(|p| {
            let p_str = p.to_string_lossy();
            p_str.contains("src/decapod/cli.rs")
                || p_str.contains("src/decapod/core/docs_cli.rs")
                || p_str.contains("assets/schemas/trajectory.schema.json")
        });

    let update_schema = touched.is_empty()
        || touched.iter().any(|p| {
            let p_str = p.to_string_lossy();
            p_str.contains("src/decapod/cli.rs")
        });

    if update_contracts {
        println!("Updating docs/agent/command-contracts.md...");
        let mut file = std::fs::File::create(&contracts_path)?;
        writeln!(file, "# Command Contracts\n")?;
        writeln!(
            file,
            "This document defines the normative operational contracts for the Decapod CLI.\n"
        )?;

        let cmd = crate::cli::Cli::command();
        for sub in cmd.get_subcommands() {
            if sub.is_hide_set() {
                continue;
            }
            let name = sub.get_name();
            let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();

            writeln!(file, "## `decapod {name}`")?;
            writeln!(file, "- **Intent:** {about}")?;

            // Extract more info if it's a known core command
            match name {
                "govern" => {
                    writeln!(file, "\n{}", trajectory_record_contract()?)?;
                }
                "docs" => {
                    writeln!(
                        file,
                        "- **Restriction:** Only handles documents under `docs/agent/`."
                    )?;
                }
                "todo" => {
                    writeln!(
                        file,
                        "- **Preconditions:** Agent must have an active session."
                    )?;
                    writeln!(file, "- **State Transition:** Managed via `todo.db`.")?;
                }
                "workspace" => {
                    writeln!(file, "- **Preconditions:** Task must be claimed.")?;
                    writeln!(
                        file,
                        "- **State Transition:** Creates git worktrees/containers."
                    )?;
                    writeln!(
                        file,
                        "- **Publication:** Publication is a governed transition and remains blocked while required validation or evidence is unsatisfied."
                    )?;
                }
                "validate" => {
                    writeln!(file, "{}", readiness_contract())?;
                }
                _ => {}
            }
            writeln!(file)?;
            if name == "validate" {
                writeln!(file, "{}\n", specs_refresh_contract())?;
            }
        }

        // Auto-scan for RPCs
        writeln!(file, "# RPC Operations (Auto-generated)\n")?;
        let rpc_src =
            std::fs::read_to_string(repo_root.join("src/decapod/core/rpc.rs")).unwrap_or_default();
        for line in rpc_src.lines() {
            if line.contains("pub struct ") && line.contains("Params {") {
                let rpc_name = line
                    .split("pub struct ")
                    .nth(1)
                    .unwrap_or("")
                    .split("Params")
                    .next()
                    .unwrap_or("");
                if !rpc_name.is_empty() {
                    writeln!(file, "### Operation: `{rpc_name}`")?;
                }
            }
        }

        writeln!(file, "\n{}", governance_artifacts_contract().trim_end())?;
    }

    if update_schema {
        println!("Updating docs/agent/config-schema.md...");
        let schema_path = repo_root.join("docs/agent/config-schema.md");
        let mut file = std::fs::File::create(&schema_path)?;
        writeln!(file, "# Configuration Schema (Auto-generated)\n")?;

        let config_src =
            std::fs::read_to_string(repo_root.join("src/decapod/cli.rs")).unwrap_or_default();
        if let Some(start) = config_src.find("pub struct DecapodProjectConfig {") {
            let end = config_src[start..].find('}').unwrap_or(0);
            let fields = &config_src[start..start + end + 1];
            writeln!(file, "```rust\n{fields}\n```")?;
        }
        writeln!(file, "\n{}", readiness_config_contract())?;
    }

    Ok(())
}

fn calculate_sha256(path: &Path) -> Result<String, error::DecapodError> {
    let content = std::fs::read(path).map_err(error::DecapodError::IoError)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{hash:x}"))
}

/// Get cached checksum for OVERRIDE.md
fn get_cached_checksum(repo_root: &Path) -> Option<String> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("managed")
        .join("override.checksum");
    std::fs::read_to_string(checksum_path).ok()
}

/// Cache checksum for OVERRIDE.md
fn cache_checksum(repo_root: &Path, checksum: &str) -> Result<(), error::DecapodError> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("managed")
        .join("override.checksum");
    // Ensure generated directory exists
    if let Some(parent) = checksum_path.parent() {
        std::fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    std::fs::write(checksum_path, checksum).map_err(error::DecapodError::IoError)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "docs",
        "type": "object",
        "properties": {
            "list": {
                "type": "null",
                "description": "List all embedded Decapod methodology documents"
            },
            "show": {
                "type": "string",
                "description": "Display a specific embedded document"
            },
            "ingest": {
                "type": "null",
                "description": "Dump all embedded constitution for agentic ingestion"
            },
            "search": {
                "type": "object",
                "description": "Return scoped constitution fragments for a problem query",
                "properties": {
                    "query": { "type": "string" },
                    "op": { "type": "string" },
                    "path": { "type": "array", "items": { "type": "string" } },
                    "tag": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer" },
                    "format": { "type": "string", "enum": ["text", "json"] }
                }
            },
            "override": {
                "type": "object",
                "description": "Validate and cache OVERRIDE.md checksum",
                "properties": {
                    "force": {
                        "type": "boolean",
                        "description": "Force re-cache even if unchanged"
                    }
                }
            }
        }
    })
}

/// Generated governance guidance must survive `docs build` without restoring
/// retired split-file authorities or encouraging end-of-run reconstruction.
fn governance_artifacts_contract() -> &'static str {
    r#"# Governance Artifact Ownership

`.decapod/governance.json` is the sole tracked governance document. Its logical
plan, trajectory, validation, claims, and optional Jev sections share normalized
intent and epoch records. Health Engine operational claims remain separate in
`.decapod/data/decapod.db`; workunit runtime manifests live in `.decapod/data/workunits/`.
Do not hand-edit either control-plane authority.

Use `decapod govern artifacts migrate` for lossless legacy adoption and
`begin-pr --id <change> --base-branch <branch>` for an explicit new-PR boundary.
Normal `workspace ensure`, `agent.init`, and validation initialize a proven fresh
worktree branch from the embedded governance template and current claimed task.
The plan starts DRAFT and the trajectory has no checks or completion proof; only
actual successful validation writes a receipt. Protected refs and read-only
status/inventory never trigger initialization. Repeats, renames, and rebases keep
the active change identity and journal. Dirty inherited or ambiguous context
fails closed rather than being overwritten.
The boundary requires discarded state to be recoverable from Git, keeps explicit
unresolved obligations, and leaves accepted completed claims in Git history.
Use `claim` for actual current-PR claims and `resolve-obligation` with proof
references to retire a carried obligation explicitly. Status and
`verify-checkpoints` are read-only and never migrate state.

Populate in order: stable PR identity → intent/scope plan → actual current claims
and planned checks → work/evidence trajectory → validation → staged-material
checkpoint per commit. Persist meaningful boundaries as work progresses; do not
wait until the final response or repeatedly rewrite unchanged content.

Before each authored commit, stage code/spec changes, run
`decapod govern artifacts checkpoint --id <unique-id> --summary "..." --proof-ref <evidence>`,
then stage governance.json and commit. Publication checks exact material and
cumulative checkpoint preservation across the entire commit graph, including merges.
`inventory --claims-note "..."` records a current-PR checkpoint, not a fake claim
or a policy-history append. `inventory --compact` performs lossless legacy
normalization. Neither surface repairs malformed evidence by overwriting it.
"#
}

#[cfg(test)]
mod governance_contract_tests {
    use super::governance_artifacts_contract;

    #[test]
    fn generated_contract_preserves_normalized_population_order() {
        let contract = governance_artifacts_contract();
        assert!(
            contract.contains("`.decapod/governance.json` is the sole tracked governance document")
        );
        assert!(!contract.contains(".decapod/governance/claims.json"));
        assert!(!contract.contains("oversized research ledger"));
        let ordered = [
            "stable PR identity",
            "intent/scope plan",
            "actual current claims",
            "planned checks",
            "work/evidence trajectory",
            "validation → staged-material",
            "checkpoint per commit",
        ];
        let mut end = contract.find("Populate in order:").unwrap();
        for part in ordered {
            let found = contract[end..]
                .find(part)
                .unwrap_or_else(|| panic!("Missing ordered step {part}"));
            end += found + part.len();
        }
        assert!(contract.contains("Persist meaningful boundaries as work progresses"));
        assert!(contract.contains("not a fake claim"));
        assert!(
            contract.contains("Read operations never")
                || contract.contains("read-only and never migrate")
        );
    }
}

#[cfg(test)]
mod touched_path_tests {
    use super::{DocsCli, DocsCommand};
    use clap::Parser;
    use std::path::PathBuf;

    #[derive(Parser)]
    struct DocsParser {
        #[command(flatten)]
        docs: DocsCli,
    }

    #[test]
    fn touched_keeps_each_argv_path_whole() {
        let paths = [
            "./a path with spaces.rs",
            "./tab\tand\nnewline.rs",
            "./--help",
            "./second.rs",
        ];
        let args = ["docs", "build", "--touched"].into_iter().chain(paths);
        let parsed = DocsParser::try_parse_from(args).unwrap();
        let DocsCommand::Build {
            touched: Some(touched),
        } = parsed.docs.command
        else {
            panic!("expected touched paths")
        };
        assert_eq!(touched, paths.map(PathBuf::from));
    }

    #[cfg(unix)]
    #[test]
    fn touched_preserves_non_utf8_filename_bytes() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = OsString::from_vec(b"./non-utf8-\xff.rs".to_vec());
        let parsed = DocsParser::try_parse_from([
            OsString::from("docs"),
            OsString::from("build"),
            OsString::from("--touched"),
            path.clone(),
        ])
        .unwrap();
        let DocsCommand::Build {
            touched: Some(touched),
        } = parsed.docs.command
        else {
            panic!("expected touched paths")
        };
        assert_eq!(touched, [PathBuf::from(path)]);
    }
}
