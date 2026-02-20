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

use crate::core::error;
use crate::core::output;
use crate::core::store::{Store, StoreKind};
use crate::{db, primitives, todo};
use regex::Regex;
use serde_json;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use ulid::Ulid;

thread_local! {
    static VALIDATION_FAILS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static VALIDATION_WARNS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn collect_repo_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), error::DecapodError> {
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

    recurse(root, out)
}

fn validate_no_legacy_namespaces(
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Namespace Purge Gate");

    let mut files = Vec::new();
    collect_repo_files(decapod_dir, &mut files)?;

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
            pass_count,
        );
    } else {
        let mut msg = String::from("Forbidden legacy namespace references found:");
        for (p, n) in offenders.iter().take(12) {
            msg.push_str(&format!(" {}({})", p.display(), n));
        }
        if offenders.len() > 12 {
            msg.push_str(&format!(" ... ({} total)", offenders.len()));
        }
        fail(&msg, fail_count);
    }
    Ok(())
}

fn validate_embedded_self_contained(
    pass_count: &mut u32,
    fail_count: &mut u32,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Embedded Self-Contained Gate");

    let constitution_dir = repo_root.join("constitution");
    if !constitution_dir.exists() {
        // This is a decapod repo, not a project with embedded docs
        skip("No constitution/ directory found (decapod repo)", &mut 0);
        return Ok(());
    }

    let mut files = Vec::new();
    collect_repo_files(&constitution_dir, &mut files)?;

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
                    || line.contains("intended as");
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
            pass_count,
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
        fail(&msg, fail_count);
    }
    Ok(())
}

fn pass(message: &str, pass_count: &mut u32) {
    *pass_count += 1;
    let _ = message;
}

fn fail(message: &str, fail_count: &mut u32) {
    *fail_count += 1;
    VALIDATION_FAILS.with(|v| v.borrow_mut().push(message.to_string()));
}

fn skip(message: &str, skip_count: &mut u32) {
    *skip_count += 1;
    let _ = message;
}

fn warn(message: &str, warn_count: &mut u32) {
    *warn_count += 1;
    VALIDATION_WARNS.with(|v| v.borrow_mut().push(message.to_string()));
}

fn info(message: &str) {
    let _ = message;
}

fn trace_gate(name: &str) {
    if std::env::var("DECAPOD_VALIDATE_TRACE").ok().as_deref() == Some("1") {
        println!("validate: trace {}", name);
    }
}

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

fn validate_user_store_blank_slate(
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Store: user (blank-slate semantics)");
    let tmp_root = std::env::temp_dir().join(format!("decapod_validate_user_{}", Ulid::new()));
    fs::create_dir_all(&tmp_root).map_err(error::DecapodError::IoError)?;

    todo::initialize_todo_db(&tmp_root)?;
    let db_path = tmp_root.join("todo.db");
    let n = count_tasks_in_db(&db_path)?;

    if n == 0 {
        pass("User store starts empty (no automatic seeding)", pass_count);
    } else {
        fail(
            &format!(
                "User store is not empty on fresh init ({} task(s) found)",
                n
            ),
            fail_count,
        );
    }
    Ok(())
}

fn validate_repo_store_dogfood(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
    _decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Store: repo (dogfood backlog semantics)");

    let events = store.root.join("todo.events.jsonl");
    if !events.is_file() {
        fail("Repo store missing todo.events.jsonl", fail_count);
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
        pass_count,
    );

    let db_path = store.root.join("todo.db");
    if !db_path.is_file() {
        fail("Repo store missing todo.db", fail_count);
        return Ok(());
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
            pass_count,
        );
    } else {
        fail(
            "Repo todo.db does NOT match rebuild from todo.events.jsonl",
            fail_count,
        );
    }

    Ok(())
}

fn validate_repo_map(
    pass_count: &mut u32,
    fail_count: &mut u32,
    _decapod_dir: &Path, // decapod_dir is no longer used for filesystem constitution checks
) -> Result<(), error::DecapodError> {
    info("Repo Map");

    // We no longer check for a filesystem directory for constitution.
    // Instead, we verify embedded docs.
    pass(
        "Methodology constitution checks will verify embedded docs.",
        pass_count,
    );

    let required_specs = ["specs/INTENT.md", "specs/SYSTEM.md"];
    let required_methodology = ["methodology/ARCHITECTURE.md"];
    for r in required_specs {
        if crate::core::assets::get_doc(r).is_some() {
            pass(
                &format!("Constitution doc {} present (embedded)", r),
                pass_count,
            );
        } else {
            fail(
                &format!("Constitution doc {} missing (embedded)", r),
                fail_count,
            );
        }
    }
    for r in required_methodology {
        if crate::core::assets::get_doc(r).is_some() {
            pass(
                &format!("Constitution doc {} present (embedded)", r),
                pass_count,
            );
        } else {
            fail(
                &format!("Constitution doc {} missing (embedded)", r),
                fail_count,
            );
        }
    }
    Ok(())
}

fn validate_docs_templates_bucket(
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Entrypoint Gate");

    // Entrypoints MUST be in the project root
    let required = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];
    for a in required {
        let p = decapod_dir.join(a);
        if p.is_file() {
            pass(&format!("Root entrypoint {} present", a), pass_count);
        } else {
            fail(
                &format!("Root entrypoint {} missing from project root", a),
                fail_count,
            );
        }
    }

    if decapod_dir.join(".decapod").join("README.md").is_file() {
        pass(".decapod/README.md present", pass_count);
    } else {
        fail(".decapod/README.md missing", fail_count);
    }

    // NEGATIVE GATE: Decapod docs MUST NOT be copied into the project
    let forbidden_docs = decapod_dir.join(".decapod").join("docs");
    if forbidden_docs.exists() {
        fail(
            "Decapod internal docs were copied into .decapod/docs/ (Forbidden)",
            fail_count,
        );
    } else {
        pass(
            "Decapod internal docs correctly excluded from project repo",
            pass_count,
        );
    }

    // NEGATIVE GATE: projects/<id> MUST NOT exist
    let forbidden_projects = decapod_dir.join(".decapod").join("projects");
    if forbidden_projects.exists() {
        fail(
            "Legacy .decapod/projects/ directory found (Forbidden)",
            fail_count,
        );
    } else {
        pass(".decapod/projects/ correctly absent", pass_count);
    }

    Ok(())
}

fn validate_entrypoint_invariants(
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Four Invariants Gate");

    // Check AGENTS.md for the four invariants
    let agents_path = decapod_dir.join("AGENTS.md");
    if !agents_path.is_file() {
        fail("AGENTS.md missing, cannot check invariants", fail_count);
        return Ok(());
    }

    let content = fs::read_to_string(&agents_path).map_err(error::DecapodError::IoError)?;

    // Exact invariant strings (tamper detection)
    let exact_invariants = [
        ("core/DECAPOD.md", "Router pointer to core/DECAPOD.md"),
        ("cargo install decapod", "Version update gate language"),
        ("decapod validate", "Validation gate language"),
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
            pass(&format!("Invariant present: {}", description), pass_count);
        } else {
            fail(&format!("Invariant missing: {}", description), fail_count);
            all_present = false;
        }
    }

    // Check for legacy router names (must not exist)
    let legacy_routers = ["MAESTRO.md", "GLOBEX.md", "CODEX.md\" as router"];
    for legacy in legacy_routers {
        if content.contains(legacy) {
            fail(
                &format!("AGENTS.md contains legacy router reference: {}", legacy),
                fail_count,
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
            pass_count,
        );
    } else {
        fail(
            &format!(
                "AGENTS.md exceeds line limit ({} lines > {})",
                line_count, MAX_AGENTS_LINES
            ),
            fail_count,
        );
        all_present = false;
    }

    // Check that agent-specific files defer to AGENTS.md and are thin
    const MAX_AGENT_SPECIFIC_LINES: usize = 50;
    for agent_file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let agent_path = decapod_dir.join(agent_file);
        if !agent_path.is_file() {
            fail(
                &format!("{} missing from project root", agent_file),
                fail_count,
            );
            all_present = false;
            continue;
        }

        let agent_content =
            fs::read_to_string(&agent_path).map_err(error::DecapodError::IoError)?;

        // Must defer to AGENTS.md
        if agent_content.contains("See `AGENTS.md`") || agent_content.contains("AGENTS.md") {
            pass(&format!("{} defers to AGENTS.md", agent_file), pass_count);
        } else {
            fail(
                &format!("{} does not reference AGENTS.md", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must reference canonical router
        if agent_content.contains("core/DECAPOD.md") {
            pass(
                &format!("{} references canonical router", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing canonical router reference", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must include explicit jail rule for .decapod access
        if agent_content.contains(".decapod files are accessed only via decapod CLI") {
            pass(
                &format!("{} includes .decapod CLI-only jail rule", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing .decapod CLI-only jail rule marker", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must include Docker git workspace mandate
        if agent_content.contains("Docker git workspaces") {
            pass(
                &format!("{} includes Docker workspace mandate", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing Docker workspace mandate marker", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must include elevated-permissions mandate for container workspace commands
        if agent_content
            .contains("request elevated permissions before Docker/container workspace commands")
        {
            pass(
                &format!("{} includes elevated-permissions mandate", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing elevated-permissions mandate marker", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must include per-agent session password mandate
        if agent_content.contains("DECAPOD_SESSION_PASSWORD") {
            pass(
                &format!("{} includes per-agent session password mandate", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "{} missing per-agent session password mandate marker",
                    agent_file
                ),
                fail_count,
            );
            all_present = false;
        }

        // Must include claim-before-work mandate
        if agent_content.contains("decapod todo claim --id <task-id>") {
            pass(
                &format!("{} includes claim-before-work mandate", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing claim-before-work mandate marker", agent_file),
                fail_count,
            );
            all_present = false;
        }

        // Must include explicit update command in startup sequence
        if agent_content.contains("cargo install decapod") {
            pass(
                &format!("{} includes version update step", agent_file),
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "{} missing version update step (`cargo install decapod`)",
                    agent_file
                ),
                fail_count,
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
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "{} exceeds line limit ({} lines > {})",
                    agent_file, agent_lines, MAX_AGENT_SPECIFIC_LINES
                ),
                fail_count,
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
                    fail_count,
                );
                all_present = false;
            }
        }
    }

    if all_present {
        pass(
            "All entrypoint files follow thin waist architecture",
            pass_count,
        );
    }

    Ok(())
}

fn validate_interface_contract_bootstrap(
    pass_count: &mut u32,
    fail_count: &mut u32,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Interface Contract Bootstrap Gate");

    // This gate applies to the decapod repository where constitution/* is present.
    // Project repos initialized by `decapod init` should not fail on missing embedded docs.
    let constitution_dir = repo_root.join("constitution");
    if !constitution_dir.exists() {
        skip(
            "No constitution/ directory found (project repo); skipping interface bootstrap checks",
            pass_count,
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
            pass(
                &format!("{} present at {}", label, path.display()),
                pass_count,
            );
        } else {
            fail(
                &format!("{} missing at {}", label, path.display()),
                fail_count,
            );
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
                    pass_count,
                );
            } else {
                fail(
                    &format!("RISK_POLICY_GATE missing marker: {}", marker),
                    fail_count,
                );
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
                    pass_count,
                );
            } else {
                fail(
                    &format!("AGENT_CONTEXT_PACK missing marker: {}", marker),
                    fail_count,
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
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Health Purity Gate");
    let mut files = Vec::new();
    collect_repo_files(decapod_dir, &mut files)?;

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
            pass_count,
        );
    } else {
        fail(
            &format!(
                "Manual health values found in non-generated files: {:?}",
                offenders
            ),
            fail_count,
        );
    }
    Ok(())
}

fn validate_project_scoped_state(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Project-Scoped State Gate");
    if store.kind != StoreKind::Repo {
        skip("Not in repo mode; skipping state scoping check", pass_count);
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
        pass("All state is correctly scoped within .decapod/", pass_count);
    } else {
        fail(
            &format!(
                "Found Decapod state files outside .decapod/: {:?}",
                offenders
            ),
            fail_count,
        );
    }
    Ok(())
}

fn validate_schema_determinism(
    pass_count: &mut u32,
    fail_count: &mut u32,
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
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        Ok(text)
    };

    let s1 = run_schema()?;
    let s2 = run_schema()?;

    if s1 == s2 && !s1.is_empty() {
        pass("Schema output is deterministic", pass_count);
    } else {
        fail("Schema output is non-deterministic or empty", fail_count);
    }
    Ok(())
}

fn validate_health_cache_integrity(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Health Cache Non-Authoritative Gate");
    let db_path = store.root.join("health.db");
    if !db_path.exists() {
        skip(
            "health.db not found; skipping health integrity check",
            pass_count,
        );
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
        pass(
            "No orphaned health cache entries (integrity pass)",
            pass_count,
        );
    } else {
        warn(
            &format!(
                "Found {} health cache entries without proof events (might be manual writes)",
                orphaned
            ),
            fail_count,
        );
    }
    Ok(())
}

fn validate_risk_map(
    store: &Store,
    pass_count: &mut u32,
    _fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Risk Map Gate");
    let map_path = store.root.join("RISKMAP.json");
    if map_path.exists() {
        pass("Risk map (blast-radius) is present", pass_count);
    } else {
        warn("Risk map missing (run `decapod riskmap init`)", pass_count);
    }
    Ok(())
}

fn validate_risk_map_violations(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Zone Violation Gate");
    let audit_log = store.root.join("broker.events.jsonl");
    if audit_log.exists() {
        let content = fs::read_to_string(audit_log)?;
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\".decapod/\"") && line.contains("\"op\":\"todo.add\"") {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass("No risk zone violations detected in audit log", pass_count);
        } else {
            fail(
                &format!("Detected operations in protected zones: {:?}", offenders),
                fail_count,
            );
        }
    }
    Ok(())
}

fn validate_policy_integrity(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Policy Integrity Gates");
    let db_path = store.root.join("policy.db");
    if !db_path.exists() {
        skip("policy.db not found; skipping policy check", pass_count);
        return Ok(());
    }

    let _conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;

    let audit_log = store.root.join("broker.events.jsonl");
    if audit_log.exists() {
        let content = fs::read_to_string(audit_log)?;
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
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "Policy approval directly mutated health state: {:?}",
                    offenders
                ),
                fail_count,
            );
        }
    }

    Ok(())
}

fn validate_knowledge_integrity(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Knowledge Integrity Gate");
    let db_path = store.root.join("knowledge.db");
    if !db_path.exists() {
        skip(
            "knowledge.db not found; skipping knowledge integrity check",
            pass_count,
        );
        return Ok(());
    }

    let conn = db::db_connect_for_validate(&db_path.to_string_lossy())?;

    let missing_provenance: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM knowledge WHERE provenance IS NULL OR provenance = ''",
            [],
            |row| row.get(0),
        )
        .map_err(error::DecapodError::RusqliteError)?;

    if missing_provenance == 0 {
        pass(
            "Knowledge provenance verified (all entries have pointers)",
            pass_count,
        );
    } else {
        fail(
            &format!(
                "Found {} knowledge entries missing mandatory provenance",
                missing_provenance
            ),
            fail_count,
        );
    }

    let audit_log = store.root.join("broker.events.jsonl");
    if audit_log.exists() {
        let content = fs::read_to_string(audit_log)?;
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"op\":\"knowledge.add\"") && line.contains("\"db_id\":\"health.db\"")
            {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass(
                "No direct health promotion from knowledge detected",
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "Knowledge system directly mutated health state: {:?}",
                    offenders
                ),
                fail_count,
            );
        }
    }

    Ok(())
}

fn validate_lineage_hard_gate(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Lineage Hard Gate");
    let todo_events = store.root.join("todo.events.jsonl");
    let federation_db = store.root.join("federation.db");
    if !todo_events.exists() || !federation_db.exists() {
        skip(
            "lineage inputs missing (todo.events.jsonl or federation.db); skipping",
            pass_count,
        );
        return Ok(());
    }

    let content = fs::read_to_string(&todo_events)?;
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

    let conn = db::db_connect_for_validate(&federation_db.to_string_lossy())?;
    let todo_db = store.root.join("todo.db");
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
            pass_count,
        );
    } else {
        fail(
            &format!("Lineage gate violations: {:?}", violations),
            fail_count,
        );
    }
    Ok(())
}

fn validate_repomap_determinism(
    pass_count: &mut u32,
    fail_count: &mut u32,
    decapod_dir: &Path,
) -> Result<(), error::DecapodError> {
    info("Repo Map Determinism Gate");
    use crate::core::repomap;
    let m1 = serde_json::to_string(&repomap::generate_map(decapod_dir)).unwrap();
    let m2 = serde_json::to_string(&repomap::generate_map(decapod_dir)).unwrap();

    if m1 == m2 && !m1.is_empty() {
        pass("Repo map output is deterministic", pass_count);
    } else {
        fail("Repo map output is non-deterministic or empty", fail_count);
    }
    Ok(())
}

fn validate_watcher_audit(
    store: &Store,
    pass_count: &mut u32,
    _fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Watcher Audit Gate");
    let audit_log = store.root.join("watcher.events.jsonl");
    if audit_log.exists() {
        pass("Watcher audit trail present", pass_count);
    } else {
        warn(
            "Watcher audit trail missing (run `decapod govern watcher run`)",
            pass_count,
        );
    }
    Ok(())
}

fn validate_watcher_purity(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Watcher Purity Gate");
    let audit_log = store.root.join("broker.events.jsonl");
    if audit_log.exists() {
        let content = fs::read_to_string(audit_log)?;
        let mut offenders = Vec::new();
        for line in content.lines() {
            if line.contains("\"actor\":\"watcher\"") {
                offenders.push(line.to_string());
            }
        }
        if offenders.is_empty() {
            pass(
                "Watcher purity verified (read-only checks only)",
                pass_count,
            );
        } else {
            fail(
                &format!(
                    "Watcher subsystem attempted brokered mutations: {:?}",
                    offenders
                ),
                fail_count,
            );
        }
    }
    Ok(())
}

fn validate_archive_integrity(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Archive Integrity Gate");
    let db_path = store.root.join("archive.db");
    if !db_path.exists() {
        skip("archive.db not found; skipping archive check", pass_count);
        return Ok(());
    }

    use crate::archive;
    let failures = archive::verify_archives(store)?;
    if failures.is_empty() {
        pass(
            "All session archives verified (content and hash match)",
            pass_count,
        );
    } else {
        fail(
            &format!("Archive integrity failures detected: {:?}", failures),
            fail_count,
        );
    }
    Ok(())
}

fn validate_control_plane_contract(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
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
        pass("No broker events yet (first run)", pass_count);
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
            pass_count,
        );
    } else {
        fail(
            &format!(
                "Control plane contract violations detected: {:?}",
                violations
            ),
            fail_count,
        );
    }

    Ok(())
}

fn validate_canon_mutation(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Canon Mutation Gate");
    let audit_log = store.root.join("broker.events.jsonl");
    if audit_log.exists() {
        let content = fs::read_to_string(audit_log)?;
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
            pass("No unauthorized canon mutations detected", pass_count);
        } else {
            warn(
                &format!(
                    "Detected direct mutations to canonical documents: {:?}",
                    offenders
                ),
                fail_count,
            );
        }
    }
    Ok(())
}

fn validate_heartbeat_invocation_gate(
    pass_count: &mut u32,
    fail_count: &mut u32,
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
                pass(msg, pass_count);
            } else {
                fail(msg, fail_count);
            }
        }
    } else {
        skip(
            "Heartbeat wiring source files absent; skipping code-level heartbeat checks",
            pass_count,
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
            pass(msg, pass_count);
        } else {
            fail(msg, fail_count);
        }
    }

    Ok(())
}

fn validate_federation_gates(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Federation Gates");

    let results = crate::plugins::federation::validate_federation(&store.root)?;

    for (gate_name, passed, message) in results {
        if passed {
            pass(&format!("[{}] {}", gate_name, message), pass_count);
        } else {
            fail(&format!("[{}] {}", gate_name, message), fail_count);
        }
    }

    Ok(())
}

fn validate_markdown_primitives_roundtrip_gate(
    store: &Store,
    pass_count: &mut u32,
    fail_count: &mut u32,
) -> Result<(), error::DecapodError> {
    info("Markdown Primitive Round-Trip Gate");
    match primitives::validate_roundtrip_gate(store) {
        Ok(()) => {
            pass(
                "Markdown primitives export and round-trip validation pass",
                pass_count,
            );
        }
        Err(err) => {
            fail(
                &format!("Markdown primitive round-trip failed: {}", err),
                fail_count,
            );
        }
    }
    Ok(())
}

/// Validates that tooling requirements are satisfied.
/// This gate ensures formatting, linting, and type checking pass before promotion.
fn validate_git_workspace_context(
    pass_count: &mut u32,
    fail_count: &mut u32,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Git Workspace Context Gate");

    // Allow bypass for testing/CI environments
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        skip(
            "Git workspace gates skipped (DECAPOD_VALIDATE_SKIP_GIT_GATES set)",
            pass_count,
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
            pass_count,
        );
    } else {
        fail(
            "Not running in container workspace - git-tracked work must execute in Docker-isolated workspace (claim.git.container_workspace_required)",
            fail_count,
        );
    }

    let git_dir = repo_root.join(".git");
    let is_worktree = git_dir.is_file() && {
        let content = fs::read_to_string(&git_dir).unwrap_or_default();
        content.contains("gitdir:")
    };

    if is_worktree {
        pass("Running in git worktree (isolated branch)", pass_count);
    } else if in_container {
        pass(
            "Container workspace detected (worktree check informational)",
            pass_count,
        );
    } else {
        fail(
            "Not running in isolated git worktree - must use container workspace for implementation work",
            fail_count,
        );
    }

    Ok(())
}

fn validate_git_protected_branch(
    pass_count: &mut u32,
    fail_count: &mut u32,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Git Protected Branch Gate");

    // Allow bypass for testing/CI environments
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        skip(
            "Git protected branch gate skipped (DECAPOD_VALIDATE_SKIP_GIT_GATES set)",
            pass_count,
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
            fail_count,
        );
    } else {
        pass(
            &format!("On working branch '{}' (not protected)", current_branch),
            pass_count,
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
                            fail_count,
                        );
                    } else {
                        pass("No unpushed commits to protected branches", pass_count);
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_tooling_gate(
    pass_count: &mut u32,
    fail_count: &mut u32,
    repo_root: &Path,
) -> Result<(), error::DecapodError> {
    info("Tooling Validation Gate");

    // Check for Cargo.toml to detect Rust projects
    let cargo_toml = repo_root.join("Cargo.toml");
    if !cargo_toml.exists() {
        // Not a Rust project, skip tooling validation for now
        // Future: Add support for other language toolchains
        skip(
            "No Cargo.toml found; skipping Rust toolchain validation",
            pass_count,
        );
        return Ok(());
    }

    let mut has_failures = false;

    // Check formatting with cargo fmt
    match std::process::Command::new("cargo")
        .args(["fmt", "--all", "--", "--check"])
        .current_dir(repo_root)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                pass("Rust code formatting passes (cargo fmt)", pass_count);
            } else {
                fail(
                    "Rust code formatting failed - run `cargo fmt --all`",
                    fail_count,
                );
                has_failures = true;
            }
        }
        Err(e) => {
            fail(&format!("Failed to run cargo fmt: {}", e), fail_count);
            has_failures = true;
        }
    }

    // Check linting with cargo clippy
    match std::process::Command::new("cargo")
        .args([
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ])
        .current_dir(repo_root)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                pass("Rust linting passes (cargo clippy)", pass_count);
            } else {
                fail(
                    "Rust linting failed - run `cargo clippy --all-targets --all-features`",
                    fail_count,
                );
                has_failures = true;
            }
        }
        Err(e) => {
            fail(&format!("Failed to run cargo clippy: {}", e), fail_count);
            has_failures = true;
        }
    }

    // Check type checking with cargo check
    match std::process::Command::new("cargo")
        .args(["check", "--all-targets", "--all-features"])
        .current_dir(repo_root)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                pass("Rust type checking passes (cargo check)", pass_count);
            } else {
                fail(
                    "Rust type checking failed - run `cargo check --all-targets --all-features`",
                    fail_count,
                );
                has_failures = true;
            }
        }
        Err(e) => {
            fail(&format!("Failed to run cargo check: {}", e), fail_count);
            has_failures = true;
        }
    }

    if !has_failures {
        pass(
            "All toolchain validations pass - project is ready for promotion",
            pass_count,
        );
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

pub fn run_validation(
    store: &Store,
    decapod_dir: &Path,
    _home_dir: &Path,
) -> Result<(), error::DecapodError> {
    VALIDATION_FAILS.with(|v| v.borrow_mut().clear());
    VALIDATION_WARNS.with(|v| v.borrow_mut().clear());
    println!("validate: running");

    // Directly get content from embedded assets
    let intent_content = crate::core::assets::get_doc("specs/INTENT.md").unwrap_or_default();
    let intent_version =
        extract_md_version(&intent_content).unwrap_or_else(|| "unknown".to_string());
    println!("validate: intent_version={}", intent_version);

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut warn_count = 0;

    // Store validations
    match store.kind {
        StoreKind::User => {
            trace_gate("validate_user_store_blank_slate");
            validate_user_store_blank_slate(&mut pass_count, &mut fail_count)?;
        }
        StoreKind::Repo => {
            trace_gate("validate_repo_store_dogfood");
            validate_repo_store_dogfood(store, &mut pass_count, &mut fail_count, decapod_dir)?;
        }
    }

    trace_gate("validate_repo_map");
    validate_repo_map(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_no_legacy_namespaces");
    validate_no_legacy_namespaces(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_embedded_self_contained");
    validate_embedded_self_contained(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_docs_templates_bucket");
    validate_docs_templates_bucket(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_entrypoint_invariants");
    validate_entrypoint_invariants(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_interface_contract_bootstrap");
    validate_interface_contract_bootstrap(&mut pass_count, &mut fail_count, decapod_dir)?;
    println!("validate: gate Four Invariants Gate");
    trace_gate("validate_health_purity");
    validate_health_purity(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_project_scoped_state");
    validate_project_scoped_state(store, &mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_schema_determinism");
    validate_schema_determinism(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_health_cache_integrity");
    validate_health_cache_integrity(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_risk_map");
    validate_risk_map(store, &mut pass_count, &mut warn_count)?;
    trace_gate("validate_risk_map_violations");
    validate_risk_map_violations(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_policy_integrity");
    validate_policy_integrity(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_knowledge_integrity");
    validate_knowledge_integrity(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_lineage_hard_gate");
    validate_lineage_hard_gate(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_repomap_determinism");
    validate_repomap_determinism(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_watcher_audit");
    validate_watcher_audit(store, &mut pass_count, &mut warn_count)?;
    trace_gate("validate_watcher_purity");
    validate_watcher_purity(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_archive_integrity");
    validate_archive_integrity(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_control_plane_contract");
    validate_control_plane_contract(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_canon_mutation");
    validate_canon_mutation(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_heartbeat_invocation_gate");
    validate_heartbeat_invocation_gate(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_markdown_primitives_roundtrip_gate");
    validate_markdown_primitives_roundtrip_gate(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_federation_gates");
    validate_federation_gates(store, &mut pass_count, &mut fail_count)?;
    trace_gate("validate_git_workspace_context");
    validate_git_workspace_context(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_git_protected_branch");
    validate_git_protected_branch(&mut pass_count, &mut fail_count, decapod_dir)?;
    trace_gate("validate_tooling_gate");
    validate_tooling_gate(&mut pass_count, &mut fail_count, decapod_dir)?;

    let fail_total = VALIDATION_FAILS
        .with(|v| v.borrow().len() as u32)
        .max(fail_count);
    let warn_total = VALIDATION_WARNS
        .with(|v| v.borrow().len() as u32)
        .max(warn_count);
    println!(
        "validate: summary pass={} fail={} warn={}",
        pass_count, fail_total, warn_total
    );

    VALIDATION_FAILS.with(|v| {
        let fails = v.borrow();
        if !fails.is_empty() {
            println!(
                "validate: failures {}: {}",
                fails.len(),
                output::preview_messages(&fails, 2, 110)
            );
        }
    });

    VALIDATION_WARNS.with(|v| {
        let warns = v.borrow();
        if !warns.is_empty() {
            println!(
                "validate: warnings {}: {}",
                warns.len(),
                output::preview_messages(&warns, 2, 110)
            );
        }
    });

    if fail_total > 0 {
        Err(error::DecapodError::ValidationError(format!(
            "{} test(s) failed.",
            fail_total
        )))
    } else {
        Ok(())
    }
}
