//! Thin cooperative host adapter. Every operation delegates to an existing
//! kernel owner; this is neither an agent runtime nor an alternate policy path.
use crate::core::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use crate::core::{agent_eval, error::DecapodError, mentor::reflection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, clap::Args)]
pub struct Cli {
    /// Bounded private JSON request; raw prompts/results are never stored here.
    #[clap(long)]
    pub input: std::path::PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: String,
    pub agent_id: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    pub boundary: Boundary,
}
fn default_timeout() -> u64 {
    120
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Boundary {
    Prompt {
        text: String,
    },
    Initialize,
    Context {
        params: crate::core::rpc::ContextResolveParams,
    },
    CreateTask {
        title: String,
        description: String,
    },
    ClaimTask {
        task_id: String,
    },
    Workspace {
        branch: String,
    },
    ToolAdmission {
        reflection: reflection::Request,
    },
    ToolResult {
        run_id: String,
        observation_sha256: String,
        exit_code: i32,
    },
    Escalation {
        reflection: reflection::Request,
    },
    Completion,
    Resume {
        run_id: String,
        checkpoint_id: String,
    },
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub schema_version: &'static str,
    pub agent_id: String,
    pub boundary: String,
    pub kernel_sha256: String,
    pub native_exit_success: bool,
    pub native_exit_code: Option<i32>,
    pub kernel_execution_blocked: Option<bool>,
    pub native_result: Value,
    pub redacted_stderr: String,
    pub publication_ready: bool,
    pub merge_authorized: bool,
    pub limitations: &'static str,
}

fn invalid(message: &str) -> DecapodError {
    DecapodError::ValidationError(message.into())
}
fn label(value: &str) -> bool {
    crate::core::trace::redact_string(value) == value
        && crate::core::db_maintenance::redact_secrets(value) == value
        && !value.is_empty()
        && value.len() <= 128
        && value != "unknown"
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}
fn sha(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn arguments(boundary: &Boundary) -> Result<Vec<String>, DecapodError> {
    let rpc = |op: &str, params: Value| {
        vec![
            "rpc".into(),
            "--op".into(),
            op.into(),
            "--params".into(),
            params.to_string(),
        ]
    };
    Ok(match boundary {
        Boundary::Prompt { .. } => {
            return Err(invalid("prompt uses the existing in-process safety owner"));
        }
        Boundary::Initialize => rpc("agent.init", json!({})),
        Boundary::Context { params } => rpc(
            "context.resolve",
            serde_json::to_value(params).map_err(|_| invalid("invalid context request"))?,
        ),
        Boundary::CreateTask { title, description } => {
            if title.trim().is_empty()
                || title.len() > 512
                || description.len() > 8192
                || title.starts_with('-')
            {
                return Err(invalid("invalid bounded task intent"));
            }
            vec![
                "todo".into(),
                "add".into(),
                title.clone(),
                "--description".into(),
                description.clone(),
                "--format".into(),
                "json".into(),
            ]
        }
        Boundary::ClaimTask { task_id } => {
            if !label(task_id) {
                return Err(invalid("explicit task id required"));
            }
            vec![
                "todo".into(),
                "claim".into(),
                "--id".into(),
                task_id.clone(),
            ]
        }
        Boundary::Workspace { branch } => {
            if !branch.starts_with("agent/")
                || branch.len() > 256
                || branch.contains("..")
                || !branch
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/_-".contains(&c))
            {
                return Err(invalid("workspace requires an explicit agent branch"));
            }
            vec![
                "workspace".into(),
                "ensure".into(),
                "--branch".into(),
                branch.clone(),
            ]
        }
        Boundary::ToolAdmission { reflection } | Boundary::Escalation { reflection } => {
            rpc("mentor.obligations", json!({"reflection": reflection}))
        }
        Boundary::ToolResult {
            run_id,
            observation_sha256,
            exit_code,
        } => {
            if !label(run_id) || !sha(observation_sha256) {
                return Err(invalid(
                    "bounded trajectory id and observation digest required",
                ));
            }
            vec![
                "govern".into(),
                "trajectory".into(),
                "record".into(),
                "--run-id".into(),
                run_id.clone(),
                "--evidence".into(),
                format!(
                    "host_reported_tool_result:sha256:{observation_sha256}:exit:{exit_code}; not independently verified execution or completion"
                ),
            ]
        }
        Boundary::Completion => vec!["validate".into(), "--format".into(), "json".into()],
        Boundary::Resume {
            run_id,
            checkpoint_id,
        } => {
            if !label(run_id) || !label(checkpoint_id) {
                return Err(invalid("explicit resume identities required"));
            }
            vec![
                "govern".into(),
                "trajectory".into(),
                "verify-resume".into(),
                "--run-id".into(),
                run_id.clone(),
                "--checkpoint-id".into(),
                checkpoint_id.clone(),
            ]
        }
    })
}

fn kernel_hash(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn execute(root: &Path, request: Request) -> Result<Response, DecapodError> {
    if request.schema_version != "decapod.native-boundary.v1"
        || !label(&request.agent_id)
        || request.timeout_seconds == 0
        || request.timeout_seconds > 7200
    {
        return Err(invalid(
            "NATIVE_BOUNDARY_IDENTITY_REQUIRED: provide explicit stable host agent_id, schema and bounded deadline; labels grant no trust or approval",
        ));
    }
    let kernel = std::env::current_exe()?;
    let before = kernel_hash(&kernel)?;
    let boundary = match &request.boundary {
        Boundary::Prompt { .. } => "prompt",
        Boundary::Initialize => "initialize",
        Boundary::Context { .. } => "context",
        Boundary::CreateTask { .. } => "create_task",
        Boundary::ClaimTask { .. } => "claim_task",
        Boundary::Workspace { .. } => "workspace",
        Boundary::ToolAdmission { .. } => "tool_admission",
        Boundary::ToolResult { .. } => "tool_result",
        Boundary::Escalation { .. } => "escalation",
        Boundary::Completion => "completion",
        Boundary::Resume { .. } => "resume",
    }
    .to_string();
    let (native_result, native_exit_code, stderr) = if let Boundary::Prompt { text } =
        &request.boundary
    {
        if text.len() > 1024 * 1024 {
            return Err(invalid("prompt exceeds native boundary budget"));
        }
        let result = agent_eval::evaluate_prompt(text);
        let code = if result.status == agent_eval::EvalStatus::Allow {
            0
        } else {
            1
        };
        (
            serde_json::to_value(result).map_err(|_| invalid("invalid safety response"))?,
            Some(code),
            String::new(),
        )
    } else {
        let args = arguments(&request.boundary)?;
        let capture = Command::new(&kernel)
            .current_dir(root)
            .args(args)
            .env("DECAPOD_AGENT_ID", &request.agent_id)
            .bounded_capture(
                ProcessLimits {
                    timeout: Duration::from_secs(request.timeout_seconds),
                    max_output_bytes: 1024 * 1024,
                },
                &Cancellation::default(),
            )?;
        if capture.termination != Termination::Exited {
            return Err(invalid(
                "NATIVE_BOUNDARY_INCOMPLETE: native process canceled, timed out or exceeded output budget; no success receipt",
            ));
        }
        let stdout = String::from_utf8_lossy(&capture.output.stdout);
        let value = serde_json::from_str(&stdout).map_err(|_| invalid("NATIVE_BOUNDARY_INVALID_RESPONSE: preserve native diagnostics privately; no structured kernel response"))?;
        (
            value,
            capture.output.status.code(),
            crate::core::trace::redact_string(&String::from_utf8_lossy(&capture.output.stderr)),
        )
    };
    if kernel_hash(&kernel)? != before {
        return Err(invalid(
            "NATIVE_BOUNDARY_KERNEL_CHANGED: retry with one pinned kernel",
        ));
    }
    let kernel_execution_blocked = native_result
        .pointer("/result/reflection/execution_blocked")
        .and_then(Value::as_bool);
    Ok(Response {
        schema_version: "decapod.native-boundary-result.v1",
        agent_id: request.agent_id,
        boundary,
        kernel_sha256: before,
        native_exit_success: native_exit_code == Some(0),
        native_exit_code,
        kernel_execution_blocked,
        native_result,
        redacted_stderr: stderr,
        publication_ready: false,
        merge_authorized: false,
        limitations: "Cooperative caller must invoke every boundary and deliver returned context before tool use. Native exit status is an observation, not completion/admission authority. Host tool-result digests are declarations; this adapter does not execute arbitrary tools, authenticate human intent, intercept unrestricted shell/forge actions, or prove model delivery. Existing native owners retain custody, reflection, proof and resume decisions.",
    })
}

pub fn run(root: &Path, cli: Cli) -> Result<(), DecapodError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(cli.input)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if file.metadata()?.nlink() != 1 {
            return Err(invalid("request must be a private regular file"));
        }
    }
    if !file.metadata()?.is_file() || file.metadata()?.len() > 1024 * 1024 {
        return Err(invalid("bounded regular native request required"));
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("native request exceeds byte budget"));
    }
    let request =
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid native boundary request"))?;
    let response = execute(root, request)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&response)
            .map_err(|_| invalid("invalid native boundary response"))?
    );
    if !response.native_exit_success
        || response
            .native_result
            .get("success")
            .and_then(Value::as_bool)
            == Some(false)
        || response.kernel_execution_blocked == Some(true)
    {
        return Err(invalid(
            "NATIVE_BOUNDARY_NOT_ADMITTED: inspect the original native result; no gate was waived",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/native_boundary_tests.rs"]
mod tests;
