//! Read-only composition of existing policy, custody, artifact and proof owners.
//! A diagnostic never mints live publication authority or imports a PASS claim.
use super::{
    bounded_process::BoundedCommand,
    db_maintenance as policy,
    error::DecapodError,
    governance_artifacts,
    store::{Store, StoreKind},
    todo, validation_history, workspace,
};
use crate::cli::DecapodProjectConfig;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::IsTerminal, path::Path, process::Command, time::Duration};

const SCHEMA: &str = "decapod.workspace-diagnostic.v1";
const OVERRIDE: &str = ".decapod/OVERRIDE.md";
const REPAIR: &str = "decapod docs show config-schema --source embedded";

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .bounded_output(Duration::from_secs(5))
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn block(blockers: &mut Vec<Value>, code: &str, action: &str) {
    if !blockers.iter().any(|b| b["code"] == code) {
        blockers.push(json!({"code":code, "next_action":action}));
    }
}

/// Proposal comparison only. These bytes never select policy or grant a role.
fn proposal(
    root: &Path,
    accepted: Option<&DecapodProjectConfig>,
    accepted_bytes: Option<&[u8]>,
) -> (Value, bool) {
    let path = root.join(".decapod/config.toml");
    let bytes = policy::read_regular_policy_bytes(&path).ok();
    let parsed = bytes
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|text| DecapodProjectConfig::parse(text).ok());
    match parsed {
        None => (
            json!({"status":"invalid", "source":"candidate:.decapod/config.toml",
            "code":"INVALID_CONFIG_SCHEMA", "repair_command":REPAIR,
            "digest":bytes.as_deref().map(digest)}),
            false,
        ),
        Some(candidate) => {
            let mut changed = Vec::new();
            if let Some(accepted) = accepted {
                let candidate = serde_json::to_value(candidate).unwrap_or(Value::Null);
                let accepted = serde_json::to_value(accepted).unwrap_or(Value::Null);
                for section in [
                    "governance",
                    "maintenance",
                    "proof",
                    "custody",
                    "laya",
                    "repo",
                    "tracker",
                    "context",
                    "init",
                ] {
                    if candidate[section] != accepted[section] {
                        changed.push(section);
                    }
                }
            }
            let same_bytes = bytes
                .as_deref()
                .zip(accepted_bytes)
                .is_some_and(|(a, b)| a == b);
            if changed.is_empty() && !same_bytes {
                changed.push("extensions_or_formatting");
            }
            (
                json!({"status":if same_bytes{"matches_accepted"}else{"proposed"},
                "source":"candidate:.decapod/config.toml", "digest":bytes.as_deref().map(digest),
                "changed_sections":changed, "grants_authority":false}),
                true,
            )
        }
    }
}

fn override_proposal(root: &Path, main: &Path, revision: Option<&str>) -> (Value, Option<String>) {
    let candidate = policy::read_regular_policy_bytes(&root.join(OVERRIDE))
        .ok()
        .map(|bytes| digest(&bytes));
    let (status, accepted_blob) = match revision.and_then(|value| value.split_once(':')) {
        Some((commit, _))
            if commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            let object = format!("{commit}:{OVERRIDE}");
            let blob = git(main, &["rev-parse", "--verify", &object]);
            let candidate_blob = candidate
                .as_ref()
                .and_then(|_| git(root, &["hash-object", "--no-filters", "--", OVERRIDE]));
            let status = if candidate_blob == blob {
                "matches_accepted"
            } else {
                "proposed"
            };
            (status, blob)
        }
        _ => ("accepted_unavailable", None),
    };
    (
        json!({"status":status,"source":OVERRIDE,"accepted_git_blob":accepted_blob,
            "candidate_digest":candidate,"grants_authority":false}),
        candidate,
    )
}

pub(crate) fn inspect(root: &Path, operation: &str, pr: Option<u64>, offline: bool) -> Value {
    let root = git(root, &["rev-parse", "--show-toplevel"])
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    let main = workspace::get_main_repo_root(&root).unwrap_or_else(|_| root.clone());
    let store = Store {
        kind: StoreKind::Repo,
        root: main.join(".decapod/data"),
    };
    let actor = crate::current_agent_id();
    let mut blockers = Vec::new();
    let resolved = policy::resolve_repository_policy(&main);
    let accepted = resolved.as_ref().ok().map(|(_, config, _, _)| config);
    let mut authority = match &resolved {
        Ok((bytes, _, revision, _)) => {
            json!({"status":"resolved", "source":"canonical_repository_policy",
            "revision":revision,"digest":digest(bytes),"assurance":"cooperative_local_git",
            "independently_authenticated_human":false})
        }
        Err((code, _)) => {
            block(
                &mut blockers,
                code,
                "Inspect accepted repository policy and its protected Git reference; candidate proposals cannot supply missing authority",
            );
            json!({"status":"unavailable","code":code,"source":"canonical_repository_policy",
                "independently_authenticated_human":false})
        }
    };
    let (mut candidate, schema_valid) = proposal(
        &root,
        accepted,
        resolved
            .as_ref()
            .ok()
            .map(|(bytes, _, _, _)| bytes.as_slice()),
    );
    let (mut proposed_override, override_digest) =
        override_proposal(&root, &main, authority["revision"].as_str());
    if !schema_valid {
        block(&mut blockers, "INVALID_CONFIG_SCHEMA", REPAIR);
    }
    let status = workspace::get_workspace_status(&root).ok();
    let checkout = workspace::observe_checkout(&root);
    let head = git(&root, &["rev-parse", "--verify", "HEAD"]);
    let branch = git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let base_branch = accepted
        .and_then(|c| c.repo.base_branch.as_deref())
        .unwrap_or("master");
    let base_ref = format!("refs/remotes/origin/{base_branch}");
    let local_base = git(&root, &["rev-parse", "--verify", &base_ref]).or_else(|| {
        git(
            &main,
            &[
                "rev-parse",
                "--verify",
                &format!("refs/heads/{base_branch}"),
            ],
        )
    });
    let merge_base = local_base
        .as_ref()
        .and_then(|base| git(&root, &["merge-base", base, "HEAD"]));
    let divergence = local_base.as_ref().and_then(|base| {
        git(
            &root,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{base}"),
            ],
        )
    });
    let inventory = governance_artifacts::inventory(&root, Some(base_branch), false).ok();
    let mut artifacts = inventory
        .as_ref()
        .and_then(|i| serde_json::to_value(i).ok())
        .unwrap_or(json!({"status":"unavailable"}));
    // Parser errors may include excerpts of rejected input. Expose the
    // structured validity result and the existing repair route only.
    if let Some(entries) = artifacts.get_mut("artifacts").and_then(Value::as_array_mut) {
        for entry in entries {
            if let Some(entry) = entry.as_object_mut() {
                entry.remove("schema_error");
            }
        }
    }
    let task_id = super::trajectory::load_trajectory_cookie(&root)
        .ok()
        .flatten()
        .and_then(|t| t.task_id);
    let custody = task_id
        .as_deref()
        .map(|task| todo::capture_execution_custody(&store.root, task, &actor));
    let task = match custody {
        Some(Ok(value)) => json!({
            "task_id": value["task_id"], "status": value["status"],
            "assignee": value["assignee"], "revision": value["revision"],
            "lease_generation": value["lease_generation"], "lease_expires_at": value["lease_expires_at"],
            "custody": "active_exclusive", "source": "canonical_task_custody"
        }),
        _ => json!({"status":"unavailable","task_id":task_id,"source":"canonical_task_custody"}),
    };
    let mut eligible = match operation {
        "validate.bypass" | "todo.journal.repair" | "todo.legacy.reconcile" => {
            let permitted =
                crate::plugins::policy::authorize_rbac_exception(&store.root, &actor, operation)
                    .is_ok();
            if !permitted {
                block(
                    &mut blockers,
                    "RBAC_PERMISSION_DENIED",
                    "Submit the bounded policy proposal for independent acceptance; ordinary configuration repair cannot grant an exceptional role",
                );
            }
            Some(permitted)
        }
        "workspace.authoring" => {
            let permitted = status.as_ref().is_some_and(|s| s.can_work)
                && checkout.registered
                && workspace::is_canonical_decapod_workspace_path(&root);
            if !permitted {
                block(
                    &mut blockers,
                    "AUTHORING_CUSTODY_REQUIRED",
                    "decapod workspace ensure",
                );
            }
            Some(permitted)
        }
        "workspace.publish" => {
            block(
                &mut blockers,
                "LIVE_PUBLICATION_RECHECK_REQUIRED",
                "decapod workspace publish",
            );
            None
        }
        name if name.starts_with("db.") => match policy::resolve_canonical_policy(&root) {
            Ok(policy) => {
                let decision = policy::evaluate_named_capability(&policy, name, None);
                let allowed = decision.decision == policy::CapabilityDecision::Allowed;
                if !allowed {
                    block(
                        &mut blockers,
                        &decision.reason_code,
                        "Inspect canonical maintenance capability policy; submit any privilege proposal for independent acceptance",
                    );
                }
                if let Err((code, _)) = policy::verify_actor_and_custody(
                    &root,
                    &policy,
                    None,
                    &crate::verify_strict_local_agent_session,
                ) {
                    block(
                        &mut blockers,
                        code,
                        "Use the existing active agent session and its claimed worktree; do not impersonate a different actor",
                    );
                }
                Some(allowed)
            }
            Err((code, _)) => {
                block(&mut blockers, code, REPAIR);
                Some(false)
            }
        },
        _ => {
            block(
                &mut blockers,
                "DIAGNOSTIC_OPERATION_UNSUPPORTED",
                "Select workspace.publish, workspace.authoring, a modeled db capability or a supported RBAC exception",
            );
            None
        }
    };
    let mut proof = head.as_deref().and_then(|sha| validation_history::lookup(&root,&store,sha).ok())
        .map(|proof| proof.diagnostic_summary(&root,offline))
        .unwrap_or(json!({"status":"not_proven","source":"canonical_full_validation_history","publication_ready":false}));
    let repository = workspace::resolve_readiness_repository(&root).ok();
    let remote = if offline {
        None
    } else {
        repository
            .as_ref()
            .filter(|r| !r.owner.is_empty())
            .and_then(|repository| {
                git(
                    &root,
                    &[
                        "ls-remote",
                        "--refs",
                        "--",
                        &repository.remote_url,
                        &format!("refs/heads/{base_branch}"),
                    ],
                )
            })
    };
    let observed_base = remote
        .as_deref()
        .and_then(|line| line.split_once('\t'))
        .filter(|(oid, reference)| {
            matches!(oid.len(), 40 | 64)
                && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
                && *reference == format!("refs/heads/{base_branch}")
        })
        .map(|(oid, _)| oid)
        .map(str::to_owned);
    if proof["accepted_base"]
        .as_str()
        .zip(observed_base.as_deref().or(local_base.as_deref()))
        .is_some_and(|(before, now)| before != now)
    {
        proof["status"] = "stale_accepted_base".into();
    }
    let mut target_pr = if offline {
        None
    } else {
        repository.as_ref().and_then(|r| {
            pr.and_then(|n| workspace::observe_pr_for_diagnostic(&root, &r.canonical_name, n).ok())
        })
    };
    let checkout_matches_pr = target_pr.as_ref().is_some_and(|pr| {
        pr["head"].as_str() == head.as_deref()
            && pr["head_branch"].as_str() == branch.as_deref()
            && pr["base_branch"].as_str() == Some(base_branch)
    });
    let forge_checks = target_pr
        .as_ref()
        .and_then(|pr| pr["required_forge_checks"].as_array());
    let failing_checks = forge_checks.is_some_and(|checks| {
        checks.iter().any(|check| {
            matches!(
                check["bucket"].as_str(),
                Some("fail" | "cancel" | "skipping")
            )
        })
    });
    let pending_checks =
        forge_checks.is_some_and(|checks| checks.iter().any(|check| check["bucket"] == "pending"));
    let passing_checks = forge_checks.is_some_and(|checks| {
        !checks.is_empty() && checks.iter().all(|check| check["bucket"] == "pass")
    });
    let container_required = accepted.map(|c| c.repo.container_workspaces);
    let in_container = status.as_ref().map(|s| s.container.in_container);
    if operation == "workspace.publish" {
        if container_required == Some(true) && in_container == Some(false) {
            block(
                &mut blockers,
                "CONTAINER_PROOF_UNESTABLISHED",
                "Use decapod workspace ensure --container when the candidate requires container validation",
            );
        }
        if inventory.as_ref().is_some_and(|i| i.dirty.blocked) {
            block(
                &mut blockers,
                "DIRTY_FILE_LIMIT_BLOCKED",
                "Inspect the dirty file groups and checkpoint the intended changes",
            );
        }
        if observed_base.is_none() {
            block(
                &mut blockers,
                "REMOTE_AUTHORITY_UNVERIFIED",
                "Retry diagnosis with network access, then use the governed publisher for live exact-candidate validation",
            );
        }
        if !inventory
            .as_ref()
            .is_some_and(|i| i.all_present && i.all_valid && i.all_semantically_current)
        {
            block(
                &mut blockers,
                "GOVERNANCE_ARTIFACTS_NOT_CURRENT",
                "decapod govern artifacts inventory",
            );
        }
        if observed_base
            .as_ref()
            .is_some_and(|base| Some(base) != local_base.as_ref())
        {
            block(
                &mut blockers,
                "STALE_ACCEPTED_BASE",
                "Fetch the configured accepted branch, reconcile the candidate and obtain fresh full validation",
            );
        }
        if pr.is_some() && target_pr.is_none() {
            block(
                &mut blockers,
                "PR_TARGET_UNVERIFIED",
                "Check access to the requested PR and retry online",
            );
        }
        if failing_checks {
            block(
                &mut blockers,
                "REQUIRED_FORGE_CHECK_BLOCKED",
                "gh pr checks --required",
            );
        }
        if pending_checks {
            block(
                &mut blockers,
                "REQUIRED_FORGE_CHECK_PENDING",
                "gh pr checks --required",
            );
        }
        if target_pr
            .as_ref()
            .is_some_and(|pr| !checkout_matches_pr || pr["state"] != "open")
        {
            block(
                &mut blockers,
                "PR_TARGET_MISMATCH",
                "Reconcile the exact workspace and PR branch before governed publication",
            );
        }
    }
    // One bounded observed snapshot, not a grant for a later mutation. Reuse
    // the canonical source again after independent reads to fence mixed epochs.
    if let Ok((original, _, revision, _)) = &resolved
        && !policy::resolve_repository_policy(&main)
            .is_ok_and(|(bytes, _, current, _)| bytes == *original && current == *revision)
    {
        authority["status"] = "changed_during_diagnosis".into();
        eligible = None;
        block(
            &mut blockers,
            "POLICY_SNAPSHOT_STALE",
            "Retry diagnosis against the current accepted policy; earlier policy eligibility cannot authorize the changed operation",
        );
    }
    let final_candidate =
        policy::read_regular_policy_bytes(&root.join(".decapod/config.toml")).ok();
    let final_override = policy::read_regular_policy_bytes(&root.join(OVERRIDE))
        .ok()
        .map(|bytes| digest(&bytes));
    if final_override != override_digest {
        proposed_override["status"] = "changed_during_diagnosis".into();
        eligible = None;
        block(
            &mut blockers,
            "DIAGNOSTIC_INPUTS_CHANGED",
            "Retry diagnosis after completing the candidate edit",
        );
    }
    let checkout_unchanged = git(&root, &["rev-parse", "--verify", "HEAD"]) == head
        && git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) == branch;
    if final_candidate
        .as_deref()
        .map(digest)
        .map(Value::String)
        .unwrap_or(Value::Null)
        != candidate.get("digest").cloned().unwrap_or(Value::Null)
        || !checkout_unchanged
    {
        candidate["status"] = "changed_during_diagnosis".into();
        eligible = None;
        block(
            &mut blockers,
            "DIAGNOSTIC_INPUTS_CHANGED",
            "Retry diagnosis after completing the candidate edit",
        );
    }
    let exit_code = if !schema_valid {
        2
    } else if operation == "workspace.publish" && (offline || observed_base.is_none()) {
        3
    } else if blockers.is_empty() {
        0
    } else {
        1
    };
    let protected_root = root == main && status.as_ref().is_some_and(|s| s.git.is_protected);
    let workspace_status = status.as_ref().map(|s| {
        json!({
            "name": root.file_name(), "path": root, "registered": checkout.registered,
            "can_work": s.can_work, "in_container": s.container.in_container,
            "runtime_available": s.container.docker_available, "source": "workspace.status"
        })
    });
    let reported_operation = if blockers
        .iter()
        .any(|b| b["code"] == "DIAGNOSTIC_OPERATION_UNSUPPORTED")
    {
        "unsupported"
    } else {
        operation
    };
    let mut next_actions: Vec<String> = Vec::new();
    for blocker in &blockers {
        if let Some(action) = blocker["next_action"].as_str()
            && !next_actions.iter().any(|existing| existing == action)
        {
            next_actions.push(action.to_string());
        }
    }
    let dirty_files = inventory
        .as_ref()
        .and_then(|i| serde_json::to_value(&i.dirty).ok())
        .unwrap_or(json!({"status":"unavailable"}));
    if let Some(pr) = target_pr.as_mut() {
        pr["matches_workspace_candidate"] = json!(checkout_matches_pr && checkout_unchanged);
    }
    super::trace::redact(
        json!({"schema_version":SCHEMA,"exit_code":exit_code,"read_only":true,
        "repository":{"root":main,"protected_root":protected_root},
        "checkout":{"path":root,"branch":branch,"head":head,"registered":checkout.registered,
            "upstream":git(&root,&["rev-parse","--abbrev-ref","--symbolic-full-name","@{upstream}"])},
        "workspace":workspace_status,"active_todo":task,
        "base_branch":{"name":base_branch,"local_sha":local_base,"observed_remote_sha":observed_base,
            "source":"accepted_repository_config_and_git","remote_verification":if offline{"offline_unverified"}else if observed_base.is_some(){"observed"}else{"unavailable"}},
        "divergence":{"ahead_behind":divergence,"merge_base":merge_base,"source":"local_git"},
        "backend":{"value":accepted.map(|c|c.repo.effective_backend()),"source":"accepted_repository_config"},
        "tracker":{"value":accepted.map(|c|&c.tracker.provider),"source":"accepted_repository_config"},
        "effective_authority":authority,"candidate_config":candidate,"candidate_override":proposed_override,
        "operation":{"name":reported_operation,"actor":actor,"actor_source":"DECAPOD_AGENT_ID","allowed_by_policy":eligible,
            "ready":eligible==Some(true)&&schema_valid&&blockers.is_empty(),"scope":"local_policy_eligibility"},
        "credentials":{"github_api":{"status":if target_pr.is_some(){"observed_read"}else{"unavailable"}},
            "git_transport":{"status":if observed_base.is_some(){"observed_read"}else{"unavailable"},"write_verified":false},
            "container_broker":{"status":"unavailable","dependency":"1066"},
            "cloud_identity":{"status":"unverified","independently_authenticated":false}},
        "container_proof":{"required":container_required,"status":match (container_required, in_container) {(Some(false),_)=>"not_required",(Some(true),Some(true))=>"runtime_detected_proof_unverified",(Some(true),Some(false))=>"unestablished",_=>"unavailable"},"captured":null,"source":"accepted_repository_config_and_workspace_status"},
        "dirty_files":dirty_files,
        "governance_artifacts":artifacts,"proof":proof,"target_pr":target_pr,
        "remote_check_readiness":{"status":if failing_checks{"blocked"}else if pending_checks{"pending"}else if passing_checks{"forge_required_checks_passing"}else{"unverified"},
            "source":"github_required_pr_checks","complete_proof_profile_verified":false},
        "publication_readiness":{"can_publish":false,"live_recheck_required":true,"source":"existing_governed_publisher"},
        "merge_authorized":false,"blockers":blockers,"next_actions":next_actions}),
    )
}

pub(crate) fn run(
    root: &Path,
    operation: &str,
    pr: Option<u64>,
    offline: bool,
    machine: bool,
) -> Result<(), DecapodError> {
    let report = inspect(root, operation, pr, offline);
    if machine {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(std::io::Error::other)?
        );
    } else {
        let code = report["exit_code"].as_i64().unwrap_or(3);
        let label = if code == 0 {
            "READY FOR REQUESTED LOCAL OPERATION"
        } else {
            "BLOCKED / UNPROVEN"
        };
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        println!(
            "Decapod workspace diagnostic: {}{}{}",
            if color {
                if code == 0 { "\x1b[32m" } else { "\x1b[33m" }
            } else {
                ""
            },
            label,
            if color { "\x1b[0m" } else { "" }
        );
        println!(
            "Branch: {} @ {}",
            report["checkout"]["branch"], report["checkout"]["head"]
        );
        println!(
            "Accepted policy: {} ({})",
            report["effective_authority"]["digest"], report["effective_authority"]["revision"]
        );
        println!("Candidate config: {}", report["candidate_config"]["status"]);
        println!(
            "Operation: {} (policy eligibility: {})",
            report["operation"]["name"], report["operation"]["allowed_by_policy"]
        );
        println!(
            "Proof: {}; publication needs live governed recheck",
            report["proof"]["status"]
        );
        if let Some(pr) = report["target_pr"].as_object() {
            println!(
                "PR #{}: {} (matches workspace candidate: {})",
                pr["number"], pr["merge_observation"]["state"], pr["matches_workspace_candidate"]
            );
            if let Some(commit) = pr["merge_observation"]["merge_commit"].as_str() {
                println!("Observed merge commit: {commit}");
            }
        }
        if let Some(blockers) = report["blockers"].as_array() {
            for blocker in blockers.iter().take(12) {
                println!("{}: {}", blocker["code"], blocker["next_action"]);
            }
        }
    }
    match report["exit_code"].as_i64().unwrap_or(3) {
        0 => Ok(()),
        code => Err(DecapodError::DiagnosticExit(code as i32)),
    }
}
