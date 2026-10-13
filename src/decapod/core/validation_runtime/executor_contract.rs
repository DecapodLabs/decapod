//! Capture accepted execution authority before any provider dispatch.
//!
//! This is the existing reviewed-transitive-code model. Separate roots prevent
//! accidental candidate self-authorization; they do not sandbox hostile build
//! scripts running as the same provider account. The final controller still
//! recompiles policy and independently checks source, target and provider proof.
use super::*;

const PROTOCOL_DECLARATION: &str =
    "pub const DUAL_ROOT_SCHEMA: &str = \"decapod.source-ci.dual-root.v1\";";

pub(super) fn capture(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    base: &str,
    enabled: bool,
) -> io::Result<Option<profile::AcceptedExecutor>> {
    // The native adapter retains its existing strict compiled contract. This
    // protocol is implemented by the configured source-CI provider carrier.
    if !enabled {
        return Ok(None);
    }
    let marker_path = "assets/tools/source_ci_custody.rs";
    let marker_entry = git(options, budget, &["ls-tree", base, "--", marker_path])?;
    if marker_entry.is_empty() {
        return Ok(None);
    }
    if !marker_entry.starts_with(b"100644 blob ")
        || marker_entry.iter().filter(|v| **v == b'\n').count() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            profile::PlanningFailure::ControllerContractUnsupported(
                "accepted executor protocol mode".into(),
            ),
        ));
    }
    let marker = git(options, budget, &["show", &format!("{base}:{marker_path}")])?;
    if !marker
        .split(|b| *b == b'\n')
        .any(|line| line == PROTOCOL_DECLARATION.as_bytes())
    {
        // A one-time protocol rollout can still use the legacy contract. The
        // caller must independently admit every H helper against this binary;
        // absence of R support never authorizes a changed candidate recipe.
        return Ok(None);
    }
    let tree = text(git(
        options,
        budget,
        &["rev-parse", "--verify", &format!("{base}^{{tree}}")],
    )?)?;
    let mut files = BTreeMap::new();
    for (path, _) in profile::definition_files() {
        let entry = git(options, budget, &["ls-tree", base, "--", path])?;
        let prefix = b"100644 blob ";
        if !entry.starts_with(prefix) || entry.iter().filter(|v| **v == b'\n').count() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                profile::PlanningFailure::ControllerContractUnsupported(format!(
                    "accepted executor mode: {path}"
                )),
            ));
        }
        let bytes = git(options, budget, &["show", &format!("{base}:{path}")])?;
        use sha2::{Digest, Sha256};
        files.insert(path.into(), format!("sha256:{:x}", Sha256::digest(&bytes)));
    }
    profile::AcceptedExecutor::from_captured(base.into(), tree, SourceInventory { files })
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Carrier files are also bound to ordinary tracked modes. Byte-only source
/// discovery cannot admit a symlink or an executable-mode change as equivalent.
pub(super) fn verify_carrier_modes(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    head: &str,
) -> io::Result<()> {
    for (path, _) in profile::carrier_files() {
        let entry = git(options, budget, &["ls-tree", head, "--", path])?;
        if !entry.starts_with(b"100644 blob ") || entry.iter().filter(|v| **v == b'\n').count() != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                profile::PlanningFailure::ControllerContractUnsupported(format!(
                    "carrier mode: {path}"
                )),
            ));
        }
    }
    Ok(())
}
