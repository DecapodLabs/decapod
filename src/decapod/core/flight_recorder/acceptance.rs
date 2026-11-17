//! Read-only acceptance projection over operator-owned host captures. This is
//! neither an evidence importer nor a second readiness engine. Source labels
//! are assertions by the capturing host, not cryptographic authentication. An
//! unrestricted process can bypass capture; absence is never universal coverage.
use crate::core::readiness_evidence::{self, ExpectedPlan, Observation, SuiteEvidence, Verdict};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path};

const SCHEMA: &str = "decapod.agent-acceptance.v1";
const FILE_LIMIT: u64 = 64 * 1024 * 1024;
const EVENT_LIMIT: usize = 10_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Host,
    Kernel,
    Git,
    Github,
    AgentDeclaration,
    Scripted,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// A single sibling filename, never an arbitrary local path.
    pub file: String,
    pub sha256: String,
    pub source: Source,
    pub public_uri: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventInput {
    pub sequence: u64,
    pub elapsed_ms: u64,
    pub artifact: String,
    /// JSON pointer into the actual capture; no editorial event kind is accepted.
    pub pointer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub schema: String,
    pub run_id: String,
    pub owner_issue: u64,
    pub repository: String,
    pub objective: String,
    pub objective_revision: u64,
    /// Operator review assertion, not agent-authored consent or proof authority.
    pub approved_objective_sha256: String,
    pub harness_version: String,
    pub kernel_version: String,
    pub kernel_sha256: String,
    pub kernel_source: String,
    pub artifacts: BTreeMap<String, Artifact>,
    pub events: Vec<EventInput>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn digest_valid(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|s| hex(s, 64))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:/()= \"".contains(&c))
        && !sensitive(value)
}

fn sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "bearer ",
        "sk-",
        "ghp_",
        "gho_",
        "github_pat_",
        "access_token",
        "refresh_token",
        "session_password",
        "password:",
        "-----begin",
        "/users/",
        "/home/",
        "@",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || value
            .split(|c: char| c.is_whitespace() || c == '"')
            .any(|part| {
                part.starts_with("eyJ")
                    || (part.len() > 100 && part.bytes().all(|c| c.is_ascii_alphanumeric()))
            })
}

fn bounded_read(path: &Path) -> io::Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > FILE_LIMIT {
        return Err(invalid("capture must be a bounded regular file"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut bytes = Vec::new();
    options
        .open(path)?
        .take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err(invalid("capture grew beyond its bound"));
    }
    Ok(bytes)
}

/// Reads existing private captures only. Output omits raw text, commands, local
/// paths, environment, session credentials, reasoning and arbitrary JSON fields.
pub fn export(index: &Path) -> io::Result<Value> {
    let capture: Capture =
        serde_json::from_slice(&bounded_read(index)?).map_err(io::Error::other)?;
    let parent = index
        .parent()
        .ok_or_else(|| invalid("capture has no parent"))?;
    let mut documents = BTreeMap::new();
    let mut total = 0usize;
    for (name, artifact) in &capture.artifacts {
        let components: Vec<_> = Path::new(&artifact.file).components().collect();
        if !identifier(name)
            || components.len() != 1
            || !matches!(components[0], Component::Normal(_))
        {
            return Err(invalid("artifact must name a sibling file"));
        }
        let bytes = bounded_read(&parent.join(&artifact.file))?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("capture overflow"))?;
        if total > 256 * 1024 * 1024 || digest(&bytes) != artifact.sha256 {
            return Err(invalid(
                "capture digest mismatch or aggregate bound exceeded",
            ));
        }
        let document = if artifact.file.ends_with(".jsonl") {
            let rows: Result<Vec<Value>, _> = bytes
                .split(|b| *b == b'\n')
                .filter(|row| !row.is_empty())
                .map(serde_json::from_slice)
                .collect();
            Value::Array(rows.map_err(io::Error::other)?)
        } else {
            serde_json::from_slice(&bytes).map_err(io::Error::other)?
        };
        documents.insert(name.clone(), document);
    }
    correlate(&capture, &documents)
}

/// Pure correlation seam. Only export() performs digest-bound file loading.
fn correlate(capture: &Capture, documents: &BTreeMap<String, Value>) -> io::Result<Value> {
    if capture.schema != SCHEMA
        || ![1757, 1758].contains(&capture.owner_issue)
        || capture.events.len() > EVENT_LIMIT
        || capture.artifacts.len() > 256
        || !identifier(&capture.run_id)
        || !identifier(&capture.repository)
        || !identifier(&capture.harness_version)
        || !identifier(&capture.kernel_version)
        || !hex(&capture.kernel_source, 40)
        || !digest_valid(&capture.kernel_sha256)
        || capture.objective_revision == 0
        || capture.objective.is_empty()
        || capture.objective.len() > 4096
        || sensitive(&capture.objective)
        || capture.approved_objective_sha256 != digest(capture.objective.as_bytes())
    {
        return Err(invalid("invalid or unsafe acceptance identity"));
    }
    for artifact in capture.artifacts.values() {
        if !digest_valid(&artifact.sha256)
            || artifact.public_uri.as_ref().is_some_and(|uri| {
                !uri.starts_with("https://github.com/")
                    || uri.contains('?')
                    || uri.contains('#')
                    || sensitive(uri)
            })
        {
            return Err(invalid("unsafe evidence reference"));
        }
    }
    let mut events = Vec::new();
    let mut findings = BTreeSet::new();
    let mut sequence = 0;
    let mut elapsed = 0;
    let mut tools = BTreeMap::new();
    let mut completed_tools = BTreeMap::new();
    let mut models = BTreeSet::new();
    let mut context = false;
    let mut claim = false;
    let mut workspace = false;
    let mut claimed_task: Option<String> = None;
    let mut work_branch: Option<String> = None;
    let mut failed = false;
    let mut repair = false;
    let mut failure_head = None;
    let mut failure_sequence = 0;
    let mut ready: Option<Value> = None;
    let mut publication: Option<Value> = None;
    let mut admitted: Option<Value> = None;
    let mut applicable = false;
    let mut interventions = 0usize;
    let mut user_messages = 0usize;
    let mut objective_observed = false;
    let mut bypass = false;
    for event in &capture.events {
        if event.sequence <= sequence || event.elapsed_ms < elapsed {
            return Err(invalid("event order is not monotone"));
        }
        sequence = event.sequence;
        elapsed = event.elapsed_ms;
        let artifact = capture
            .artifacts
            .get(&event.artifact)
            .ok_or_else(|| invalid("missing event source"))?;
        let document = documents
            .get(&event.artifact)
            .ok_or_else(|| invalid("missing capture document"))?;
        let value = document
            .pointer(&event.pointer)
            .ok_or_else(|| invalid("missing captured event"))?;
        let mut kind = "observation";
        let mut facts = json!({});
        match artifact.source {
            Source::AgentDeclaration => {
                kind = "agent_declaration";
                findings.insert("AGENT_DECLARATION_IS_NOT_PROOF");
            }
            Source::Scripted => {
                kind = "scripted_control";
                findings.insert("SCRIPTED_IS_NOT_REAL_AGENT");
            }
            Source::Host => match value["type"].as_str() {
                Some("message_end") if value["message"]["role"] == "user" => {
                    user_messages += 1;
                    if user_messages == 1 {
                        let text = value["message"]["content"]
                            .as_array()
                            .map(|parts| {
                                parts
                                    .iter()
                                    .filter(|part| part["type"] == "text")
                                    .filter_map(|part| part["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                            .unwrap_or_default();
                        objective_observed = text == capture.objective;
                        if !objective_observed {
                            findings.insert("ORIGINAL_OBJECTIVE_MISMATCH");
                        }
                        kind = "objective_received";
                    } else {
                        kind = "post_objective_input";
                    }
                }
                Some("message_end") if value["message"]["role"] == "assistant" => {
                    if let (Some(provider), Some(model)) = (
                        value["message"]["provider"].as_str(),
                        value["message"]["model"].as_str(),
                    ) && identifier(provider)
                        && identifier(model)
                        && value["message"]["stopReason"] != "error"
                    {
                        models.insert((provider.to_owned(), model.to_owned()));
                        kind = "model_observed";
                        facts = json!({"provider":provider,"model":model});
                    }
                }
                Some("tool_execution_start") => {
                    let id = value["toolCallId"]
                        .as_str()
                        .ok_or_else(|| invalid("tool lacks identity"))?;
                    let name = value["toolName"]
                        .as_str()
                        .ok_or_else(|| invalid("tool lacks name"))?;
                    if !identifier(id)
                        || !identifier(name)
                        || completed_tools.contains_key(id)
                        || tools.insert(id.to_owned(), name.to_owned()).is_some()
                    {
                        return Err(invalid("duplicate or invalid host tool identity"));
                    }
                    kind = "agent_tool_started";
                    facts = json!({"tool_call_id":id,"tool":name});
                }
                Some("tool_execution_end") => {
                    let id = value["toolCallId"]
                        .as_str()
                        .ok_or_else(|| invalid("tool lacks identity"))?;
                    let name = value["toolName"].as_str().unwrap_or("");
                    if tools.remove(id).as_deref() != Some(name) {
                        return Err(invalid("unmatched host tool result"));
                    }
                    completed_tools.insert(id.to_owned(), (name.to_owned(), event.sequence));
                    kind = "agent_tool_completed";
                    facts = json!({"tool_call_id":id,"tool":name,"is_error":value["isError"].as_bool()});
                }
                Some("human_intervention") => {
                    interventions += 1;
                    kind = "human_intervention";
                }
                Some("direct_publication") | Some("denied_transition_bypass") => {
                    bypass = true;
                    applicable = false;
                    findings.insert("OBSERVED_HOST_BYPASS");
                    kind = "host_bypass_observed";
                }
                _ => {}
            },
            Source::Git => {
                if value["repository"] != capture.repository {
                    return Err(invalid("Git observation belongs to another repository"));
                }
                // The host samples actual Git state; narrative is not accepted here.
                let head = value["head"]
                    .as_str()
                    .ok_or_else(|| invalid("Git observation lacks head"))?;
                let base = value["base"]
                    .as_str()
                    .ok_or_else(|| invalid("Git observation lacks base"))?;
                if !hex(head, 40) || !hex(base, 40) || value["clean"].as_bool().is_none() {
                    return Err(invalid("invalid Git observation"));
                }
                kind = "source_observed";
                facts = json!({"head":head,"base":base,"clean":value["clean"]});
                let agent_tool = value["tool_call_id"]
                    .as_str()
                    .and_then(|id| completed_tools.get(id));
                if failed
                    && failure_head.as_deref().is_some_and(|old| old != head)
                    && agent_tool.is_some_and(|(tool, sequence)| {
                        *sequence > failure_sequence
                            && matches!(tool.as_str(), "write" | "edit" | "bash")
                    })
                {
                    repair = true;
                    kind = "agent_changed_failed_source";
                }
                if let Some(receipt) = &ready
                    && (receipt["material"]["head_revision"] != head
                        || receipt["material"]["accepted_base_revision"] != base
                        || value["clean"] != true)
                {
                    applicable = false;
                    ready = None;
                    publication = None;
                    kind = "evidence_became_stale";
                    findings.insert("SOURCE_OR_BASE_CHANGED");
                }
            }
            Source::Kernel => {
                if value["repository"] != capture.repository {
                    return Err(invalid("kernel observation belongs to another repository"));
                }
                // An envelope is emitted by the host's transparent CLI capture,
                // never inferred from an assistant's description of a command.
                if value["binary_sha256"] != capture.kernel_sha256 {
                    return Err(invalid(
                        "kernel capture executable differs from pinned binary",
                    ));
                }
                let operation = value["operation"]
                    .as_str()
                    .ok_or_else(|| invalid("kernel capture lacks operation"))?;
                let code = value["exit_code"]
                    .as_i64()
                    .ok_or_else(|| invalid("kernel capture lacks exit status"))?;
                let output = &value["output"];
                if code == 0 {
                    match operation {
                        "context" => {
                            context = true;
                            kind = "context_consulted";
                        }
                        "claim" if output["status"] == "ok" => {
                            claimed_task = output["id"]
                                .as_str()
                                .filter(|id| identifier(id))
                                .map(str::to_owned);
                            claim = claimed_task.is_some();
                            kind = "task_claimed";
                            facts = json!({"task_id":claimed_task,"lease_generation":output["result"]["lease_generation"].as_u64()});
                        }
                        "workspace" if output["can_work"] == true => {
                            work_branch = output["branch"]
                                .as_str()
                                .filter(|branch| identifier(branch))
                                .map(str::to_owned);
                            workspace = work_branch
                                .as_ref()
                                .zip(claimed_task.as_ref())
                                .is_some_and(|(branch, task)| branch.contains(task.as_str()));
                            kind = "workspace_bound";
                            facts = json!({"branch":work_branch,"task_id":claimed_task});
                        }
                        "validate" => {
                            publication = None;
                            applicable = false;
                            let plan: ExpectedPlan = serde_json::from_value(value["plan"].clone())
                                .map_err(io::Error::other)?;
                            let suite: SuiteEvidence =
                                serde_json::from_value(value["observations"].clone())
                                    .map_err(io::Error::other)?;
                            // Historical correlation only; the capturing host's
                            // independently compiled plan is an explicit trust input.
                            let verified = readiness_evidence::verify(
                                &plan,
                                &suite.before,
                                &suite.facts,
                                &suite,
                            );
                            let receipt = &value["receipt"];
                            let mut unsigned = receipt.clone();
                            if let Some(object) = unsigned.as_object_mut() {
                                object.remove("receipt_hash");
                            }
                            if output["publication_ready"] == true
                                && verified.verdict == Verdict::Ready
                                && receipt["status"] == "ok"
                                && receipt["kind"] == "native_readiness_receipt"
                                && receipt["proof_plan_digest"].as_str()
                                    == Some(plan.digest().map_err(io::Error::other)?.as_str())
                                && receipt["material"]
                                    == serde_json::to_value(&suite.before)
                                        .map_err(io::Error::other)?
                                && receipt["material"]["repository"] == capture.repository
                                && receipt["material"]["work_branch"].as_str()
                                    == work_branch.as_deref()
                                && receipt["scope"] == "repository_local_readiness"
                                && receipt["evidence_digest"].as_str()
                                    == Some(
                                        readiness_evidence::digest(&suite)
                                            .map_err(io::Error::other)?
                                            .as_str(),
                                    )
                                && receipt["receipt_hash"].as_str()
                                    == Some(
                                        readiness_evidence::digest(&unsigned)
                                            .map_err(io::Error::other)?
                                            .as_str(),
                                    )
                            {
                                ready = Some(receipt.clone());
                                kind = "fresh_validation_observed";
                                facts = json!({"head":suite.before.head_revision,"base":suite.before.accepted_base_revision,"receipt_hash":receipt["receipt_hash"],"scope":receipt["scope"],"checks":suite.results.len()});
                            } else {
                                ready = None;
                                applicable = false;
                                findings.insert("INCOMPLETE_OR_MISMATCHED_PROOF");
                            }
                        }
                        "publish"
                            if output["status"] == "ok"
                                && output["pr_verified"] == true
                                && output["remote_verified"] == true =>
                        {
                            if let Some(receipt) = &ready
                                && output["commit_hash"] == receipt["material"]["head_revision"]
                                && output["branch"] == receipt["material"]["work_branch"]
                            {
                                publication = Some(output.clone());
                                kind = "publication_observed";
                            } else {
                                findings.insert("PUBLICATION_WITHOUT_CURRENT_PROOF");
                            }
                        }
                        _ => {}
                    }
                } else {
                    let reason = value["reason_codes"]
                        .as_array()
                        .ok_or_else(|| invalid("rejection lacks codes"))?;
                    let codes: Vec<&str> = reason.iter().filter_map(Value::as_str).collect();
                    if codes.iter().any(|s| !identifier(s)) {
                        return Err(invalid("unsafe rejection code"));
                    }
                    kind = "kernel_rejection_observed";
                    facts = json!({"codes":codes,"exit_code":code});
                    ready = None;
                    applicable = false;
                    publication = None;
                    if operation == "validate"
                        && let Ok(suite) = serde_json::from_value::<SuiteEvidence>(value["observations"].clone())
                            && let Ok(plan) = serde_json::from_value::<ExpectedPlan>(value["plan"].clone())
                            && suite.expected_plan_digest == plan.digest().map_err(io::Error::other)?
                            && suite.before.repository == capture.repository
                            && suite.results.iter().any(|r| plan.checks.iter().any(|check| check.key == r.key && check.requirement == readiness_evidence::Requirement::Execute)
                                && matches!(r.observation, Observation::Executed {conclusion: readiness_evidence::Conclusion::Failure, exit_code: Some(n), ..} if n != 0))
                    {
                        failed = true;
                        failure_sequence = event.sequence;
                        failure_head = Some(suite.before.head_revision);
                        kind = "required_check_failed";
                    }
                    if codes.contains(&"PR_CREATED_NOT_ADMITTED") {
                        publication = None;
                        findings.insert("PR_CREATED_NOT_ADMITTED");
                    }
                }
            }
            Source::Github => {
                let head = value["head"]
                    .as_str()
                    .ok_or_else(|| invalid("forge readback lacks head"))?;
                let base = value["base"]
                    .as_str()
                    .ok_or_else(|| invalid("forge readback lacks actual target"))?;
                let url = value["url"]
                    .as_str()
                    .ok_or_else(|| invalid("forge readback lacks URL"))?;
                if !hex(head, 40)
                    || !hex(base, 40)
                    || !url.starts_with(&format!("https://github.com/{}/pull/", capture.repository))
                    || sensitive(url)
                {
                    return Err(invalid("invalid forge readback"));
                }
                kind = "github_readback";
                facts = json!({"head":head,"base":base,"pr_url":url,"merged":value["merged"]});
                if let (Some(receipt), Some(published)) = (&ready, &publication)
                    && receipt["material"]["head_revision"] == head
                    && receipt["material"]["accepted_base_revision"] == base
                    && published["pr_url"] == url
                    && published["commit_hash"] == head
                    && value["branch"] == receipt["material"]["work_branch"]
                    && value["base_branch"] == receipt["material"]["base_branch"]
                    && value["merged"] == false
                {
                    admitted = Some(facts.clone());
                    applicable = true;
                    kind = "pr_admission_independently_correlated";
                } else {
                    applicable = false;
                    findings.insert("FORGE_SUBJECT_NOT_CURRENTLY_CORRELATED");
                }
            }
        }
        let id = digest(
            format!(
                "{}:{}:{}:{}",
                capture.run_id, event.sequence, artifact.sha256, event.pointer
            )
            .as_bytes(),
        );
        events.push(json!({"id":id,"sequence":event.sequence,"elapsed_ms":event.elapsed_ms,"kind":kind,"source":artifact.source,"evidence":{"sha256":artifact.sha256,"pointer":event.pointer,"public_uri":artifact.public_uri},"facts":facts}));
    }
    if !tools.is_empty() {
        findings.insert("INCOMPLETE_HOST_TOOL_CAPTURE");
    }
    if !context {
        findings.insert("DECAPOD_CONTEXT_NOT_OBSERVED");
    }
    if !claim || !workspace {
        findings.insert("CUSTODY_NOT_OBSERVED");
    }
    if !failed {
        findings.insert("EXECUTED_FAILURE_NOT_OBSERVED");
    }
    if !repair {
        findings.insert("REPAIR_AFTER_FAILURE_NOT_OBSERVED");
    }
    interventions = interventions.max(user_messages.saturating_sub(1));
    if !objective_observed {
        findings.insert("ORIGINAL_OBJECTIVE_NOT_OBSERVED");
    }
    let autonomous = objective_observed
        && admitted.is_some()
        && failed
        && repair
        && context
        && claim
        && workspace
        && !models.is_empty()
        && tools.is_empty()
        && interventions == 0
        && !bypass
        && !findings.contains("SCRIPTED_IS_NOT_REAL_AGENT");
    Ok(json!({
        "schema":SCHEMA,"run_id":capture.run_id,"owner_issue":format!("https://github.com/DecapodLabs/decapod/issues/{}",capture.owner_issue),
        "repository":capture.repository,"objective":{"revision":capture.objective_revision,"text":capture.objective},
        "harness":{"name":"pi","version":capture.harness_version,"models_observed":models},
        "kernel":{"version":capture.kernel_version,"binary_sha256":capture.kernel_sha256,"source":capture.kernel_source},
        "observed_autonomous_admission":autonomous,"admission_observed":admitted,"current_admission_applicable":applicable,
        "publication_ready":false,"merge_authorized":false,"human_interventions":interventions,
        "assurance":"Read-only correlation of trusted-host captures, kernel outputs and independent Git/forge readback. Digests bind bytes; they do not authenticate a producer. Arbitrary uncaptured host activity and semantic correctness remain unproven.",
        "findings":findings,"events":events
    }))
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/acceptance_observer_tests.rs"]
mod tests;
