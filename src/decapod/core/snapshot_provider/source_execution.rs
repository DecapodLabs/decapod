//! Verify source-owned execution custody only after provider authentication of
//! archive bytes. A mandatory compiled preparation policy independently checks
//! declared versions/platform, effective context and transitive tool/input
//! custody. Measured values never authorize themselves.
use super::artifact_archive::ArtifactFiles;
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
/// Trusted executor identity constructed from the accepted executor's Git
/// objects and reviewed profile, never deserialized from source receipts.
#[derive(Clone, Debug)]
pub struct ExecutorExpectation {
    pub revision: String,
    pub tree: String,
    pub profile_digest: String,
}
#[derive(Clone, Debug)]
pub struct SourceExpectation {
    pub workflow: String,
    pub job: String,
    pub check: String,
    pub matrix: Option<String>,
    pub platform: String,
    pub base: String,
    pub head: String,
    pub snapshot: String,
    pub execution_revision: String,
    pub tree: String,
    pub provider: String,
    pub run_id: String,
    pub run_attempt: String,
    /// Authenticated native producer for Buildkite, including buildkite_gha.
    /// Imported shell receipts instead expose the independently expected logical job.
    pub job_id: String,
    pub step_key: String,
    pub profile_digest: String,
    /// Exact compact source-derived recipe array; never copied from a receipt.
    pub invocations: Vec<u8>,
    pub artifact_source_paths: BTreeSet<String>,
    pub execution_mode: String,
    /// None retains the single-root contract. Some requires independently bound
    /// dual-root preparation and execution; it never permits a legacy fallback.
    pub executor: Option<ExecutorExpectation>,
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| invalid("source custody field missing"))
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    required(v, key)?
        .as_str()
        .ok_or_else(|| invalid("source custody string malformed"))
}
fn count(v: &Value, key: &str) -> Result<u64> {
    required(v, key)?
        .as_u64()
        .ok_or_else(|| invalid("source custody count malformed"))
}
fn truth(v: &Value, key: &str, expected: bool) -> Result<()> {
    ensure(
        required(v, key)?.as_bool() == Some(expected),
        "source custody state flag mismatch",
    )
}
fn digest(v: &str) -> bool {
    v.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn value(bytes: &[u8]) -> Result<Value> {
    // This local wrapper rejects duplicate keys throughout manifests and recipes.
    struct Strict(Value);
    impl<'de> serde::Deserialize<'de> for Strict {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Strict;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("duplicate-free JSON")
                }
                fn visit_bool<E: serde::de::Error>(
                    self,
                    v: bool,
                ) -> std::result::Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
                    serde_json::Number::from_f64(v)
                        .map(|n| Strict(Value::Number(n)))
                        .ok_or_else(|| E::custom("nonfinite JSON"))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_string<E: serde::de::Error>(
                    self,
                    v: String,
                ) -> std::result::Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
                    Ok(Strict(Value::Null))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut a: A,
                ) -> std::result::Result<Strict, A::Error> {
                    let mut values = vec![];
                    while let Some(v) = a.next_element::<Strict>()? {
                        values.push(v.0);
                    }
                    Ok(Strict(values.into()))
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut a: A,
                ) -> std::result::Result<Strict, A::Error> {
                    let mut values = serde_json::Map::new();
                    while let Some((k, v)) = a.next_entry::<String, Strict>()? {
                        if values.insert(k, v.0).is_some() {
                            return Err(serde::de::Error::custom("duplicate JSON key"));
                        }
                    }
                    Ok(Strict(Value::Object(values)))
                }
            }
            d.deserialize_any(Visitor)
        }
    }
    ensure(bytes.len() <= 16 * 1024 * 1024, "source JSON exceeds bound")?;
    serde_json::from_slice::<Strict>(bytes)
        .map(|v| v.0)
        .map_err(|_| invalid("invalid or duplicate source custody JSON"))
}
fn prefix(e: &SourceExpectation) -> Result<String> {
    ensure(slug(&e.check), "source check path invalid")?;
    Ok(format!("{}/", e.check))
}
fn file<'a>(files: &'a ArtifactFiles, prefix: &str, path: &str) -> Result<&'a [u8]> {
    ensure(
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".."),
        "source member path unsafe",
    )?;
    files.file(&format!("{prefix}{path}"))
}
fn identity(v: &Value, e: &SourceExpectation) -> Result<()> {
    for (k, expected) in [
        ("workflow", e.workflow.as_str()),
        ("job", &e.job),
        ("check", &e.check),
        ("platform", &e.platform),
        ("base", &e.base),
        ("head", &e.head),
        ("snapshot", &e.snapshot),
        ("execution_revision", &e.execution_revision),
        ("provider", &e.provider),
        ("run_id", &e.run_id),
        ("run_attempt", &e.run_attempt),
        (
            "job_id",
            if e.provider == "buildkite_gha" {
                &e.job
            } else {
                &e.job_id
            },
        ),
        (
            "step_key",
            if e.provider == "buildkite_gha" {
                ""
            } else {
                &e.step_key
            },
        ),
        ("profile_digest", &e.profile_digest),
    ] {
        ensure(
            string(v, k)? == expected,
            "source custody does not match independent identity",
        )?;
    }
    ensure(
        required(v, "matrix")?
            == &serde_json::to_value(&e.matrix)
                .map_err(|_| invalid("invalid expected source matrix"))?
            && string(v, "invocation_digest")? == hash(&e.invocations),
        "source matrix/recipe mismatch",
    )?;
    truth(v, "authenticated", false)?;
    truth(v, "complete_pr_profile", false)?;
    Ok(())
}
fn execution_context(v: &Value, e: &SourceExpectation, final_execution: bool) -> Result<()> {
    let schema = match (e.executor.is_some(), final_execution) {
        (false, false) => "decapod.source-ci.preparation.v1",
        (false, true) => "decapod.source-ci.execution.v2",
        (true, false) => "decapod.source-ci.preparation.v2",
        (true, true) => "decapod.source-ci.execution.v3",
    };
    ensure(
        string(v, "schema")? == schema,
        "source custody schema mismatch",
    )?;
    match &e.executor {
        Some(executor) => {
            // Exact object equality rejects missing, unknown, mistyped, or
            // receipt-selected roots. Subject identity remains the independent
            // source expectation, including when executor and subject differ.
            let expected = serde_json::json!({
                "schema": "decapod.source-ci.dual-root.v1",
                "executor_revision": executor.revision,
                "executor_tree": executor.tree,
                "executor_profile_digest": executor.profile_digest,
                "subject_revision": e.execution_revision,
                "subject_tree": e.tree,
                "executor_closure_digest": executor.profile_digest,
            });
            ensure(
                required(v, "execution_context")? == &expected,
                "source dual-root context differs from independent executor/subject identity",
            )?;
            if final_execution {
                ensure(
                    string(v, "final_executor_closure_digest")? == executor.profile_digest,
                    "source final executor closure differs from independent profile",
                )?;
            } else {
                ensure(
                    v.get("final_executor_closure_digest").is_none(),
                    "source preparation cannot assert final executor closure",
                )?;
            }
        }
        None => ensure(
            v.get("execution_context").is_none()
                && v.get("final_executor_closure_digest").is_none(),
            "legacy source custody cannot assert dual-root context",
        )?,
    }
    Ok(())
}
fn stream(files: &ArtifactFiles, prefix: &str, log: &Value) -> Result<String> {
    truth(log, "truncated", false)?;
    let bytes = file(files, prefix, string(log, "path")?)?;
    ensure(
        count(log, "bytes")? == bytes.len() as u64
            && count(log, "stored_bytes")? == bytes.len() as u64
            && string(log, "sha256")? == hash(bytes)
            && string(log, "stored_sha256")? == hash(bytes),
        "source log bytes differ from measurement",
    )?;
    Ok(hash(bytes))
}
fn tools(
    files: &ArtifactFiles,
    prefix: &str,
    stem: &str,
    canonical: &[u8],
) -> Result<BTreeMap<String, String>> {
    let entries = value(canonical)?
        .as_array()
        .cloned()
        .ok_or_else(|| invalid("toolchain must be an array"))?;
    let logs = value(file(files, prefix, &format!("{stem}tool-logs.json"))?)?
        .as_array()
        .cloned()
        .ok_or_else(|| invalid("tool logs must be an array"))?;
    let mut actual = BTreeMap::new();
    let mut measured_logs = BTreeSet::new();
    for entry in &entries {
        // Tagged closure records are interpreted by the mandatory compiled policy,
        // never mistaken for direct --version probes (some also name a program).
        if entry.get("kind").is_some() {
            ensure(
                entry["kind"].as_str().is_some_and(|kind| !kind.is_empty()),
                "invalid toolchain closure kind",
            )?;
            continue;
        }
        let Some(program) = entry.get("program").and_then(Value::as_str) else {
            continue;
        };
        ensure(
            digest(string(entry, "executable_sha256")?)
                && actual
                    .insert(program.into(), string(entry, "executable_sha256")?.into())
                    .is_none(),
            "duplicate/invalid measured tool",
        )?;
        ensure(
            (entry["version_args"] == serde_json::json!(["--version"])
                || (program == "ajv" && entry["version_args"] == serde_json::json!(["help"])))
                && entry["exit_code"] == 0
                && entry["signal"].is_null(),
            "version probe failed or unsupported",
        )?;
        truth(entry, "success", true)?;
        let rows: Vec<_> = logs.iter().filter(|r| r["program"] == program).collect();
        ensure(rows.len() == 1, "tool log missing or ambiguous")?;
        let row = rows[0];
        for side in ["stdout", "stderr"] {
            let log = required(row, side)?;
            let measured = stream(files, prefix, log)?;
            ensure(
                entry[format!("{side}_sha256")] == measured
                    && entry[format!("{side}_bytes")] == log["bytes"],
                "tool version bytes not bound to canonical toolchain",
            )?;
        }
        measured_logs.insert(program.to_string());
    }
    let mut probes = BTreeSet::new();
    for row in &logs {
        if let Some(program) = row.get("program").and_then(Value::as_str) {
            ensure(
                row.get("probe").is_none() && measured_logs.contains(program),
                "unknown version log",
            )?;
        } else {
            let probe = string(row, "probe")?;
            ensure(
                !probe.is_empty() && probe.len() <= 256 && probes.insert(probe),
                "duplicate/invalid closure probe",
            )?;
            // Probe semantics/required inventory are checked by compiled policy;
            // raw bytes and truncation are checked here for every advertised probe.
            for side in ["stdout", "stderr"] {
                stream(files, prefix, required(row, side)?)?;
            }
        }
    }
    ensure(
        logs.len() == measured_logs.len() + probes.len(),
        "unexpected tool logs",
    )?;
    Ok(actual)
}
/// Values supplied to mandatory trusted policy. That policy must reject missing
/// nested tools/action/input custody; direct-version success alone is insufficient.
pub struct MeasuredPreparation<'a> {
    pub manifest: &'a Value,
    pub toolchain: &'a Value,
    pub context: &'a Value,
    pub files: &'a ArtifactFiles,
    pub member_prefix: &'a str,
}
#[derive(Debug)]
pub struct AuthenticatedPreparation {
    origin: String,
    archive_digest: String,
    expected: SourceExpectation,
    manifest: Vec<u8>,
    toolchain: Vec<u8>,
    context: Vec<u8>,
    tools: BTreeMap<String, String>,
    digest: String,
}
impl AuthenticatedPreparation {
    pub fn toolchain_digest(&self) -> String {
        hash(&self.toolchain)
    }
    pub fn context_digest(&self) -> String {
        hash(&self.context)
    }
    pub fn preparation_digest(&self) -> &str {
        &self.digest
    }
    pub fn expected(&self) -> &SourceExpectation {
        &self.expected
    }
}
/// The source worker names a workflow file; the authenticated importer names
/// its repository path. Join only a single canonical filename, never strip an
/// arbitrary observed path or infer expected identity from the provider plan.
fn source_workflow_path(name: &str) -> Result<String> {
    ensure(
        !name.is_empty()
            && name.len() <= 255
            && (name.ends_with(".yml") || name.ends_with(".yaml"))
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')),
        "source workflow must be a canonical filename",
    )?;
    Ok(format!(".github/workflows/{name}"))
}
/// Imported shell claims require the sealed provider-authenticated recipe, not
/// merely a UUID-shaped run ID or a caller-supplied standalone receipt.
pub fn verify_imported_preparation(
    files: &ArtifactFiles,
    expected: &SourceExpectation,
    recipe: &super::public_buildkite::ImportedRecipe,
    verify_policy: impl FnOnce(MeasuredPreparation<'_>) -> Result<()>,
) -> Result<AuthenticatedPreparation> {
    ensure(
        expected.provider == "buildkite_gha"
            && expected.run_attempt == "0"
            && recipe.logical_result() == "success",
        "unsupported imported producer/attempt/result",
    )?;
    // The current public inventory adapter rejects all retries. Do not derive
    // native attempt authority from the receipt or assume a later retry is zero.
    let result = value(recipe.result().bytes())?;
    let producer = required(&result, "producer")?;
    let plan = recipe.plan_json();
    let workflow = required(plan, "workflow")?;
    let workflow_path = string(workflow, "path")?;
    ensure(
        workflow_path.strip_prefix("./").unwrap_or(workflow_path)
            == source_workflow_path(&expected.workflow)?,
        "imported source workflow differs from authenticated recipe",
    )?;
    ensure(
        string(producer, "build_id")? == expected.run_id
            && string(producer, "job_id")? == expected.job_id
            && string(producer, "step_key")? == expected.step_key
            && recipe.result().job_id() == expected.job_id
            && string(workflow, "logical_job_id")? == expected.job
            && string(required(plan, "target")?, "step_key")? == expected.step_key
            && string(required(plan, "event")?, "sha")? == expected.snapshot,
        "imported source claim differs from authenticated native recipe producer",
    )?;
    verify_preparation_inner(files, expected, verify_policy)
}
pub fn verify_preparation(
    files: &ArtifactFiles,
    expected: &SourceExpectation,
    verify_policy: impl FnOnce(MeasuredPreparation<'_>) -> Result<()>,
) -> Result<AuthenticatedPreparation> {
    ensure(
        expected.provider != "buildkite_gha",
        "imported runtime receipt requires authenticated ImportedRecipe",
    )?;
    verify_preparation_inner(files, expected, verify_policy)
}
fn verify_preparation_inner(
    files: &ArtifactFiles,
    expected: &SourceExpectation,
    verify_policy: impl FnOnce(MeasuredPreparation<'_>) -> Result<()>,
) -> Result<AuthenticatedPreparation> {
    ensure(
        [
            &expected.base,
            &expected.head,
            &expected.snapshot,
            &expected.execution_revision,
            &expected.tree,
        ]
        .into_iter()
        .all(|s| oid(s))
            && digest(&expected.profile_digest),
        "invalid expected source objects/digest",
    )?;
    if let Some(executor) = &expected.executor {
        ensure(
            oid(&executor.revision)
                && oid(&executor.tree)
                && digest(&executor.profile_digest)
                && executor.profile_digest == expected.profile_digest,
            "invalid independent executor objects/profile digest",
        )?;
    }
    ensure(
        matches!(
            expected.execution_mode.as_str(),
            "source_checkout" | "authenticated_integration_artifacts"
        ),
        "unsupported source execution mode",
    )?;
    match expected.provider.as_str() {
        "buildkite" | "buildkite_gha" => {
            ensure(
                uuid(&expected.run_id)
                    && uuid(&expected.job_id)
                    && expected.run_attempt.parse::<u32>().is_ok()
                    && !expected.step_key.is_empty()
                    && files.origin().starts_with(&format!(
                        "buildkite:{}/jobs/{}/",
                        expected.run_id, expected.job_id
                    )),
                "Buildkite source receipt differs from authenticated native producer",
            )?;
        }
        "github" => {
            ensure(
                expected.run_id.parse::<u64>().is_ok_and(|n| n > 0)
                    && expected.run_attempt.parse::<u32>().is_ok_and(|n| n > 0)
                    && expected.job_id == expected.job
                    && expected.step_key.is_empty()
                    && files.origin().starts_with("github:")
                    && files.origin().contains(&format!(
                        "/runs/{}/attempts/{}/artifacts/",
                        expected.run_id, expected.run_attempt
                    )),
                "GitHub source receipt differs from authenticated run/attempt",
            )?;
        }
        _ => return Err(invalid("unsupported source execution provider")),
    }
    let recipe = value(&expected.invocations)?;
    ensure(
        recipe
            .as_array()
            .is_some_and(|v| !v.is_empty() && v.len() <= 1000),
        "empty/oversized independently expected source recipe",
    )?;
    let root = prefix(expected)?;
    let p = format!("{root}preparation/");
    let manifest = file(files, &p, "preparation.json")?;
    let v = value(manifest)?;
    execution_context(&v, expected, false)?;
    ensure(
        string(&v, "tree")? == expected.tree,
        "source preparation tree mismatch",
    )?;
    identity(&v, expected)?;
    truth(&v, "preparation_complete", true)?;
    let invocations = file(files, &p, "invocations.json")?;
    ensure(
        invocations == expected.invocations,
        "prepared recipe differs from independent source",
    )?;
    let canonical = file(files, &p, "toolchain.json")?;
    ensure(
        string(&v, "toolchain_digest")? == hash(canonical),
        "preparation toolchain digest mismatch",
    )?;
    let context = file(files, &p, "context.json")?;
    ensure(
        string(&v, "context_digest")? == hash(context) && digest(string(&v, "executor_digest")?),
        "preparation effective context/executor unbound",
    )?;
    let measured = tools(files, &p, "", canonical)?;
    let tc = value(canonical)?;
    let cx = value(context)?;
    verify_policy(MeasuredPreparation {
        manifest: &v,
        toolchain: &tc,
        context: &cx,
        files,
        member_prefix: &p,
    })?;
    Ok(AuthenticatedPreparation {
        origin: files.origin().into(),
        archive_digest: files.archive_digest().into(),
        expected: expected.clone(),
        manifest: manifest.into(),
        toolchain: canonical.into(),
        context: context.into(),
        tools: measured,
        digest: hash(manifest),
    })
}
fn raw_results(raw: &[u8]) -> Result<Vec<&[u8]>> {
    let needle = b"\"results\":[";
    let begin = raw
        .windows(needle.len())
        .position(|w| w == needle)
        .ok_or_else(|| invalid("canonical result array missing"))?
        + needle.len();
    let mut cursor = begin;
    let mut results = vec![];
    loop {
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if raw.get(cursor) == Some(&b']') {
            break;
        }
        ensure(raw.get(cursor) == Some(&b'{'), "result object missing")?;
        let start = cursor;
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        loop {
            let b = *raw
                .get(cursor)
                .ok_or_else(|| invalid("result object truncated"))?;
            cursor += 1;
            if quoted {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    quoted = false;
                }
            } else {
                match b {
                    b'"' => quoted = true,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth = depth
                            .checked_sub(1)
                            .ok_or_else(|| invalid("result nesting invalid"))?;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }
        results.push(&raw[start..cursor]);
        while raw.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        match raw.get(cursor) {
            Some(b',') => cursor += 1,
            Some(b']') => break,
            _ => return Err(invalid("invalid result array delimiter")),
        }
    }
    Ok(results)
}
#[derive(Debug)]
pub struct MeasuredArtifact {
    pub source_path: String,
    pub digest: String,
    pub bytes: u64,
}
#[derive(Debug)]
pub struct AuthenticatedExecution {
    pub(crate) output_digest: String,
    artifacts: Vec<MeasuredArtifact>,
    invocation_digest: String,
    toolchain_digest: String,
    preparation_digest: String,
    execution_digest: String,
}
impl AuthenticatedExecution {
    pub fn output_digest(&self) -> &str {
        &self.output_digest
    }
    pub fn artifacts(&self) -> &[MeasuredArtifact] {
        &self.artifacts
    }
    pub fn invocation_digest(&self) -> &str {
        &self.invocation_digest
    }
    pub fn toolchain_digest(&self) -> &str {
        &self.toolchain_digest
    }
    pub fn preparation_digest(&self) -> &str {
        &self.preparation_digest
    }
    pub fn execution_digest(&self) -> &str {
        &self.execution_digest
    }
    pub fn observation(&self) -> readiness_evidence::Observation {
        readiness_evidence::Observation::Executed {
            execution_id: self.execution_digest.clone(),
            conclusion: readiness_evidence::Conclusion::Success,
            exit_code: Some(0),
            output_digest: self.output_digest.clone(),
            artifact_digest: if self.artifacts.is_empty() {
                None
            } else {
                Some(hash(
                    &serde_json::to_vec(
                        &self
                            .artifacts
                            .iter()
                            .map(|a| (&a.source_path, &a.digest, a.bytes))
                            .collect::<Vec<_>>(),
                    )
                    .expect("measured artifact serialization"),
                ))
            },
        }
    }
}
fn final_revision_matches(e: &SourceExpectation, final_revision: &Value) -> bool {
    let expected_final =
        if e.workflow == "docs_sync.yml" && e.job == "sync-docs" && e.check == "docs-drift" {
            &e.head
        } else {
            &e.execution_revision
        };
    match e.execution_mode.as_str() {
        "source_checkout" => final_revision.as_str() == Some(expected_final.as_str()),
        "authenticated_integration_artifacts" => final_revision.is_null(),
        _ => false,
    }
}
pub fn verify_execution(
    files: &ArtifactFiles,
    preparation: &AuthenticatedPreparation,
) -> Result<AuthenticatedExecution> {
    ensure(
        files.origin() == preparation.origin
            && files.archive_digest() == preparation.archive_digest,
        "source execution archive producer changed",
    )?;
    let e = &preparation.expected;
    let p = format!("{}execution/", prefix(e)?);
    let raw = file(files, &p, "execution.json")?;
    let manifest = value(raw)?;
    execution_context(&manifest, e, true)?;
    identity(&manifest, e)?;
    ensure(
        final_revision_matches(e, required(&manifest, "final_execution_revision")?),
        "source final revision does not match finite execution mode/recipe",
    )?;
    for key in ["execution_complete", "logs_complete"] {
        truth(&manifest, key, true)?;
    }
    ensure(
        string(&manifest, "status")? == "passed"
            && required(&manifest, "error")?.is_null()
            && string(&manifest, "candidate_tree")? == e.tree
            && string(&manifest, "snapshot_tree")? == e.tree
            && string(&manifest, "execution_mode")? == e.execution_mode,
        "source execution failed/incomplete/context mismatch",
    )?;
    ensure(
        string(&manifest, "preparation_digest")? == preparation.digest
            && string(&manifest, "toolchain_digest")? == hash(&preparation.toolchain)
            && string(&manifest, "context_digest")? == hash(&preparation.context),
        "execution replaced frozen preparation",
    )?;
    ensure(
        file(files, &p, "frozen-preparation.json")? == preparation.manifest
            && file(files, &p, "preparation.json")? == preparation.manifest
            && file(files, &p, "toolchain.json")? == preparation.toolchain
            && file(files, &p, "after-toolchain.json")? == preparation.toolchain
            && file(files, &p, "context.json")? == preparation.context
            && file(files, &p, "invocations.json")? == e.invocations,
        "execution preparation/toolchain/context/recipe changed",
    )?;
    tools(files, &p, "", &preparation.toolchain)?;
    tools(files, &p, "after-", &preparation.toolchain)?;
    let expected_recipe = value(&e.invocations)?;
    let expected_rows = expected_recipe.as_array().unwrap();
    let rows = required(&manifest, "results")?
        .as_array()
        .ok_or_else(|| invalid("execution results missing"))?;
    let raw_rows = raw_results(raw)?;
    ensure(
        rows.len() == expected_rows.len() && raw_rows.len() == rows.len() && !rows.is_empty(),
        "missing/extra source command executions",
    )?;
    let mut artifacts = vec![];
    let mut paths = BTreeSet::new();
    for a in required(&manifest, "artifacts")?
        .as_array()
        .ok_or_else(|| invalid("artifact ledger missing"))?
    {
        let path = string(a, "source_path")?;
        ensure(
            e.artifact_source_paths.contains(path) && paths.insert(path.to_string()),
            "unexpected/duplicate source artifact",
        )?;
        let bytes = file(files, &p, string(a, "path")?)?;
        ensure(
            bytes.len() as u64 == count(a, "bytes")? && hash(bytes) == string(a, "sha256")?,
            "produced artifact bytes unbound",
        )?;
        artifacts.push(MeasuredArtifact {
            source_path: path.into(),
            digest: hash(bytes),
            bytes: bytes.len() as u64,
        });
    }
    ensure(
        paths == e.artifact_source_paths,
        "required produced artifact missing",
    )?;
    let mut previous = preparation.digest.clone();
    let mut output = vec![];
    for ((row, raw_row), invocation) in rows.iter().zip(raw_rows).zip(expected_rows) {
        ensure(
            required(row, "invocation")? == invocation
                && string(row, "previous_record_sha256")? == previous
                && row["exit_code"] == 0
                && required(row, "signal")?.is_null(),
            "source command skipped/changed/failed/hash-chain broken",
        )?;
        truth(row, "success", true)?;
        let program = string(invocation, "program")?;
        let executable = string(row, "executable_sha256")?;
        ensure(digest(executable), "unmeasured command executable")?;
        if let Some(expected) = preparation.tools.get(program) {
            ensure(
                executable == expected,
                "command executable changed after preparation",
            )?;
        } else {
            let cwd = string(invocation, "cwd")?;
            let path = format!("{cwd}/{program}");
            let normalized = path
                .split('/')
                .filter(|p| *p != ".")
                .collect::<Vec<_>>()
                .join("/");
            ensure(
                !normalized.split('/').any(|p| p == "..")
                    && artifacts
                        .iter()
                        .any(|a| a.source_path == normalized && a.digest == executable),
                "command executable lacks prepared or produced-artifact custody",
            )?;
        }
        output.push((
            stream(files, &p, required(row, "stdout")?)?,
            stream(files, &p, required(row, "stderr")?)?,
        ));
        previous = hash(raw_row);
    }
    ensure(
        string(&manifest, "terminal_record_sha256")? == previous,
        "terminal command chain mismatch",
    )?;
    artifacts.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(AuthenticatedExecution {
        output_digest: hash(
            &serde_json::to_vec(&output).map_err(|_| invalid("output identity serialization"))?,
        ),
        artifacts,
        invocation_digest: hash(&e.invocations),
        toolchain_digest: hash(&preparation.toolchain),
        preparation_digest: preparation.digest.clone(),
        execution_digest: hash(raw),
    })
}
#[cfg(test)]
mod tests;
