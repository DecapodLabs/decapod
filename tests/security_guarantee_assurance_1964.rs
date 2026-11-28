//! Security guarantee-to-enforcement boundary, negative-first assurance, and
//! mutation-detection suite (`DecapodLabs/decapod#1964`, parent ledger `#1933`).
//!
//! Complements (without duplicating or reopening) `#1795`, `#1763`, `#1944`,
//! `#1878`, `#1886`, `#1895`, and `#1896`:
//! 1. Verifies `docs/security-assurance-matrix.md` covers all published security
//!    surfaces with explicit scope, component, observation path, classification,
//!    and residual assumptions, without unqualified absolute guarantees.
//! 2. Exercises negative-first + positive controls on `cloud_proof` subject
//!    binding, CLI `--pre-build` / stale-receipt / self-declared proof refusal,
//!    and `governed_transition::admit` state-identity staleness.
//! 3. Extends `#1795` single-fault mutation checks to uncovered high-consequence
//!    boundaries (`RULE-APPROVAL-006`, `RULE-CLOUD-PROOF-008`, `RULE-HTTP-READONLY-009`).

use decapod::core::bounded_process::{BUILD_TIMEOUT, BoundedCommand};
use decapod::core::cloud_proof::{
    AdmitOrPublishProofRequest, CloudControlDenialKind, CloudControlError, ProofCaptureRequest,
    ProofControlTransport, ProofEvidenceReference, ProofGitIdentity, ProofSubject,
    VerifyProofTransitionRequest, classify_control_status, new_uuid_v4, proof_capture_fingerprint,
    reconcile_operation_after_unknown, submit_admit_or_publish, submit_capture,
    submit_retire_historical, submit_verify,
};
use decapod::core::governed_transition::{
    AdmissionCause, AdmissionRequest, StateIdentity, TransitionPhase, admit,
};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const MATRIX_DOC: &str = include_str!("../docs/security-assurance-matrix.md");
const README_DOC: &str = include_str!("../README.md");
const SECURITY_DOC: &str = include_str!("../SECURITY.md");
const TRUST_INVENTORY_DOC: &str = include_str!("../docs/trust-and-egress-inventory.md");
const TS_README_DOC: &str = include_str!("../bindings/typescript/README.md");

#[derive(Default)]
struct StubProofTransport {
    writes: RefCell<Vec<Result<Value, CloudControlError>>>,
    reads: RefCell<Vec<Result<Value, CloudControlError>>>,
}

impl StubProofTransport {
    fn with_write(resp: Result<Value, CloudControlError>) -> Self {
        let s = Self::default();
        s.writes.borrow_mut().push(resp);
        s
    }
}

impl ProofControlTransport for StubProofTransport {
    fn write_control(&self, _op_name: &str, _payload: &Value) -> Result<Value, CloudControlError> {
        self.writes.borrow_mut().remove(0)
    }

    fn read_control(&self, _op_name: &str, _payload: &Value) -> Result<Value, CloudControlError> {
        self.reads.borrow_mut().remove(0)
    }
}

fn decapod_bin() -> PathBuf {
    let raw = env!("CARGO_BIN_EXE_decapod");
    Path::new(raw)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(raw))
}

#[test]
fn security_assurance_matrix_covers_published_claims_without_unqualified_absolutes() {
    for required_surface in [
        "README.md",
        "SECURITY.md",
        "docs/trust-and-egress-inventory.md",
        "docs/publication-admission-acceptance.md",
        "docs/book/src/reference/cloud-projects.md",
        "bindings/typescript/README.md",
    ] {
        assert!(
            MATRIX_DOC.contains(required_surface),
            "security-assurance-matrix.md must map published surface {required_surface}"
        );
    }

    let valid_classes = [
        "proven_for_exact_subject",
        "trusted_assertion",
        "conditional",
        "not_proven",
        "stale",
        "failed",
        "unsupported",
    ];
    let inventory_rows: Vec<&str> = MATRIX_DOC
        .lines()
        .filter(|line| line.starts_with("| **"))
        .collect();
    assert!(
        inventory_rows.len() >= 10,
        "expected full cross-layer claim matrix (>= 10 rows), found {}",
        inventory_rows.len()
    );
    for row in inventory_rows {
        assert!(
            valid_classes.iter().any(|cls| row.contains(cls)),
            "matrix row missing explicit assurance class: {row}"
        );
    }

    // Drift check: forbid unqualified absolute security claims across public/normative docs.
    for (name, content) in [
        ("README.md", README_DOC),
        ("SECURITY.md", SECURITY_DOC),
        ("docs/security-assurance-matrix.md", MATRIX_DOC),
        ("bindings/typescript/README.md", TS_README_DOC),
    ] {
        let lower = content.to_ascii_lowercase();
        for forbidden in ["unspoofable", "100% guaranteed", "guaranteed secure"] {
            assert!(
                !lower.contains(forbidden),
                "{name} contains unqualified security claim '{forbidden}'"
            );
        }
    }
    assert!(
        TRUST_INVENTORY_DOC.contains("never claim unqualified \"zero telemetry\"")
            && !README_DOC.to_ascii_lowercase().contains("zero telemetry"),
        "docs must not make unqualified zero-telemetry claims"
    );
}

#[test]
fn negative_first_controls_refuse_wrong_subject_stale_state_and_prebuild_promotion() {
    // 1. Governed transition admission rejects stale StateIdentity and forbids missing prerequisites.
    let s0 = StateIdentity {
        head: "a".repeat(40),
        rules_fingerprint: "sha256:rules-v1".into(),
        verifier: "ci-v1".into(),
    };
    let s1 = StateIdentity {
        head: "b".repeat(40),
        rules_fingerprint: "sha256:rules-v1".into(),
        verifier: "ci-v1".into(),
    };
    let stale_req = AdmissionRequest {
        actor: "agent-1964".into(),
        paths: BTreeSet::from(["src/decapod/lib.rs".into()]),
        forbidden: BTreeSet::new(),
        file_touch_budget: Some(4),
        authorized: s0.clone(),
        other_claims: Vec::new(),
        prerequisites_proven: true,
        own_lease_expired: false,
    };
    let stale_adm = admit(&s1, &stale_req);
    assert_eq!(stale_adm.phase, TransitionPhase::Stale);
    assert_eq!(stale_adm.cause, Some(AdmissionCause::AuthorizationStale));

    let valid_adm = admit(&s0, &stale_req);
    assert_eq!(valid_adm.phase, TransitionPhase::Admissible);
    assert_eq!(valid_adm.cause, Some(AdmissionCause::InScope));

    // 2. Cloud proof transitions reject wrong subject/repo, wrong commit, wrong state, and
    //    self-escalating flags even on authenticated HTTP 200 responses.
    let op_cap = new_uuid_v4().expect("cap uuid");
    let capture_req = ProofCaptureRequest {
        contract: "propodus.proof-capture.v1".into(),
        contract_digest: format!("sha256:{}", "1".repeat(64)),
        operation_id: op_cap.clone(),
        subject: ProofSubject {
            repository_id: "DecapodLabs/decapod".into(),
            task_id: "secu_01test".into(),
            workspace_id: "ws_01test".into(),
            custody_id: "cust_01test".into(),
            task_revision: 3,
            fencing_generation: 2,
            candidate: ProofGitIdentity {
                commit: "a".repeat(40),
                tree: "b".repeat(40),
            },
            accepted_target: ProofGitIdentity {
                commit: "c".repeat(40),
                tree: "d".repeat(40),
            },
            integration: None,
            policy_digest: format!("sha256:{}", "2".repeat(64)),
            requirements_digest: format!("sha256:{}", "3".repeat(64)),
            material_digest: format!("sha256:{}", "4".repeat(64)),
            prerequisites_digest: format!("sha256:{}", "5".repeat(64)),
        },
        evidence: vec![ProofEvidenceReference {
            kind: "suite".into(),
            digest: format!("sha256:{}", "6".repeat(64)),
            bytes: 64,
            provenance_digest: format!("sha256:{}", "7".repeat(64)),
        }],
    };
    let fp = proof_capture_fingerprint(&capture_req).expect("fingerprint");

    // Wrong project_id on capture response fails closed.
    let wrong_repo_cap = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "status": "captured",
        "operation_id": op_cap,
        "project_id": "DecapodLabs/other-repo",
        "task_id": "secu_01test",
    })));
    assert!(
        submit_capture(&wrong_repo_cap, "DecapodLabs/decapod", 3, &capture_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    // Capture falsely claiming acceptance_supported=true fails closed.
    let escalated_cap = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "status": "captured",
        "operation_id": op_cap,
        "project_id": "DecapodLabs/decapod",
        "acceptance_supported": true,
    })));
    assert!(
        submit_capture(&escalated_cap, "DecapodLabs/decapod", 3, &capture_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    // Legitimate capture positive control passes.
    let ok_cap = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "status": "captured",
        "operation_id": op_cap,
        "project_id": "DecapodLabs/decapod",
        "task_id": "secu_01test",
        "fingerprint": fp,
        "acceptance_supported": false,
        "publication_eligible": false,
        "forge_merge_authority": false,
    })));
    assert_eq!(
        submit_capture(&ok_cap, "DecapodLabs/decapod", 3, &capture_req).unwrap()["status"],
        "captured"
    );

    // retire_historical refuses non-retired state or publication_eligible=true, and accepts valid retirement.
    let retire_op = new_uuid_v4().expect("retire op");
    let bad_retire = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "historically_accepted",
        "operation_id": retire_op,
        "attestation_id": op_cap,
        "fingerprint": fp,
        "publication_eligible": true,
        "forge_merge_authority": false,
    })));
    assert!(
        submit_retire_historical(
            &bad_retire,
            "DecapodLabs/decapod",
            &retire_op,
            &op_cap,
            &fp,
            "superseded"
        )
        .unwrap_err()
        .to_string()
        .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    let ok_retire = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "historically_accepted",
        "operation_id": retire_op,
        "attestation_id": op_cap,
        "fingerprint": fp,
        "publication_eligible": false,
        "forge_merge_authority": false,
    })));
    assert_eq!(
        submit_retire_historical(
            &ok_retire,
            "DecapodLabs/decapod",
            &retire_op,
            &op_cap,
            &fp,
            "superseded"
        )
        .unwrap()["state"],
        "historically_accepted"
    );

    // reconcile_operation_after_unknown refuses failed state even when found=true.
    let failed_reconcile = StubProofTransport::default();
    failed_reconcile.reads.borrow_mut().push(Ok(json!({
        "found": true,
        "receipt": {
            "state": "failed",
            "operation_id": op_cap,
            "fingerprint": fp,
        }
    })));
    assert!(
        reconcile_operation_after_unknown(
            &failed_reconcile,
            "DecapodLabs/decapod",
            &op_cap,
            Some(&fp)
        )
        .unwrap_err()
        .to_string()
        .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    // Unused imports check when verify/admit helpers are exercised via unit tests too:
    let verify_req = VerifyProofTransitionRequest {
        project_id: "DecapodLabs/decapod".into(),
        operation_id: new_uuid_v4().expect("v op"),
        attestation_id: op_cap.clone(),
        expected_fingerprint: fp.clone(),
        expected_candidate_commit: "a".repeat(40),
        expected_candidate_tree: "b".repeat(40),
        verified_evidence_digest: format!("sha256:{}", "8".repeat(64)),
        verifier_scope: "independent-ci-v1".into(),
    };
    let mismatched_verify = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "independently_verified",
        "operation_id": verify_req.operation_id,
        "attestation_id": op_cap,
        "fingerprint": fp,
        "candidate": { "commit": "a".repeat(40), "tree": "b".repeat(40) },
        "verified_evidence_digest": format!("sha256:{}", "9".repeat(64)),
        "references_verified": true,
    })));
    assert!(
        submit_verify(&mismatched_verify, &verify_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );
    let admit_req = AdmitOrPublishProofRequest {
        project_id: "DecapodLabs/decapod".into(),
        operation_id: new_uuid_v4().expect("a op"),
        attestation_id: op_cap,
        expected_fingerprint: fp,
        expected_task_revision: 3,
        expected_fencing_generation: 2,
        expected_candidate_commit: "a".repeat(40),
        expected_accepted_target_commit: "c".repeat(40),
        expected_policy_digest: format!("sha256:{}", "2".repeat(64)),
    };
    let bad_admit = StubProofTransport::with_write(Ok(json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "currently_admissible",
        "operation_id": admit_req.operation_id,
        "attestation_id": admit_req.attestation_id,
        "fingerprint": admit_req.expected_fingerprint,
        "task_revision": 3,
        "fencing_generation": 2,
        "candidate": { "commit": "a".repeat(40), "tree": "b".repeat(40) },
        "accepted_target": { "commit": "c".repeat(40), "tree": "d".repeat(40) },
        "policy_digest": format!("sha256:{}", "f".repeat(64)),
        "references_verified": true,
        "logical_custody_verified": true,
        "acceptance_supported": true,
        "publication_eligible": false,
        "forge_merge_authority": false,
    })));
    assert!(
        submit_admit_or_publish(&bad_admit, "admit", &admit_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );
    let _ = (
        CloudControlDenialKind::OutcomeUnknown,
        classify_control_status(Some(409), "conflict", "detail", false),
    );

    // 3. Container signer rejects origin suffix/port/userinfo/fragment smuggling while accepting exact origin.
    {
        use decapod::core::cloud_dpop::DeviceKeyStore;
        use decapod::core::container_signer::{
            ContainerSignRequest, ContainerSignerPolicy, SignerCapabilityKind,
            WorkspaceCredentialsConfig,
        };
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let signer_tmp = TempDir::new().expect("signer tmp");
        let signer_repo = signer_tmp.path().join("repo");
        fs::create_dir_all(&signer_repo).expect("mkdir signer repo");
        let ks = DeviceKeyStore::with_root_dir(
            "https://cloud.example.invalid",
            "DecapodLabs/decapod",
            &signer_repo,
            &signer_tmp.path().join("keys"),
        )
        .expect("keystore");
        let key = ks.load_or_create().expect("key");
        let now = 1_780_000_000;
        let policy = ContainerSignerPolicy {
            allowed_repository: "DecapodLabs/decapod".into(),
            allowed_origin: "https://cloud.example.invalid".into(),
            allowed_agent_id: "agent-1964".into(),
            credentials: WorkspaceCredentialsConfig {
                propodus_cloud: Some("signer-broker".into()),
                ..Default::default()
            },
            session_expires_at: now + 300,
            revoked: Arc::new(AtomicBool::new(false)),
            device_key_store: Some(ks),
            access_token_digest: None,
        };
        let smuggled = policy.sign_request(
            &ContainerSignRequest {
                capability: SignerCapabilityKind::PropodusCloud,
                repository: "DecapodLabs/decapod".into(),
                agent_id: "agent-1964".into(),
                method: "POST".into(),
                url: "https://cloud.example.invalid.evil.com/functions/v1/decapod-onboarding/cloud-query".into(),
                nonce: None,
                aud: Some("https://cloud.example.invalid".into()),
                access_token_sha256: Some("ath_1964".into()),
            },
            now,
            Some(&key),
        );
        assert!(!smuggled.ok, "origin suffix smuggling must fail closed");
        assert_eq!(
            smuggled.code.as_deref(),
            Some("CONTAINER_SIGNER_ORIGIN_DENIED")
        );

        let valid_sig = policy.sign_request(
            &ContainerSignRequest {
                capability: SignerCapabilityKind::PropodusCloud,
                repository: "DecapodLabs/decapod".into(),
                agent_id: "agent-1964".into(),
                method: "POST".into(),
                url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query"
                    .into(),
                nonce: None,
                aud: Some("https://cloud.example.invalid".into()),
                access_token_sha256: Some("ath_1964".into()),
            },
            now,
            Some(&key),
        );
        assert!(
            valid_sig.ok,
            "legitimate signer control failed: {:?}",
            valid_sig
        );
    }

    // 4. Compiled CLI binary: authenticated session + `validate --pre-build` MUST NOT grant
    //    `publication_ready` or `proof_ready`.
    let tmp = TempDir::new().expect("cli tmpdir");
    let repo = tmp.path();
    assert!(
        Command::new("git")
            .current_dir(repo)
            .args(["init", "-q", "-b", "master"])
            .bounded_output(BUILD_TIMEOUT)
            .expect("git init")
            .status
            .success()
    );
    let bin = decapod_bin();
    let init_out = Command::new(&bin)
        .current_dir(repo)
        .args(["init", "--force", "--no-specs", "--no-container-workspaces"])
        .bounded_output(BUILD_TIMEOUT)
        .expect("decapod init");
    assert!(init_out.status.success());
    let pre = Command::new(&bin)
        .current_dir(repo)
        .args(["validate", "--pre-build", "--format", "json"])
        .env("DECAPOD_AGENT_ID", "agent-1964")
        .bounded_output(BUILD_TIMEOUT)
        .expect("decapod validate --pre-build");
    let pre_json: Value = serde_json::from_slice(&pre.stdout).expect("pre-build json");
    assert_eq!(pre_json["publication_ready"], json!(false));
    assert_eq!(pre_json["proof_ready"], json!(false));
}

struct AssuranceFaultSpec {
    rule_id: &'static str,
    source_file: &'static str,
    target_snippet: &'static str,
    fault_replacement: &'static str,
    cargo_args: &'static [&'static str],
}

fn copy_repo_for_isolation(src: &Path, dst: &Path, is_root: bool) {
    fs::create_dir_all(dst).expect("mkdir isolated");
    for entry in fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if is_root
            && (matches!(name_str.as_ref(), "target" | ".git" | ".decapod")
                || name_str.starts_with("bazel-"))
        {
            continue;
        }
        if name_str.ends_with(".bak") {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let meta = fs::symlink_metadata(&src_path).expect("metadata");
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&src_path).expect("read symlink");
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dst_path).expect("symlink");
        } else if meta.is_dir() {
            copy_repo_for_isolation(&src_path, &dst_path, false);
        } else if meta.is_file() {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

fn run_isolated_cargo(repo: &Path, target_dir: &Path, args: &[&str]) -> Output {
    let cargo_bin = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo_bin);
    cmd.current_dir(repo).args(args);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["DECAPOD_", "GIT_", "BUILDKITE", "GITHUB"]
            .iter()
            .any(|p| name.starts_with(p))
        {
            cmd.env_remove(key);
        }
    }
    for key in [
        "CARGO_MANIFEST_DIR",
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION",
        "OUT_DIR",
        "CARGO_BIN_EXE_decapod",
    ] {
        cmd.env_remove(key);
    }
    cmd.env("CARGO_TARGET_DIR", target_dir);
    // Keep isolated fault-injection builds unoptimized so single-fault checks stay well within bounded CI budgets.
    cmd.env("CARGO_PROFILE_DEV_OPT_LEVEL", "0");
    cmd.env("CARGO_PROFILE_TEST_OPT_LEVEL", "0");
    if std::env::var_os("CARGO_BUILD_JOBS").is_none() {
        cmd.env("CARGO_BUILD_JOBS", "2");
    }
    cmd.bounded_output(std::time::Duration::from_secs(1200))
        .expect("isolated cargo execution")
}

fn parsed_counts(stdout: &str) -> (u64, u64) {
    let mut passed = 0u64;
    let mut failed = 0u64;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("test result:") {
            continue;
        }
        for seg in trimmed.split(';') {
            let toks: Vec<&str> = seg.split_whitespace().collect();
            for pair in toks.windows(2) {
                if let Ok(n) = pair[0].parse::<u64>() {
                    match pair[1].trim_end_matches('.') {
                        "passed" => passed += n,
                        "failed" => failed += n,
                        _ => {}
                    }
                }
            }
        }
    }
    (passed, failed)
}

#[test]
fn mutation_checks_detect_weakened_uncovered_security_boundaries() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sandbox = TempDir::new().expect("sandbox tmpdir");
    let iso_repo = sandbox.path().join("repo");
    let iso_target = sandbox.path().join("target");
    copy_repo_for_isolation(&manifest_dir, &iso_repo, true);
    let src_config = manifest_dir.join(".decapod").join("config.toml");
    if src_config.exists() {
        let dst_config = iso_repo.join(".decapod").join("config.toml");
        fs::create_dir_all(dst_config.parent().expect("config parent")).expect("create .decapod");
        fs::copy(&src_config, &dst_config).expect("copy .decapod/config.toml");
    }

    let specs = [
        AssuranceFaultSpec {
            rule_id: "RULE-APPROVAL-006",
            source_file: "src/decapod/core/governed_transition.rs",
            target_snippet: r#"    if request.authorized != *current {
        return Admission {
            phase: TransitionPhase::Stale,
            reason: "authorization was not issued for the current state".to_string(),
            cause: Some(AdmissionCause::AuthorizationStale),
        };
    }"#,
            fault_replacement: "    let _ = (&request.authorized, current);",
            cargo_args: &[
                "test",
                "--offline",
                "--lib",
                "core::governed_transition::tests::authorization_against_s0_is_stale_at_s1",
            ],
        },
        AssuranceFaultSpec {
            rule_id: "RULE-CLOUD-PROOF-008",
            source_file: "src/decapod/core/cloud_proof.rs",
            target_snippet: r#"    assert_optional_str_match(receipt, "project_id", project_id)?;
    assert_required_str_match(receipt, "operation_id", operation_id)?;
    assert_required_str_match(receipt, "attestation_id", attestation_id)?;
    assert_required_str_match(receipt, "fingerprint", expected_fingerprint)?;
    assert_not_escalated_true(receipt, "forge_merge_authority")?;"#,
            fault_replacement: "    let _ = (project_id, operation_id, attestation_id, expected_fingerprint);",
            cargo_args: &[
                "test",
                "--offline",
                "--lib",
                "core::cloud_proof::tests::proof_transitions_reject_mismatched_subject_stale_fingerprint_and_self_escalation",
            ],
        },
        AssuranceFaultSpec {
            rule_id: "RULE-HTTP-READONLY-009",
            source_file: "src/decapod/core/http_transport.rs",
            target_snippet: r#"    if config.read_only_only {
        match request.method.as_str() {"#,
            fault_replacement: r#"    if false && config.read_only_only {
        match request.method.as_str() {"#,
            cargo_args: &[
                "test",
                "--offline",
                "--lib",
                "core::http_transport::tests::read_only_credential_refuses_mutating_rpc_op_and_admits_allowlisted_op",
            ],
        },
        AssuranceFaultSpec {
            rule_id: "RULE-SIGNER-ORIGIN-010",
            source_file: "src/decapod/core/container_signer.rs",
            target_snippet: r#"        let Some(path_part) = extract_origin_relative_path(&req.url, allowed_origin) else {
            return deny(
                "CONTAINER_SIGNER_ORIGIN_DENIED",
                "request URL does not match the bound Cloud origin",
            );
        };"#,
            fault_replacement: r#"        let Some(path_part) = req.url.strip_prefix(allowed_origin) else {
            return deny(
                "CONTAINER_SIGNER_ORIGIN_DENIED",
                "request URL does not match the bound Cloud origin",
            );
        };"#,
            cargo_args: &[
                "test",
                "--offline",
                "--lib",
                "core::container_signer::tests::container_signer_rejects_origin_suffix_port_userinfo_fragment_and_path_smuggling",
            ],
        },
        AssuranceFaultSpec {
            rule_id: "RULE-HTTP-AUTH-011",
            source_file: "src/decapod/core/http_transport.rs",
            target_snippet: r#"    if !verify_bearer_token(
        request.headers.get("authorization").map(String::as_str),
        &config.auth_token,
    ) {"#,
            fault_replacement: r#"    if false
        && !verify_bearer_token(
            request.headers.get("authorization").map(String::as_str),
            &config.auth_token,
        )
    {"#,
            cargo_args: &[
                "test",
                "--offline",
                "--lib",
                "core::http_transport::tests::slow_and_unauthorized_peers_fail_closed_within_timeout",
            ],
        },
    ];

    // 1. Baseline positive + negative controls must pass in the unfaulted isolated copy.
    for spec in &specs {
        let base_out = run_isolated_cargo(&iso_repo, &iso_target, spec.cargo_args);
        let stdout = String::from_utf8_lossy(&base_out.stdout);
        let (passed, failed) = parsed_counts(&stdout);
        assert!(
            base_out.status.success() && passed > 0 && failed == 0,
            "baseline check failed for {}:\nstdout:\n{}\nstderr:\n{}",
            spec.rule_id,
            stdout,
            String::from_utf8_lossy(&base_out.stderr)
        );
    }

    // 2. Single-fault mutation must compile cleanly and be detected (failed > 0, never unexercised).
    for spec in &specs {
        let file = iso_repo.join(spec.source_file);
        let original = fs::read_to_string(&file).expect("read isolated source");
        assert!(
            original.contains(spec.target_snippet),
            "target snippet missing in {} for {}",
            spec.source_file,
            spec.rule_id
        );
        let faulted = original.replacen(spec.target_snippet, spec.fault_replacement, 1);
        assert_ne!(faulted, original, "no-op fault for {}", spec.rule_id);
        fs::write(&file, &faulted).expect("write faulted source");

        let fault_out = run_isolated_cargo(&iso_repo, &iso_target, spec.cargo_args);
        fs::write(&file, &original).expect("restore isolated source");

        let stdout = String::from_utf8_lossy(&fault_out.stdout);
        let stderr = String::from_utf8_lossy(&fault_out.stderr);
        let (passed, failed) = parsed_counts(&stdout);
        assert!(
            !stderr.contains("could not compile") && !stderr.contains("error[E"),
            "faulted {} failed to compile:\n{stderr}",
            spec.rule_id
        );
        assert!(
            passed + failed > 0 && failed > 0 && !fault_out.status.success(),
            "fault {} was NOT detected (passed={passed}, failed={failed}):\nstdout:\n{stdout}",
            spec.rule_id
        );
    }
}
