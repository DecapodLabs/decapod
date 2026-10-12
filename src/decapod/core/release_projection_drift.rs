//! Read-only projection proof for strictly proven version-only releases.
//! Used after normal projection refresh within validation and by its CI drift gate.
//! This proof never substitutes for the rest of `decapod validate --refresh-specs`.
//! Only exact current-release projections may differ from the immutable tree.

use crate::core::{
    entrypoint_integrity as entrypoints, fs_permissions, project_specs as specs, release_metadata,
};
use crate::plugins::container;
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path, process::Command};

const DOCKERFILE: &str = ".decapod/managed/Dockerfile.decapod";
const ATTESTATION_START: &str = "<!-- decapod:codebase-attestation:start -->";
const ATTESTATION_END: &str = "<!-- decapod:codebase-attestation:end -->";
pub type Check<T> = Result<T, Box<dyn std::error::Error>>;

fn git(repo: &Path, args: &[&str]) -> Check<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn blob(repo: &Path, head: &str, path: &str) -> Check<String> {
    let entry = git(repo, &["ls-tree", head, "--", path])?;
    if !entry.starts_with("100644 blob ") || !entry.ends_with(&format!("\t{path}\n")) {
        return Err(format!("{path}: expected a tracked regular non-executable file").into());
    }
    git(repo, &["show", &format!("{head}:{path}")])
}

fn require_equal(path: &str, actual: &str, expected: &str) -> Check<()> {
    if actual != expected {
        return Err(format!("{path}: substantive or unexpected projection drift").into());
    }
    Ok(())
}

fn read_regular(repo: &Path, path: &str) -> Check<String> {
    if !fs::symlink_metadata(repo.join(path))?.file_type().is_file() {
        return Err(format!("{path}: projection is not a regular file").into());
    }
    Ok(fs::read_to_string(repo.join(path))?)
}

// Replace exactly one digest inside the canonical attestation block, retaining
// every other byte, including authored prose, capabilities and path inventory.
fn expected_spec(old: &str, fingerprint: &str) -> Check<String> {
    let (before, block) = old
        .split_once(ATTESTATION_START)
        .ok_or("missing attestation start")?;
    let (body, after) = block
        .split_once(ATTESTATION_END)
        .ok_or("missing attestation end")?;
    if before.contains(ATTESTATION_END)
        || body.contains(ATTESTATION_START)
        || after.contains(ATTESTATION_START)
        || after.contains(ATTESTATION_END)
    {
        return Err("ambiguous attestation boundaries".into());
    }
    let prefix = "- Repository signal fingerprint: `";
    let lines: Vec<_> = body
        .lines()
        .filter(|line| line.starts_with(prefix))
        .collect();
    if lines.len() != 1 || body.matches(prefix).count() != 1 {
        return Err("expected exactly one generated repository fingerprint".into());
    }
    let digest = lines[0]
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix('`'))
        .ok_or("malformed repository fingerprint")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("malformed repository fingerprint digest".into());
    }
    let updated = body.replacen(lines[0], &format!("{prefix}{fingerprint}`"), 1);
    Ok(format!(
        "{before}{ATTESTATION_START}{updated}{ATTESTATION_END}{after}"
    ))
}

fn expected_dockerfile(old: &str, old_release: &str) -> Check<String> {
    container::managed_dockerfile_matches_release(old, old_release)?;
    for arg in ["DECAPOD_IMAGE", "DECAPOD_VERSION"] {
        if old
            .lines()
            .filter(|line| line.trim_start().starts_with(&format!("ARG {arg}=")))
            .count()
            != 1
        {
            return Err(format!("ambiguous Dockerfile {arg} header").into());
        }
    }
    // Invoke the real, bounded header refresh on a private fixture. This also
    // captures its exact whitespace behavior; no normalization hides changes.
    let mut scratch = tempfile::Builder::new();
    scratch.prefix("decapod-release-projection-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        scratch.permissions(fs::Permissions::from_mode(0o700));
    }
    let temp = scratch.tempdir()?;
    let path = temp.path().join(DOCKERFILE);
    fs_permissions::ensure_private_dir(path.parent().ok_or("missing Dockerfile parent")?)?;
    fs_permissions::write_private(&path, old)?;
    container::refresh_managed_dockerfile_release(temp.path())?;
    Ok(fs::read_to_string(path)?)
}

fn check_changed_paths(repo: &Path, head: &str, allowed: &BTreeSet<&str>) -> Check<()> {
    // Include ignored files: an ignored source file still contributes to the
    // repository fingerprint. Only the generator's excluded runtime/build
    // directory names may contain new files after validation.
    let untracked = git(repo, &["ls-files", "--others", "-z"])?;
    if untracked.split_terminator('\0').any(|path| {
        !Path::new(path).parent().is_some_and(|parent| {
            parent.components().any(|component| {
                matches!(
                    component.as_os_str().to_str(),
                    Some(".decapod" | "target" | "node_modules" | ".venv")
                )
            })
        })
    }) {
        return Err("untracked files are not release projections".into());
    }
    let changes = git(
        repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--raw",
            "-z",
            head,
            "--",
        ],
    )?;
    let fields: Vec<_> = changes.split_terminator('\0').collect();
    if fields.len() % 2 != 0 {
        return Err("malformed Git change inventory".into());
    }
    for change in fields.chunks_exact(2) {
        let path = change[1];
        // The real validation harness writes its existing governance receipt.
        if path == ".decapod/governance.json" {
            continue;
        }
        let metadata: Vec<_> = change[0].split_whitespace().collect();
        if !allowed.contains(path)
            || metadata.len() != 5
            || metadata[0] != ":100644"
            || metadata[1] != "100644"
            || metadata[4] != "M"
        {
            return Err(format!("{path}: unexpected path, mode, addition or deletion").into());
        }
    }
    Ok(())
}

pub fn verify(repo: &Path, event: &str, base: &str, head: &str) -> Check<()> {
    // A staged edit reverted only in the working tree must not disappear from
    // release proof. CLI projection refreshes are unstaged, so a clean index
    // is a strict, sufficient boundary for both validation and its drift gate.
    git(
        repo,
        &["diff", "--cached", "--quiet", "--no-ext-diff", "HEAD", "--"],
    )?;
    if !release_metadata::classify(repo, event, base, head)? {
        return Err("immutable event range is not a version-only release".into());
    }
    git(
        repo,
        &["diff", "--quiet", "--no-ext-diff", head, "HEAD", "--"],
    )?;
    let mut allowed: BTreeSet<_> = entrypoints::ENTRYPOINT_FILES.into_iter().collect();
    allowed.extend([DOCKERFILE, specs::LOCAL_PROJECT_SPECS_MANIFEST]);
    allowed.extend(specs::LOCAL_PROJECT_SPECS.iter().map(|spec| spec.path));
    check_changed_paths(repo, head, &allowed)?;
    let cargo: toml::Value = toml::from_str(&blob(repo, head, "Cargo.toml")?)?;
    if cargo["package"]["version"].as_str() != Some(entrypoints::RELEASE_VERSION) {
        return Err("checker binary does not match the classified release version".into());
    }

    let mut expected: Value =
        serde_json::from_str(&blob(repo, head, specs::LOCAL_PROJECT_SPECS_MANIFEST)?)?;
    let actual: Value =
        serde_json::from_str(&read_regular(repo, specs::LOCAL_PROJECT_SPECS_MANIFEST)?)?;
    let old_release = expected["decapod_release"]
        .as_str()
        .ok_or("missing manifest release")?
        .to_string();
    let mut old_entrypoints = Vec::new();
    for path in entrypoints::ENTRYPOINT_FILES {
        let old_body = entrypoints::render_entrypoint_for_release(path, &old_release)
            .ok_or("unknown entrypoint")?;
        require_equal(path, &blob(repo, head, path)?, &old_body)?;
        let old_fingerprint = old_body
            .lines()
            .nth(1)
            .and_then(|line| line.strip_prefix("<!-- decapod-fingerprint: "))
            .and_then(|line| line.strip_suffix(" -->"))
            .ok_or("malformed canonical entrypoint fingerprint")?;
        old_entrypoints.push(serde_json::json!({
            "path": path,
            "template_hash": specs::hash_text(&old_body),
            "content_hash": specs::hash_text(&old_body),
            "fingerprint": old_fingerprint,
        }));
        require_equal(
            path,
            &read_regular(repo, path)?,
            &entrypoints::render_entrypoint(path).ok_or("unknown entrypoint")?,
        )?;
    }
    if expected["entrypoints"] != Value::Array(old_entrypoints) {
        return Err("baseline entrypoint inventory or template hashes are not canonical".into());
    }
    require_equal(
        DOCKERFILE,
        &read_regular(repo, DOCKERFILE)?,
        &expected_dockerfile(&blob(repo, head, DOCKERFILE)?, &old_release)?,
    )?;
    let fingerprint = specs::repo_signal_fingerprint(repo)?;
    for spec in specs::LOCAL_PROJECT_SPECS {
        require_equal(
            spec.path,
            &read_regular(repo, spec.path)?,
            &expected_spec(&blob(repo, head, spec.path)?, &fingerprint)?,
        )?;
    }
    let files = expected["files"]
        .as_array_mut()
        .ok_or("missing manifest file inventory")?;
    if files.len() != specs::LOCAL_PROJECT_SPECS.len() {
        return Err("unexpected manifest file inventory".into());
    }
    for (file, spec) in files.iter_mut().zip(specs::LOCAL_PROJECT_SPECS) {
        if file["path"].as_str() != Some(spec.path) || file["fingerprint"].as_str() != Some("") {
            return Err("unexpected manifest file path or fingerprint".into());
        }
        file["content_hash"] = specs::hash_text(&read_regular(repo, spec.path)?).into();
    }
    // Compare all other fields unchanged, including unknown fields, schema,
    // templates, config hash, capability definition and capability inventory.
    if expected["config_input_hash"].as_str() != Some(&specs::config_input_hash(repo)?) {
        return Err("configuration hash drift is not a release-only change".into());
    }
    let timestamp = actual["generated_at"]
        .as_str()
        .ok_or("missing generation timestamp")?;
    if !timestamp.strip_suffix('Z').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return Err("unexpected generation timestamp".into());
    }
    expected["generated_at"] = actual["generated_at"].clone();
    expected["repo_signal_fingerprint"] = fingerprint.into();
    expected["spec_input_hash"] = specs::spec_input_hash(repo)?.into();
    expected["decapod_release"] = entrypoints::RELEASE_VERSION.into();
    expected["entrypoints"] = serde_json::to_value(specs::entrypoint_manifest_entries(repo)?)?;
    if actual != expected {
        return Err("manifest contains drift beyond exact verified release hashes".into());
    }
    Ok(())
}

/// Prove the material-spec exception from immutable event evidence, or from a
/// simple local feature head. Partial event evidence never falls back to Git.
/// The configured base remains authoritative even if an event supplies SHAs.
pub(crate) fn verify_for_validation(
    repo: &Path,
    base_ref: &str,
    evidence: [Option<String>; 3],
) -> Check<()> {
    let [event, base, head] = evidence;
    let (event, base, head) = match (event, base, head) {
        (Some(event), Some(base), Some(head)) => (event, base, head),
        (None, None, None) => {
            // Without event metadata, do not mistake a synthetic merge for its
            // PR head and silently classify newer base code as release-only.
            let parents = git(repo, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
            if parents.split_whitespace().count() != 2 {
                return Err(
                    "local release proof requires an unambiguous single-parent head".into(),
                );
            }
            (
                "pull_request".to_string(),
                git(
                    repo,
                    &["rev-parse", "--verify", &format!("{base_ref}^{{commit}}")],
                )?
                .trim()
                .to_string(),
                git(repo, &["rev-parse", "--verify", "HEAD^{commit}"])?
                    .trim()
                    .to_string(),
            )
        }
        _ => return Err("incomplete immutable release event evidence".into()),
    };
    // Run the shared classifier before using externally supplied strings as
    // revision operands. It only accepts complete, nonzero object IDs.
    if !release_metadata::classify(repo, &event, &base, &head)? {
        return Err("immutable event range is not a version-only release".into());
    }
    git(repo, &["merge-base", "--is-ancestor", &base, base_ref])?;
    verify(repo, &event, &base, &head)
}

#[cfg(test)]
#[path = "../../../assets/tools/release_projection_drift_tests.rs"]
pub(crate) mod tests;
