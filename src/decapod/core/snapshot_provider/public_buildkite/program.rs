//! Bounded structural sanity for authenticated buildkite-gha v0.102.3 plans.
//!
//! Shapes were checked against official schemas/job-plan.schema.json and actual
//! Linux/macOS plans from public builds1209–1213. This is not a second compiler,
//! complete schema validator, expression interpreter or compiler attestation.
//! Provider custody identifies the trusted importer; the controller separately
//! pins compiler identity and the complete workflow/local-action carrier bytes.
use super::*;

fn object(value: &Value) -> Result<&serde_json::Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| invalid("importer program object malformed"))
}
fn bounded_array(value: &Value, max: usize) -> Result<&[Value]> {
    let rows = value
        .as_array()
        .ok_or_else(|| invalid("importer program array malformed"))?;
    ensure(rows.len() <= max, "importer program array exceeds bound")?;
    Ok(rows)
}
fn site(value: &Value) -> Result<()> {
    string(value, "source")?;
    string(field(value, "location")?, "field")?;
    Ok(())
}
fn bindings(value: &Value) -> Result<()> {
    let mut names = BTreeSet::new();
    for row in bounded_array(value, 1024)? {
        let name = string(row, "name")?;
        ensure(
            !name.is_empty() && names.insert(name),
            "invalid importer binding name",
        )?;
        site(field(row, "value")?)?;
    }
    Ok(())
}
fn control(value: &Value, numeric: bool) -> Result<()> {
    let fields = object(value)?;
    ensure(fields.len() <= 1, "ambiguous importer program control")?;
    for (key, value) in fields {
        match key.as_str() {
            "literal" => ensure(
                if numeric {
                    value.is_number()
                } else {
                    value.is_boolean()
                },
                "importer control literal malformed",
            )?,
            "expression" => site(value)?,
            _ => return Err(invalid("unknown importer control field")),
        }
    }
    Ok(())
}
fn invocation(value: &Value, locks: &BTreeSet<&str>) -> Result<()> {
    site(field(value, "uses")?)?;
    ensure(
        !string(field(value, "uses")?, "source")?.is_empty()
            && locks.contains(string(value, "lock")?),
        "importer action invocation lock missing",
    )?;
    if let Some(inputs) = value.get("with") {
        bindings(inputs)?;
    }
    Ok(())
}
fn executable(step: &Value, locks: &BTreeSet<&str>, composite: bool) -> Result<()> {
    match (step.get("run"), step.get("invocation")) {
        (Some(run), None) => {
            let command = field(run, "command")?;
            site(command)?;
            ensure(
                !string(command, "source")?.trim().is_empty(),
                "empty importer run command",
            )?;
            if !composite {
                site(field(run, "shell")?)?;
                site(field(run, "working_directory")?)?;
            }
        }
        (None, Some(uses)) => invocation(uses, locks)?,
        _ => return Err(invalid("invalid importer executable step")),
    }
    Ok(())
}

/// Reject missing/empty programs and broken typed references before discovery.
/// Semantic authority remains the pinned carrier plus trusted provider importer.
/// Valid-shaped neighboring steps are not independently recompiled here.
pub(super) fn verify(plan: &Value) -> Result<()> {
    let program = field(plan, "program")?;
    ensure(
        object(program)?
            .keys()
            .all(|key| matches!(key.as_str(), "version" | "job" | "actions"))
            && program["version"].as_u64() == Some(2),
        "unsupported importer program version or fields",
    )?;
    let mut locks = BTreeSet::new();
    let mut workspace_locks = BTreeSet::new();
    let actions = plan
        .get("actions")
        .map(|v| bounded_array(v, 1024))
        .transpose()?
        .unwrap_or(&[]);
    for action in actions {
        let id = string(action, "id")?;
        ensure(
            id.strip_prefix("a-").is_some_and(|id| {
                id.len() == 16
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            }) && locks.insert(id)
                && digest(string(action, "source_digest")?),
            "invalid importer action lock",
        )?;
        match string(action, "source")? {
            "workspace" => {
                ensure(
                    !string(action, "path")?.is_empty(),
                    "missing workspace action path",
                )?;
                workspace_locks.insert(id);
            }
            "github" => ensure(
                !string(action, "repository")?.is_empty()
                    && !string(action, "requested_ref")?.is_empty()
                    && string(action, "commit")?.len() == 40
                    && oid(string(action, "commit")?),
                "invalid immutable importer action source",
            )?,
            _ => return Err(invalid("unsupported importer action source")),
        }
    }
    for action in actions {
        if let Some(children) = action.get("children") {
            let children = object(children)?;
            ensure(children.len() <= 1024, "too many importer action children")?;
            for child in children.values() {
                ensure(
                    locks.contains(string(child, "lock")?),
                    "importer action child lock missing",
                )?;
            }
        }
    }
    let job = field(program, "job")?;
    object(job)?;
    site(field(job, "condition")?)?;
    control(field(job, "continue_on_error")?, false)?;
    let defaults = field(job, "defaults")?;
    site(field(defaults, "shell")?)?;
    site(field(defaults, "working_directory")?)?;
    object(field(job, "services")?)?;
    if let Some(env) = job.get("env") {
        bindings(env)?;
    }
    let steps = bounded_array(field(job, "steps")?, 1000)?;
    ensure(!steps.is_empty(), "importer program contains no steps")?;
    let mut ids = BTreeSet::new();
    for step in steps {
        ensure(
            object(step)?.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "id" | "kind"
                        | "source"
                        | "condition"
                        | "continue_on_error"
                        | "timeout_minutes"
                        | "name"
                        | "background"
                        | "targets"
                        | "env"
                        | "run"
                        | "invocation"
                )
            }),
            "unknown importer step field",
        )?;
        let id = string(step, "id")?;
        ensure(
            !id.is_empty() && id.len() <= 255 && ids.insert(id.to_ascii_lowercase()),
            "invalid importer step ID",
        )?;
        string(field(step, "source")?, "field")?;
        site(field(step, "condition")?)?;
        site(field(step, "name")?)?;
        control(field(step, "continue_on_error")?, false)?;
        control(field(step, "timeout_minutes")?, true)?;
        if let Some(env) = step.get("env") {
            bindings(env)?;
        }
        match string(step, "kind")? {
            "run" => {
                ensure(
                    step.get("run").is_some(),
                    "importer run step missing command",
                )?;
                executable(step, &locks, false)?;
            }
            "uses" => {
                ensure(
                    step.get("invocation").is_some(),
                    "importer action step missing invocation",
                )?;
                executable(step, &locks, false)?;
            }
            "wait" | "wait-all" | "cancel" => { /* Typed controls are validated by the pinned compiler/runtime. */
            }
            _ => return Err(invalid("unsupported importer step kind")),
        }
    }
    if let Some(action_programs) = program.get("actions") {
        let action_programs = object(action_programs)?;
        ensure(
            action_programs.len() <= 1024,
            "too many importer action programs",
        )?;
        for (id, action) in action_programs {
            ensure(
                locks.contains(id.as_str()),
                "importer action program has no source lock",
            )?;
            site(field(action, "pre_if")?)?;
            site(field(action, "post_if")?)?;
            match string(action, "runtime")? {
                "composite" => {
                    let steps = bounded_array(field(action, "steps")?, 1000)?;
                    ensure(!steps.is_empty(), "empty importer composite action")?;
                    for step in steps {
                        for key in ["name", "shell", "working_directory", "condition"] {
                            site(field(step, key)?)?;
                        }
                        executable(step, &locks, true)?;
                    }
                }
                "node16" | "node24" => {
                    ensure(
                        !string(action, "main")?.is_empty(),
                        "missing importer action entrypoint",
                    )?;
                }
                "docker" => {
                    ensure(
                        !string(action, "image")?.is_empty(),
                        "missing importer action image",
                    )?;
                }
                _ => return Err(invalid("unsupported importer action runtime")),
            }
        }
    }
    // Upstream native checkout/upload/download adapters intentionally omit an
    // action program. Local workspace actions have no such native adapter.
    for id in workspace_locks {
        ensure(
            program
                .get("actions")
                .and_then(|actions| actions.get(id))
                .is_some(),
            "importer workspace action program missing",
        )?;
    }
    Ok(())
}
