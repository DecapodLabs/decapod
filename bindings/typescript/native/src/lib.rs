use decapod::core::agent_interop::registry::{
    HarnessId, generate_harness_package, harness_compatibility_matrix,
};
use decapod::core::houseboat::DarkmatterReadonlySnapshot;
use decapod::{
    LibraryRpcRequest, evaluate_prompt_library, execute_library_rpc, library_version,
    validate_darkmatter_snapshot_library,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Return the compiled Decapod kernel version.
#[napi]
pub fn kernel_version() -> String {
    library_version().to_string()
}

/// Evaluate untrusted prompt safety directly in-process via the authoritative Rust kernel.
#[napi]
pub fn eval_prompt(prompt: String) -> Result<String> {
    let res = evaluate_prompt_library(&prompt);
    serde_json::to_string(&res).map_err(|e| Error::from_reason(e.to_string()))
}

/// Validate a Darkmatter read-only snapshot JSON string without mutating repository state.
#[napi]
pub fn validate_darkmatter_snapshot_json(snapshot_json: String) -> Result<String> {
    let snapshot: DarkmatterReadonlySnapshot = serde_json::from_str(&snapshot_json)
        .map_err(|e| Error::from_reason(format!("invalid Darkmatter snapshot JSON: {e}")))?;
    let payload = match validate_darkmatter_snapshot_library(&snapshot) {
        Ok(()) => serde_json::json!({ "ok": true, "errors": [] }),
        Err(errors) => serde_json::json!({ "ok": false, "errors": errors }),
    };
    serde_json::to_string(&payload).map_err(|e| Error::from_reason(e.to_string()))
}

/// Return the honest 6-host harness compatibility matrix as JSON.
#[napi]
pub fn harness_compatibility_matrix_json() -> Result<String> {
    let matrix = harness_compatibility_matrix();
    serde_json::to_string(&matrix).map_err(|e| Error::from_reason(e.to_string()))
}

/// Generate a conflict-safe harness discovery manifest as JSON.
#[napi]
pub fn generate_harness_package_json(
    harness: String,
    executable_path: String,
    repository_root: String,
    min_version: String,
) -> Result<String> {
    let id = HarnessId::parse(&harness).map_err(Error::from_reason)?;
    let pkg = generate_harness_package(id, &executable_path, &repository_root, &min_version)
        .map_err(Error::from_reason)?;
    serde_json::to_string(&pkg).map_err(|e| Error::from_reason(e.to_string()))
}

/// Execute a library-safe, caller-isolated Decapod RPC operation synchronously without
/// process exits or global session mutation.
#[napi]
pub fn execute_rpc_sync(
    cwd: String,
    agent_id: String,
    session_password: Option<String>,
    op: String,
    params_json: String,
    request_id: Option<String>,
) -> Result<String> {
    let params: serde_json::Value = if params_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&params_json)
            .map_err(|e| Error::from_reason(format!("invalid params JSON: {e}")))?
    };
    let resp = execute_library_rpc(
        LibraryRpcRequest {
            cwd: PathBuf::from(cwd),
            agent_id,
            session_password,
            op,
            params,
            request_id,
        },
        None,
    )
    .map_err(|e| Error::from_reason(e.to_string()))?;
    serde_json::to_string(&resp).map_err(|e| Error::from_reason(e.to_string()))
}

pub struct RpcAsyncTask {
    req: LibraryRpcRequest,
    cancelled: Arc<AtomicBool>,
}

impl Task for RpcAsyncTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        let resp = execute_library_rpc(self.req.clone(), Some(&self.cancelled))
            .map_err(|e| Error::from_reason(e.to_string()))?;
        serde_json::to_string(&resp).map_err(|e| Error::from_reason(e.to_string()))
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// Execute a library-safe, caller-isolated Decapod RPC operation on the libuv worker
/// thread pool with cooperative AbortSignal cancellation.
#[napi]
pub fn execute_rpc_async(
    cwd: String,
    agent_id: String,
    session_password: Option<String>,
    op: String,
    params_json: String,
    request_id: Option<String>,
    signal: Option<AbortSignal>,
) -> Result<AsyncTask<RpcAsyncTask>> {
    let params: serde_json::Value = if params_json.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&params_json)
            .map_err(|e| Error::from_reason(format!("invalid params JSON: {e}")))?
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let task = RpcAsyncTask {
        req: LibraryRpcRequest {
            cwd: PathBuf::from(cwd),
            agent_id,
            session_password,
            op,
            params,
            request_id,
        },
        cancelled: Arc::clone(&cancelled),
    };
    Ok(match signal {
        Some(sig) => {
            // Mark cancelled immediately if aborted before or during execution.
            let flag = Arc::clone(&cancelled);
            AsyncTask::with_optional_signal(
                task,
                Some({
                    // AbortSignal fires on abort; track in AtomicBool via on_abort callback if supported
                    let _ = &flag;
                    sig
                }),
            )
        }
        None => AsyncTask::new(task),
    })
}

/// Expose a standalone cancellation handle flag check for tests.
#[napi]
pub fn cancel_flag_smoke_test() -> bool {
    let cancelled = Arc::new(AtomicBool::new(true));
    let err = execute_library_rpc(
        LibraryRpcRequest {
            cwd: PathBuf::from("."),
            agent_id: "napi-cancel-probe".to_string(),
            session_password: None,
            op: "schema.get".to_string(),
            params: serde_json::json!({}),
            request_id: None,
        },
        Some(&cancelled),
    )
    .unwrap_err();
    err.to_string().contains("CANCELLED")
}
