//! Nonrecursive candidate health: projections are produced in a third owned
//! clone. The immutable D used for archives/packages is never refreshed.
use super::*;
impl PreparedRelease<'_> {
    pub(super) fn candidate_health(&mut self) -> io::Result<BTreeMap<String, String>> {
        let root = self.storage.path().join("health-candidate");
        let git = self.context.inputs.tools.git.clone();
        self.context.run(
            &git,
            &[
                "-c".into(),
                "protocol.file.allow=always".into(),
                "clone".into(),
                "--no-local".into(),
                "--no-hardlinks".into(),
                "--no-checkout".into(),
                "--".into(),
                self.root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ],
            self.storage.path(),
        )?;
        self.context.git(&root, &["remote", "remove", "origin"])?;
        self.context.git(
            &root,
            &["checkout", "--detach", &self.binding.derived_commit],
        )?;
        let config: toml::Value = toml::from_str(&text(read_bounded(
            &root.join(".decapod/config.toml"),
            1024 * 1024,
        )?)?)
        .map_err(|e| invalid(e.to_string()))?;
        let branch = config
            .get("repo")
            .and_then(|v| v.get("base_branch"))
            .and_then(|v| v.as_str())
            .filter(|b| !b.is_empty())
            .ok_or_else(|| invalid("candidate health requires explicit configured base branch"))?;
        let base_ref = format!("refs/remotes/origin/{branch}");
        self.context.git(&root, &["check-ref-format", &base_ref])?;
        self.context.git(
            &root,
            &["update-ref", &base_ref, &self.binding.source_commit],
        )?;
        if checked(release_profile::snapshot_digest(&snapshot(
            &mut self.context,
            &root,
        )?))?
            != self.binding.derived_material_sha256
        {
            return Err(invalid("health clone differs from immutable derivative"));
        }
        let cargo = self.context.inputs.tools.cargo.clone();
        self.context.run(
            &cargo,
            &[
                "build",
                "--locked",
                "--features",
                "build-tools",
                "--bin",
                "decapod",
                "--bin",
                "decapod-release-projection-drift",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
            &root,
        )?;
        let candidate = root.join("target/debug/decapod").canonicalize()?;
        let helper = root
            .join("target/debug/decapod-release-projection-drift")
            .canonicalize()?;
        let capabilities = self.context.run(
            &candidate,
            &["capabilities".into(), "--format".into(), "json".into()],
            &root,
        )?;
        let capability_value = checked(release_profile::unique_json(&capabilities))?;
        let measured = capability_value
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| invalid("candidate capabilities missing release version"))?
            .to_owned();
        if measured != self.generated.version {
            return Err(invalid("candidate binary version differs from D"));
        }
        let environment = self.context.environment.clone();
        for (key, value) in [
            ("CHANGE_EVENT", "pull_request"),
            ("CHANGE_BASE", self.binding.source_commit.as_str()),
            ("CHANGE_HEAD", self.binding.derived_commit.as_str()),
            ("DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT", "1"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "120"),
            ("DECAPOD_CLOUD_AUTH_MODE", "mock"),
            ("CI", "true"),
        ] {
            self.context.environment.insert(key.into(), value.into());
        }
        let check = |this: &mut Self| -> io::Result<()> {
            this.context.run(
                &helper,
                &[
                    "--repo".into(),
                    root.to_string_lossy().into_owned(),
                    "--event".into(),
                    "pull_request".into(),
                    "--base".into(),
                    this.binding.source_commit.clone(),
                    "--head".into(),
                    this.binding.derived_commit.clone(),
                ],
                &root,
            )?;
            Ok(())
        };
        // The immutable H->D delta was verified before this clone. Projections
        // are checked only after project-check regenerates them for D.
        let run_id = format!("derived-release-{}", &self.binding.derived_commit[..16]);
        self.context.run(
            &candidate,
            &[
                "govern",
                "trajectory",
                "init",
                "--run-id",
                &run_id,
                "--original-intent",
                "Validate the derived Decapod release",
                "--derived-intent",
                "Run release project prerequisites and exact projection verification",
                "--boundary",
                "Disposable derived-release validation checkout",
                "--scope",
                ".decapod/governance.json",
                "--destination",
                "CI derived release validation",
                "--phase",
                "validation",
                "--next-transition",
                "publish",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
            &root,
        )?;
        let status = self
            .context
            .run(&candidate, &["session".into(), "status".into()], &root)?;
        if status != b"Session active\n" {
            return Err(invalid("candidate auto-acquired session is not active"));
        }
        // Explicit session acquire can self-update via cargo install. Existing
        // command auto-acquisition is the safe production standalone-CI path.
        self.context.run(
            &candidate,
            &["govern", "project-check", "--refresh-specs"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            &root,
        )?;
        check(self)?;
        for args in [
            ["govern", "watcher", "run"],
            ["govern", "health", "summary"],
        ] {
            let output = self.context.run(
                &candidate,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                &root,
            )?;
            if args[1] == "health" {
                let value: serde_json::Value =
                    serde_json::from_slice(&output).map_err(|e| invalid(e.to_string()))?;
                if value.get("pending_approvals").and_then(|v| v.as_u64()) != Some(0)
                    || value.get("watcher_stale").and_then(|v| v.as_bool()) != Some(false)
                    || ["CONTRADICTED", "STALE"].iter().any(|key| {
                        value
                            .get("health_summary")
                            .and_then(|v| v.get(key))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            != 0
                    })
                    || !value.get("health_summary").is_some_and(|v| v.is_object())
                {
                    return Err(invalid("candidate health summary is not ready"));
                }
            }
        }
        check(self)?;
        self.context.environment = environment;
        let projected = snapshot(&mut self.context, &root)?;
        let projected_material_sha256 = checked(release_profile::snapshot_digest(&projected))?;
        let index_tree = self.context.git(&root, &["write-tree"])?.trim().to_owned();
        if index_tree != self.binding.derived_git_tree {
            return Err(invalid("health mutated the candidate index"));
        }
        let alternate_index = self.storage.path().join("projected.index");
        fs::copy(root.join(".git/index"), &alternate_index)?;
        self.context.environment.insert(
            "GIT_INDEX_FILE".into(),
            alternate_index.to_string_lossy().into_owned(),
        );
        self.context.git(&root, &["add", "--update", "--", "."])?;
        let projected_tree = self.context.git(&root, &["write-tree"])?.trim().to_owned();
        self.context.environment.remove("GIT_INDEX_FILE");
        let material: BTreeMap<_, _> = projected
            .iter()
            .map(|(path, file)| {
                (
                    path.clone(),
                    serde_json::json!({"mode":file.mode,"sha256":release_profile::sha256(&file.bytes)}),
                )
            })
            .collect();
        let governance = read_bounded(
            &root.join(".decapod/governance.json"),
            self.context.limits.max_material_bytes,
        )?;
        let report = serde_json::json!({"schema":"derived-release-candidate-health/1","source_commit":self.binding.source_commit,
            "accepted_base":self.binding.accepted_base_commit,"derived_commit":self.binding.derived_commit,
            "derived_tree":self.binding.derived_git_tree,"derived_material_sha256":self.binding.derived_material_sha256,
            "projected_tree":projected_tree,"projected_material_sha256":projected_material_sha256,
            "configured_base_ref":base_ref,"configured_base_commit":self.binding.source_commit,"index_tree":index_tree,
            "candidate_version":measured,"candidate_sha256":release_profile::sha256(&read_bounded(&candidate,self.context.limits.max_material_bytes)?),
            "projection_helper_sha256":release_profile::sha256(&read_bounded(&helper,self.context.limits.max_material_bytes)?),
            "governance_sha256":release_profile::sha256(&governance),"tracked_material":material,
            "session_mode":"existing-command-auto-acquire; no self-updating explicit session command"});
        let bytes = serde_json::to_vec(&report)?;
        fs::write(self.storage.path().join("candidate-health.json"), &bytes)?;
        Ok(BTreeMap::from([(
            "candidate-health.json".into(),
            release_profile::sha256(&bytes),
        )]))
    }
}
