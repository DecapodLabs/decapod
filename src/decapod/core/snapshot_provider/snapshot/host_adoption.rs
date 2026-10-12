//! Recheck an already published exact workspace HEAD supplied by a host.
use super::*;

/// The host locator is untrusted and must equal the frozen local H. No alternate
/// server commit, local object fetch, reference mutation or deletion is allowed.
pub fn adopt_host_snapshot(
    runner: &mut impl Runner,
    prior: &PreparedSnapshot,
    observed_server_s: &str,
    budget: &Budget<'_>,
) -> Result<PreparedSnapshot> {
    budget.remaining()?;
    ensure(
        observed_server_s == prior.request.source_head,
        "host workspace locator must equal the exact source HEAD",
    )?;
    let current = resume(
        runner,
        prior.root(),
        prior.request().clone(),
        prior.transport_head(),
        budget,
    )?;
    provider::verify_remote_binding(runner, &current, budget)?;
    ensure(
        temporary_ref(runner, &current, budget)?.as_deref() == Some(observed_server_s),
        "host workspace reference differs from exact source HEAD",
    )?;
    let adopted = resume(
        runner,
        prior.root(),
        prior.request().clone(),
        observed_server_s,
        budget,
    )?;
    provider::verify_remote_binding(runner, &adopted, budget)?;
    Ok(adopted)
}

#[cfg(test)]
mod tests;
