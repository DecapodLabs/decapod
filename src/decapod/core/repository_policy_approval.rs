//! Independent approval carriers for an exact repository-readiness transition.
use super::invalid;
use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
use crate::core::error::DecapodError;
use crate::core::readiness_profile::PlanningFailure;
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, process::Command};

const SIGNERS_PATH: &str = ".decapod/repository-policy-signers.json";
const SIGNERS_SCHEMA: &str = "decapod.repository-policy-signers.v1";
const APPROVAL_SCHEMA: &str = "decapod.repository-policy-approval.v1";
const MAX_APPROVAL_SECONDS: i64 = 86_400;
pub(super) const SIGNED_APPROVAL_ENV: &str = "DECAPOD_REPO_TRANSITION_APPROVAL";
pub(super) const CLOUD_DECISION_ENV: &str = "DECAPOD_REPO_TRANSITION_DECISION_ID";

pub(crate) struct ApprovalInput {
    pub(super) ci_local: bool,
    pub(super) signed: Option<std::ffi::OsString>,
    pub(crate) ci_target: Option<String>,
}

impl ApprovalInput {
    pub(crate) fn from_environment(ci_local: bool) -> Self {
        Self {
            ci_local,
            signed: std::env::var_os(SIGNED_APPROVAL_ENV),
            ci_target: ["GITHUB_BASE_REF", "BUILDKITE_PULL_REQUEST_BASE_BRANCH"]
                .into_iter()
                .filter_map(|key| std::env::var(key).ok())
                .find(|value| !value.is_empty()),
        }
    }

    #[cfg(test)]
    pub(crate) fn ci_with_signature(signed: String) -> Self {
        Self {
            ci_local: true,
            signed: Some(signed.into()),
            ci_target: Some("main".into()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Signers {
    schema: String,
    keys: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedApproval {
    schema: String,
    key_id: String,
    transition_id: String,
    issued_at: i64,
    expires_at: i64,
    signature: String,
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn signed_approval_is_live(keys: &[u8], envelope: &[u8], expected_id: &str, now: i64) -> bool {
    use ring::signature::{ED25519, UnparsedPublicKey};

    if keys.len() > 16 * 1024 || envelope.len() > 4096 {
        return false;
    }
    let Ok(signers) = serde_json::from_slice::<Signers>(keys) else {
        return false;
    };
    let Ok(approval) = serde_json::from_slice::<SignedApproval>(envelope) else {
        return false;
    };
    if signers.schema != SIGNERS_SCHEMA
        || approval.schema != APPROVAL_SCHEMA
        || approval.transition_id != expected_id
        || approval.issued_at > now
        || approval.expires_at <= now
        || approval.expires_at.saturating_sub(approval.issued_at) > MAX_APPROVAL_SECONDS
        || approval.key_id.is_empty()
        || approval.key_id.len() > 64
        || !approval
            .key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return false;
    }
    let Some(key) = signers
        .keys
        .get(&approval.key_id)
        .and_then(|key| decode_hex::<32>(key))
    else {
        return false;
    };
    let Some(signature) = decode_hex::<64>(&approval.signature) else {
        return false;
    };
    let Ok(message) = serde_json::to_vec(&(
        APPROVAL_SCHEMA,
        &approval.key_id,
        &approval.transition_id,
        approval.issued_at,
        approval.expires_at,
    )) else {
        return false;
    };
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&message, &signature)
        .is_ok()
}

pub(crate) fn git_object(root: &Path, args: &[&str]) -> Result<Vec<u8>, PlanningFailure> {
    let git = crate::readiness_installed_tool(root, "git")
        .map_err(|_| invalid("POLICY_APPROVAL_GIT_UNAVAILABLE: trusted Git is required"))?;
    let mut command = Command::new(git);
    command
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_NAMESPACE")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env("GIT_CONFIG_COUNT", "0")
        .arg("--no-replace-objects")
        .args(args);
    let output = command.bounded_output(CONTROL_TIMEOUT).map_err(|_| {
        invalid("POLICY_APPROVAL_GIT_UNAVAILABLE: could not inspect the exact Git subject")
    })?;
    if !output.status.success() {
        return Err(invalid(
            "POLICY_APPROVAL_GIT_UNAVAILABLE: could not inspect the exact Git subject",
        ));
    }
    Ok(output.stdout)
}

pub(super) fn verify_signed(
    root: &Path,
    base: &str,
    transition_id: &str,
    envelope: &[u8],
) -> Result<bool, PlanningFailure> {
    let keys = git_object(root, &["show", &format!("{base}:{SIGNERS_PATH}")])?;
    Ok(signed_approval_is_live(
        &keys,
        envelope,
        transition_id,
        time::OffsetDateTime::now_utc().unix_timestamp(),
    ))
}

pub(super) fn cloud_backend(root: &Path, base: &str) -> Result<bool, PlanningFailure> {
    #[derive(Deserialize, Default)]
    struct BackendConfig {
        #[serde(default)]
        repo: crate::cli::RepoContext,
    }
    let read = |bytes: &[u8]| -> Result<bool, PlanningFailure> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            invalid("POLICY_APPROVAL_CONFIG_INVALID: expected UTF-8 project policy")
        })?;
        let config: BackendConfig = toml::from_str(text).map_err(|_| {
            invalid("POLICY_APPROVAL_CONFIG_INVALID: could not read repository backend")
        })?;
        Ok(config.repo.effective_backend().is_cloud())
    };
    if read(&git_object(
        root,
        &["show", &format!("{base}:.decapod/config.toml")],
    )?)? {
        return Ok(true);
    }
    let candidate = std::fs::read(root.join(".decapod/config.toml"))
        .map_err(|_| invalid("POLICY_APPROVAL_CONFIG_INVALID: missing candidate configuration"))?;
    read(&candidate)
}

#[cfg(feature = "supabase-cloud")]
mod cloud {
    use super::*;
    use dactyl_db::{OperationKind, ResourceBudget};
    use serde_json::{Value, json};

    const GOVERNANCE_RPC: &str = "propodus_governance_v1";
    const CAPABILITY: &str = "governance.exception.authorize";

    struct Subject<'a> {
        base: &'a str,
        head: &'a str,
        tree: String,
        obligation: String,
    }

    impl<'a> Subject<'a> {
        fn new(
            root: &Path,
            base: &'a str,
            head: &'a str,
            transition_id: &str,
        ) -> Result<Self, DecapodError> {
            if decode_hex::<20>(base).is_none()
                || decode_hex::<20>(head).is_none()
                || decode_hex::<32>(transition_id).is_none()
            {
                return Err(error("CLOUD_POLICY_APPROVAL_INVALID_SUBJECT"));
            }
            let tree = git_text(
                root,
                &["rev-parse", "--verify", &format!("{head}^{{tree}}")],
            )?;
            Ok(Self {
                base,
                head,
                tree,
                obligation: format!("{}:{transition_id}", super::super::POLICY_TRANSITION_SCOPE),
            })
        }

        fn request(&self, action: &str, project: &str, decision: &str) -> Value {
            json!({
                "version": 1,
                "action": action,
                "project_id": project,
                "decision_id": decision,
                "capability": CAPABILITY,
                "obligation_id": self.obligation,
                "base_commit": self.base,
                "candidate_commit": self.head,
                "affected_commit": self.head,
                "tree_oid": self.tree
            })
        }

        fn matches_decision(&self, response: &Value, project: &str, decision: &str) -> bool {
            response["version"] == 1
                && response["project_id"] == project
                && response["decision_id"] == decision
                && response["capability"] == CAPABILITY
                && response["obligation_id"] == self.obligation
                && response["base_commit"] == self.base
                && response["candidate_commit"] == self.head
                && response["affected_commit"] == self.head
                && response["tree_oid"] == self.tree
        }

        fn decision_is_live(
            &self,
            response: &Value,
            project: &str,
            decision: &str,
            now: time::OffsetDateTime,
        ) -> bool {
            use time::format_description::well_known::Rfc3339;
            self.matches_decision(response, project, decision)
                && response["status"] == "valid_historical_exception"
                && response["current_generation_matches"] == true
                && matches!(
                    response["authority_basis"].as_str(),
                    Some("verified_repo_admin" | "delegated_repo_lead")
                )
                && ["expires_at", "authority_expires_at"].iter().all(|field| {
                    response[field]
                        .as_str()
                        .and_then(|value| time::OffsetDateTime::parse(value, &Rfc3339).ok())
                        .is_some_and(|expires| expires > now)
                })
        }
    }

    fn error(message: &str) -> DecapodError {
        DecapodError::ValidationError(message.into())
    }

    fn git_text(root: &Path, args: &[&str]) -> Result<String, DecapodError> {
        let bytes = git_object(root, args)
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_GIT_SUBJECT_UNAVAILABLE"))?;
        String::from_utf8(bytes)
            .map(|text| text.trim().to_owned())
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_GIT_SUBJECT_INVALID"))
    }

    fn uuid(value: &str) -> bool {
        let sizes = [8, 4, 4, 4, 12];
        value.len() == 36
            && value.split('-').zip(sizes).all(|(part, size)| {
                part.len() == size && part.bytes().all(|b| b.is_ascii_hexdigit())
            })
    }

    pub(super) fn verify(
        root: &Path,
        base: &str,
        head: &str,
        transition_id: &str,
        decision: &str,
    ) -> Result<bool, DecapodError> {
        if !uuid(decision) {
            return Err(error("CLOUD_POLICY_APPROVAL_DECISION_ID_INVALID"));
        }
        let subject = Subject::new(root, base, head, transition_id)?;
        let identity = crate::core::repo_identity::resolve_repository_identity(root)?;
        let (connector, routed, bearer) =
            crate::core::cloud_client::connect_control(root, &identity)?;
        let request = subject.request("verify_decision", &routed.canonical_name, decision);
        let response = connector
            .control_request(
                GOVERNANCE_RPC,
                &request,
                Some(&bearer),
                OperationKind::Read,
                ResourceBudget::default(),
            )
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_DECISION_UNAVAILABLE: verify the exact decision ID and authenticated repository access"))?;
        Ok(subject.decision_is_live(
            &response,
            &routed.canonical_name,
            decision,
            time::OffsetDateTime::now_utc(),
        ))
    }

    pub(super) fn approve(
        root: &Path,
        transition_id: &str,
        lifetime: u64,
    ) -> Result<String, DecapodError> {
        use ring::rand::{SecureRandom, SystemRandom};

        if !(60..=MAX_APPROVAL_SECONDS as u64).contains(&lifetime) {
            return Err(error(
                "CLOUD_POLICY_APPROVAL_EXPIRY_INVALID: require 60 through 86400 seconds",
            ));
        }
        let branch = crate::core::workspace::resolve_base_branch(root, None);
        let base = git_text(
            root,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/remotes/origin/{branch}^{{commit}}"),
            ],
        )?;
        let head = git_text(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
        let accepted = git_object(root, &["show", &format!("{base}:.decapod/config.toml")])
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_GIT_SUBJECT_UNAVAILABLE"))?;
        let candidate = git_object(root, &["show", &format!("{head}:.decapod/config.toml")])
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_GIT_SUBJECT_UNAVAILABLE"))?;
        let expected = super::super::transition_id_from_config(&base, &head, &accepted, &candidate)
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_INVALID_SUBJECT: review the accepted target and candidate contracts"))?;
        if expected != transition_id {
            return Err(error(
                "CLOUD_POLICY_APPROVAL_TRANSITION_MISMATCH: rerun validation for the exact base and head",
            ));
        }
        let subject = Subject::new(root, &base, &head, transition_id)?;
        let mut random = [0_u8; 16];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| error("CLOUD_POLICY_APPROVAL_DECISION_ID_UNAVAILABLE"))?;
        random[6] = (random[6] & 0x0f) | 0x40;
        random[8] = (random[8] & 0x3f) | 0x80;
        let raw = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let decision = format!(
            "{}-{}-{}-{}-{}",
            &raw[..8],
            &raw[8..12],
            &raw[12..16],
            &raw[16..20],
            &raw[20..]
        );

        let identity = crate::core::repo_identity::resolve_repository_identity(root)?;
        let (connector, routed, bearer) =
            crate::core::cloud_client::connect_control(root, &identity)?;
        let mut request = subject.request("authorize_exception", &routed.canonical_name, &decision);
        request["expires_in_seconds"] = json!(lifetime);
        request["reason"] = json!(
            "Maintainer approval of the exact repository-readiness policy transition. Required proof still runs."
        );
        let response = connector
            .control_request(
                GOVERNANCE_RPC,
                &request,
                Some(&bearer),
                OperationKind::Write,
                ResourceBudget::default(),
            )
            .map_err(|_| DecapodError::ValidationError(format!(
                "CLOUD_POLICY_APPROVAL_UNCONFIRMED: the service did not confirm the request. Check repository governance authority and look up decision {decision} before retrying."
            )))?;
        if !subject.matches_decision(&response, &routed.canonical_name, &decision)
            || !matches!(
                response["status"].as_str(),
                Some("authorized" | "already_authorized")
            )
        {
            return Err(error("CLOUD_POLICY_APPROVAL_DECISION_MISMATCH"));
        }
        Ok(decision)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn cloud_decision_requires_exact_live_repository_authority() {
            let subject = Subject {
                base: "base",
                head: "head",
                tree: "tree".into(),
                obligation: "repo.pr-readiness.v1:transition".into(),
            };
            let mut approved = subject.request("verify_decision", "owner/repo", "decision");
            approved["status"] = json!("valid_historical_exception");
            approved["current_generation_matches"] = json!(true);
            approved["authority_basis"] = json!("verified_repo_admin");
            approved["expires_at"] = json!("1970-01-01T00:00:10Z");
            approved["authority_expires_at"] = json!("1970-01-01T00:00:10Z");
            let now = time::OffsetDateTime::from_unix_timestamp(5).unwrap();
            assert!(subject.decision_is_live(&approved, "owner/repo", "decision", now));
            assert!(!subject.decision_is_live(&approved, "owner/other", "decision", now));
            assert!(!subject.decision_is_live(&approved, "owner/repo", "other", now));
            for field in [
                "base_commit",
                "candidate_commit",
                "tree_oid",
                "obligation_id",
                "authority_basis",
            ] {
                let mut mismatch = approved.clone();
                mismatch[field] = json!("wrong");
                assert!(!subject.decision_is_live(&mismatch, "owner/repo", "decision", now));
            }
            approved["current_generation_matches"] = json!(false);
            assert!(!subject.decision_is_live(&approved, "owner/repo", "decision", now));
            approved["current_generation_matches"] = json!(true);
            approved["authority_expires_at"] = json!("1970-01-01T00:00:05Z");
            assert!(!subject.decision_is_live(&approved, "owner/repo", "decision", now));
        }
    }
}

#[cfg(feature = "supabase-cloud")]
pub(super) fn verify_cloud(
    root: &Path,
    base: &str,
    head: &str,
    transition_id: &str,
    decision: &str,
) -> Result<bool, PlanningFailure> {
    cloud::verify(root, base, head, transition_id, decision).map_err(|_| {
        invalid("CLOUD_POLICY_APPROVAL_DECISION_UNAVAILABLE: verify the exact decision ID and authenticated repository access")
    })
}

#[cfg(not(feature = "supabase-cloud"))]
pub(super) fn verify_cloud(
    _root: &Path,
    _base: &str,
    _head: &str,
    _transition_id: &str,
    _decision: &str,
) -> Result<bool, PlanningFailure> {
    Err(invalid(
        "CLOUD_POLICY_APPROVAL_UNSUPPORTED: this build requires the supabase-cloud capability",
    ))
}

pub(crate) fn approve_cloud(
    root: &Path,
    transition_id: &str,
    expires_in_seconds: u64,
) -> Result<String, DecapodError> {
    #[cfg(feature = "supabase-cloud")]
    return cloud::approve(root, transition_id, expires_in_seconds);
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (root, transition_id, expires_in_seconds);
        Err(DecapodError::NotImplemented(
            "CLOUD_POLICY_APPROVAL_UNSUPPORTED: this build requires the supabase-cloud capability"
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::json;

    fn hex(value: &[u8]) -> String {
        value.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn signature_requires_accepted_key_exact_transition_and_live_expiry() {
        let issuer = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let stranger = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        let keys = serde_json::to_vec(&json!({
            "schema": SIGNERS_SCHEMA,
            "keys": {"maintainer": hex(issuer.public_key().as_ref())}
        }))
        .unwrap();
        let id = "f".repeat(64);
        let receipt = |signer: &Ed25519KeyPair, expires| {
            let message =
                serde_json::to_vec(&(APPROVAL_SCHEMA, "maintainer", &id, 10, expires)).unwrap();
            serde_json::to_vec(&json!({
                "schema": APPROVAL_SCHEMA, "key_id": "maintainer", "transition_id": id,
                "issued_at": 10, "expires_at": expires,
                "signature": hex(signer.sign(&message).as_ref())
            }))
            .unwrap()
        };
        let valid = receipt(&issuer, 20);
        assert!(signed_approval_is_live(&keys, &valid, &id, 11));
        assert!(!signed_approval_is_live(&keys, &valid, &id, 9));
        assert!(!signed_approval_is_live(&keys, &valid, &id, 20));
        assert!(!signed_approval_is_live(&keys, &valid, &"e".repeat(64), 11));
        assert!(!signed_approval_is_live(
            &keys,
            &receipt(&stranger, 20),
            &id,
            11
        ));
        assert!(!signed_approval_is_live(
            &keys,
            &receipt(&issuer, 86_411),
            &id,
            11
        ));
        assert!(!signed_approval_is_live(&keys, b"", &id, 11));
    }
}
