//! Aggregate only the fixed workflow artifact layout. The CI provider/importer
//! independently authenticates artifact run/attempt/producer custody.
use decapod::core::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    derived_release_driver::{self, ExpectedTransport},
    derived_release_profile::{self, FileMaterial, Snapshot},
};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Component, Path},
    process::Command,
    time::Duration,
};
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn read(path: &Path) -> io::Result<Vec<u8>> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.file_type().is_symlink() || m.len() > 512 * 1024 * 1024 {
        return Err(invalid("invalid bounded artifact member"));
    }
    fs::read(path)
}
fn env(key: &str) -> io::Result<String> {
    std::env::var(key).map_err(|_| invalid(format!("missing trusted {key}")))
}
fn git(args: &[&str]) -> io::Result<Vec<u8>> {
    let result = Command::new("git")
        .args([
            "--no-replace-objects",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_LAZY_FETCH", "1")
        .bounded_capture(
            ProcessLimits {
                timeout: Duration::from_secs(30),
                max_output_bytes: 16 * 1024 * 1024,
            },
            &Cancellation::default(),
        )?;
    if result.termination != Termination::Exited || !result.output.status.success() {
        return Err(invalid("immutable source Git read failed"));
    }
    Ok(result.output.stdout)
}
fn parse_attempt(raw: &str) -> io::Result<u32> {
    let attempt: u32 = raw.parse().map_err(|_| invalid("invalid run attempt"))?;
    if attempt == 0 || attempt.to_string() != raw {
        return Err(invalid("run attempt must be canonical and nonzero"));
    }
    Ok(attempt)
}
fn verify_attempt_context(bytes: &[u8], attempt: u32, run_id: &str) -> io::Result<()> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
    if value.get("run_attempt").and_then(|v| v.as_str()) != Some(attempt.to_string().as_str())
        || value.get("run_id").and_then(|v| v.as_str()) != Some(run_id)
    {
        return Err(invalid(
            "release artifact belongs to another workflow run/attempt",
        ));
    }
    Ok(())
}
pub fn run(directory: &Path) -> io::Result<()> {
    let attempt = parse_attempt(&env("GITHUB_RUN_ATTEMPT")?)?;
    let run_id = env("GITHUB_RUN_ID")?;
    let h = env("SOURCE_HEAD")?;
    let b = env("ACCEPTED_BASE")?;
    let tree = env("SOURCE_TREE")?;
    let date = env("RELEASE_DATE")?;
    let contract = env("SETUP_CONTRACT_SHA256")?;
    if String::from_utf8_lossy(&git(&["rev-parse", "HEAD"])?).trim() != h {
        return Err(invalid("aggregate source changed"));
    }
    let profile = derived_release_profile::profile(
        &fs::read_to_string("Cargo.toml")?,
        &fs::read_to_string(".github/release.toml")?,
        &fs::read_to_string("dist-workspace.toml")?,
        &fs::read_to_string("dist.toml")?,
    )
    .map_err(invalid)?;
    let mut before = Snapshot::new();
    let listing = git(&["ls-files", "--stage", "-z"])?;
    let mut total = 0;
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry).map_err(|_| invalid("non-UTF-8 tracked path"))?;
        let (metadata, path) = entry
            .split_once('\t')
            .ok_or_else(|| invalid("invalid index entry"))?;
        if !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(invalid("unsafe tracked path"));
        }
        let mode = u32::from_str_radix(metadata.split(' ').next().unwrap_or(""), 8)
            .map_err(|_| invalid("invalid index mode"))?;
        let bytes = match mode {
            0o100644 | 0o100755 => read(Path::new(path))?,
            0o120000 => fs::read_link(path)?.to_string_lossy().as_bytes().to_vec(),
            _ => return Err(invalid("unsupported index mode")),
        };
        total += bytes.len();
        if total > 512 * 1024 * 1024 {
            return Err(invalid("source material budget exceeded"));
        }
        before.insert(path.into(), FileMaterial { mode, bytes });
    }
    let expected = ExpectedTransport {
        source_commit: &h,
        source_git_tree: &tree,
        accepted_base: &b,
        release_date: &date,
        setup_contract_sha256: &contract,
    };
    let prepare_root = directory.join(format!("derived-release-prepare-{attempt}"));
    let foundation_root = directory.join(format!("derived-release-foundation-{attempt}"));
    for root in [&prepare_root, &foundation_root] {
        verify_attempt_context(&read(&root.join("context.json"))?, attempt, &run_id)?;
    }
    let preparation = prepare_root.join("prepare/transport");
    let (receipt, generated) =
        derived_release_driver::verify_exported_transport(&profile, &before, &expected, |name| {
            read(&preparation.join(name))
        })?;
    let mut reports = Vec::new();
    let mut artifacts = BTreeMap::new();
    let mut stages: Vec<_> = ["plan", "package", "global", "candidate"]
        .iter()
        .map(|s| (foundation_root.join(s), None))
        .collect();
    for target in &profile.targets {
        let root = directory.join(format!("derived-release-{target}-{attempt}"));
        verify_attempt_context(&read(&root.join("context.json"))?, attempt, &run_id)?;
        stages.push((root.join("local"), Some(target)));
    }
    for (root, target) in &stages {
        let report = derived_release_driver::verify_exported_stage(
            &profile,
            &receipt,
            &generated,
            &read(&root.join("derived-release-proof.json"))?,
            |name| read(&root.join(name)),
        )?;
        if let Some(target) = target
            && !matches!(&report.stage,derived_release_driver::Stage::Local{target:t} if t==*target)
        {
            return Err(invalid("target artifact producer mismatch"));
        }
        for (name, digest) in &report.artifacts {
            if artifacts.insert(name.clone(), digest.clone()).is_some() {
                return Err(invalid("duplicate aggregate artifact"));
            }
        }
        reports.push(report);
    }
    let manifest = read(&foundation_root.join("plan/evidence/plan-dist-manifest.json"))?;
    let inventory = derived_release_profile::verify_dist_plan(&profile, &generated, &manifest)
        .map_err(invalid)?;
    derived_release_driver::verify_complete_reports(
        &profile,
        &generated,
        &inventory,
        &h,
        &receipt.binding.derived_commit,
        &reports,
    )?;
    let value = serde_json::json!({"schema":"derived-release-aggregate/1","run_id":run_id,"run_attempt":attempt,"binding":receipt.binding,"generated":generated,"reports":reports,"artifacts":artifacts});
    let bytes = serde_json::to_vec(&value)?;
    fs::write(directory.join("derived-release-aggregate.json"), &bytes)?;
    println!("{}", String::from_utf8_lossy(&bytes));
    Ok(())
}

#[cfg(test)]
mod attempt_tests {
    use super::*;
    #[test]
    fn attempts_are_canonical_bounded_and_nonzero() {
        for raw in ["", "0", "01", "-1", "+1", "4294967296", "1/../2"] {
            assert!(parse_attempt(raw).is_err(), "{raw}");
        }
        assert_eq!(parse_attempt("1").unwrap(), 1);
        assert_eq!(parse_attempt("4294967295").unwrap(), u32::MAX);
    }
    #[test]
    fn previous_attempt_cannot_satisfy_current_custody() {
        let first = br#"{"run_id":"42","run_attempt":"1"}"#;
        let second = br#"{"run_id":"42","run_attempt":"2"}"#;
        assert!(verify_attempt_context(first, 1, "42").is_ok());
        assert!(verify_attempt_context(first, 2, "42").is_err());
        assert!(verify_attempt_context(second, 2, "42").is_ok());
        assert!(verify_attempt_context(second, 2, "43").is_err());
        assert!(verify_attempt_context(br#"{"run_id":"42","run_attempt":2}"#, 2, "42").is_err());
    }
}
