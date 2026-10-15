//! Fixed authenticated compile-once producer/consumer chain. The large test tar
//! is measured by the source producer and again by each actual shard executor;
//! only bounded descriptor/helper/receipt bytes cross the final proof importer.
use super::*;
use provider::{
    artifact_archive::ArtifactFiles,
    source_execution::{MeasuredPreparation, SourceExpectation},
};
use serde_json::Value;
use source::custody::integration_custody as custody;
#[derive(Default)]
pub(super) struct Dependencies {
    pub inputs: BTreeMap<String, String>,
    prepared: Option<custody::PreparedDependencies>,
    receipts: BTreeMap<String, Vec<u8>>,
}
fn ensure(ok: bool, message: &str) -> io::Result<()> {
    if ok { Ok(()) } else { Err(invalid(message)) }
}
fn expected_context(project: &CurrentProject, source: &SourceExpectation) -> custody::Context {
    let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
    let shards = source::inventory(&paths, false)
        .into_iter()
        .filter(|j| j.job == "test-integration" && j.applicable)
        .filter_map(|j| j.matrix.map(str::to_string))
        .collect();
    custody::Context {
        event: project.facts.event.clone(),
        base: source.base.clone(),
        head: source.head.clone(),
        merge_base: source.base.clone(),
        validation_head: source.execution_revision.clone(),
        tree: source.tree.clone(),
        shards,
    }
}
fn source_bytes(path: &str) -> io::Result<&'static [u8]> {
    source::workflow_owned::SOURCES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, b)| *b)
        .ok_or_else(|| invalid("fixed integration recipe missing"))
}
fn produced_bytes<'a>(files: &'a ArtifactFiles, check: &str, path: &str) -> io::Result<&'a [u8]> {
    let prefix = format!("{check}/execution/");
    let manifest: Value = serde_json::from_slice(files.file(&format!("{prefix}execution.json"))?)
        .map_err(io::Error::other)?;
    let rows: Vec<_> = manifest["artifacts"]
        .as_array()
        .ok_or_else(|| invalid("verified artifact ledger absent"))?
        .iter()
        .filter(|a| a["source_path"] == path)
        .collect();
    ensure(
        rows.len() == 1,
        "required verified integration artifact absent/duplicate",
    )?;
    let member = rows[0]["path"]
        .as_str()
        .ok_or_else(|| invalid("verified artifact path absent"))?;
    let bytes = files.file(&format!("{prefix}{member}"))?;
    ensure(
        rows[0]["sha256"] == hash(bytes),
        "verified integration artifact bytes changed",
    )?;
    Ok(bytes)
}
pub(super) fn preparation(
    project: &CurrentProject,
    check: &source::Check,
    expected: &SourceExpectation,
    measured: &MeasuredPreparation<'_>,
    known: &Dependencies,
) -> io::Result<()> {
    let cx = expected_context(project, expected);
    let values: BTreeMap<String, String> =
        serde_json::from_value(measured.context.clone()).map_err(io::Error::other)?;
    for (key, value) in [
        ("CHANGE_BASE", cx.base.as_str()),
        ("CHANGE_HEAD", cx.head.as_str()),
        ("CUSTODY_EVENT", cx.event.as_str()),
        ("CUSTODY_BASE", cx.base.as_str()),
        ("CUSTODY_HEAD", cx.head.as_str()),
        ("CUSTODY_MERGE_BASE", cx.merge_base.as_str()),
        ("CUSTODY_VALIDATION_HEAD", cx.validation_head.as_str()),
        ("CUSTODY_TREE", cx.tree.as_str()),
    ] {
        ensure(
            values.get(key).map(String::as_str) == Some(value),
            "integration context differs from independently selected B/H/V/tree",
        )?;
    }
    let allowed = [
        "SOURCE_CI_EVENT",
        "SOURCE_CI_BASE",
        "SOURCE_CI_HEAD",
        "SOURCE_CI_MERGE_BASE",
        "SOURCE_CI_RELEASE_ONLY",
        "CHANGE_BASE",
        "CHANGE_HEAD",
        "CUSTODY_EVENT",
        "CUSTODY_BASE",
        "CUSTODY_HEAD",
        "CUSTODY_MERGE_BASE",
        "CUSTODY_VALIDATION_HEAD",
        "CUSTODY_TREE",
        "CUSTODY_SHARD",
        "CUSTODY_SHARDS",
        "CUSTODY_ARCHIVE_SHA256",
        "CUSTODY_HELPER_SHA256",
        "CUSTODY_PREPARED_CONTEXT",
        "CUSTODY_COUNT",
        "CUSTODY_PREPARE_RESULT",
        "CUSTODY_WORKER_RESULT",
    ];
    ensure(
        values.keys().all(|k| allowed.contains(&k.as_str())),
        "unknown integration effective context",
    )?;
    if check.id == "integration-prepare" {
        return Ok(());
    }
    let prepared = known
        .prepared
        .as_ref()
        .ok_or_else(|| invalid("authenticated integration preparation dependency missing"))?;
    let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
    prepared
        .verify_selection(&cx, &source::workflow_owned::integration_targets(&paths))
        .map_err(invalid)?;
    ensure(
        custody::Context::from_bindings(&values).map_err(invalid)? == cx,
        "integration worker context differs from authenticated preparation",
    )?;
    ensure(
        values.get("CUSTODY_ARCHIVE_SHA256") == Some(&prepared.archive_sha256)
            && values.get("CUSTODY_HELPER_SHA256") == Some(&prepared.helper_sha256),
        "integration archive/helper context is not source-produced",
    )?;
    ensure(
        measured.manifest["executor_digest"] == format!("sha256:{}", prepared.executor_sha256),
        "integration executor differs from authenticated producer bootstrap",
    )?;
    if check.job == "test-integration" {
        ensure(
            values.get("CUSTODY_SHARD").map(String::as_str) == check.matrix,
            "wrong actual integration shard",
        )?;
        let inputs: Vec<_> = measured
            .toolchain
            .as_array()
            .ok_or_else(|| invalid("toolchain missing"))?
            .iter()
            .filter(|r| {
                r["kind"] == "input_artifact"
                    && r["path"] == "target/integration-download/integration-tests.tar"
            })
            .collect();
        ensure(
            inputs.len() == 1,
            "measured actual shard input archive missing/duplicate",
        )?;
        let digest = inputs[0]["sha256"]
            .as_str()
            .and_then(|s| s.strip_prefix("sha256:"))
            .ok_or_else(|| invalid("actual shard archive digest missing"))?;
        let bytes = inputs[0]["bytes"]
            .as_u64()
            .ok_or_else(|| invalid("actual shard archive size missing"))?;
        prepared
            .verify_worker_input(
                digest,
                bytes,
                &prepared.helper_sha256,
                &prepared.executor_sha256,
            )
            .map_err(invalid)?;
    } else if check.id == "integration-status" {
        ensure(
            values.get("CUSTODY_COUNT") == Some(&cx.shards.len().to_string())
                && values.get("CUSTODY_PREPARED_CONTEXT") == Some(&cx.encode().map_err(invalid)?)
                && values.get("CUSTODY_PREPARE_RESULT").map(String::as_str) == Some("success")
                && values.get("CUSTODY_WORKER_RESULT").map(String::as_str)
                    == Some(if cx.shards.is_empty() {
                        "skipped"
                    } else {
                        "success"
                    }),
            "integration status selector/result context incomplete",
        )?;
        custody::verify_receipt_set(&cx, &prepared.archive_sha256, &known.receipts)
            .map_err(invalid)?;
    } else {
        return Err(invalid("unknown integration consumer"));
    }
    Ok(())
}
pub(super) fn record(
    project: &CurrentProject,
    check: &source::Check,
    expected: &SourceExpectation,
    files: &ArtifactFiles,
    known: &mut Dependencies,
) -> io::Result<()> {
    if check.id == "integration-prepare" {
        let descriptor = produced_bytes(
            files,
            check.id,
            "target/integration-custody-artifacts/preparation.v1",
        )?;
        let prepared = custody::PreparedDependencies::parse(descriptor).map_err(invalid)?;
        let context = expected_context(project, expected);
        let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
        prepared
            .verify_selection(
                &context,
                &source::workflow_owned::integration_targets(&paths),
            )
            .map_err(invalid)?;
        let helper = produced_bytes(
            files,
            check.id,
            "target/integration-custody-artifacts/integration-custody",
        )?;
        let preparation: Value = serde_json::from_slice(
            files.file(&format!("{}/preparation/preparation.json", check.id))?,
        )
        .map_err(io::Error::other)?;
        ensure(
            hash(helper) == format!("sha256:{}", prepared.helper_sha256)
                && preparation["executor_digest"] == format!("sha256:{}", prepared.executor_sha256),
            "integration producer helper/executor hash mismatch",
        )?;
        ensure(
            hash(source_bytes(
                "assets/tools/source_ci/integration-worker.sh",
            )?) == format!("sha256:{}", prepared.worker_script_sha256)
                && hash(source_bytes(
                    "assets/tools/source_ci/integration-status.sh",
                )?) == format!("sha256:{}", prepared.status_script_sha256),
            "integration bootstrap scripts differ from compiled source",
        )?;
        known.inputs.insert(
            "target/integration-custody/integration-custody".into(),
            hash(helper),
        );
        if !context.shards.is_empty() {
            known.inputs.insert(
                "target/integration-download/integration-tests.tar".into(),
                format!("sha256:{}", prepared.archive_sha256),
            );
        }
        ensure(
            known.prepared.replace(prepared).is_none(),
            "duplicate integration preparation producer",
        )?;
    } else if check.job == "test-integration" {
        let prepared = known
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("authenticated integration preparation missing"))?;
        let shard = check
            .matrix
            .ok_or_else(|| invalid("integration shard missing"))?;
        let path = format!("target/integration-receipts/{shard}.receipt");
        let bytes = produced_bytes(files, check.id, &path)?;
        custody::verify_receipt_bytes(&prepared.context, &prepared.archive_sha256, shard, bytes)
            .map_err(invalid)?;
        known.inputs.insert(path, hash(bytes));
        ensure(
            known
                .receipts
                .insert(format!("{shard}.receipt"), bytes.to_vec())
                .is_none(),
            "duplicate actual shard receipt",
        )?;
    }
    Ok(())
}
pub(super) fn required_inputs(
    project: &CurrentProject,
    check: &source::Check,
    expected: &SourceExpectation,
) -> BTreeSet<String> {
    if source::native_vega::is_check(check.id) {
        return source::native_vega::input_files(check)
            .into_iter()
            .collect();
    }
    if check.id == "integration-status" {
        let mut paths =
            BTreeSet::from(["target/integration-custody/integration-custody".to_string()]);
        for shard in expected_context(project, expected).shards {
            paths.insert(format!("target/integration-receipts/{shard}.receipt"));
        }
        return paths;
    }
    let paths: BTreeSet<_> =
        source::workflow_owned::input_files(check, &project.identity_inputs.root)
            .into_iter()
            .collect();
    paths
}
