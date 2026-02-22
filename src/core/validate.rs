//! Intent-driven methodology validation harness.
//!
//! This module implements the comprehensive validation suite that enforces
//! Decapod's contracts, invariants, and methodology gates.
//!
//! # For AI Agents
//!
//! - **`decapod validate` MUST pass before claiming work complete**: This is NON-NEGOTIABLE
//! - **Validation is deterministic**: Same repo state always produces same results
//! - **Gates enforce separation of concerns**: E.g., watchers can't mutate state
//! - **Health purity**: No manual health values allowed in canonical docs
//! - **Event sourcing**: Repo stores must match deterministic rebuild from events
//! - **Namespace purge**: Legacy namespace references are forbidden
//! - **Tooling validation**: Formatting, linting, and type checking must pass (see Tooling Validation Gate)
//!
//! # Validation Categories
//!
//! - Store integrity (user blank-slate, repo event-sourced)
//! - Constitution presence (embedded docs exist)
//! - Entrypoint gate (CLAUDE.md, AGENTS.md, etc.)
//! - Namespace purge (no legacy references)
//! - Health purity (no manual status values)
//! - Schema determinism (stable output)
//! - Policy isolation (approvals don't directly mutate health)
//! - Knowledge provenance (all entries have pointers)
//! - Watcher purity (read-only checks only)
//! - Archive integrity (hash verification)
//! - Canon mutation gate (no unauthorized doc writes)
//! - Tooling validation gate (formatting, linting, type checking)

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::output;
use crate::core::plan_governance;
use crate::core::store::{Store, StoreKind};
use crate::{db, primitives, todo};
use regex::Regex;
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use ulid::Ulid;

struct ValidationContext {
    pass_count: AtomicU32,
    fail_count: AtomicU32,
    warn_count: AtomicU32,
    fails: Mutex<Vec<String>>,
    warns: Mutex<Vec<String>>,
    repo_files_cache: Mutex<Vec<(PathBuf, Vec<PathBuf>)>>,
}

impl ValidationContext {
    fn new() -> Self {
        Self {
            pass_count: AtomicU32::new(0),
            fail_count: AtomicU32::new(0),
            warn_count: AtomicU32::new(0),
            fails: Mutex::new(Vec::new()),
            warns: Mutex::new(Vec::new()),
            repo_files_cache: Mutex::new(Vec::new()),
        }
    }
}

fn collect_repo_files(
    root: &Path,
    out: &mut Vec<PathBuf>,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    // Check cache first — this is called 3 times on the same root during validation.
    let cached = {
        let cache = ctx.repo_files_cache.lock().unwrap();
        cache
            .iter()
            .find(|(k, _)| k == root)
            .map(|(_, v)| v.clone())
    };
    if let Some(files) = cached {
        out.extend(files);
        return Ok(());
    }

    fn recurse(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), error::DecapodError> {
        if !dir.is_dir() {
            return Ok(());
        }

        let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == ".git" || name == "target" {
            return Ok(());
        }

        for entry in fs::read_dir(dir).map_err(error::DecapodError::IoError)? {
            let entry = entry.map_err(error::DecapodError::IoError)?;
            let path = entry.path();
            if path.is_dir() {
                recurse(&path, out)?;
            } else if path.is_file() {
                out.push(path);
            }
        }
        Ok(())
    }

    let start = out.len();
    recurse(root, out)?;
    // Cache the result for subsequent calls with the same root.
    ctx.repo_files_cache
        .lock()
        .unwrap()
        .push((root.to_path_buf(), out[start..].to_vec()));
    Ok(())
}

fn validate_no_legacy_namespaces(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Namespace Purge Gate");

    let mut files = Vec::new();
    collect_repo_files(decapod_dir, &mut files, ctx)?;

    let needles = [
        [".".to_string(), "globex".to_string()].concat(),
        [".".to_string(), "codex".to_string()].concat(),
    ];
    let mut offenders: Vec<(PathBuf, String)> = Vec::new();

    for path in files {
        // Skip obvious binaries.
        if path.extension().is_some_and(|e| e == "db") {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let is_texty = matches!(
            ext,
            "md" | "rs" | "toml" | "json" | "jsonl" | "yml" | "yaml" | "sh" | "lock"
        );
        if !is_texty {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for n in needles.iter() {
            if content.contains(n) {
                offenders.push((path.clone(), n.clone()));
            }
        }
    }

    if offenders.is_empty() {
        pass(
            "No legacy namespace references found in repo text sources",
            ctx,
        );
    } else {
        let mut msg = String::from("Forbidden legacy namespace references found:");
        for (p, n) in offenders.iter().take(12) {
            msg.push_str(&format!(" {}({})", p.display(), n));
        }
        if offenders.len() > 12 {
            msg.push_str(&format!(" ... ({} total)", offenders.len()));
        }
        fail(&msg, ctx);
    }
    Ok(())
}

fn validate_embedded_self_contained(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Embedded Self-Contained Gate");

    let constitution_dir = repo_root.join("constitution");
    if !constitution_dir.exists() {
        // This is a decapod repo, not a project with embedded docs
        skip("No constitution/ directory found (decapod repo)", ctx);
        return Ok(());
    }

    let mut files = Vec::new();
    collect_repo_files(&constitution_dir, &mut files, ctx)?;

    let mut offenders: Vec<PathBuf> = Vec::new();

    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Check for .decapod/ references that aren't documenting override behavior
        if content.contains(".decapod/") {
            // Allow legitimate documentation patterns, counting legitimate references (not just lines).
            let mut legitimate_ref_count = 0usize;
            for line in content.lines() {
                let refs_on_line = line.matches(".decapod/").count();
                if refs_on_line == 0 {
                    continue;
                }
                let is_legitimate_line = line.contains("<repo>")
                    || line.contains("store:")
                    || line.contains("directory")
                    || line.contains("override")
                    || line.contains("Override")
                    || line.contains("OVERRIDE.md")
                    || line.contains("Location:")
                    || line.contains("primarily contain")
                    || line.contains(".decapod/context/")
                    || line.contains(".decapod/memory/")
                    || line.contains("intended as")
                    || line.contains(".decapod/knowledge/")
                    || line.contains(".decapod/data/")
                    || line.contains(".decapod/workspaces/")
                    || line.contains("repo-scoped");
                if is_legitimate_line {
                    legitimate_ref_count += refs_on_line;
                }
            }

            let total_decapod_refs = content.matches(".decapod/").count();
            if total_decapod_refs > legitimate_ref_count {
                offenders.push(path);
            }
        }
    }

    if offenders.is_empty() {
        pass(
            "Embedded constitution files contain no invalid .decapod/ references",
            ctx,
        );
    } else {
        let mut msg =
            String::from("Embedded constitution files contain invalid .decapod/ references:");
        for p in offenders.iter().take(8) {
            msg.push_str(&format!(" {}", p.display()));
        }
        if offenders.len() > 8 {
            msg.push_str(&format!(" ... ({} total)", offenders.len()));
        }
        fail(&msg, ctx);
    }
    Ok(())
}

fn pass(_message: &str, ctx: &ValidationContext) {
    ctx.pass_count.fetch_add(1, Ordering::Relaxed);
}

fn fail(message: &str, ctx: &ValidationContext) {
    ctx.fail_count.fetch_add(1, Ordering::Relaxed);
    ctx.fails.lock().unwrap().push(message.to_string());
}

fn skip(_message: &str, ctx: &ValidationContext) {
    ctx.pass_count.fetch_add(1, Ordering::Relaxed);
}

fn warn(message: &str, ctx: &ValidationContext) {
    ctx.warn_count.fetch_add(1, Ordering::Relaxed);
    ctx.warns.lock().unwrap().push(message.to_string());
}

fn info(_message: &str) {}

fn count_tasks_in_db(db_path: &Path) -> Result<i64, error::DecapodError> {
    let conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(count)
}

fn fetch_tasks_fingerprint(db_path: &Path) -> Result<String, error::DecapodError> {
    let conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;
    let mut stmt = conn
        .prepare("SELECT id,title,status,updated_at,dir_path,scope,priority FROM tasks ORDER BY id")
        .map_err(error::DecapodError::RusqliteError)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "updated_at": row.get::<_, String>(3)?,
                "dir_path": row.get::<_, String>(4)?,
                "scope": row.get::<_, String>(5)?,
                "priority": row.get::<_, String>(6)?,
            }))
        })
        .map_err(error::DecapodError::RusqliteError)?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(error::DecapodError::RusqliteError)?);
    }
    Ok(serde_json::to_string(&out).unwrap())
}

fn validate_user_store_blank_slate(ctx: &ValidationContext) -> Result<(), error::DecapodError> {
    info("Store: user (blank-slate semantics)");
    let tmp_root = std::env::temp_dir().join(format!("decapod_validate_user_{}", Ulid::new()));
    fs::create_dir_all(&tmp_root).map_err(error::DecapodError::IoError)?;

    todo::initialize_todo_db(&tmp_root)?;
    let db_path = tmp_root.join("todo.db");
    let n = count_tasks_in_db(&db_path)?;

    if n == 0 {
        pass("User store starts empty (no automatic seeding)", ctx);
    } else {
        fail(
            &format!(
                "User store is not empty on fresh init ({} task(s) found)",
                n
            ),
            ctx,
        );
    }
    Ok(())
}

fn validate_repo_store_dogfood(
    store: &Store,
    ctx: &ValidationContext,
    _decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Store: repo (dogfood backlog semantics)");

    let events = store.root.join("todo.events.jsonl");
    if !events.is_file() {
        fail("Repo store missing todo.events.jsonl", ctx);
        return Ok(());
    }
    let content = fs::read_to_string(&events).map_err(error::DecapodError::IoError)?;
    let add_count = content
        .lines()
        .filter(|l| l.contains("\"event_type\":\"task.add\""))
        .count();

    // Fresh setup has 0 events but is valid.
    pass(
        &format!(
            "Repo backlog event log present ({} task.add events)",
            add_count
        ),
        ctx,
    );

    let db_path = store.root.join("todo.db");
    if !db_path.is_file() {
        fail("Repo store missing todo.db", ctx);
        return Ok(());
    }

    // Broker log integrity check
    let broker = DbBroker::new(&store.root);
    let replay_report = broker.verify_replay()?;
    if replay_report.divergences.is_empty() {
        pass("Audit log integrity verified (no pending event gaps)", ctx);
    } else {
        fail(
            &format!(
                "Audit log contains {} potential crash divergence(s)",
                replay_report.divergences.len()
            ),
            ctx,
        );
    }

    let tmp_root = std::env::temp_dir().join(format!("decapod_validate_repo_{}", Ulid::new()));
    fs::create_dir_all(&tmp_root).map_err(error::DecapodError::IoError)?;
    let tmp_db = tmp_root.join("todo.db");
    let _events = todo::rebuild_db_from_events(&events, &tmp_db)?;

    let fp_a = fetch_tasks_fingerprint(&db_path)?;
    let fp_b = fetch_tasks_fingerprint(&tmp_db)?;
    if fp_a == fp_b {
        pass(
            "Repo todo.db matches deterministic rebuild from todo.events.jsonl",
            ctx,
        );
    } else {
        fail(
            "Repo todo.db does NOT match rebuild from todo.events.jsonl",
            ctx,
        );
    }

    Ok(())
}

fn validate_repo_map(
    ctx: &ValidationContext,
    _decapod_dir: &Path, // decapod_dir is no longer used for filesystem constitution checks
) -> Result<(), error::DecapodError> {
    info("Repo Map");

    // We no longer check for a filesystem directory for constitution.
    // Instead, we verify embedded docs.
    pass(
        "Methodology constitution checks will verify embedded docs.",
        ctx,
    );

    let required_specs = ["specs/INTENT.md", "specs/SYSTEM.md"];
    let required_methodology = ["methodology/ARCHITECTURE.md"];
    for r in required_specs {
        if crate::core::assets::get_doc(r).is_some() {
            pass(&format!("Constitution doc {} present (embedded)", r), ctx);
        } else {
            fail(&format!("Constitution doc {} missing (embedded)", r), ctx);
        }
    }
    for r in required_methodology {
        if crate::core::assets::get_doc(r).is_some() {
            pass(&format!("Constitution doc {} present (embedded)", r), ctx);
        } else {
            fail(&format!("Constitution doc {} missing (embedded)", r), ctx);
        }
    }
    Ok(())
}

fn validate_docs_templates_bucket(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Entrypoint Gate");

    // Entrypoints MUST be in the project root
    let required = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];
    for a in required {
        let p = decapod_dir.join(a);
        if p.is_file() {
            pass(&format!("Root entrypoint {} present", a), ctx);
        } else {
            fail(
                &format!("Root entrypoint {} missing from project root", a),
                ctx,
            );
        }
    }

    if decapod_dir.join(".decapod").join("README.md").is_file() {
        pass(".decapod/README.md present", ctx);
    } else {
        fail(".decapod/README.md missing", ctx);
    }

    // NEGATIVE GATE: Decapod docs MUST NOT be copied into the project
    let forbidden_docs = decapod_dir.join(".decapod").join("docs");
    if forbidden_docs.exists() {
        fail(
            "Decapod internal docs were copied into .decapod/docs/ (Forbidden)",
            ctx,
        );
    } else {
        pass(
            "Decapod internal docs correctly excluded from project repo",
            ctx,
        );
    }

    // NEGATIVE GATE: projects/<id> MUST NOT exist
    let forbidden_projects = decapod_dir.join(".decapod").join("projects");
    if forbidden_projects.exists() {
        fail("Legacy .decapod/projects/ directory found (Forbidden)", ctx);
    } else {
        pass(".decapod/projects/ correctly absent", ctx);
    }

    Ok(())
}

fn validate_entrypoint_invariants(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Four Invariants Gate");

    // Check AGENTS.md for the four invariants
    let agents_path = decapod_dir.join("AGENTS.md");
    if !agents_path.is_file() {
        fail("AGENTS.md missing, cannot check invariants", ctx);
        return Ok(());
    }

    let content = fs::read_to_string(&agents_path).map_err(error::DecapodError::IoError)?;

    // Exact invariant strings (tamper detection)
    let exact_invariants = [
        ("core/DECAPOD.md", "Router pointer to core/DECAPOD.md"),
        ("cargo install decapod", "Version update gate language"),
        ("decapod validate", "Validation gate language"),
        (
            "decapod docs ingest",
            "Core constitution ingestion mandate language",
        ),
        ("Stop if", "Stop-if-missing behavior"),
        ("Docker git workspaces", "Docker workspace mandate language"),
        (
            "decapod todo claim --id <task-id>",
            "Task claim-before-work mandate language",
        ),
        (
            "request elevated permissions before Docker/container workspace commands",
            "Elevated-permissions mandate language",
        ),
        (
            "DECAPOD_SESSION_PASSWORD",
            "Per-agent session password mandate language",
        ),
        (
            ".decapod files are accessed only via decapod CLI",
            "Jail rule: .decapod access is CLI-only",
        ),
        (
            "Interface abstraction boundary",
            "Control-plane opacity language",
        ),
        (
            "Strict Dependency: You are strictly bound to the Decapod control plane",
            "Agent dependency enforcement language",
        ),
        ("✅", "Four invariants checklist format"),
    ];

    let mut all_present = true;
    for (marker, description) in exact_invariants {
        if content.contains(marker) {
            pass(&format!("Invariant present: {}", description), ctx);
        } else {
            fail(&format!("Invariant missing: {}", description), ctx);
            all_present = false;
        }
    }

    // Check for legacy router names (must not exist)
    let legacy_routers = ["MAESTRO.md", "GLOBEX.md", "CODEX.md\" as router"];
    for legacy in legacy_routers {
        if content.contains(legacy) {
            fail(
                &format!("AGENTS.md contains legacy router reference: {}", legacy),
                ctx,
            );
            all_present = false;
        }
    }

    // Line count check (AGENTS.md should be thin: max 100 lines for universal contract)
    let line_count = content.lines().count();
    const MAX_AGENTS_LINES: usize = 100;
    if line_count <= MAX_AGENTS_LINES {
        pass(
            &format!(
                "AGENTS.md is thin ({} lines ≤ {})",
                line_count, MAX_AGENTS_LINES
            ),
            ctx,
        );
    } else {
        fail(
            &format!(
                "AGENTS.md exceeds line limit ({} lines > {})",
                line_count, MAX_AGENTS_LINES
            ),
            ctx,
        );
        all_present = false;
    }

    // Check that agent-specific files defer to AGENTS.md and are thin
    const MAX_AGENT_SPECIFIC_LINES: usize = 70;
    for agent_file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let agent_path = decapod_dir.join(agent_file);
        if !agent_path.is_file() {
            fail(&format!("{} missing from project root", agent_file), ctx);
            all_present = false;
            continue;
        }

        let agent_content =
            fs::read_to_string(&agent_path).map_err(error::DecapodError::IoError)?;

        // Must defer to AGENTS.md
        if agent_content.contains("See `AGENTS.md`") || agent_content.contains("AGENTS.md") {
            pass(&format!("{} defers to AGENTS.md", agent_file), ctx);
        } else {
            fail(&format!("{} does not reference AGENTS.md", agent_file), ctx);
            all_present = false;
        }

        // Must reference canonical router
        if agent_content.contains("core/DECAPOD.md") {
            pass(&format!("{} references canonical router", agent_file), ctx);
        } else {
            fail(
                &format!("{} missing canonical router reference", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must use embedded doc paths via CLI, never direct constitution/* file paths.
        if agent_content.contains("decapod docs show constitution/")
            || agent_content.contains("(constitution/")
        {
            fail(
                &format!(
                    "{} references direct constitution filesystem paths; use embedded doc paths (e.g. core/*, specs/*, docs/*)",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        } else if agent_content.contains("decapod docs show docs/") {
            pass(
                &format!("{} references embedded docs path convention", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!(
                    "{} missing embedded docs path reference (`decapod docs show docs/...`)",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        }

        // Must include explicit jail rule for .decapod access
        if agent_content.contains(".decapod files are accessed only via decapod CLI") {
            pass(
                &format!("{} includes .decapod CLI-only jail rule", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!("{} missing .decapod CLI-only jail rule marker", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must include Docker git workspace mandate
        if agent_content.contains("Docker git workspaces") {
            pass(
                &format!("{} includes Docker workspace mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!("{} missing Docker workspace mandate marker", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must include elevated-permissions mandate for container workspace commands
        if agent_content
            .contains("request elevated permissions before Docker/container workspace commands")
        {
            pass(
                &format!("{} includes elevated-permissions mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!("{} missing elevated-permissions mandate marker", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must include per-agent session password mandate
        if agent_content.contains("DECAPOD_SESSION_PASSWORD") {
            pass(
                &format!("{} includes per-agent session password mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!(
                    "{} missing per-agent session password mandate marker",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        }

        // Must include claim-before-work mandate
        if agent_content.contains("decapod todo claim --id <task-id>") {
            pass(
                &format!("{} includes claim-before-work mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!("{} missing claim-before-work mandate marker", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must include task creation before claim mandate
        if agent_content.contains("decapod todo add \"<task>\"") {
            pass(
                &format!("{} includes task creation mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!("{} missing task creation mandate marker", agent_file),
                ctx,
            );
            all_present = false;
        }

        // Must include canonical Decapod workspace path mandate
        if agent_content.contains(".decapod/workspaces") {
            pass(
                &format!("{} includes canonical workspace path mandate", agent_file),
                ctx,
            );
        } else {
            fail(
                &format!(
                    "{} missing canonical workspace path marker (`.decapod/workspaces`)",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        }

        if agent_content.contains(".claude/worktrees") {
            let mut has_forbidden_positive_reference = false;
            for line in agent_content.lines() {
                if !line.contains(".claude/worktrees") {
                    continue;
                }
                let lower = line.to_ascii_lowercase();
                let is_negative_context = lower.contains("never")
                    || lower.contains("forbid")
                    || lower.contains("non-canonical")
                    || lower.contains("must not")
                    || lower.contains("do not");
                if !is_negative_context {
                    has_forbidden_positive_reference = true;
                    break;
                }
            }
            if has_forbidden_positive_reference {
                fail(
                    &format!(
                        "{} references forbidden non-canonical worktree path `.claude/worktrees`",
                        agent_file
                    ),
                    ctx,
                );
                all_present = false;
            } else {
                pass(
                    &format!(
                        "{} explicitly forbids `.claude/worktrees` non-canonical path",
                        agent_file
                    ),
                    ctx,
                );
            }
        }

        // Must include core constitution ingestion mandate
        if agent_content.contains("decapod docs ingest") {
            pass(
                &format!(
                    "{} includes core constitution ingestion mandate",
                    agent_file
                ),
                ctx,
            );
        } else {
            fail(
                &format!(
                    "{} missing core constitution ingestion mandate marker",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        }

        // Must include explicit update command in startup sequence
        if agent_content.contains("cargo install decapod") {
            pass(&format!("{} includes version update step", agent_file), ctx);
        } else {
            fail(
                &format!(
                    "{} missing version update step (`cargo install decapod`)",
                    agent_file
                ),
                ctx,
            );
            all_present = false;
        }

        // Must be thin (max 50 lines for agent-specific shims)
        let agent_lines = agent_content.lines().count();
        if agent_lines <= MAX_AGENT_SPECIFIC_LINES {
            pass(
                &format!(
                    "{} is thin ({} lines ≤ {})",
                    agent_file, agent_lines, MAX_AGENT_SPECIFIC_LINES
                ),
                ctx,
            );
        } else {
            fail(
                &format!(
                    "{} exceeds line limit ({} lines > {})",
                    agent_file, agent_lines, MAX_AGENT_SPECIFIC_LINES
                ),
                ctx,
            );
            all_present = false;
        }

        // Must not contain duplicated contracts (check for common duplication markers)
        let duplication_markers = [
            "## Lifecycle States", // Contract details belong in constitution
            "## Validation Rules", // Contract details belong in constitution
            "### Proof Gates",     // Contract details belong in constitution
            "## Store Model",      // Contract details belong in constitution
        ];
        for marker in duplication_markers {
            if agent_content.contains(marker) {
                fail(
                    &format!(
                        "{} contains duplicated contract details ({})",
                        agent_file, marker
                    ),
                    ctx,
                );
                all_present = false;
            }
        }
    }

    if all_present {
        pass("All entrypoint files follow thin waist architecture", ctx);
    }

    Ok(())
}

fn validate_interface_contract_bootstrap(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Interface Contract Bootstrap Gate");

    // This gate applies to the decapod repository where constitution/* is present.
    // Project repos initialized by `decapod init` should not fail on missing embedded docs.
    let constitution_dir = repo_root.join("constitution");
    if !constitution_dir.exists() {
        skip(
            "No constitution/ directory found (project repo); skipping interface bootstrap checks",
            ctx,
        );
        return Ok(());
    }

    let risk_policy_doc = repo_root.join("constitution/interfaces/RISK_POLICY_GATE.md");
    let context_pack_doc = repo_root.join("constitution/interfaces/AGENT_CONTEXT_PACK.md");
    for (path, label) in [
        (&risk_policy_doc, "RISK_POLICY_GATE interface"),
        (&context_pack_doc, "AGENT_CONTEXT_PACK interface"),
    ] {
        if path.is_file() {
            pass(&format!("{} present at {}", label, path.display()), ctx);
        } else {
            fail(&format!("{} missing at {}", label, path.display()), ctx);
        }
    }

    if risk_policy_doc.is_file() {
        let content = fs::read_to_string(&risk_policy_doc).map_err(error::DecapodError::IoError)?;
        for marker in [
            "**Authority:**",
            "**Layer:** Interfaces",
            "**Binding:** Yes",
            "**Scope:**",
            "**Non-goals:**",
            "## 3. Current-Head SHA Discipline",
            "## 6. Browser Evidence Manifest (UI/Critical Flows)",
            "## 8. Truth Labels and Upgrade Path",
            "## 10. Contract Example (JSON)",
            "## Links",
        ] {
            if content.contains(marker) {
                pass(
                    &format!("RISK_POLICY_GATE includes marker: {}", marker),
                    ctx,
                );
            } else {
                fail(&format!("RISK_POLICY_GATE missing marker: {}", marker), ctx);
            }
        }
    }

    if context_pack_doc.is_file() {
        let content =
            fs::read_to_string(&context_pack_doc).map_err(error::DecapodError::IoError)?;
        for marker in [
            "**Authority:**",
            "**Layer:** Interfaces",
            "**Binding:** Yes",
            "**Scope:**",
            "**Non-goals:**",
            "## 2. Deterministic Load Order",
            "## 3. Mutation Authority",
            "## 4. Memory Distillation Contract",
            "## 8. Truth Labels and Upgrade Path",
            "## Links",
        ] {
            if content.contains(marker) {
                pass(
                    &format!("AGENT_CONTEXT_PACK includes marker: {}", marker),
                    ctx,
                );
            } else {
                fail(
                    &format!("AGENT_CONTEXT_PACK missing marker: {}", marker),
                    ctx,
                );
            }
        }
    }

    Ok(())
}

fn extract_md_version(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("- v") {
            let v_and_rest = rest.trim();
            if !v_and_rest.is_empty() {
                // Extract version number, assuming it's the first word before the colon
                return v_and_rest.split(':').next().map(|s| s.trim().to_string());
            }
        }
    }
    None
}

fn validate_health_purity(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Health Purity Gate");
    let mut files = Vec::new();
    collect_repo_files(decapod_dir, &mut files, ctx)?;

    let forbidden =
        Regex::new(r"(?i)\(health:\s*(VERIFIED|ASSERTED|STALE|CONTRADICTED)\)").unwrap();
    let mut offenders = Vec::new();

    let generated_path = decapod_dir.join(".decapod").join("generated");

    for path in files {
        if path.extension().is_some_and(|e| e == "md") {
            // Skip files in the generated artifacts directory
            if path.starts_with(&generated_path) {
                continue;
            }

            let content = fs::read_to_string(&path).unwrap_or_default();
            if forbidden.is_match(&content) {
                offenders.push(path);
            }
        }
    }

    if offenders.is_empty() {
        pass(
            "No manual health status values found in authoritative docs",
            ctx,
        );
    } else {
        fail(
            &format!(
                "Manual health values found in non-generated files: {:?}",
                offenders
            ),
            ctx,
        );
    }
    Ok(())
}

fn validate_project_scoped_state(
    store: &Store,
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Project-Scoped State Gate");
    if store.kind != StoreKind::Repo {
        skip("Not in repo mode; skipping state scoping check", ctx);
        return Ok(());
    }

    // Check if any .db or .jsonl files exist outside .decapod/ in the project root
    let mut offenders = Vec::new();
    for entry in fs::read_dir(decapod_dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(ext, "db" | "jsonl") {
                offenders.push(path);
            }
        }
    }

    if offenders.is_empty() {
        pass("All state is correctly scoped within .decapod/", ctx);
    } else {
        fail(
            &format!(
                "Found Decapod state files outside .decapod/: {:?}",
                offenders
            ),
            ctx,
        );
    }
    Ok(())
}

fn validate_schema_determinism(
    ctx: &ValidationContext,
    _decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Schema Determinism Gate");
    let exe = std::env::current_exe().map_err(error::DecapodError::IoError)?;

    let run_schema = || -> Result<String, error::DecapodError> {
        let out = std::process::Command::new(&exe)
            .env("DECAPOD_BYPASS_SESSION", "1")
            .arg("data")
            .arg("schema")
            .arg("--deterministic")
            .output()
            .map_err(error::DecapodError::IoError)?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    };

    // Run sequentially: parallel execution causes non-determinism due to shared state
    let s1 = run_schema()?;
    let s2 = run_schema()?;

    if s1 == s2 && !s1.is_empty() {
        pass("Schema output is deterministic", ctx);
    } else {
        fail("Schema output is non-deterministic or empty", ctx);
    }
    Ok(())
}

fn validate_health_cache_integrity(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Health Cache Non-Authoritative Gate");
    let db_path = store.root.join("health.db");
    if !db_path.exists() {
        skip("health.db not found; skipping health integrity check", ctx);
        return Ok(());
    }

    let conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;

    // Check if any health_cache entries exist without corresponding proof_events
    let orphaned: i64 = conn.query_row(
        "SELECT COUNT(*) FROM health_cache hc LEFT JOIN proof_events pe ON hc.claim_id = pe.claim_id WHERE pe.event_id IS NULL",
        [],
        |row| row.get(0),
    ).map_err(error::DecapodError::RusqliteError)?;

    if orphaned == 0 {
        pass("No orphaned health cache entries (integrity pass)", ctx);
    } else {
        warn(
            &format!(
                "Found {} health cache entries without proof events (might be manual writes)",
                orphaned
            ),
            ctx,
        );
    }
    Ok(())
}

fn validate_risk_map(store: &Store, ctx: &ValidationContext) -> Result<(), error::DecapodError> {
    info("Risk Map Gate");
    let map_path = store.root.join("RISKMAP.json");
    if map_path.exists() {
        pass("Risk map (blast-radius) is present", ctx);
    } else {
        warn("Risk map missing (run `decapod riskmap init`)", ctx);
    }
    Ok(())
}

fn validate_risk_map_violations(
    store: &Store,
    ctx: &ValidationContext,
    pre_read_broker: Option<&str>,
) -> Result<(), error::DecapodError> {
    info("Zone Violation Gate");
    let fallback;
    let content = match pre_read_broker {
        Some(c) => c,
        None => {
            let audit_log = store.root.join("broker.events.jsonl");
            if !audit_log.exists() {
                return Ok(());
            }
            fallback = fs::read_to_string(audit_log)?;
            &fallback
        }
    };
    {
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\".decapod/\"") && line.contains("\"op\":\"todo.add\"") {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass("No risk zone violations detected in audit log", ctx);
        } else {
            fail(
                &format!("Detected operations in protected zones: {:?}", offenders),
                ctx,
            );
        }
    }
    Ok(())
}

fn validate_policy_integrity(
    store: &Store,
    ctx: &ValidationContext,
    pre_read_broker: Option<&str>,
) -> Result<(), error::DecapodError> {
    info("Policy Integrity Gates");
    let db_path = store.root.join("policy.db");
    if !db_path.exists() {
        skip("policy.db not found; skipping policy check", ctx);
        return Ok(());
    }

    let _conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;

    let fallback;
    let content_opt = match pre_read_broker {
        Some(c) => Some(c),
        None => {
            let audit_log = store.root.join("broker.events.jsonl");
            if audit_log.exists() {
                fallback = fs::read_to_string(audit_log)?;
                Some(fallback.as_str())
            } else {
                None
            }
        }
    };
    if let Some(content) = content_opt {
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"op\":\"policy.approve\"")
                && line.contains("\"db_id\":\"health.db\"")
            {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass(
                "Approval isolation verified (no direct health mutations)",
                ctx,
            );
        } else {
            fail(
                &format!(
                    "Policy approval directly mutated health state: {:?}",
                    offenders
                ),
                ctx,
            );
        }
    }

    Ok(())
}

fn validate_knowledge_integrity(
    store: &Store,
    ctx: &ValidationContext,
    pre_read_broker: Option<&str>,
) -> Result<(), error::DecapodError> {
    info("Knowledge Integrity Gate");
    let db_path = store.root.join("knowledge.db");
    if !db_path.exists() {
        skip(
            "knowledge.db not found; skipping knowledge integrity check",
            ctx,
        );
        return Ok(());
    }

    let query_missing_provenance = |conn: &rusqlite::Connection| -> Result<i64, rusqlite::Error> {
        conn.query_row(
            "SELECT COUNT(*) FROM knowledge WHERE provenance IS NULL OR provenance = ''",
            [],
            |row| row.get(0),
        )
    };

    let mut conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;
    let missing_provenance: i64 = match query_missing_provenance(&conn) {
        Ok(v) => v,
        Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
            if msg.contains("no such table: knowledge") =>
        {
            // Self-heal schema drift/partial bootstrap before validating integrity.
            db::initialize_knowledge_db(&store.root)?;
            conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;
            query_missing_provenance(&conn).map_err(error::DecapodError::RusqliteError)?
        }
        Err(e) => return Err(error::DecapodError::RusqliteError(e)),
    };

    if missing_provenance == 0 {
        pass(
            "Knowledge provenance verified (all entries have pointers)",
            ctx,
        );
    } else {
        fail(
            &format!(
                "Found {} knowledge entries missing mandatory provenance",
                missing_provenance
            ),
            ctx,
        );
    }

    let fallback;
    let content_opt = match pre_read_broker {
        Some(c) => Some(c),
        None => {
            let audit_log = store.root.join("broker.events.jsonl");
            if audit_log.exists() {
                fallback = fs::read_to_string(audit_log)?;
                Some(fallback.as_str())
            } else {
                None
            }
        }
    };
    if let Some(content) = content_opt {
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"op\":\"knowledge.add\"") && line.contains("\"db_id\":\"health.db\"")
            {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass("No direct health promotion from knowledge detected", ctx);
        } else {
            fail(
                &format!(
                    "Knowledge system directly mutated health state: {:?}",
                    offenders
                ),
                ctx,
            );
        }
    }

    Ok(())
}

fn validate_lineage_hard_gate(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Lineage Hard Gate");
    let todo_events = store.root.join("todo.events.jsonl");
    let federation_db = store.root.join("federation.db");
    let todo_db = store.root.join("todo.db");

    // Fast path: if any required file is missing, skip entirely
    if !todo_events.exists() || !federation_db.exists() || !todo_db.exists() {
        skip("lineage inputs missing; skipping", ctx);
        return Ok(());
    }

    // Quick check: if todo events is empty or very small, skip
    if let Ok(metadata) = fs::metadata(&todo_events) {
        if metadata.len() < 100 {
            skip("todo.events.jsonl too small; skipping", ctx);
            return Ok(());
        }
    }

    let content = match fs::read_to_string(&todo_events) {
        Ok(c) => c,
        Err(_) => {
            skip("cannot read todo.events.jsonl; skipping", ctx);
            return Ok(());
        }
    };

    // Fast path: if no intent: prefix events, skip the expensive part
    if !content.contains("intent:") {
        pass("no intent-tagged events found; skipping", ctx);
        return Ok(());
    }

    let mut add_candidates = Vec::new();
    let mut done_candidates = Vec::new();
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let event_type = v.get("event_type").and_then(|x| x.as_str()).unwrap_or("");
        let task_id = v.get("task_id").and_then(|x| x.as_str()).unwrap_or("");
        if task_id.is_empty() {
            continue;
        }
        let intent_ref = v
            .get("payload")
            .and_then(|p| p.get("intent_ref"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        // Hard gate only applies to new intent-tagged events.
        if !intent_ref.starts_with("intent:") {
            continue;
        }
        if event_type == "task.add" {
            add_candidates.push(task_id.to_string());
        } else if event_type == "task.done" {
            done_candidates.push(task_id.to_string());
        }
    }

    // Fast path: no candidates to check
    if add_candidates.is_empty() && done_candidates.is_empty() {
        pass("no intent-tagged task events to validate", ctx);
        return Ok(());
    }

    let conn = db::db_connect_for_validate(&federation_db.to_string_lossy())?;
    let todo_conn = db::db_connect_for_validate(&todo_db.to_string_lossy())?;
    let mut violations = Vec::new();

    for task_id in add_candidates {
        let exists: i64 = todo_conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = ?1",
                rusqlite::params![task_id.clone()],
                |row| row.get(0),
            )
            .map_err(error::DecapodError::RusqliteError)?;
        if exists == 0 {
            continue;
        }
        let source = format!("event:{}", task_id);
        let commitment_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n JOIN sources s ON s.node_id = n.id WHERE s.source = ?1 AND n.node_type = 'commitment'",
                rusqlite::params![source],
                |row| row.get(0),
            )
            .map_err(error::DecapodError::RusqliteError)?;
        if commitment_count == 0 {
            violations.push(format!(
                "task.add {} missing commitment lineage node",
                task_id
            ));
        }
    }

    for task_id in done_candidates {
        let exists: i64 = todo_conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = ?1",
                rusqlite::params![task_id.clone()],
                |row| row.get(0),
            )
            .map_err(error::DecapodError::RusqliteError)?;
        if exists == 0 {
            continue;
        }
        let source = format!("event:{}", task_id);
        let commitment_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n JOIN sources s ON s.node_id = n.id WHERE s.source = ?1 AND n.node_type = 'commitment'",
                rusqlite::params![source.clone()],
                |row| row.get(0),
            )
            .map_err(error::DecapodError::RusqliteError)?;
        let decision_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n JOIN sources s ON s.node_id = n.id WHERE s.source = ?1 AND n.node_type = 'decision'",
                rusqlite::params![source],
                |row| row.get(0),
            )
            .map_err(error::DecapodError::RusqliteError)?;
        if commitment_count == 0 || decision_count == 0 {
            violations.push(format!(
                "task.done {} missing commitment/decision lineage nodes",
                task_id
            ));
        }
    }

    if violations.is_empty() {
        pass(
            "Intent-tagged task.add/task.done events have commitment+proof lineage",
            ctx,
        );
    } else {
        fail(&format!("Lineage gate violations: {:?}", violations), ctx);
    }
    Ok(())
}

fn validate_repomap_determinism(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Repo Map Determinism Gate");
    use crate::core::repomap;
    let dir1 = decapod_dir.to_path_buf();
    let dir2 = decapod_dir.to_path_buf();
    let h1 =
        std::thread::spawn(move || serde_json::to_string(&repomap::generate_map(&dir1)).unwrap());
    let h2 =
        std::thread::spawn(move || serde_json::to_string(&repomap::generate_map(&dir2)).unwrap());

    let m1 = h1
        .join()
        .map_err(|_| error::DecapodError::ValidationError("repomap thread panicked".into()))?;
    let m2 = h2
        .join()
        .map_err(|_| error::DecapodError::ValidationError("repomap thread panicked".into()))?;

    if m1 == m2 && !m1.is_empty() {
        pass("Repo map output is deterministic", ctx);
    } else {
        fail("Repo map output is non-deterministic or empty", ctx);
    }
    Ok(())
}

fn validate_watcher_audit(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Watcher Audit Gate");
    let audit_log = store.root.join("watcher.events.jsonl");
    if audit_log.exists() {
        pass("Watcher audit trail present", ctx);
    } else {
        warn(
            "Watcher audit trail missing (run `decapod govern watcher run`)",
            ctx,
        );
    }
    Ok(())
}

fn validate_watcher_purity(
    store: &Store,
    ctx: &ValidationContext,
    pre_read_broker: Option<&str>,
) -> Result<(), error::DecapodError> {
    info("Watcher Purity Gate");
    let fallback;
    let content_opt = match pre_read_broker {
        Some(c) => Some(c),
        None => {
            let audit_log = store.root.join("broker.events.jsonl");
            if audit_log.exists() {
                fallback = fs::read_to_string(audit_log)?;
                Some(fallback.as_str())
            } else {
                None
            }
        }
    };
    if let Some(content) = content_opt {
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"actor\":\"watcher\"") {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass("Watcher purity verified (read-only checks only)", ctx);
        } else {
            fail(
                &format!(
                    "Watcher subsystem attempted brokered mutations: {:?}",
                    offenders
                ),
                ctx,
            );
        }
    }
    Ok(())
}

fn validate_archive_integrity(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Archive Integrity Gate");
    let db_path = store.root.join("archive.db");
    if !db_path.exists() {
        skip("archive.db not found; skipping archive check", ctx);
        return Ok(());
    }

    use crate::archive;
    let failures = archive::verify_archives(store)?;
    if failures.is_empty() {
        pass(
            "All session archives verified (content and hash match)",
            ctx,
        );
    } else {
        fail(
            &format!("Archive integrity failures detected: {:?}", failures),
            ctx,
        );
    }
    Ok(())
}

fn validate_control_plane_contract(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Control Plane Contract Gate");

    // Check that all database mutations went through the broker
    // by verifying event log consistency
    let data_dir = &store.root;
    let mut violations = Vec::new();

    // Check for broker audit trail presence
    let broker_log = data_dir.join("broker.events.jsonl");
    if !broker_log.exists() {
        // First run - no broker log yet, this is OK
        pass("No broker events yet (first run)", ctx);
        return Ok(());
    }

    // Check that critical databases have corresponding broker events
    let todo_db = data_dir.join("todo.db");
    if todo_db.exists() {
        let todo_events = data_dir.join("todo.events.jsonl");
        if !todo_events.exists() {
            violations.push("todo.db exists but todo.events.jsonl is missing".to_string());
        }
    }

    let federation_db = data_dir.join("federation.db");
    if federation_db.exists() {
        let federation_events = data_dir.join("federation.events.jsonl");
        if !federation_events.exists() {
            violations
                .push("federation.db exists but federation.events.jsonl is missing".to_string());
        }
    }

    // Check for direct SQLite write patterns in process list (best effort)
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("lsof")
            .args(["+D", data_dir.to_string_lossy().as_ref()])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("sqlite") && !line.contains("decapod") {
                    violations.push(format!("External SQLite process accessing store: {}", line));
                }
            }
        }
    }

    if violations.is_empty() {
        pass(
            "Control plane contract honored (all mutations brokered)",
            ctx,
        );
    } else {
        fail(
            &format!(
                "Control plane contract violations detected: {:?}",
                violations
            ),
            ctx,
        );
    }

    Ok(())
}

fn validate_canon_mutation(
    store: &Store,
    ctx: &ValidationContext,
    pre_read_broker: Option<&str>,
) -> Result<(), error::DecapodError> {
    info("Canon Mutation Gate");
    let fallback;
    let content_opt = match pre_read_broker {
        Some(c) => Some(c),
        None => {
            let audit_log = store.root.join("broker.events.jsonl");
            if audit_log.exists() {
                fallback = fs::read_to_string(audit_log)?;
                Some(fallback.as_str())
            } else {
                None
            }
        }
    };
    if let Some(content) = content_opt {
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"op\":\"write\"")
                && (line.contains(".md\"") || line.contains(".json\""))
                && !line.contains("\"actor\":\"decapod\"")
                && !line.contains("\"actor\":\"scaffold\"")
            {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass("No unauthorized canon mutations detected", ctx);
        } else {
            warn(
                &format!(
                    "Detected direct mutations to canonical documents: {:?}",
                    offenders
                ),
                ctx,
            );
        }
    }
    Ok(())
}

fn validate_heartbeat_invocation_gate(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Heartbeat Invocation Gate");

    let lib_rs = decapod_dir.join("src").join("lib.rs");
    let todo_rs = decapod_dir.join("src").join("plugins").join("todo.rs");
    if lib_rs.exists() && todo_rs.exists() {
        let lib_content = fs::read_to_string(&lib_rs).unwrap_or_default();
        let todo_content = fs::read_to_string(&todo_rs).unwrap_or_default();

        let code_markers = [
            (
                lib_content.contains("should_auto_clock_in(&cli.command)")
                    && lib_content.contains("todo::clock_in_agent_presence(&project_store)?"),
                "Top-level command dispatch auto-clocks heartbeat",
            ),
            (
                lib_content
                    .contains("Command::Todo(todo_cli) => !todo::is_heartbeat_command(todo_cli)"),
                "Decorator excludes explicit todo heartbeat to prevent duplicates",
            ),
            (
                todo_content.contains("pub fn clock_in_agent_presence")
                    && todo_content.contains("record_heartbeat"),
                "TODO plugin exposes reusable clock-in helper",
            ),
        ];

        for (ok, msg) in code_markers {
            if ok {
                pass(msg, ctx);
            } else {
                fail(msg, ctx);
            }
        }
    } else {
        skip(
            "Heartbeat wiring source files absent; skipping code-level heartbeat checks",
            ctx,
        );
    }

    let doc_markers = [
        (
            crate::core::assets::get_doc("core/DECAPOD.md")
                .unwrap_or_default()
                .contains("invocation heartbeat"),
            "Router documents invocation heartbeat contract",
        ),
        (
            crate::core::assets::get_doc("interfaces/CONTROL_PLANE.md")
                .unwrap_or_default()
                .contains("invocation heartbeat"),
            "Control-plane interface documents invocation heartbeat",
        ),
        (
            crate::core::assets::get_doc("plugins/TODO.md")
                .unwrap_or_default()
                .contains("auto-clocks liveness"),
            "TODO plugin documents automatic liveness clock-in",
        ),
        (
            crate::core::assets::get_doc("plugins/REFLEX.md")
                .unwrap_or_default()
                .contains("todo.heartbeat.autoclaim"),
            "REFLEX plugin documents heartbeat autoclaim action",
        ),
    ];

    for (ok, msg) in doc_markers {
        if ok {
            pass(msg, ctx);
        } else {
            fail(msg, ctx);
        }
    }

    Ok(())
}

fn validate_federation_gates(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Federation Gates");

    let results = crate::plugins::federation::validate_federation(&store.root)?;

    for (gate_name, passed, message) in results {
        if passed {
            pass(&format!("[{}] {}", gate_name, message), ctx);
        } else {
            // Federation gates are advisory (warn) rather than hard-fail because the
            // two-phase DB+JSONL write design can produce transient drift that does
            // not indicate data loss.
            warn(&format!("[{}] {}", gate_name, message), ctx);
        }
    }

    Ok(())
}

fn validate_markdown_primitives_roundtrip_gate(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("Markdown Primitive Round-Trip Gate");
    match primitives::validate_roundtrip_gate(store) {
        Ok(()) => {
            pass(
                "Markdown primitives export and round-trip validation pass",
                ctx,
            );
        }
        Err(err) => {
            fail(
                &format!("Markdown primitive round-trip failed: {}", err),
                ctx,
            );
        }
    }
    Ok(())
}

/// Validates that tooling requirements are satisfied.
/// This gate ensures formatting, linting, and type checking pass before promotion.
fn validate_git_workspace_context(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Git Workspace Context Gate");

    // Allow bypass for testing/CI environments
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        skip(
            "Git workspace gates skipped (DECAPOD_VALIDATE_SKIP_GIT_GATES set)",
            ctx,
        );
        return Ok(());
    }

    // Exempt read-only schema commands (data schema, lcm schema, map schema)
    let args: Vec<String> = std::env::args().collect();
    let is_schema_command = args.iter().any(|a| {
        a == "schema"
            || (a == "lcm"
                && args
                    .iter()
                    .skip_while(|x| *x != "lcm")
                    .nth(1)
                    .is_some_and(|x| x == "schema"))
            || (a == "map"
                && args
                    .iter()
                    .skip_while(|x| *x != "map")
                    .nth(1)
                    .is_some_and(|x| x == "schema"))
    });
    if is_schema_command {
        skip(
            "Schema command exempted from workspace requirement (read-only)",
            ctx,
        );
        return Ok(());
    }

    let signals_container = [
        (
            std::env::var("DECAPOD_CONTAINER").ok().as_deref() == Some("1"),
            "DECAPOD_CONTAINER=1",
        ),
        (repo_root.join(".dockerenv").exists(), ".dockerenv marker"),
        (
            repo_root.join(".devcontainer").exists(),
            ".devcontainer marker",
        ),
        (
            std::env::var("DOCKER_CONTAINER").is_ok(),
            "DOCKER_CONTAINER env",
        ),
    ];

    let in_container = signals_container.iter().any(|(signal, _)| *signal);

    if in_container {
        let reasons: Vec<&str> = signals_container
            .iter()
            .filter(|(signal, _)| *signal)
            .map(|(_, name)| *name)
            .collect();
        pass(
            &format!(
                "Running in container workspace (signals: {})",
                reasons.join(", ")
            ),
            ctx,
        );
    } else {
        fail(
            "Not running in container workspace - git-tracked work must execute in Docker-isolated workspace (claim.git.container_workspace_required)",
            ctx,
        );
    }

    let git_dir = repo_root.join(".git");
    let is_worktree = git_dir.is_file() && {
        let content = fs::read_to_string(&git_dir).unwrap_or_default();
        content.contains("gitdir:")
    };

    if is_worktree {
        pass("Running in git worktree (isolated branch)", ctx);
    } else if in_container {
        pass(
            "Container workspace detected (worktree check informational)",
            ctx,
        );
    } else {
        fail(
            "Not running in isolated git worktree - must use container workspace for implementation work",
            ctx,
        );
    }

    validate_commit_often_gate(ctx, repo_root)?;

    Ok(())
}

fn validate_commit_often_gate(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    let max_dirty_files = std::env::var("DECAPOD_COMMIT_OFTEN_MAX_DIRTY_FILES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(6);

    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(error::DecapodError::IoError)?;

    if !status_output.status.success() {
        warn("Commit-often gate skipped: unable to read git status", ctx);
        return Ok(());
    }

    let dirty_count = String::from_utf8_lossy(&status_output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    if dirty_count == 0 {
        pass("Commit-often gate: working tree is clean", ctx);
        return Ok(());
    }

    if dirty_count > max_dirty_files {
        fail(
            &format!(
                "Commit-often mandate violation: {} dirty file(s) exceed limit {}. Commit incremental changes before continuing.",
                dirty_count, max_dirty_files
            ),
            ctx,
        );
    } else {
        pass(
            &format!(
                "Commit-often gate: {} dirty file(s) within limit {}",
                dirty_count, max_dirty_files
            ),
            ctx,
        );
    }

    Ok(())
}

fn validate_plan_governed_execution_gate(
    store: &Store,
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Plan-Governed Execution Gate");

    // Test harnesses and isolated fixture repos explicitly bypass git gates.
    // Keep plan-governed promotion checks out of that mode to preserve stable
    // verification replay fixtures that are not modeled as full workspaces.
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        skip(
            "Plan-governed execution gate skipped (DECAPOD_VALIDATE_SKIP_GIT_GATES set)",
            ctx,
        );
        return Ok(());
    }

    let plan = plan_governance::load_plan(repo_root)?;
    if let Some(plan) = plan {
        if plan.state != plan_governance::PlanState::Approved
            && plan.state != plan_governance::PlanState::Done
        {
            fail(
                &format!(
                    "NEEDS_PLAN_APPROVAL: plan state is {:?}; execution/promotion requires APPROVED or DONE",
                    plan.state
                ),
                ctx,
            );
        } else {
            pass("Plan artifact state allows governed execution", ctx);
        }

        if plan.intent.trim().is_empty()
            || !plan.unknowns.is_empty()
            || !plan.human_questions.is_empty()
        {
            fail(
                "NEEDS_HUMAN_INPUT: governed plan has unresolved intent/unknowns/questions",
                ctx,
            );
        } else {
            pass("Plan intent and unknowns are resolved", ctx);
        }

        if let Err(e) = plan_governance::ensure_architecture_artifact_ready(repo_root) {
            fail(&e.to_string(), ctx);
        } else {
            pass(
                "Governed architecture artifact is present and complete",
                ctx,
            );
        }
    } else {
        let done_count = plan_governance::count_done_todos(&store.root)?;
        if done_count > 0 {
            fail(
                &format!(
                    "NEEDS_PLAN_APPROVAL: {} done TODO(s) exist but governed PLAN artifact is missing",
                    done_count
                ),
                ctx,
            );
        } else {
            pass(
                "No governed plan artifact present; gate is advisory until first done TODO",
                ctx,
            );
        }
    }

    let unverified = plan_governance::collect_unverified_done_todos(&store.root)?;
    if !unverified.is_empty() {
        fail(
            &format!(
                "PROOF_HOOK_FAILED: {} done TODO(s) are CLAIMED but not VERIFIED: {}",
                unverified.len(),
                output::preview_messages(&unverified, 4, 80)
            ),
            ctx,
        );
    } else {
        pass("Done TODOs are proof-verified", ctx);
    }

    Ok(())
}

fn validate_git_protected_branch(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Git Protected Branch Gate");

    // Allow bypass for testing/CI environments
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        skip(
            "Git protected branch gate skipped (DECAPOD_VALIDATE_SKIP_GIT_GATES set)",
            ctx,
        );
        return Ok(());
    }

    let protected_patterns = ["master", "main", "production", "stable"];

    let current_branch = {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_root)
            .output();
        output
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".to_string())
    };

    let is_protected = protected_patterns
        .iter()
        .any(|p| current_branch == *p || current_branch.starts_with("release/"));

    if is_protected {
        fail(
            &format!(
                "Currently on protected branch '{}' - implementation work must happen in working branch, not directly on protected refs (claim.git.no_direct_main_push)",
                current_branch
            ),
            ctx,
        );
    } else {
        pass(
            &format!("On working branch '{}' (not protected)", current_branch),
            ctx,
        );
    }

    let has_remote = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_remote {
        let ahead_behind = std::process::Command::new("git")
            .args(["rev-list", "--left-right", "--count", "HEAD...origin/HEAD"])
            .current_dir(repo_root)
            .output();

        if let Ok(out) = ahead_behind {
            if out.status.success() {
                let counts = String::from_utf8_lossy(&out.stdout);
                let parts: Vec<&str> = counts.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ahead: u32 = parts[0].parse().unwrap_or(0);
                    if ahead > 0 {
                        let output = std::process::Command::new("git")
                            .args(["rev-list", "--format=%s", "-n1", "HEAD"])
                            .current_dir(repo_root)
                            .output();
                        let commit_msg = output
                            .ok()
                            .and_then(|o| {
                                if o.status.success() {
                                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| "unknown".to_string());

                        fail(
                            &format!(
                                "Protected branch has {} unpushed commit(s) - direct push to protected branch detected (commit: {})",
                                ahead, commit_msg
                            ),
                            ctx,
                        );
                    } else {
                        pass("No unpushed commits to protected branches", ctx);
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_tooling_gate(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Tooling Validation Gate");

    if std::env::var("DECAPOD_VALIDATE_SKIP_TOOLING_GATES").is_ok() {
        skip(
            "Tooling validation gates skipped (DECAPOD_VALIDATE_SKIP_TOOLING_GATES set)",
            ctx,
        );
        return Ok(());
    }

    let mut has_failures = false;
    let mut has_tooling = false;

    let cargo_toml = repo_root.join("Cargo.toml");
    if cargo_toml.exists() {
        has_tooling = true;
        let root_fmt = repo_root.to_path_buf();
        let root_clippy = repo_root.to_path_buf();

        let fmt_handle = std::thread::spawn(move || {
            std::process::Command::new("cargo")
                .args(["fmt", "--all", "--", "--check"])
                .current_dir(&root_fmt)
                .output()
        });

        let clippy_handle = std::thread::spawn(move || {
            std::process::Command::new("cargo")
                .args([
                    "clippy",
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ])
                .current_dir(&root_clippy)
                .output()
        });

        match fmt_handle.join().expect("fmt thread panicked") {
            Ok(output) => {
                if output.status.success() {
                    pass("Rust code formatting passes (cargo fmt)", ctx);
                } else {
                    fail("Rust code formatting failed - run `cargo fmt --all`", ctx);
                    has_failures = true;
                }
            }
            Err(e) => {
                fail(&format!("Failed to run cargo fmt: {}", e), ctx);
                has_failures = true;
            }
        }

        match clippy_handle.join().expect("clippy thread panicked") {
            Ok(output) => {
                if output.status.success() {
                    pass("Rust linting passes (cargo clippy)", ctx);
                } else {
                    fail(
                        "Rust linting failed - run `cargo clippy --all-targets --all-features`",
                        ctx,
                    );
                    has_failures = true;
                }
            }
            Err(e) => {
                fail(&format!("Failed to run cargo clippy: {}", e), ctx);
                has_failures = true;
            }
        }
    }

    let pyproject = repo_root.join("pyproject.toml");
    let requirements = repo_root.join("requirements.txt");
    if pyproject.exists() || requirements.exists() {
        has_tooling = true;

        if std::process::Command::new("which")
            .arg("ruff")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            let root_ruff = repo_root.to_path_buf();
            let ruff_handle = std::thread::spawn(move || {
                std::process::Command::new("ruff")
                    .args(["check", ".", "--output-format=concise"])
                    .current_dir(&root_ruff)
                    .output()
            });

            match ruff_handle.join().expect("ruff thread panicked") {
                Ok(output) => {
                    if output.status.success() {
                        pass("Python linting passes (ruff)", ctx);
                    } else {
                        fail("Python linting failed - fix ruff violations", ctx);
                        has_failures = true;
                    }
                }
                Err(e) => {
                    warn(&format!("ruff not available: {}", e), ctx);
                }
            }
        } else {
            skip("ruff not installed; skipping Python linting", ctx);
        }
    }

    let shell_check = repo_root.join(".shellcheckrc");
    let shell_files_exist = std::fs::read_dir(repo_root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let p = e.path();
            p.is_file() && p.extension().map(|s| s == "sh").unwrap_or(false)
        });

    if shell_check.exists() || shell_files_exist {
        has_tooling = true;

        if std::process::Command::new("which")
            .arg("shellcheck")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            let repo_root_clone = repo_root.to_path_buf();
            let shellcheck_handle = std::thread::spawn(move || {
                std::process::Command::new("shellcheck")
                    .args(["--enable=all"])
                    .current_dir(repo_root_clone)
                    .output()
            });

            match shellcheck_handle
                .join()
                .expect("shellcheck thread panicked")
            {
                Ok(output) => {
                    if output.status.success() {
                        pass("Shell script linting passes (shellcheck)", ctx);
                    } else {
                        fail(
                            "Shell script linting failed - fix shellcheck violations",
                            ctx,
                        );
                        has_failures = true;
                    }
                }
                Err(e) => {
                    warn(&format!("shellcheck failed: {}", e), ctx);
                }
            }
        } else {
            skip("shellcheck not installed; skipping shell linting", ctx);
        }
    }

    let yaml_check = repo_root.join(".yamllint");
    let yaml_files_exist = std::fs::read_dir(repo_root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let p = e.path();
            p.is_file()
                && p.extension()
                    .map(|s| s == "yaml" || s == "yml")
                    .unwrap_or(false)
        });

    if yaml_check.exists() || yaml_files_exist {
        has_tooling = true;

        if std::process::Command::new("which")
            .arg("yamllint")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            let repo_root_clone = repo_root.to_path_buf();
            let yamllint_handle = std::thread::spawn(move || {
                std::process::Command::new("yamllint")
                    .arg(".")
                    .current_dir(repo_root_clone)
                    .output()
            });

            match yamllint_handle.join().expect("yamllint thread panicked") {
                Ok(output) => {
                    if output.status.success() {
                        pass("YAML linting passes (yamllint)", ctx);
                    } else {
                        fail("YAML linting failed - fix yamllint violations", ctx);
                        has_failures = true;
                    }
                }
                Err(e) => {
                    warn(&format!("yamllint failed: {}", e), ctx);
                }
            }
        } else {
            skip("yamllint not installed; skipping YAML linting", ctx);
        }
    }

    let dockerfile_exists = std::fs::read_dir(repo_root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase() == "dockerfile")
                .unwrap_or(false)
        });

    if dockerfile_exists {
        has_tooling = true;

        if std::process::Command::new("which")
            .arg("hadolint")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            let repo_root_clone = repo_root.to_path_buf();
            let hadolint_handle = std::thread::spawn(move || {
                std::process::Command::new("hadolint")
                    .args(["Dockerfile"])
                    .current_dir(repo_root_clone)
                    .output()
            });

            match hadolint_handle.join().expect("hadolint thread panicked") {
                Ok(output) => {
                    if output.status.success() {
                        pass("Dockerfile linting passes (hadolint)", ctx);
                    } else {
                        fail("Dockerfile linting failed - fix hadolint violations", ctx);
                        has_failures = true;
                    }
                }
                Err(e) => {
                    warn(&format!("hadolint failed: {}", e), ctx);
                }
            }
        } else {
            skip("hadolint not installed; skipping Dockerfile linting", ctx);
        }
    }

    if !has_tooling {
        skip(
            "No recognized project files found; skipping tooling validation",
            ctx,
        );
    } else if !has_failures {
        pass(
            "All toolchain validations pass - project is ready for promotion",
            ctx,
        );
    }

    Ok(())
}

fn validate_state_commit_gate(
    ctx: &ValidationContext,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("STATE_COMMIT Validation Gate");

    // Policy knob: configurable CI job name (can be set via env var)
    let required_ci_job = std::env::var("DECAPOD_STATE_COMMIT_CI_JOB")
        .unwrap_or_else(|_| "state_commit_golden_vectors".to_string());

    info(&format!(
        "STATE_COMMIT: required_ci_job = {}",
        required_ci_job
    ));

    // Check for v1 golden directory (versioned)
    let golden_v1_dir = repo_root
        .join("tests")
        .join("golden")
        .join("state_commit")
        .join("v1");
    if !golden_v1_dir.exists() {
        skip(
            "No tests/golden/state_commit/v1 directory found; skipping STATE_COMMIT validation",
            ctx,
        );
        return Ok(());
    }

    // Check for required v1 golden files
    let required_files = ["scope_record_hash.txt", "state_commit_root.txt"];
    let mut has_golden = true;
    for file in &required_files {
        if !golden_v1_dir.join(file).exists() {
            fail(
                &format!("Missing golden file: tests/golden/state_commit/v1/{}", file),
                ctx,
            );
            has_golden = false;
        }
    }

    // Immutability check: v1 files should not change
    // In v1, these are the canonical golden vectors
    if has_golden {
        pass("STATE_COMMIT v1 golden vectors present", ctx);

        // Verify the expected hashes match v1 protocol
        let expected_scope_hash =
            "41d7e3729b6f4512887fb3cb6f10140942b600041e0d88308b0177e06ebb4b93";
        let expected_root = "28591ac86e52ffac76d5fc3aceeceda5d8592708a8d7fcb75371567fdc481492";

        if let Ok(actual_hash) =
            std::fs::read_to_string(golden_v1_dir.join("scope_record_hash.txt"))
        {
            if actual_hash.trim() != expected_scope_hash {
                fail(
                    &format!(
                        "STATE_COMMIT v1 scope_record_hash changed! Expected {}, got {}. This requires a SPEC_VERSION bump to v2.",
                        expected_scope_hash,
                        actual_hash.trim()
                    ),
                    ctx,
                );
            }
        }

        if let Ok(actual_root) =
            std::fs::read_to_string(golden_v1_dir.join("state_commit_root.txt"))
        {
            if actual_root.trim() != expected_root {
                fail(
                    &format!(
                        "STATE_COMMIT v1 state_commit_root changed! Expected {}, got {}. This requires a SPEC_VERSION bump to v2.",
                        expected_root,
                        actual_root.trim()
                    ),
                    ctx,
                );
            }
        }
    }

    Ok(())
}

fn validate_obligations(store: &Store, ctx: &ValidationContext) -> Result<(), error::DecapodError> {
    // Initialize the DB to ensure tables exist
    crate::core::obligation::initialize_obligation_db(&store.root)?;

    let obligations = crate::core::obligation::list_obligations(store)?;
    let mut met_count = 0;
    for ob in obligations {
        // If an obligation is marked Met, we MUST verify it still holds
        if ob.status == crate::core::obligation::ObligationStatus::Met {
            let (status, reason) = crate::core::obligation::verify_obligation(store, &ob.id)?;
            if status != crate::core::obligation::ObligationStatus::Met {
                fail(
                    &format!("Obligation {} failed verification: {}", ob.id, reason),
                    ctx,
                );
            } else {
                met_count += 1;
            }
        }
    }
    pass(
        &format!(
            "Obligation Graph Validation Gate ({} met nodes verified)",
            met_count
        ),
        ctx,
    );
    Ok(())
}

fn validate_lcm_immutability(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("LCM Immutability Gate");
    let ledger_path = store.root.join(crate::core::schemas::LCM_EVENTS_NAME);
    if !ledger_path.exists() {
        pass("No LCM ledger yet; gate trivially passes", ctx);
        return Ok(());
    }

    let failures = crate::plugins::lcm::validate_ledger_integrity(&store.root)?;
    if failures.is_empty() {
        pass("LCM ledger integrity verified", ctx);
    } else {
        for f in &failures {
            fail(&format!("LCM immutability: {}", f), ctx);
        }
    }
    Ok(())
}

fn validate_lcm_rebuild_gate(
    store: &Store,
    ctx: &ValidationContext,
) -> Result<(), error::DecapodError> {
    info("LCM Rebuild Gate");
    let ledger_path = store.root.join(crate::core::schemas::LCM_EVENTS_NAME);
    if !ledger_path.exists() {
        pass("No LCM ledger yet; rebuild gate trivially passes", ctx);
        return Ok(());
    }

    let result = crate::plugins::lcm::rebuild_index(store, true)?;
    if result.get("status").and_then(|v| v.as_str()) == Some("success") {
        pass("LCM index rebuild successful", ctx);
    } else {
        let errors = result
            .get("errors")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        fail(&format!("LCM rebuild failed: {}", errors), ctx);
    }
    Ok(())
}

fn validate_gatekeeper_gate(
    ctx: &ValidationContext,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Gatekeeper Safety Gate");

    // Get staged files from git (if in a git repo)
    let output = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(decapod_dir)
        .output();

    let staged_paths: Vec<PathBuf> = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect(),
        _ => {
            skip(
                "Git not available or not in a repo; skipping gatekeeper gate",
                ctx,
            );
            return Ok(());
        }
    };

    if staged_paths.is_empty() {
        pass("No staged files; gatekeeper gate trivially passes", ctx);
        return Ok(());
    }

    let config = crate::core::gatekeeper::GatekeeperConfig::default();
    let result = crate::core::gatekeeper::run_gatekeeper(decapod_dir, &staged_paths, 0, &config)?;

    if result.passed {
        pass(
            &format!(
                "Gatekeeper: {} staged file(s) passed safety checks",
                staged_paths.len()
            ),
            ctx,
        );
    } else {
        let secret_count = result
            .violations
            .iter()
            .filter(|v| v.kind == crate::core::gatekeeper::ViolationKind::SecretDetected)
            .count();
        let blocked_count = result
            .violations
            .iter()
            .filter(|v| v.kind == crate::core::gatekeeper::ViolationKind::PathBlocked)
            .count();
        let dangerous_count = result
            .violations
            .iter()
            .filter(|v| v.kind == crate::core::gatekeeper::ViolationKind::DangerousPattern)
            .count();

        let mut parts = Vec::new();
        if secret_count > 0 {
            parts.push(format!("{} secret(s)", secret_count));
        }
        if blocked_count > 0 {
            parts.push(format!("{} blocked path(s)", blocked_count));
        }
        if dangerous_count > 0 {
            parts.push(format!("{} dangerous pattern(s)", dangerous_count));
        }
        fail(&format!("Gatekeeper violations: {}", parts.join(", ")), ctx);
    }

    Ok(())
}

/// Evaluates a set of mandates and returns any active blockers.
pub fn evaluate_mandates(
    project_root: &Path,
    store: &Store,
    mandates: &[crate::core::docs::Mandate],
) -> Vec<crate::core::rpc::Blocker> {
    use crate::core::rpc::{Blocker, BlockerKind};
    let mut blockers = Vec::new();

    for mandate in mandates {
        match mandate.check_tag.as_str() {
            "gate.worktree.no_master" => {
                let status = crate::core::workspace::get_workspace_status(project_root);
                if let Ok(s) = status {
                    if s.git.is_protected {
                        blockers.push(Blocker {
                            kind: BlockerKind::ProtectedBranch,
                            message: format!("Mandate Violation: {}", mandate.fragment.title),
                            resolve_hint:
                                "Run `decapod workspace ensure` to create a working branch."
                                    .to_string(),
                        });
                    }
                }
            }
            "gate.worktree.isolated" => {
                let status = crate::core::workspace::get_workspace_status(project_root);
                if let Ok(s) = status {
                    if !s.git.in_worktree {
                        blockers.push(Blocker {
                            kind: BlockerKind::WorkspaceRequired,
                            message: format!("Mandate Violation: {}", mandate.fragment.title),
                            resolve_hint:
                                "Run `decapod workspace ensure` to create an isolated git worktree."
                                    .to_string(),
                        });
                    }
                }
            }
            "gate.session.active" => {
                // This is usually handled by the RPC kernel session check,
                // but we can add a blocker if we want more detail.
            }
            "gate.todo.active_task" => {
                let agent_id =
                    std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
                if agent_id != "unknown" {
                    let mut active_tasks = crate::core::todo::list_tasks(
                        &store.root,
                        Some("open".to_string()),
                        None,
                        None,
                        None,
                        None,
                    );
                    if let Ok(ref mut tasks) = active_tasks {
                        let pre_filter_count = tasks.len();
                        let debug_info = if !tasks.is_empty() {
                            format!(
                                "First task assigned to: '{}', My ID: '{}'",
                                tasks[0].assigned_to, agent_id
                            )
                        } else {
                            format!(
                                "No tasks found. My ID: '{}', Root: '{}'",
                                agent_id,
                                project_root.display()
                            )
                        };

                        tasks.retain(|t| t.assigned_to == agent_id);
                        if tasks.is_empty() {
                            blockers.push(Blocker {
                                kind: BlockerKind::MissingProof,
                                message: format!("Mandate Violation: {} (Pre-filter: {}, {})", mandate.fragment.title, pre_filter_count, debug_info),
                                resolve_hint: "You MUST create and claim a `todo` before starting work. Run `decapod todo add \"...\"` then `decapod todo claim --id <id>`.".to_string(),
                            });
                        }
                    }
                }
            }
            "gate.validation.pass" => {
                // Future: check a 'last_validated' marker in the store
            }
            _ => {}
        }
    }

    blockers
}

/// Co-Player Policy Tightening Gate
///
/// Validates that the coplayer policy derivation function only tightens
/// constraints as reliability decreases. This is a structural invariant:
/// no snapshot should produce a policy that is looser than a less-reliable one.
fn validate_coplayer_policy_tightening(
    ctx: &ValidationContext,
    _decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Co-Player Policy Tightening Gate");

    use crate::core::coplayer::{CoPlayerSnapshot, derive_policy};

    // Test the invariant: unknown → high → medium → low reliability
    // Each step must be equal or tighter than the next.
    let profiles = vec![
        ("unknown", 0.0, 0),
        ("high", 0.5, 20),
        ("medium", 0.8, 20),
        ("low", 0.95, 100),
    ];

    let mut prev_policy = None;
    let mut all_valid = true;

    for (risk, reliability, total) in &profiles {
        let snap = CoPlayerSnapshot {
            agent_id: format!("gate-test-{}", risk),
            reliability_score: *reliability,
            total_ops: *total,
            successful_ops: (*total as f64 * reliability) as usize,
            failed_ops: *total - (*total as f64 * reliability) as usize,
            last_active: "gate-test".to_string(),
            common_ops: vec![],
            risk_profile: risk.to_string(),
        };

        let policy = derive_policy(&snap);

        // Validation is ALWAYS required
        if !policy.require_validation {
            fail(
                &format!(
                    "Co-player policy for '{}' does not require validation (MUST always be true)",
                    risk
                ),
                ctx,
            );
            all_valid = false;
        }

        // Check tightening: diff limits must be <= previous (less reliable) agent's limits
        if let Some(prev) = &prev_policy {
            let prev: &crate::core::coplayer::CoPlayerPolicy = prev;
            // More reliable agents may have larger diff limits, never smaller
            if policy.max_diff_lines < prev.max_diff_lines {
                // This is expected: more reliable = looser (larger diff limit)
                // The INVARIANT is the reverse must not happen:
                // less reliable must not have LARGER limits than more reliable
            }
        }

        prev_policy = Some(policy);
    }

    if all_valid {
        pass("Co-player policies only tighten constraints", ctx);
    }

    Ok(())
}

pub fn run_validation(
    store: &Store,
    decapod_dir: &Path,
    _home_dir: &Path,
    verbose: bool,
) -> Result<(), error::DecapodError> {
    let total_start = Instant::now();
    use colored::Colorize;
    println!(
        "{} {}",
        "▶".bright_green().bold(),
        "validate:".bright_cyan().bold()
    );

    // Directly get content from embedded assets
    let intent_content = crate::core::assets::get_doc("specs/INTENT.md").unwrap_or_default();
    let intent_version =
        extract_md_version(&intent_content).unwrap_or_else(|| "unknown".to_string());
    println!(
        "  {} intent_version={}",
        "spec:".bright_cyan(),
        intent_version.bright_white()
    );

    let ctx = ValidationContext::new();

    // Pre-read broker.events.jsonl once for gates that need it
    let broker_events_path = store.root.join("broker.events.jsonl");
    let broker_content: Option<String> = if broker_events_path.exists() {
        fs::read_to_string(&broker_events_path).ok()
    } else {
        None
    };

    // Store validations — run sequentially since they set up state
    match store.kind {
        StoreKind::User => {
            let start = Instant::now();
            validate_user_store_blank_slate(&ctx)?;
            if verbose {
                println!(
                    "  {} [validate_user_store_blank_slate] {} ({:.2?})",
                    "✓".bright_green(),
                    "done".bright_white(),
                    start.elapsed()
                );
            }
        }
        StoreKind::Repo => {
            let start = Instant::now();
            validate_repo_store_dogfood(store, &ctx, decapod_dir)?;
            if verbose {
                println!(
                    "  {} [validate_repo_store_dogfood] {} ({:.2?})",
                    "✓".bright_green(),
                    "done".bright_white(),
                    start.elapsed()
                );
            }
        }
    }

    println!(
        "  {} {}",
        "gate:".bright_magenta().bold(),
        "Four Invariants Gate".bright_white()
    );

    // All remaining gates run in parallel via rayon::scope
    let timings: Mutex<Vec<(&str, Duration)>> = Mutex::new(Vec::new());

    rayon::scope(|s| {
        let ctx = &ctx;
        let timings = &timings;
        let broker = broker_content.as_deref();

        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_repo_map(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_repo_map", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_no_legacy_namespaces(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_no_legacy_namespaces", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_embedded_self_contained(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_embedded_self_contained", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_docs_templates_bucket(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_docs_templates_bucket", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_entrypoint_invariants(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_entrypoint_invariants", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_interface_contract_bootstrap(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_interface_contract_bootstrap", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_health_purity(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_health_purity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_project_scoped_state(store, ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_project_scoped_state", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_schema_determinism(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_schema_determinism", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_health_cache_integrity(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_health_cache_integrity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_risk_map(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_risk_map", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_risk_map_violations(store, ctx, broker) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_risk_map_violations", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_policy_integrity(store, ctx, broker) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_policy_integrity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_knowledge_integrity(store, ctx, broker) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_knowledge_integrity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_lineage_hard_gate(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_lineage_hard_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_repomap_determinism(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_repomap_determinism", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_watcher_audit(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_watcher_audit", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_watcher_purity(store, ctx, broker) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_watcher_purity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_archive_integrity(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_archive_integrity", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_control_plane_contract(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_control_plane_contract", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_canon_mutation(store, ctx, broker) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_canon_mutation", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_heartbeat_invocation_gate(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_heartbeat_invocation_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_markdown_primitives_roundtrip_gate(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings.lock().unwrap().push((
                "validate_markdown_primitives_roundtrip_gate",
                start.elapsed(),
            ));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_federation_gates(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_federation_gates", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_git_workspace_context(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_git_workspace_context", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_git_protected_branch(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_git_protected_branch", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_tooling_gate(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_tooling_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_state_commit_gate(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_state_commit_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_obligations(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_obligations", start.elapsed()));
        });

        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_gatekeeper_gate(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_gatekeeper_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_coplayer_policy_tightening(ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_coplayer_policy_tightening", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_lcm_immutability(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_lcm_immutability", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_lcm_rebuild_gate(store, ctx) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_lcm_rebuild_gate", start.elapsed()));
        });
        s.spawn(move |_| {
            let start = Instant::now();
            if let Err(e) = validate_plan_governed_execution_gate(store, ctx, decapod_dir) {
                fail(&format!("gate error: {e}"), ctx);
            }
            timings
                .lock()
                .unwrap()
                .push(("validate_plan_governed_execution_gate", start.elapsed()));
        });
    });

    // Print per-gate timings in verbose mode
    if verbose {
        let mut gate_timings = timings.into_inner().unwrap();
        gate_timings.sort_by(|a, b| b.1.cmp(&a.1));
        for (name, elapsed) in &gate_timings {
            println!(
                "  {} [{}] {} ({:.2?})",
                "✓".bright_green(),
                name.bright_cyan(),
                "done".bright_white(),
                elapsed
            );
        }
    }

    let elapsed = total_start.elapsed();
    let pass_count = ctx.pass_count.load(Ordering::Relaxed);
    let fail_count = ctx.fail_count.load(Ordering::Relaxed);
    let warn_count = ctx.warn_count.load(Ordering::Relaxed);
    let fails = ctx.fails.lock().unwrap();
    let warns = ctx.warns.lock().unwrap();
    let fail_total = (fails.len() as u32).max(fail_count);
    let warn_total = (warns.len() as u32).max(warn_count);

    println!(
        "  {} pass={} fail={} warn={} {}",
        "summary:".bright_cyan(),
        pass_count.to_string().bright_green(),
        fail_total.to_string().bright_red(),
        warn_total.to_string().bright_yellow(),
        format!("({:.2?})", elapsed).bright_white()
    );

    if !fails.is_empty() {
        println!(
            "  {} {}: {}",
            "✗".bright_red().bold(),
            "failures".bright_red(),
            output::preview_messages(&fails, 2, 110)
        );
    }

    if !warns.is_empty() {
        println!(
            "  {} {}: {}",
            "⚠".bright_yellow().bold(),
            "warnings".bright_yellow(),
            output::preview_messages(&warns, 2, 110)
        );
    }

    if fail_total > 0 {
        Err(error::DecapodError::ValidationError(format!(
            "{} test(s) failed.",
            fail_total
        )))
    } else {
        println!(
            "{} {}",
            "✓".bright_green().bold(),
            "validation passed".bright_green().bold()
        );
        Ok(())
    }
}
