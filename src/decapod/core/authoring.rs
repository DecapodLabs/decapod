//! Host-local checkout admission, separate from repository-scoped task custody.
//! Cloud task profiles deliberately do not infer filesystem authority from paths.
use super::*;

fn denied(code: &str) -> error::DecapodError {
    error::DecapodError::ValidationError(format!(
        "{code}: inspect current task custody and checkout; do not impersonate another actor or retry an ambiguous write"
    ))
}

/// Observe the filesystem ourselves; callers cannot supply a verified identity.
/// A successful claim alone never admits authoring. Recheck on each governed use.
pub fn admit_authoring(
    root: &Path,
    checkout: &Path,
    actor: &str,
) -> Result<(), error::DecapodError> {
    check_authoring(root, checkout, actor, true)
}

pub fn check_authoring(
    root: &Path,
    checkout: &Path,
    actor: &str,
    bind: bool,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    if broker.is_cloud() {
        return Err(error::DecapodError::NotImplemented("AUTHORING_PROFILE_UNSUPPORTED: cloud tasks/events do not support verified host checkout bindings; no local shadow or fallback".into()));
    }
    if actor.trim().is_empty() || actor == "unknown" {
        return Err(denied("AUTHORING_ACTOR_REQUIRED"));
    }
    let observed = crate::core::workspace::observe_checkout(checkout);
    let main = crate::core::workspace::get_main_repo_root(checkout)?;
    let expected_store = main.join(".decapod/data");
    // A repository merely placed under the managed directory is not a linked
    // checkout of this repository. Never authorize it from its branch label.
    let expected_git = fs::canonicalize(main.join(".git")).ok();
    let store_matches = fs::canonicalize(root)
        .ok()
        .zip(fs::canonicalize(&expected_store).ok())
        .is_some_and(|(requested, expected)| requested == expected);
    if !observed.registered
        || expected_git
            .as_ref()
            .is_none_or(|git| git.to_string_lossy() != observed.repository_id)
        || observed.branch.is_empty()
        || !crate::core::workspace::is_canonical_decapod_workspace_path(checkout)
        || !store_matches
    {
        return Err(denied("AUTHORING_CHECKOUT_UNVERIFIED"));
    }
    let now = now_iso();
    let check = |conn: &Connection| -> Result<(), error::DecapodError> {
        // Reuse workspace branch resolution: full IDs take precedence over
        // compact ULID prefixes. Ambiguous hashes never acquire custody.
        let mut stmt = conn.prepare("SELECT id,hash FROM tasks ORDER BY id LIMIT 10001")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut ids = Vec::new();
        for (count, row) in rows.enumerate() {
            if count >= 10_000 {
                return Err(denied("AUTHORING_CUSTODY_LIMIT"));
            }
            let (id, hash) = row?;
            if crate::core::workspace::branch_names_task(&observed.branch, &id, &hash) {
                ids.push(id);
            }
        }
        if ids.len() != 1 {
            return Err(denied("AUTHORING_TASK_AMBIGUOUS"));
        }
        let task_id = &ids[0];

        let (owner, generation, expiry, lifecycle, status): (String, i64, Option<String>, String, String) = conn.query_row(
            "SELECT assigned_to,COALESCE(lease_generation,0),lease_expires_at,COALESCE(lease_lifecycle,''),status FROM tasks WHERE id=?1",
            [task_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        if owner != actor
            || generation <= 0
            || !matches!(status.as_str(), "open" | "in_progress")
            || !LeaseLifecycle::parse(Some(&lifecycle)).holds_exclusive_custody()
            || fleet_coord::lease_state(expiry.as_deref(), &now) != fleet_coord::LeaseState::Active
        {
            return Err(denied("AUTHORING_CUSTODY_STALE"));
        }
        task_journal::check_execution_authority(conn, task_id)?;
        if !load_dependency_readiness(conn, task_id)?.is_ready() {
            return Err(denied("AUTHORING_DEPENDENCIES_BLOCKED"));
        }
        if bind {
            // Retire only durably inactive custody. Live bindings survive branch switches,
            // including switches to another task held by the very same actor.
            conn.execute("DELETE FROM authoring_bindings WHERE worktree_id=?1 AND task_id IN (SELECT id FROM tasks WHERE status IN ('done','archived') OR assigned_to='' OR lease_lifecycle IN ('yielded','released','expired') OR CAST(rtrim(lease_expires_at,'Z') AS INTEGER)<=?2)", params![observed.worktree_id, parse_epoch_z(&now).unwrap_or(0) as i64])?;
        }
        let bound: Option<(String,String,i64,String,String)> = conn.query_row(
            "SELECT task_id,agent_id,lease_generation,branch,repository_id FROM authoring_bindings WHERE worktree_id=?1",
            [&observed.worktree_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let current = (
            task_id.clone(),
            actor.to_owned(),
            generation,
            observed.branch.clone(),
            observed.repository_id.clone(),
        );
        if let Some(ref prior) = bound {
            if prior.0 != *task_id {
                return Err(denied("AUTHORING_CHECKOUT_OCCUPIED"));
            }
            if prior != &current && !bind {
                return Err(denied("AUTHORING_BINDING_STALE"));
            }
            if prior.3 != current.3 || prior.4 != current.4 {
                return Err(denied("AUTHORING_BINDING_STALE"));
            }
        } else if !bind {
            return Err(denied("AUTHORING_BINDING_REQUIRED"));
        }
        // Detect changes across the transaction as well as across invocations.
        if crate::core::workspace::observe_checkout(checkout) != observed {
            return Err(denied("AUTHORING_CHECKOUT_CHANGED"));
        }
        if bind && bound.as_ref() != Some(&current) {
            conn.execute("INSERT INTO authoring_bindings(worktree_id,task_id,agent_id,lease_generation,branch,repository_id,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(worktree_id) DO UPDATE SET agent_id=excluded.agent_id,lease_generation=excluded.lease_generation,updated_at=excluded.updated_at", params![observed.worktree_id, task_id, actor, generation, observed.branch, observed.repository_id, now])?;
            // Filesystem admission is a workspace lifecycle event, not a
            // mutation of the task projection/journal. Commit with the binding.
            crate::core::events::append_on_conn(
                conn,
                crate::core::events::WORKSPACE_LIFECYCLE,
                &serde_json::json!({"ts":now,"event_id":crate::core::ulid::new_ulid(),
                    "event_type":"authoring.bind","status":"success","actor":actor,
                    "task_id":task_id,"payload":{"checkout":observed,"lease_generation":generation,"previous":bound}}),
            )?;
        }
        Ok(())
    };
    if bind {
        broker.with_transaction(&todo_db_path(root), actor, None, "todo.claim", |conn| {
            ensure_schema(conn)?;
            check(conn)
        })
    } else {
        let conn = crate::core::db::db_connect_for_validate(
            todo_db_path(root)
                .to_str()
                .ok_or_else(|| denied("AUTHORING_STORE_INVALID"))?,
        )?;
        check(&conn)
    }
}
