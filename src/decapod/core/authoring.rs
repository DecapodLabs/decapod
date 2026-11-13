//! Host-local checkout admission, separate from repository-scoped task custody.
//! Cloud task profiles deliberately do not infer filesystem authority from paths.
use super::*;

fn denied(code: &str) -> error::DecapodError {
    error::DecapodError::ValidationError(format!(
        "{code}: inspect current task custody and checkout; do not impersonate another actor or retry an ambiguous write"
    ))
}

fn denied_with_detail(code: &str, detail: &str) -> error::DecapodError {
    error::DecapodError::ValidationError(format!(
        "{code}: {detail}; inspect current task custody and checkout; do not impersonate another actor or retry an ambiguous write"
    ))
}

#[allow(clippy::too_many_arguments)]
fn emit_authoring_release_on_conn(
    conn: &Connection,
    actor: &str,
    task_id: &str,
    worktree_id: &str,
    branch: &str,
    repository_id: &str,
    lease_generation: i64,
    reason: &str,
    now: &str,
) -> Result<(), error::DecapodError> {
    crate::core::events::append_on_conn(
        conn,
        crate::core::events::WORKSPACE_LIFECYCLE,
        &serde_json::json!({
            "ts": now,
            "event_id": crate::core::ulid::new_ulid(),
            "event_type": "authoring.release",
            "status": "success",
            "actor": actor,
            "task_id": task_id,
            "payload": {
                "worktree_id": worktree_id,
                "branch": branch,
                "repository_id": repository_id,
                "lease_generation": lease_generation,
                "reason": reason,
            }
        }),
    )?;
    Ok(())
}

/// Retire all checkout bindings for `task_id` inside an active task-custody
/// transaction (`yield`, `release`, `reclaim`, `handoff`, `done`, `archive`).
pub(crate) fn retire_task_bindings_on_conn(
    conn: &Connection,
    task_id: &str,
    actor: &str,
    reason: &str,
    now: &str,
) -> Result<usize, error::DecapodError> {
    if !conn.has_table("authoring_bindings")? {
        return Ok(0);
    }
    let mut stmt = conn.prepare(
        "SELECT worktree_id, lease_generation, branch, repository_id
         FROM authoring_bindings
         WHERE task_id = ?1
         ORDER BY worktree_id
         LIMIT 65",
    )?;
    let rows: Vec<(String, i64, String, String)> = stmt
        .query_map([task_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Ok(0);
    }
    conn.execute(
        "DELETE FROM authoring_bindings WHERE task_id = ?1",
        [task_id],
    )?;
    let effective_actor = if actor.trim().is_empty() || actor == "unknown" {
        "decapod"
    } else {
        actor
    };
    for (worktree_id, lease_generation, branch, repository_id) in &rows {
        emit_authoring_release_on_conn(
            conn,
            effective_actor,
            task_id,
            worktree_id,
            branch,
            repository_id,
            *lease_generation,
            reason,
            now,
        )?;
    }
    Ok(rows.len())
}

/// Advance an already-admitted checkout binding when the same owner extends
/// the active lease via `todo renew`. Unbound checkouts and transferred leases
/// remain unadmitted until an explicit `workspace ensure`.
pub(crate) fn renew_task_binding_on_conn(
    conn: &Connection,
    task_id: &str,
    actor: &str,
    prev_generation: i64,
    next_generation: i64,
    now: &str,
) -> Result<usize, error::DecapodError> {
    if !conn.has_table("authoring_bindings")? {
        return Ok(0);
    }
    let bound: Option<(String, String, String)> = conn
        .query_row(
            "SELECT worktree_id, branch, repository_id
             FROM authoring_bindings
             WHERE task_id = ?1 AND agent_id = ?2 AND lease_generation = ?3",
            params![task_id, actor, prev_generation],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((worktree_id, branch, repository_id)) = bound else {
        return Ok(0);
    };
    let changed = conn.execute(
        "UPDATE authoring_bindings
         SET lease_generation = ?1, updated_at = ?2
         WHERE worktree_id = ?3 AND task_id = ?4 AND agent_id = ?5 AND lease_generation = ?6",
        params![
            next_generation,
            now,
            worktree_id,
            task_id,
            actor,
            prev_generation
        ],
    )?;
    if changed > 0 {
        crate::core::events::append_on_conn(
            conn,
            crate::core::events::WORKSPACE_LIFECYCLE,
            &serde_json::json!({
                "ts": now,
                "event_id": crate::core::ulid::new_ulid(),
                "event_type": "authoring.renew",
                "status": "success",
                "actor": actor,
                "task_id": task_id,
                "payload": {
                    "worktree_id": worktree_id,
                    "branch": branch,
                    "repository_id": repository_id,
                    "previous_lease_generation": prev_generation,
                    "lease_generation": next_generation,
                }
            }),
        )?;
    }
    Ok(changed)
}

/// Release a checkout binding by `worktree_id` inside a dedicated broker
/// transaction (used by `workspace prune` after worktree removal).
pub fn release_authoring_binding(
    root: &Path,
    worktree_id: &str,
    actor: &str,
    reason: &str,
) -> Result<bool, error::DecapodError> {
    if worktree_id.trim().is_empty() {
        return Ok(false);
    }
    let broker = DbBroker::new(root);
    if broker.is_cloud() {
        return Ok(false);
    }
    let db_path = todo_db_path(root);
    if !db_path.exists() {
        return Ok(false);
    }
    let effective_actor = if actor.trim().is_empty() || actor == "unknown" {
        "decapod"
    } else {
        actor
    };
    let now = now_iso();
    broker.with_transaction(
        &db_path,
        effective_actor,
        None,
        "workspace.authoring.release",
        |conn| {
            if !conn.has_table("authoring_bindings")? {
                return Ok(false);
            }
            let existing: Option<(String, i64, String, String)> = conn
                .query_row(
                    "SELECT task_id, lease_generation, branch, repository_id
                     FROM authoring_bindings
                     WHERE worktree_id = ?1",
                    [worktree_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;
            let Some((task_id, lease_generation, branch, repository_id)) = existing else {
                return Ok(false);
            };
            conn.execute(
                "DELETE FROM authoring_bindings WHERE worktree_id = ?1",
                [worktree_id],
            )?;
            emit_authoring_release_on_conn(
                conn,
                effective_actor,
                &task_id,
                worktree_id,
                &branch,
                &repository_id,
                lease_generation,
                reason,
                &now,
            )?;
            Ok(true)
        },
    )
}

/// Enforce named exclusive shared-resource reservations (`resource:<name>`) at
/// actual shared mutation boundaries without serializing isolated authoring.
pub fn enforce_shared_resource_available(
    root: &Path,
    actor: &str,
    requested_resources: &[&str],
    affected_operation: &str,
) -> Result<(), error::DecapodError> {
    if requested_resources.is_empty() {
        return Ok(());
    }
    let broker = DbBroker::new(root);
    if broker.is_cloud() {
        return Ok(());
    }
    let db_path = todo_db_path(root);
    if !db_path.exists() {
        return Ok(());
    }
    let conn = crate::core::db::db_connect_for_validate(
        db_path
            .to_str()
            .ok_or_else(|| denied("AUTHORING_STORE_INVALID"))?,
    )?;
    if !conn.has_table("tasks")? {
        return Ok(());
    }
    let now = now_iso();
    let active = list_active_exclusive_claims(&conn, &now)?;
    let has_verified_actor = !actor.trim().is_empty() && actor != "unknown";
    for requested in requested_resources {
        for claim in &active {
            if claim.agent_id.trim().is_empty()
                || matches!(
                    claim.lease_lifecycle,
                    fleet_coord::LeaseLifecycle::Yielded
                        | fleet_coord::LeaseLifecycle::Released
                        | fleet_coord::LeaseLifecycle::Expired
                )
                || matches!(
                    fleet_coord::lease_state(claim.lease_expires_at.as_deref(), &now),
                    fleet_coord::LeaseState::Expired
                )
            {
                continue;
            }
            if has_verified_actor && claim.agent_id == actor {
                continue;
            }
            if let Some(matched_resource) =
                fleet_coord::matching_resource_reservation(requested, &claim.scope)
            {
                return Err(error::DecapodError::ValidationError(format!(
                    "SHARED_RESOURCE_OCCUPIED: operation '{affected_operation}' requires shared resource '{matched_resource}', which is exclusively reserved by active task '{}' (owner '{}', predicate=resource_reservation, affected_operation={affected_operation}). Isolated worktree authoring and local validation remain permitted; wait for custody release or coordinate handoff before mutating '{matched_resource}'.",
                    claim.task_id, claim.agent_id
                )));
            }
        }
    }
    Ok(())
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
        if !conn.has_table("tasks")? {
            return Err(denied_with_detail(
                "AUTHORING_EVIDENCE_UNAVAILABLE",
                "canonical task custody schema is absent",
            ));
        }
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

        let (owner, generation, expiry, lifecycle, status): (
            String,
            i64,
            Option<String>,
            String,
            String,
        ) = conn.query_row(
            "SELECT assigned_to,COALESCE(lease_generation,0),lease_expires_at,COALESCE(lease_lifecycle,''),status FROM tasks WHERE id=?1",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
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
        if !bind && !conn.has_table("authoring_bindings")? {
            return Err(denied("AUTHORING_BINDING_REQUIRED"));
        }

        let mut replaced_missing_worktree: Option<String> = None;
        if bind {
            // Retire only durably inactive custody on this checkout or task.
            // Live bindings survive branch switches, including switches to
            // another task held by the very same actor.
            let now_epoch = parse_epoch_z(&now).unwrap_or(0) as i64;
            let mut inactive_stmt = conn.prepare(
                "SELECT b.worktree_id, b.task_id, b.lease_generation, b.branch, b.repository_id
                 FROM authoring_bindings b
                 LEFT JOIN tasks t ON t.id = b.task_id
                 WHERE (b.worktree_id = ?1 OR b.task_id = ?2)
                   AND (
                       t.id IS NULL
                       OR t.status IN ('done', 'archived')
                       OR t.assigned_to = ''
                       OR t.lease_lifecycle IN ('yielded', 'released', 'expired')
                       OR t.lease_expires_at IS NULL
                       OR t.lease_expires_at = ''
                       OR CAST(rtrim(t.lease_expires_at, 'Z') AS INTEGER) <= ?3
                   )
                 ORDER BY b.worktree_id
                 LIMIT 65",
            )?;
            let inactive_rows: Vec<(String, String, i64, String, String)> = inactive_stmt
                .query_map(params![observed.worktree_id, task_id, now_epoch], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (wt_id, retired_task_id, retired_gen, retired_branch, retired_repo) in inactive_rows
            {
                conn.execute(
                    "DELETE FROM authoring_bindings WHERE worktree_id = ?1",
                    [&wt_id],
                )?;
                emit_authoring_release_on_conn(
                    conn,
                    actor,
                    &retired_task_id,
                    &wt_id,
                    &retired_branch,
                    &retired_repo,
                    retired_gen,
                    "inactive_custody_retired",
                    &now,
                )?;
            }
        }

        let bound: Option<(String, String, i64, String, String)> = conn
            .query_row(
                "SELECT task_id,agent_id,lease_generation,branch,repository_id FROM authoring_bindings WHERE worktree_id=?1",
                [&observed.worktree_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
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

        if bind {
            let other_task_binding: Option<(String, String, i64, String, String)> = conn
                .query_row(
                    "SELECT worktree_id, agent_id, lease_generation, branch, repository_id
                     FROM authoring_bindings
                     WHERE task_id = ?1 AND worktree_id != ?2",
                    params![task_id, observed.worktree_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            if let Some((other_wt_id, other_agent, other_gen, other_branch, other_repo)) =
                other_task_binding
            {
                if other_agent != actor {
                    conn.execute(
                        "DELETE FROM authoring_bindings WHERE worktree_id = ?1",
                        [&other_wt_id],
                    )?;
                    emit_authoring_release_on_conn(
                        conn,
                        actor,
                        task_id,
                        &other_wt_id,
                        &other_branch,
                        &other_repo,
                        other_gen,
                        "superseded_lease_owner",
                        &now,
                    )?;
                } else if crate::core::workspace::is_registered_worktree_id(&main, &other_wt_id) {
                    return Err(denied_with_detail(
                        "AUTHORING_TASK_ALREADY_BOUND",
                        &format!(
                            "task '{task_id}' already holds a live checkout binding on branch '{other_branch}' ({other_wt_id}); one active task lease may bind at most one live worktree"
                        ),
                    ));
                } else {
                    conn.execute(
                        "DELETE FROM authoring_bindings WHERE worktree_id = ?1",
                        [&other_wt_id],
                    )?;
                    emit_authoring_release_on_conn(
                        conn,
                        actor,
                        task_id,
                        &other_wt_id,
                        &other_branch,
                        &other_repo,
                        other_gen,
                        "missing_worktree_replaced",
                        &now,
                    )?;
                    replaced_missing_worktree = Some(other_wt_id);
                }
            }
        }

        // Detect changes across the transaction as well as across invocations.
        if crate::core::workspace::observe_checkout(checkout) != observed {
            return Err(denied("AUTHORING_CHECKOUT_CHANGED"));
        }
        if bind && bound.as_ref() != Some(&current) {
            conn.execute(
                "INSERT INTO authoring_bindings(worktree_id,task_id,agent_id,lease_generation,branch,repository_id,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(worktree_id) DO UPDATE SET
                     agent_id=excluded.agent_id,
                     lease_generation=excluded.lease_generation,
                     updated_at=excluded.updated_at",
                params![
                    observed.worktree_id,
                    task_id,
                    actor,
                    generation,
                    observed.branch,
                    observed.repository_id,
                    now
                ],
            )?;
            // Filesystem admission is a workspace lifecycle event, not a
            // mutation of the task projection/journal. Commit with the binding.
            let mut payload = serde_json::json!({
                "checkout": observed,
                "lease_generation": generation,
                "previous": bound,
            });
            if let Some(replaced) = replaced_missing_worktree {
                payload["replaced_missing_worktree"] = serde_json::json!(replaced);
            }
            crate::core::events::append_on_conn(
                conn,
                crate::core::events::WORKSPACE_LIFECYCLE,
                &serde_json::json!({
                    "ts": now,
                    "event_id": crate::core::ulid::new_ulid(),
                    "event_type": "authoring.bind",
                    "status": "success",
                    "actor": actor,
                    "task_id": task_id,
                    "payload": payload,
                }),
            )?;
        }
        Ok(())
    };
    if bind {
        broker.with_transaction(
            &todo_db_path(root),
            actor,
            None,
            "workspace.authoring.bind",
            |conn| {
                ensure_schema(conn)?;
                check(conn)
            },
        )
    } else {
        let db_path = todo_db_path(root);
        if !db_path.exists() {
            return Err(denied_with_detail(
                "AUTHORING_EVIDENCE_UNAVAILABLE",
                "canonical task datastore is absent",
            ));
        }
        let path_str = db_path
            .to_str()
            .ok_or_else(|| denied("AUTHORING_STORE_INVALID"))?;
        let conn = crate::core::db::db_connect_for_validate(path_str).map_err(|_| {
            denied_with_detail(
                "AUTHORING_EVIDENCE_UNAVAILABLE",
                "canonical task datastore could not be opened for verification",
            )
        })?;
        check(&conn)
    }
}
