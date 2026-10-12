//! Strict decoder for independently provider-authenticated artifact members.
//! Authentication, producer identity and archive bounds are caller obligations;
//! an arbitrary local JSON report must never enter this API as trusted custody.
use super::*;
use serde::de::DeserializeOwned;
pub struct ExpectedTransport<'a> {
    pub source_commit: &'a str,
    pub source_git_tree: &'a str,
    pub accepted_base: &'a str,
    pub release_date: &'a str,
    pub setup_contract_sha256: &'a str,
}
fn parse<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("oversize release evidence JSON"));
    }
    serde_json::from_value(checked(release_profile::unique_json(bytes))?)
        .map_err(|e| invalid(e.to_string()))
}
fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn verify_records(
    records: &[CommandRecord],
    get: &mut impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<()> {
    if records.is_empty() {
        return Err(invalid("missing measured command records"));
    }
    let mut total = 0usize;
    for row in records {
        if row.exit_code != 0
            || !hash(&row.program_sha256)
            || row.arguments.is_empty()
            || !hash(&row.environment_sha256)
            || !Path::new(&row.cwd).is_absolute()
        {
            return Err(invalid("failed/incomplete command record"));
        }
        if row.output_policy
            != if row.arguments == ["session", "status"] {
                "session-status-first-line"
            } else {
                "raw"
            }
        {
            return Err(invalid("unreviewed command output filtering"));
        }
        for (suffix, digest) in [
            ("stdout", &row.stdout_sha256),
            ("stderr", &row.stderr_sha256),
        ] {
            if !hash(digest) {
                return Err(invalid("invalid log digest"));
            }
            let bytes = get(&format!("evidence/logs/{digest}.{suffix}"))?;
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| invalid("log budget overflow"))?;
            if total > 512 * 1024 * 1024 || release_profile::sha256(&bytes) != *digest {
                return Err(invalid("missing/altered measured command log"));
            }
        }
    }
    Ok(())
}
pub fn verify_exported_transport(
    profile: &ReleaseProfile,
    before: &Snapshot,
    expected: &ExpectedTransport<'_>,
    mut get: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<(ReleaseTransport, GeneratedRelease)> {
    let receipt: ReleaseTransport = parse(&get("transport.json")?)?;
    let b = &receipt.binding;
    if receipt.schema != "derived-release-transport/1"
        || b.source_commit != expected.source_commit
        || b.source_git_tree != expected.source_git_tree
        || b.accepted_base_commit != expected.accepted_base
        || b.release_date != expected.release_date
        || receipt
            .generation_tools
            .values()
            .any(|t| t.production_resolution_sha256 != expected.setup_contract_sha256)
        || receipt.bundle_sha256 != release_profile::sha256(&get("derived.bundle")?)
    {
        return Err(invalid("preparation source/setup/bundle binding mismatch"));
    }
    verify_records(&receipt.preparation_records, &mut get)?;
    for (key, tool) in &receipt.generation_tools {
        let args: Vec<String> = if *key == Tool::CargoDist {
            vec!["dist".into(), "--version".into()]
        } else {
            vec!["--version".into()]
        };
        let row = receipt
            .preparation_records
            .iter()
            .find(|r| {
                r.program_sha256 == tool.executable_sha256
                    && r.arguments == args
                    && get(&format!("evidence/logs/{}.stdout", r.stdout_sha256))
                        .ok()
                        .and_then(|v| String::from_utf8(v).ok())
                        .is_some_and(|v| {
                            v.split_whitespace()
                                .nth(if *key == Tool::Git { 2 } else { 1 })
                                == Some(tool.actual_version.as_str())
                        })
            })
            .ok_or_else(|| {
                invalid("generation tool identity is not backed by measured version output")
            })?;
        if row.output_policy != "raw" {
            return Err(invalid("generation tool observation filtered"));
        }
    }
    let expected_update = checked(profile.generation_command(expected.release_date))?.args;
    if !receipt.preparation_records.iter().any(|r| {
        r.arguments.starts_with(&expected_update)
            && r.arguments.len() == expected_update.len() + 2
            && r.arguments[expected_update.len()] == "--repo-url"
    }) {
        return Err(invalid("actual release-plz update execution missing"));
    }
    let mut after = before.clone();
    for name in ["Cargo.toml", "Cargo.lock", "CHANGELOG.md"] {
        after.insert(
            name.into(),
            FileMaterial {
                mode: 0o100644,
                bytes: get(&format!("delta/{name}"))?,
            },
        );
    }
    let generated = checked(release_profile::verify_generation(
        profile,
        before,
        &after,
        b,
        &receipt.generation_tools,
    ))?;
    Ok((receipt, generated))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Execution {
    schema: String,
    generation_binding: GenerationBinding,
    generated: GeneratedRelease,
    tool_setup_sha256: String,
    preparation_records: Vec<CommandRecord>,
    stage: StageReport,
}
pub fn verify_exported_stage(
    profile: &ReleaseProfile,
    receipt: &ReleaseTransport,
    generated: &GeneratedRelease,
    proof_bytes: &[u8],
    mut get: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<StageReport> {
    let proof: Execution = parse(proof_bytes)?;
    if proof.schema != "derived-release-execution/2"
        || proof.generation_binding != receipt.binding
        || proof.generated != *generated
        || proof.stage.source_commit != receipt.binding.source_commit
        || proof.stage.derived_commit != receipt.binding.derived_commit
        || proof.stage.binding_sha256 != generated.binding_sha256
        || proof.tool_setup_sha256 != proof.stage.tool_setup_sha256
        || !hash(&proof.tool_setup_sha256)
    {
        return Err(invalid(
            "stage differs from transported source/derived/setup binding",
        ));
    }
    let setup = &proof.stage.worker_setup;
    checked(release_profile::verify_tools(profile, &setup.tools))?;
    if release_profile::sha256(&serde_json::to_vec(setup)?) != proof.tool_setup_sha256
        || setup.semver_checks_version != receipt.semver_checks_version
        || setup.installer_definition_sha256 != receipt.installer_definition_sha256
        || !hash(&setup.semver_checks_sha256)
        || !hash(&setup.curl_sha256)
        || !hash(&setup.xz_sha256)
        || setup.tools.iter().any(|(key, tool)| {
            receipt.generation_tools.get(key).is_none_or(|original| {
                tool.production_resolution_sha256 != original.production_resolution_sha256
                    || (*key != Tool::Git && tool.actual_version != original.actual_version)
            })
        })
    {
        return Err(invalid(
            "worker tool setup differs from source/shared-installer contract",
        ));
    }
    for (key, tool) in &setup.tools {
        let arguments: Vec<String> = if *key == Tool::CargoDist {
            vec!["dist".into(), "--version".into()]
        } else {
            vec!["--version".into()]
        };
        let record = proof
            .preparation_records
            .iter()
            .find(|r| r.program_sha256 == tool.executable_sha256 && r.arguments == arguments)
            .ok_or_else(|| invalid("worker tool executable observation missing"))?;
        let output = get(&format!("evidence/logs/{}.stdout", record.stdout_sha256))?;
        let text =
            std::str::from_utf8(&output).map_err(|_| invalid("tool version output not UTF-8"))?;
        let measured = text
            .split_whitespace()
            .nth(if *key == Tool::Git { 2 } else { 1 });
        if measured != Some(tool.actual_version.as_str()) {
            return Err(invalid("worker measured tool version mismatch"));
        }
    }
    let semver = proof
        .preparation_records
        .iter()
        .find(|r| r.program_sha256 == setup.semver_checks_sha256 && r.arguments == ["--version"])
        .ok_or_else(|| invalid("worker semver-checks observation missing"))?;
    let semver_output = get(&format!("evidence/logs/{}.stdout", semver.stdout_sha256))?;
    if version(&semver_output, "cargo-semver-checks")? != setup.semver_checks_version {
        return Err(invalid("worker semver-checks measured version mismatch"));
    }
    verify_records(&proof.preparation_records, &mut get)?;
    verify_records(&proof.stage.command_records, &mut get)?;
    let cargo = &setup.tools[&Tool::Cargo].executable_sha256;
    let plan_args = vec![
        "dist".into(),
        "plan".into(),
        "--tag".into(),
        generated.tag.clone(),
        "--output-format=json".into(),
    ];
    let executed = |args: &Vec<String>| {
        proof
            .stage
            .command_records
            .iter()
            .any(|r| &r.program_sha256 == cargo && r.arguments == *args)
    };
    if !executed(&plan_args) {
        return Err(invalid("real cargo-dist plan execution missing"));
    }
    let required = match &proof.stage.stage {
        Stage::Plan => None,
        Stage::Package => Some(vec![
            "package".into(),
            "--locked".into(),
            "--package".into(),
            profile.package.clone(),
        ]),
        Stage::Local { target } => Some(checked(
            profile.artifact_build_arguments(Some(target), Some(&generated.tag)),
        )?),
        Stage::Global => Some(checked(
            profile.artifact_build_arguments(None, Some(&generated.tag)),
        )?),
        Stage::Candidate => Some(
            [
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
            .collect(),
        ),
    };
    if required.as_ref().is_some_and(|args| !executed(args)) {
        return Err(invalid("actual required release stage command missing"));
    }
    let plan_bytes = get("evidence/plan-dist-manifest.json")?;
    if !proof.stage.command_records.iter().any(|r| {
        r.arguments == plan_args && r.stdout_sha256 == release_profile::sha256(&plan_bytes)
    }) {
        return Err(invalid("plan manifest differs from actual command output"));
    }
    let inventory = checked(release_profile::verify_dist_plan(
        profile,
        generated,
        &plan_bytes,
    ))?;
    let expected: BTreeSet<String> = match &proof.stage.stage {
        Stage::Plan => BTreeSet::new(),
        Stage::Package => {
            BTreeSet::from([format!("{}-{}.crate", profile.package, generated.version)])
        }
        Stage::Candidate => BTreeSet::from(["candidate-health.json".into()]),
        Stage::Global => BTreeSet::from([
            inventory.source_archive.clone(),
            format!("{}.sha256", inventory.source_archive),
        ]),
        Stage::Local { target } => {
            let name = inventory
                .target_archives
                .get(target)
                .ok_or_else(|| invalid("unknown local target"))?;
            BTreeSet::from([name.clone(), format!("{name}.sha256")])
        }
    };
    if proof
        .stage
        .artifacts
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>()
        != expected
    {
        return Err(invalid("stage artifact inventory mismatch"));
    }
    for (name, digest) in &proof.stage.artifacts {
        if !relative(name)
            || Path::new(name).components().count() != 1
            || !hash(digest)
            || release_profile::sha256(&get(&format!("release-artifacts/{name}"))?) != *digest
        {
            return Err(invalid("actual stage artifact hash mismatch"));
        }
    }
    if proof.stage.stage == Stage::Candidate {
        let value = checked(release_profile::unique_json(&get(
            "release-artifacts/candidate-health.json",
        )?))?;
        for (field, expected) in [
            ("source_commit", receipt.binding.source_commit.as_str()),
            ("derived_commit", receipt.binding.derived_commit.as_str()),
            (
                "accepted_base",
                receipt.binding.accepted_base_commit.as_str(),
            ),
            ("derived_tree", receipt.binding.derived_git_tree.as_str()),
            (
                "derived_material_sha256",
                receipt.binding.derived_material_sha256.as_str(),
            ),
            (
                "configured_base_commit",
                receipt.binding.source_commit.as_str(),
            ),
            ("candidate_version", generated.version.as_str()),
        ] {
            if value.get(field).and_then(|v| v.as_str()) != Some(expected) {
                return Err(invalid("candidate health identity mismatch"));
            }
        }
        for field in [
            "projected_material_sha256",
            "candidate_sha256",
            "projection_helper_sha256",
            "governance_sha256",
        ] {
            if !value.get(field).and_then(|v| v.as_str()).is_some_and(hash) {
                return Err(invalid("candidate health material measurement missing"));
            }
        }
        if !value
            .get("projected_tree")
            .and_then(|v| v.as_str())
            .is_some_and(oid)
            || !value
                .get("tracked_material")
                .is_some_and(|v| v.is_object() && !v.as_object().unwrap().is_empty())
        {
            return Err(invalid("candidate projection output identity missing"));
        }
        let candidate = value["candidate_sha256"].as_str().unwrap();
        let helper = value["projection_helper_sha256"].as_str().unwrap();
        let commands = &proof.stage.command_records;
        let position = |args: &[&str]| {
            commands
                .iter()
                .position(|r| r.program_sha256 == candidate && r.arguments == args)
        };
        let capability = position(&["capabilities", "--format", "json"])
            .ok_or_else(|| invalid("candidate identity execution missing"))?;
        let session = position(&["session", "status"])
            .ok_or_else(|| invalid("candidate session gate missing"))?;
        let project = position(&["govern", "project-check", "--refresh-specs"])
            .ok_or_else(|| invalid("candidate project/material execution missing"))?;
        let watcher = position(&["govern", "watcher", "run"])
            .ok_or_else(|| invalid("candidate watcher missing"))?;
        let health = position(&["govern", "health", "summary"])
            .ok_or_else(|| invalid("candidate health summary missing"))?;
        let checks: Vec<_> = commands
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.program_sha256 == helper
                    && r.arguments.len() == 8
                    && r.arguments[0] == "--repo"
                    && r.arguments[2..]
                        == [
                            "--event",
                            "pull_request",
                            "--base",
                            receipt.binding.source_commit.as_str(),
                            "--head",
                            receipt.binding.derived_commit.as_str(),
                        ]
            })
            .map(|(i, _)| i)
            .collect();
        if checks.len() != 2
            || !(capability < session
                && session < project
                && project < checks[0]
                && checks[0] < watcher
                && watcher < health
                && health < checks[1])
        {
            return Err(invalid(
                "candidate gates/projections missing or out of order",
            ));
        }
        let status = get(&format!(
            "evidence/logs/{}.stdout",
            commands[session].stdout_sha256
        ))?;
        if status != b"Session active\n" {
            return Err(invalid("candidate session inactive"));
        }
        let caps = checked(release_profile::unique_json(&get(&format!(
            "evidence/logs/{}.stdout",
            commands[capability].stdout_sha256
        ))?))?;
        if caps.get("version").and_then(|v| v.as_str()) != Some(generated.version.as_str()) {
            return Err(invalid("candidate measured capability version mismatch"));
        }
        let summary = checked(release_profile::unique_json(&get(&format!(
            "evidence/logs/{}.stdout",
            commands[health].stdout_sha256
        ))?))?;
        if summary.get("pending_approvals").and_then(|v| v.as_u64()) != Some(0)
            || summary.get("watcher_stale").and_then(|v| v.as_bool()) != Some(false)
            || !summary.get("health_summary").is_some_and(|v| v.is_object())
            || ["CONTRADICTED", "STALE"].iter().any(|key| {
                summary
                    .get("health_summary")
                    .and_then(|v| v.get(key))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    != 0
            })
        {
            return Err(invalid("measured candidate health is not ready"));
        }
    }
    Ok(proof.stage)
}
