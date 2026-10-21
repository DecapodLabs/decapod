//! One generated commit is transported to every worker. Provider artifact/run
//! custody is checked by the caller; this layer never treats JSON as authority.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseTransport {
    pub schema: String,
    pub binding: GenerationBinding,
    pub generation_tools: BTreeMap<Tool, ToolResolution>,
    pub bundle_sha256: String,
    pub preparation_records: Vec<CommandRecord>,
    pub tool_setup_sha256: String,
    pub semver_checks_version: String,
    pub semver_checks_sha256: String,
    pub installer_definition_sha256: String,
}

pub fn prepare<'a>(
    inputs: &'a DriverInputs,
    owned_parent: &Path,
    limits: DriverLimits,
    cancel: &'a Cancellation,
) -> io::Result<PreparedRelease<'a>> {
    prepare_inner(inputs, owned_parent, limits, cancel, None)
}
pub fn import_release<'a>(
    inputs: &'a DriverInputs,
    owned_parent: &Path,
    limits: DriverLimits,
    cancel: &'a Cancellation,
    bundle: &Path,
    receipt: &ReleaseTransport,
) -> io::Result<PreparedRelease<'a>> {
    if receipt.schema != "derived-release-transport/1"
        || receipt.binding.source_commit != inputs.source_head
        || receipt.binding.accepted_base_commit != inputs.accepted_base
        || receipt.binding.release_date != inputs.release_date
        || receipt.bundle_sha256
            != release_profile::sha256(&read_bounded(bundle, limits.max_material_bytes)?)
        || receipt.preparation_records.is_empty()
        || receipt.preparation_records.iter().any(|r| r.exit_code != 0)
        || receipt.semver_checks_version != inputs.semver_checks_production_version
        || receipt.installer_definition_sha256 != inputs.semver_checks_installer_definition_sha256
    {
        return Err(invalid("transport/source/setup custody mismatch"));
    }
    prepare_inner(
        inputs,
        owned_parent,
        limits,
        cancel,
        Some((bundle, receipt)),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn import_prepared<'a>(
    inputs: &'a DriverInputs,
    storage: tempfile::TempDir,
    root: PathBuf,
    mut context: Context<'a>,
    profile: ReleaseProfile,
    before: Snapshot,
    worker_tools: BTreeMap<Tool, ToolResolution>,
    semver_checks_version: String,
    semver_checks_sha256: String,
    bundle: &Path,
    receipt: &ReleaseTransport,
) -> io::Result<PreparedRelease<'a>> {
    let binding = &receipt.binding;
    let history = context.git(&root, &["log", "--format=raw", &inputs.source_head])?;
    let tags = context.git(
        &root,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/tags",
        ],
    )?;
    if release_profile::sha256(format!("{history}\n{tags}").as_bytes()) != binding.history_sha256
        || context.git(&root, &["rev-parse", "HEAD^{tree}"])?.trim() != binding.source_git_tree
        || context
            .git(
                &root,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/tags/{}^{{commit}}", binding.baseline_tag),
                ],
            )?
            .trim()
            != binding.baseline_commit
    {
        return Err(invalid("transport history/tags/source tree mismatch"));
    }
    let bundle_path = bundle
        .to_str()
        .ok_or_else(|| invalid("non-UTF-8 bundle path"))?;
    // Accept precisely one head and one prerequisite. No tag/reference imports.
    let raw = read_bounded(bundle, context.limits.max_material_bytes)?;
    let boundary = raw
        .windows(2)
        .position(|p| p == b"\n\n")
        .ok_or_else(|| invalid("missing bundle header"))?;
    let expected = format!("# v2 git bundle\n-{} ", binding.source_commit);
    let header =
        std::str::from_utf8(&raw[..boundary]).map_err(|_| invalid("invalid bundle header"))?;
    let lines: Vec<_> = header.lines().collect();
    if lines.len() != 3
        || !header.starts_with(&expected)
        || lines[2] != format!("{} HEAD", binding.derived_commit)
    {
        return Err(invalid("bundle must contain only D with prerequisite H"));
    }
    context.git(&root, &["bundle", "verify", bundle_path])?;
    let heads = context.git(&root, &["bundle", "unbundle", bundle_path])?;
    if heads.trim() != format!("{} HEAD", binding.derived_commit) {
        return Err(invalid(
            "unbundled head differs from transported derivative",
        ));
    }
    if context
        .git(
            &root,
            &["show", "-s", "--format=%P", &binding.derived_commit],
        )?
        .trim()
        != binding.source_commit
        || context
            .git(
                &root,
                &["rev-parse", &format!("{}^{{tree}}", binding.derived_commit)],
            )?
            .trim()
            != binding.derived_git_tree
    {
        return Err(invalid("derived commit parent/tree mismatch"));
    }
    context.git(&root, &["checkout", "--detach", &binding.derived_commit])?;
    let after = snapshot(&mut context, &root)?;
    let generated = checked(release_profile::verify_generation(
        &profile,
        &before,
        &after,
        binding,
        &receipt.generation_tools,
    ))?;
    let worker_setup = worker_setup(
        inputs,
        worker_tools,
        semver_checks_version.clone(),
        semver_checks_sha256.clone(),
        context.limits.max_material_bytes,
    )?;
    let tool_setup_sha256 = release_profile::sha256(&serde_json::to_vec(&worker_setup)?);
    let preparation_records = context.records.clone();
    Ok(PreparedRelease {
        storage,
        root,
        context,
        profile,
        binding: binding.clone(),
        generated,
        semver_checks_version,
        semver_checks_sha256,
        installer_definition_sha256: inputs.semver_checks_installer_definition_sha256.clone(),
        preparation_records,
        tool_setup_sha256,
        generation_tools: receipt.generation_tools.clone(),
        worker_setup,
    })
}

impl PreparedRelease<'_> {
    pub fn export_transport(&mut self, destination: &Path) -> io::Result<ReleaseTransport> {
        self.unchanged()?;
        if destination.exists() || destination.starts_with(&self.context.inputs.source_root) {
            return Err(invalid("transport destination must be fresh owned scratch"));
        }
        fs::create_dir(destination)?;
        let bundle = destination.join("derived.bundle");
        let path = bundle
            .to_str()
            .ok_or_else(|| invalid("non-UTF-8 bundle destination"))?;
        self.context.git(
            &self.root,
            &[
                "bundle",
                "create",
                path,
                "HEAD",
                &format!("^{}", self.binding.source_commit),
            ],
        )?;
        let bundle_sha256 = release_profile::sha256(&read_bounded(
            &bundle,
            self.context.limits.max_material_bytes,
        )?);
        let receipt = ReleaseTransport {
            schema: "derived-release-transport/1".into(),
            binding: self.binding.clone(),
            generation_tools: self.generation_tools.clone(),
            bundle_sha256,
            preparation_records: self.context.records.clone(),
            tool_setup_sha256: self.tool_setup_sha256.clone(),
            semver_checks_version: self.semver_checks_version.clone(),
            semver_checks_sha256: self.semver_checks_sha256.clone(),
            installer_definition_sha256: self.installer_definition_sha256.clone(),
        };
        fs::write(
            destination.join("transport.json"),
            serde_json::to_vec(&receipt)?,
        )?;
        let delta = destination.join("delta");
        fs::create_dir(&delta)?;
        let mut paths = vec!["Cargo.toml", "Cargo.lock", "CHANGELOG.md"];
        if self
            .root
            .join(crate::core::release_metadata::NATIVE_WORKER_LOCK)
            .try_exists()?
        {
            paths.push(crate::core::release_metadata::NATIVE_WORKER_LOCK);
        }
        for name in paths {
            let output = delta.join(name);
            fs::create_dir_all(
                output
                    .parent()
                    .ok_or_else(|| invalid("delta parent missing"))?,
            )?;
            fs::write(
                output,
                read_bounded(
                    &self.root.join(name),
                    self.context.limits.max_material_bytes,
                )?,
            )?;
        }
        self.export_evidence(&destination.join("evidence"))?;
        Ok(receipt)
    }
}
