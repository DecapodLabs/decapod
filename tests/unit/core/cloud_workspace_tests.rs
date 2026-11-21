use super::*;
use crate::core::container_signer::HostContainerSignerServer;
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::tempdir;

struct MockCustodyTransport {
    writes: RefCell<Vec<(String, JsonValue)>>,
    reads: RefCell<Vec<(String, JsonValue)>>,
    next_write: RefCell<Vec<Result<JsonValue, CloudControlError>>>,
    next_read: RefCell<Vec<Result<JsonValue, CloudControlError>>>,
}

impl MockCustodyTransport {
    fn new() -> Self {
        Self {
            writes: RefCell::new(Vec::new()),
            reads: RefCell::new(Vec::new()),
            next_write: RefCell::new(Vec::new()),
            next_read: RefCell::new(Vec::new()),
        }
    }

    fn push_write(&self, r: Result<JsonValue, CloudControlError>) {
        self.next_write.borrow_mut().push(r);
    }

    fn push_read(&self, r: Result<JsonValue, CloudControlError>) {
        self.next_read.borrow_mut().push(r);
    }
}

impl ProofControlTransport for MockCustodyTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.writes
            .borrow_mut()
            .push((op_name.to_string(), payload.clone()));
        self.next_write.borrow_mut().remove(0)
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.reads
            .borrow_mut()
            .push((op_name.to_string(), payload.clone()));
        self.next_read.borrow_mut().remove(0)
    }
}

#[test]
fn custody_bind_persists_operation_id_before_submit_and_reconciles_lost_ack() {
    let tmp = tempdir().unwrap();
    let main_root = tmp.path().join("repo");
    let wt = main_root.join(".decapod/workspaces/agent-w2-feat_01cloud");
    fs::create_dir_all(&wt).unwrap();
    let store = CloudCustodyStore::at(tmp.path().join("custody"));
    let transport = MockCustodyTransport::new();

    let params = CloudCustodyBindParams {
        origin: "https://service.example".into(),
        project_id: "DecapodLabs/dactyl".into(),
        task_id: "feat_01cloud".into(),
        task_title: "Hosted custody test".into(),
        native_task_version: 3,
        workspace_id: "ws_1234".into(),
        worktree_path: wt.clone(),
        branch: "agent/w2/feat_01cloud-work".into(),
        actor: "agy-w2".into(),
        ttl_seconds: 600,
        invocation_id: Some("inv_01abc".into()),
        container_expected: true,
    };

    // First attempt: custody_bind returns OutcomeUnknown AND get_operation is temporarily unavailable.
    transport.push_write(Err(CloudControlError {
        http_status: None,
        code: "outcome_unknown".into(),
        detail: "tcp reset".into(),
        kind: CloudControlDenialKind::OutcomeUnknown,
    }));
    transport.push_read(Err(CloudControlError {
        http_status: None,
        code: "transport_timeout".into(),
        detail: "read timed out".into(),
        kind: CloudControlDenialKind::Unavailable,
    }));

    let err = custody_bind_or_reconcile(&transport, &store, &main_root, &params).unwrap_err();
    assert!(!err.to_string().is_empty());

    let saved = store
        .load(&main_root, &wt)
        .unwrap()
        .expect("pending record preserved");
    let pending_op = saved
        .pending_operation_id
        .clone()
        .expect("operation_id persisted before submit");
    let pending_custody = saved.custody_id.clone();
    assert_eq!(transport.writes.borrow().len(), 1);
    assert_eq!(
        transport.writes.borrow()[0].1["operation_id"].as_str(),
        Some(pending_op.as_str())
    );

    // Second invocation (after lost response): reconciles via `get_operation` WITHOUT repeating `custody_bind`,
    // validates identity, and confirms the active fence with `get_custody`.
    let future = crate::core::time::format_rfc3339_nanos(
        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    )
    .unwrap();
    transport.push_read(Ok(json!({
        "found": true,
        "receipt": {
            "version": 1,
            "profile": "candidate-acceptance-v1",
            "status": "custody_bound",
            "operation_id": pending_op,
            "project_id": "DecapodLabs/dactyl",
            "task_id": "feat_01cloud",
            "workspace_id": "ws_1234",
            "custody_id": pending_custody,
            "task_revision": 3,
            "fencing_generation": 9,
            "authority_generation": 2,
            "expires_at": future,
        }
    })));
    transport.push_read(Ok(json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "status": "custody_current",
        "project_id": "DecapodLabs/dactyl",
        "task_id": "feat_01cloud",
        "workspace_id": "ws_1234",
        "custody_id": pending_custody,
        "task_revision": 3,
        "fencing_generation": 9,
        "authority_generation": 2,
        "expires_at": future,
    })));

    let (bound, receipt) = custody_bind_or_reconcile(&transport, &store, &main_root, &params)
        .expect("second invocation must reconcile pending operation cleanly");
    assert_eq!(
        transport.writes.borrow().len(),
        1,
        "must not double-submit custody_bind"
    );
    assert_eq!(receipt.task_revision, 3);
    assert_eq!(receipt.fencing_generation, 9);
    assert!(bound.pending_operation_id.is_none());

    // Subsequent drift in native_task_version must fail closed before reusing the fence.
    let mut drifted = params.clone();
    drifted.native_task_version = 4;
    let drift_err =
        custody_bind_or_reconcile(&transport, &store, &main_root, &drifted).unwrap_err();
    assert!(drift_err.to_string().contains("CLOUD_CUSTODY_DRIFT"));
}

struct MockAdmission {
    allow: AtomicBool,
    calls: Mutex<Vec<String>>,
}

impl ContainerAdmissionVerifier for MockAdmission {
    fn verify_admission(
        &self,
        _worktree_path: &Path,
        invocation_id: &str,
    ) -> Result<String, DecapodError> {
        self.calls.lock().unwrap().push(invocation_id.to_string());
        if self.allow.load(Ordering::SeqCst) {
            Ok("a".repeat(64))
        } else {
            Err(DecapodError::ValidationError(
                "CLOUD_CONTAINER_ADMISSION_DENIED: replacement container or host-only run detected"
                    .into(),
            ))
        }
    }
}

struct MockRelayExecutor {
    calls: Mutex<Vec<(ContainerRelayOperation, String, JsonValue)>>,
}

impl CloudRelayExecutor for MockRelayExecutor {
    fn execute(
        &self,
        operation: ContainerRelayOperation,
        rpc_op: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, DecapodError> {
        self.calls
            .lock()
            .unwrap()
            .push((operation, rpc_op.to_string(), payload.clone()));
        Ok(json!({
            "version": 1,
            "profile": "candidate-acceptance-v1",
            "status": "custody_current",
            "project_id": payload["project_id"],
            "task_id": payload["task_id"],
            "workspace_id": payload["workspace_id"],
            "custody_id": payload["custody_id"],
            "task_revision": 2,
            "fencing_generation": 7,
            "authority_generation": 1,
            "expires_at": crate::core::time::format_rfc3339_nanos(
                time::OffsetDateTime::now_utc() + time::Duration::hours(1)
            ).unwrap(),
        }))
    }
}

#[test]
fn host_cloud_relay_enforces_container_admission_scope_and_allowlist() {
    use crate::core::container_signer::{ContainerSignerPolicy, WorkspaceCredentialsConfig};

    let tmp = tempdir().unwrap();
    let wt = tmp.path().join("wt");
    fs::create_dir_all(&wt).unwrap();
    let sock_path = tmp.path().join("relay.sock");

    let admission = Arc::new(MockAdmission {
        allow: AtomicBool::new(true),
        calls: Mutex::new(Vec::new()),
    });
    let executor = Arc::new(MockRelayExecutor {
        calls: Mutex::new(Vec::new()),
    });
    let relay_handler = Arc::new(HostCloudRelayHandler {
        project_id: "DecapodLabs/dactyl".into(),
        workspace_id: "ws_relay_1".into(),
        worktree_path: wt.clone(),
        task_id: "feat_relay".into(),
        custody_id: "custody_relay".into(),
        invocation_id: "inv_relay".into(),
        admission: Some(admission.clone()),
        executor: executor.clone(),
    });
    let policy = ContainerSignerPolicy {
        allowed_repository: "DecapodLabs/dactyl".into(),
        allowed_origin: "https://service.example".into(),
        allowed_agent_id: "agy-w2".into(),
        credentials: WorkspaceCredentialsConfig::default(),
        session_expires_at: 0,
        revoked: Arc::new(AtomicBool::new(false)),
        device_key_store: None,
        access_token_digest: None,
    };
    let _server =
        HostContainerSignerServer::bind_with_relay(&sock_path, policy, Some(relay_handler))
            .expect("bind relay server");

    let relay_transport = RelayProofControlTransport::new(
        sock_path.clone(),
        "inv_relay".into(),
        "ws_relay_1".into(),
        "DecapodLabs/dactyl".into(),
        "feat_relay".into(),
        "custody_relay".into(),
    );
    let receipt = custody_get(
        &relay_transport,
        &CustodyGetRequest {
            project_id: "DecapodLabs/dactyl".into(),
            task_id: "feat_relay".into(),
            workspace_id: "ws_relay_1".into(),
            custody_id: "custody_relay".into(),
            task_revision: 2,
            fencing_generation: 7,
        },
    )
    .expect("authorized container relay custody check must succeed");
    assert_eq!(receipt.fencing_generation, 7);
    assert_eq!(executor.calls.lock().unwrap().len(), 1);
    assert_eq!(admission.calls.lock().unwrap().len(), 1);

    // Cross-project or non-allowlisted operations must fail closed before reaching executor.
    let cross_project = RelayProofControlTransport::new(
        sock_path.clone(),
        "inv_relay".into(),
        "ws_relay_1".into(),
        "ForeignOrg/other".into(),
        "feat_relay".into(),
        "custody_relay".into(),
    );
    assert!(
        cross_project
            .read_control(
                PROOF_ACCEPTANCE_READ_OP,
                &json!({"project_id": "ForeignOrg/other"})
            )
            .is_err()
    );
    let non_allowlisted = request_relay_via_socket(
        &sock_path,
        &ContainerRelayRequest {
            kind: "cloud_relay".into(),
            invocation_id: "inv_relay".into(),
            workspace_id: "ws_relay_1".into(),
            project_id: "DecapodLabs/dactyl".into(),
            task_id: "feat_relay".into(),
            custody_id: "custody_relay".into(),
            operation: ContainerRelayOperation::ProofControlWrite,
            rpc_op: "device-revoke".into(),
            payload: json!({"action": "revoke_all"}),
        },
    )
    .unwrap();
    assert!(!non_allowlisted.ok);
    assert_eq!(
        non_allowlisted.code.as_deref(),
        Some("CLOUD_CONTAINER_ADMISSION_DENIED")
    );
    assert_eq!(executor.calls.lock().unwrap().len(), 1);

    // Replacement container / host-only failure must block relay even for valid scope.
    admission.allow.store(false, Ordering::SeqCst);
    let err = custody_get(
        &relay_transport,
        &CustodyGetRequest {
            project_id: "DecapodLabs/dactyl".into(),
            task_id: "feat_relay".into(),
            workspace_id: "ws_relay_1".into(),
            custody_id: "custody_relay".into(),
            task_revision: 2,
            fencing_generation: 7,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("CLOUD_ENTITLEMENT_DENIED")
            || err.to_string().contains("CLOUD_CONTAINER_ADMISSION_DENIED"),
        "unexpected error: {err}"
    );
    assert_eq!(executor.calls.lock().unwrap().len(), 1);
}

#[test]
fn cloud_container_launch_command_contains_only_non_secret_handoff_and_no_sqlite() {
    let tmp = tempdir().unwrap();
    let main_repo = tmp.path().join("repo");
    let wt = main_repo.join(".decapod/workspaces/wt-cloud");
    fs::create_dir_all(main_repo.join(".git")).unwrap();
    fs::create_dir_all(&wt).unwrap();
    let record = CloudCustodyRecord {
        version: 1,
        origin: "https://service.example".into(),
        project_id: "DecapodLabs/dactyl".into(),
        task_id: "feat_123".into(),
        task_title: "Cloud task".into(),
        native_task_version: 2,
        workspace_id: "ws_123".into(),
        worktree_path: wt.clone(),
        branch: "agent/w2/feat_123".into(),
        actor: "agy-w2".into(),
        custody_id: "custody_123".into(),
        pending_operation_id: None,
        pending_custody_id: None,
        pending_native_task_version: None,
        task_revision: Some(2),
        fencing_generation: Some(4),
        expires_at: None,
        invocation_id: Some("inv_123".into()),
        container_expected: true,
        container_runtime: Some("docker".into()),
        container_name: None,
        container_id: None,
    };
    let sock = tmp.path().join("relay.sock");
    let cmd = cloud_container_workspace_launch_command(
        &main_repo,
        &wt,
        "docker",
        "decapod:test",
        "inv_123",
        &record,
        Some(&sock),
    )
    .expect("build cloud launch cmd");
    assert!(cmd.contains("DATASTORE=supabase"));
    assert!(cmd.contains("DECAPOD_CLOUD_CONTAINER_RELAY=1"));
    assert!(cmd.contains("DECAPOD_CLOUD_TASK_ID=feat_123"));
    assert!(cmd.contains("DECAPOD_CLOUD_CUSTODY_ID=custody_123"));
    assert!(cmd.contains("DECAPOD_CLOUD_INVOCATION_ID=inv_123"));
    assert!(!cmd.contains("DECAPOD_LOCAL_ONLY=1"));
    assert!(!cmd.contains("decapod.db"));
    assert!(!cmd.contains("database backup"));
}
