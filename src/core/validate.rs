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

use crate::core::error;
use crate::core::store::{Store, StoreKind};
use crate::core::tui;
use crate::{db, todo};
use regex::Regex;
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};
use ulid::Ulid;

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

    let constitution_embedded = repo_root.join("constitution").join("embedded");
    if !constitution_embedded.exists() {
        // This is a decapod repo, not a project with embedded docs
        skip(
            "No constitution/embedded/ directory found (decapod repo)",
            &mut 0,
        );
        return Ok(());
    }

    let mut files = Vec::new();
    collect_repo_files(&constitution_embedded, &mut files)?;

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
            // Allow legitimate override documentation patterns
            let lines_with_legitimate_refs = content
                .lines()
                .filter(|line| {
                    line.contains(".decapod/")
                        && (
                            line.contains("<repo>") ||  // Path documentation like `<repo>/.decapod/project`
                    line.contains("store:") ||  // Store documentation
                    line.contains("directory") || // Directory explanations
                    line.contains("override") || // Override instructions (lowercase)
                    line.contains("Override") || // Override instructions (capitalized)
                    line.contains("OVERRIDE.md") || // OVERRIDE.md file references
                    line.contains("Location:") || // Location descriptions
                    line.contains("primarily contain") || // Directory descriptions
                    line.contains("intended as")
                            // Template descriptions
                        )
                })
                .count();

            // If there are .decapod/ references but none are legitimate documentation, flag it
            let total_decapod_refs = content.matches(".decapod/").count();
            if total_decapod_refs > lines_with_legitimate_refs {
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
    use colored::Colorize;
    *pass_count += 1;
    println!("    {} {}", "●".bright_green(), message.bright_white());
}

fn fail(message: &str, fail_count: &mut u32) {
    use colored::Colorize;
    *fail_count += 1;
    eprintln!("    {} {}", "●".bright_red(), message.bright_white());
}

fn skip(message: &str, skip_count: &mut u32) {
    use colored::Colorize;
    *skip_count += 1;
    println!("    {} {}", "○".bright_yellow(), message.bright_white());
}

fn warn(message: &str, warn_count: &mut u32) {
    use colored::Colorize;
    *warn_count += 1;
    println!("    {} {}", "●".bright_yellow(), message.bright_white());
}

fn info(message: &str) {
    use colored::Colorize;
    println!("    {} {}", "ℹ".bright_cyan(), message.bright_black());
}

fn count_tasks_in_db(db_path: &Path) -> Result<i64, error::DecapodError> {
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(count)
}

fn fetch_tasks_fingerprint(db_path: &Path) -> Result<String, error::DecapodError> {
    let conn = db::db_connect(&db_path.to_string_lossy())?;
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

    let required_specs = [
        "specs/INTENT.md",
        "specs/ARCHITECTURE.md",
        "specs/SYSTEM.md",
    ];
    for r in required_specs {
        if crate::core::assets::get_doc(&format!("embedded/{}", r)).is_some() {
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
    let required = [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        "CODEX.md",
        "OPENCODE.md",
    ];
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
        ("decapod validate", "Validation gate language"),
        ("Stop if", "Stop-if-missing behavior"),
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
    for agent_file in ["CLAUDE.md", "GEMINI.md", "CODEX.md", "OPENCODE.md"] {
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

    let conn = db::db_connect(&db_path.to_string_lossy())?;

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

    let _conn = db::db_connect(&db_path.to_string_lossy())?;

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

    let conn = db::db_connect(&db_path.to_string_lossy())?;

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
            "Watcher audit trail missing (run `decapod watcher run`)",
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

pub fn run_validation(
    store: &Store,
    decapod_dir: &Path,
    _home_dir: &Path,
) -> Result<(), error::DecapodError> {
    use colored::Colorize;

    tui::render_box(
        "⚡ PROOF HARNESS - VALIDATION PROTOCOL",
        "Intent-Driven Methodology Enforcement",
        tui::BoxStyle::Magenta,
    );
    println!();

    // Directly get content from embedded assets
    let intent_content =
        crate::core::assets::get_doc("embedded/specs/INTENT.md").unwrap_or_default();
    let intent_version =
        extract_md_version(&intent_content).unwrap_or_else(|| "unknown".to_string());
    println!(
        "  {} Intent Version: {}",
        "ℹ".bright_cyan(),
        intent_version.bright_white()
    );
    println!();

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut warn_count = 0;

    // Store validations
    match store.kind {
        StoreKind::User => {
            validate_user_store_blank_slate(&mut pass_count, &mut fail_count)?;
        }
        StoreKind::Repo => {
            validate_repo_store_dogfood(store, &mut pass_count, &mut fail_count, decapod_dir)?;
        }
    }

    validate_repo_map(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_no_legacy_namespaces(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_embedded_self_contained(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_docs_templates_bucket(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_entrypoint_invariants(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_health_purity(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_project_scoped_state(store, &mut pass_count, &mut fail_count, decapod_dir)?;
    validate_schema_determinism(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_health_cache_integrity(store, &mut pass_count, &mut fail_count)?;
    validate_risk_map(store, &mut pass_count, &mut warn_count)?;
    validate_risk_map_violations(store, &mut pass_count, &mut fail_count)?;
    validate_policy_integrity(store, &mut pass_count, &mut fail_count)?;
    validate_knowledge_integrity(store, &mut pass_count, &mut fail_count)?;
    validate_repomap_determinism(&mut pass_count, &mut fail_count, decapod_dir)?;
    validate_watcher_audit(store, &mut pass_count, &mut warn_count)?;
    validate_watcher_purity(store, &mut pass_count, &mut fail_count)?;
    validate_archive_integrity(store, &mut pass_count, &mut fail_count)?;
    validate_canon_mutation(store, &mut pass_count, &mut fail_count)?;

    tui::print_summary(pass_count as usize, fail_count as usize);

    if fail_count > 0 {
        Err(error::DecapodError::ValidationError(format!(
            "{} test(s) failed.",
            fail_count
        )))
    } else {
        Ok(())
    }
}
