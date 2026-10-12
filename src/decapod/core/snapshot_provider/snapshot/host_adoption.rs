//! Adopt an authorized host-created transport commit without changing its
//! independently frozen source request or any ordinary Git reference.
use super::*;

/// `observed_server_s` is only an untrusted locator. The caller must retain the
/// original handoff and serialize the returned value into a new private handoff;
/// evidence or execution plans bound to the previous S must not be reused.
///
/// This reads the provider and adds immutable objects locally. It never creates,
/// updates or deletes a local or remote ref, index, worktree file, or FETCH_HEAD.
/// Existing Runner authorization remains mandatory for every operation.
pub fn adopt_host_snapshot(
    runner: &mut impl Runner,
    prior: &PreparedSnapshot,
    observed_server_s: &str,
    budget: &Budget<'_>,
) -> Result<PreparedSnapshot> {
    ensure(
        oid(observed_server_s) && observed_server_s.len() == prior.request.source_head.len(),
        "invalid host transport object locator",
    )?;
    // Revalidate the old local preparation as well as the actual current source.
    // An equivalent tree never permits changing H, branch, B, UUID or tag set.
    let prior = resume(
        runner,
        prior.root(),
        prior.request().clone(),
        prior.transport_head(),
        budget,
    )?;
    let pending = PreparedSnapshot {
        request: prior.request().clone(),
        transport_head: observed_server_s.into(),
        root: prior.root().into(),
    };
    // Check the authenticated provider's immutable object metadata and current
    // ref/base/tag bindings BEFORE fetching an untrusted object locator. This
    // private provisional value is never returned or accepted as evidence.
    provider::verify_remote_binding(runner, &pending, budget)?;
    ensure(
        temporary_ref(runner, &pending, budget)?.as_deref() == Some(observed_server_s),
        "host snapshot reference differs from observed object",
    )?;
    check_transport_configuration(runner, prior.root(), budget)?;
    checked(
        runner,
        git_spec(
            prior.root(),
            &[
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--no-auto-maintenance",
                "--no-write-commit-graph",
                "--no-recurse-submodules",
                "--no-prune",
                "--no-prune-tags",
                "--refmap=",
                "--",
                &prior.request.repository.https_url(),
                observed_server_s,
            ],
            true,
        ),
        budget,
    )?;
    // Raw local Git object identity closes the API's final-LF projection gap.
    let adopted = resume(
        runner,
        prior.root(),
        prior.request().clone(),
        observed_server_s,
        budget,
    )?;
    provider::verify_remote_binding(runner, &adopted, budget)?;
    ensure(
        temporary_ref(runner, &adopted, budget)?.as_deref() == Some(observed_server_s),
        "host snapshot reference changed during adoption",
    )?;
    Ok(adopted)
}

#[cfg(test)]
mod tests;
