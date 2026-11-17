use super::*;
use tempfile::TempDir;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(dead_code)]
#[path = "../../common/consumer_readiness_fixture.rs"]
mod consumer_readiness_fixture;

fn git(repo: &Path, args: &[&str]) -> String {
    text(run(repo, "git", args, "test git").unwrap())
        .unwrap()
        .trim()
        .to_string()
}
fn write(repo: &Path, path: &str, body: &str) {
    let path = repo.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "master"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    write(dir.path(), "README.md", "base\n");
    write(dir.path(), ".gitignore", ".decapod/data/\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["checkout", "-qb", "feature"]);
    dir
}
fn bundle(repo: &Path) {
    crate::core::governance_document::begin_pr(repo, "publication-test", "master").unwrap();
    let receipt = current_receipt_fixture(repo);
    save_receipt(repo, &receipt);
    git(repo, &["add", "."]);
    crate::core::governance_document::checkpoint(
        repo,
        "bundle",
        "Initial publication proof",
        vec!["test:publication".into()],
    )
    .unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "bundle"]);
}

fn proof() -> RemoteProof {
    RemoteProof {
        head: "a".repeat(40),
        base: "b".repeat(40),
        paths: REQUIRED_PR_GOVERNANCE_ARTIFACTS
            .iter()
            .map(|p| p.to_string())
            .chain([".decapod/managed/specs/INTERFACES.md".to_string()])
            .collect(),
    }
}
fn pr(proof: &RemoteProof) -> serde_json::Value {
    serde_json::json!({"number":7,"html_url":"https://github.com/owner/repo/pull/7","state":"open","draft":true,
        "head":{"ref":"feature","sha":proof.head,"repo":{"full_name":"owner/repo"}},
        "base":{"ref":"master","sha":proof.base,"repo":{"full_name":"owner/repo"}},
        "changed_files":proof.paths.len()})
}
fn bytes(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}
fn files(proof: &RemoteProof) -> serde_json::Value {
    serde_json::json!([proof
        .paths
        .iter()
        .map(|p| serde_json::json!({"filename":p,"status":"modified"}))
        .collect::<Vec<_>>()])
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod live_admission {
    use super::*;
    use crate::core::readiness_evidence::{Conclusion, Observation, SuiteEvidence};
    use crate::core::validation_history::PublicationReadiness;
    use crate::core::validation_runtime::ValidationReport;
    use consumer_readiness_fixture::Fixture;

    fn fixture() -> Fixture {
        let mut fixture = Fixture::new();
        fixture.options.git =
            crate::readiness_installed_tool(&fixture.options.root, "git").unwrap();
        let root = &fixture.options.root;
        write(
            root,
            ".decapod/managed/specs/INTENT.md",
            "# Consumer\nBoth source assertions must pass before PR admission.\n",
        );
        crate::core::trajectory::init_trajectory(
            root,
            crate::core::trajectory::TrajectoryInit {
                run_id: "publication-repair".into(),
                task_id: Some("consumer-source-repair".into()),
                intent_id: None,
                original_intent: "Repair the source and admit only matching complete proof".into(),
                derived_intent: "Keep both real source assertions required".into(),
                active_boundaries: vec!["payload.txt".into()],
                repo_scope: vec!["payload.txt".into()],
                destination: None,
                current_phase: Some("validation".into()),
                next_transitions: vec![],
                blockers: vec![],
            },
        )
        .unwrap();
        fixture.commit("record publication intent and material specification");
        fixture
    }

    fn remote(fixture: &Fixture) -> RemoteProof {
        let root = &fixture.options.root;
        // The transport and immutable objects are real; no head/base is invented.
        git(root, &["push", "origin", "consumer-work"]);
        let (head, base) = remote_heads(
            root,
            fixture.upstream.to_str().unwrap(),
            "consumer-work",
            "main",
        )
        .unwrap();
        RemoteProof {
            paths: changed_paths(root, &base, &head).unwrap(),
            head,
            base,
        }
    }

    fn pr_value(proof: &RemoteProof) -> serde_json::Value {
        serde_json::json!({"number":7,"html_url":"https://github.com/consumer/project/pull/7","state":"open","draft":true,
            "head":{"ref":"consumer-work","sha":proof.head,"repo":{"full_name":"consumer/project"}},
            "base":{"ref":"main","sha":proof.base,"repo":{"full_name":"consumer/project"}},
            "changed_files":proof.paths.len()})
    }

    fn check(
        fixture: &Fixture,
        readiness: &PublicationReadiness,
        proof: &RemoteProof,
    ) -> Result<(), DecapodError> {
        readiness.verify_subject(
            "consumer/project",
            "consumer-work",
            &proof.head,
            "main",
            &proof.base,
        )?;
        readiness.verify_current(&fixture.options.root)?;
        verify_committed_bundle(&fixture.options.root, &proof.head)?;
        verify_remote_unchanged(
            &fixture.options.root,
            fixture.upstream.to_str().unwrap(),
            "consumer-work",
            "main",
            proof,
        )
    }

    // Only forge I/O is simulated. These are the publisher's actual admission,
    // creation and readback functions, driven by live real-execution proof.
    fn publish(
        fixture: &Fixture,
        report: ValidationReport,
        proof: &RemoteProof,
        mutate_at: usize,
        creates: &std::cell::Cell<usize>,
    ) -> Result<String, DecapodError> {
        let readiness = PublicationReadiness::from_report(report, None)?;
        let mut replies = vec![
            bytes(serde_json::json!([[]])),
            b"created".to_vec(),
            bytes(serde_json::json!([[{"number":7}]])),
            bytes(pr_value(proof)),
            bytes(files(proof)),
            bytes(pr_value(proof)),
            bytes(pr_value(proof)),
            bytes(pr_value(proof)),
        ]
        .into_iter();
        let mut calls = 0;
        ensure_and_verify_pr_with(
            &mut |args| {
                calls += 1;
                if args.starts_with(&["pr", "create"]) {
                    creates.set(creates.get() + 1);
                }
                if calls == mutate_at {
                    write(
                        &fixture.options.root,
                        "payload.txt",
                        "changed during forge readback\n",
                    );
                }
                Ok(replies.next().expect("bounded forge protocol"))
            },
            "consumer/project",
            "consumer-work",
            "main",
            proof,
            &mut || check(fixture, &readiness, proof),
            "repair",
            "real source proof",
        )
    }

    #[test]
    fn failing_agent_repairs_source_and_only_fresh_exact_proof_admits_pr() {
        let fixture = fixture();
        write(&fixture.options.root, "payload.txt", "broken\n");
        fixture.commit("agent introduces actual source defect");
        let failed_head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
        let failed = fixture.validate(&mut fixture.host());
        assert!(!failed.ready && !failed.proof_ready, "{failed:?}");
        let failed_path = failed.evidence_path.clone().unwrap();
        let failed_bytes = std::fs::read(failed_path.join("observations.json")).unwrap();
        let observations: SuiteEvidence = serde_json::from_slice(&failed_bytes).unwrap();
        assert_eq!(observations.results.len(), 2);
        assert!(observations.results.iter().any(|row| matches!(
            row.observation,
            Observation::Executed {
                conclusion: Conclusion::Failure,
                ..
            }
        )));
        assert!(!failed_path.join("completion").exists());
        let creates = std::cell::Cell::new(0);
        assert!(publish(&fixture, failed, &remote(&fixture), 0, &creates).is_err());
        assert_eq!(creates.get(), 0);

        write(&fixture.options.root, "payload.txt", "ready\n");
        fixture.commit("agent repairs source after failed evidence");
        let repaired = fixture.validate(&mut fixture.host());
        assert!(repaired.ready && repaired.proof_ready, "{repaired:?}");
        assert_ne!(repaired.evidence_path.as_ref(), Some(&failed_path));
        let evidence: SuiteEvidence = serde_json::from_slice(
            &std::fs::read(
                repaired
                    .evidence_path
                    .as_ref()
                    .unwrap()
                    .join("observations.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(evidence.results.len(), 2);
        assert!(evidence.results.iter().all(|row| matches!(
            row.observation,
            Observation::Executed {
                conclusion: Conclusion::Success,
                exit_code: Some(0),
                ..
            }
        )));
        let proof = remote(&fixture);
        assert_ne!(proof.head, failed_head);
        assert_eq!(evidence.before.head_revision, proof.head);
        assert_eq!(evidence.before.accepted_base_revision, proof.base);
        assert_eq!(
            publish(&fixture, repaired, &proof, 0, &creates).unwrap(),
            "https://github.com/consumer/project/pull/7"
        );
        assert_eq!(creates.get(), 1);
        assert_eq!(
            std::fs::read(failed_path.join("observations.json")).unwrap(),
            failed_bytes
        );

        // An old genuine success cannot cover different bytes at the same path.
        let old = fixture.validate(&mut fixture.host());
        write(
            &fixture.options.root,
            "payload.txt",
            "new defect at the same path\n",
        );
        fixture.commit("agent changes actual source after successful checks");
        let changed = remote(&fixture);
        let rejected = std::cell::Cell::new(0);
        let error = publish(&fixture, old, &changed, 0, &rejected)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("PUBLICATION_PROOF_SUBJECT_CHANGED"),
            "{error}"
        );
        assert_eq!(rejected.get(), 0);
        let new_failure = fixture.validate(&mut fixture.host());
        assert!(
            !new_failure.ready,
            "old proof must not conceal the new real failure"
        );
        write(&fixture.options.root, "payload.txt", "ready\n");
        fixture.commit("repair the new defect and obtain new proof");
        let fresh = fixture.validate(&mut fixture.host());
        assert!(fresh.ready, "{fresh:?}");
        assert!(publish(&fixture, fresh, &remote(&fixture), 0, &rejected).is_ok());
        assert_eq!(rejected.get(), 1);
    }

    #[test]
    fn accepted_base_movement_after_success_cannot_admit_a_pr() {
        let fixture = fixture();
        let report = fixture.validate(&mut fixture.host());
        assert!(report.ready, "{report:?}");
        // This advance occurs after validation's final target read, the window
        // that a stability check against only the new remote base would miss.
        git(
            &fixture.options.root,
            &["push", "origin", "HEAD:refs/heads/main"],
        );
        let proof = remote(&fixture);
        let creates = std::cell::Cell::new(0);
        let error = publish(&fixture, report, &proof, 0, &creates)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("PUBLICATION_PROOF_SUBJECT_CHANGED"),
            "{error}"
        );
        assert_eq!(creates.get(), 0);
    }

    #[test]
    fn local_changes_during_lookup_or_final_readback_never_report_admission() {
        for mutate_at in [1, 6] {
            let fixture = fixture();
            let report = fixture.validate(&mut fixture.host());
            assert!(report.ready, "{report:?}");
            let creates = std::cell::Cell::new(0);
            let error = publish(&fixture, report, &remote(&fixture), mutate_at, &creates)
                .unwrap_err()
                .to_string();
            if mutate_at == 6 {
                assert!(error.contains("PR_CREATED_NOT_ADMITTED"), "{error}");
                assert!(error.contains("draft_confirmed"), "{error}");
            }
            assert_eq!(creates.get(), usize::from(mutate_at == 6));
        }
    }

    #[test]
    fn successful_report_or_receipt_without_live_completion_cannot_admit_pr() {
        let fixture = fixture();
        let mut report = fixture.validate(&mut fixture.host());
        assert!(report.ready, "{report:?}");
        report.completed_evidence = None;
        let creates = std::cell::Cell::new(0);
        let error = publish(&fixture, report, &remote(&fixture), 0, &creates)
            .unwrap_err()
            .to_string();
        assert!(error.contains("PUBLICATION_PROOF_MISSING"), "{error}");
        assert_eq!(creates.get(), 0);
    }

    #[test]
    fn omitted_skipped_or_failed_required_observation_cannot_authorize_creation() {
        for defect in ["missing", "skipped", "failure"] {
            let fixture = fixture();
            let mut report = fixture.validate(&mut fixture.host());
            assert!(report.ready, "{report:?}");
            let mut proof =
                serde_json::to_value(report.completed_evidence.take().unwrap()).unwrap();
            if defect == "missing" {
                proof["suite"]["results"].as_array_mut().unwrap().pop();
            } else {
                proof["suite"]["results"][0]["observation"]["conclusion"] = defect.into();
            }
            // Negative control: recomposing an agent's success report does not
            // make its incomplete/unsuccessful required evidence admissible.
            report.completed_evidence = Some(serde_json::from_value(proof).unwrap());
            let creates = std::cell::Cell::new(0);
            assert!(
                publish(&fixture, report, &remote(&fixture), 0, &creates).is_err(),
                "{defect}"
            );
            assert_eq!(creates.get(), 0);
        }
    }

    #[test]
    fn current_source_governance_policy_and_measured_tools_must_still_match() {
        for defect in ["source", "governance", "policy", "tool"] {
            let mut fixture = fixture();
            // Use an owned closure file; never alter the installed interpreter.
            let closure = fixture._storage.path().join("runtime-closure");
            std::fs::write(&closure, "reviewed auxiliary runtime bytes\n").unwrap();
            for execution in fixture
                .proof
                .repository_contract
                .as_mut()
                .unwrap()
                .executions
                .values_mut()
            {
                execution.toolchain_files.insert(
                    "owned-test-closure".into(),
                    closure.to_str().unwrap().into(),
                );
            }
            fixture.review_commands();
            fixture.save_config();
            fixture.commit("review measured execution closure");
            let report = fixture.validate(&mut fixture.host());
            assert!(report.ready, "{report:?}");
            let proof = remote(&fixture);
            match defect {
                "source" => write(
                    &fixture.options.root,
                    "payload.txt",
                    "changed uncommitted bytes\n",
                ),
                "governance" => {
                    crate::core::trajectory::record_trajectory(
                        &fixture.options.root,
                        "publication-repair",
                        crate::core::trajectory::TrajectoryUpdate {
                            tool_calls: vec!["new work after successful validation".into()],
                            ..Default::default()
                        },
                    )
                    .unwrap();
                }
                "policy" => {
                    let path = fixture
                        .options
                        .root
                        .join(consumer_readiness_fixture::CONFIG);
                    let mut content = std::fs::read_to_string(&path).unwrap();
                    content.push_str("\n# changed after successful validation\n");
                    std::fs::write(path, content).unwrap();
                }
                _ => std::fs::write(&closure, "different runtime bytes\n").unwrap(),
            }
            let creates = std::cell::Cell::new(0);
            assert!(
                publish(&fixture, report, &proof, 0, &creates).is_err(),
                "{defect}"
            );
            assert_eq!(creates.get(), 0, "{defect}");
        }
    }

    #[test]
    fn rejection_is_appended_without_rewriting_successful_historical_proof() {
        let fixture = fixture();
        let report = fixture.validate(&mut fixture.host());
        assert!(report.ready, "{report:?}");
        let directory = report.evidence_path.clone().unwrap();
        let original = std::fs::read(directory.join("historical-proof.json")).unwrap();
        let readiness = PublicationReadiness::from_report(report, None).unwrap();
        let reason = "PR_CREATED_NOT_ADMITTED: exact subject changed; quarantine=draft_confirmed";
        let path = readiness.record_publication_rejection(reason).unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(record["admitted"], false);
        assert_eq!(record["merge_authorized"], false);
        assert_eq!(record["reason"], reason);
        assert_eq!(
            record["material"]["head_revision"],
            git(&fixture.options.root, &["rev-parse", "HEAD"])
        );
        assert_eq!(
            std::fs::read(directory.join("historical-proof.json")).unwrap(),
            original
        );
    }
}

#[test]
fn failed_admission_demotes_only_the_matching_pr_and_confirms_quarantine() {
    let p = proof();
    let mut ready = pr(&p);
    ready["draft"] = false.into();
    let mut calls = Vec::new();
    let mut replies = vec![
        Ok(bytes(ready)),
        Err(failure("lost acknowledgement")),
        Ok(bytes(pr(&p))),
    ]
    .into_iter();
    let state = quarantine_pr(
        &mut |args| {
            calls.push(args.join(" "));
            replies.next().unwrap()
        },
        "owner/repo",
        "feature",
        "master",
        7,
    )
    .unwrap();
    assert_eq!(state, "draft_confirmed");
    assert_eq!(calls[1], "pr ready 7 --undo --repo owner/repo");
    let mut unrelated = pr(&p);
    unrelated["head"]["ref"] = "another-agent".into();
    let mut count = 0;
    assert!(
        quarantine_pr(
            &mut |_| {
                count += 1;
                Ok(bytes(unrelated.clone()))
            },
            "owner/repo",
            "feature",
            "master",
            7
        )
        .is_err()
    );
    assert_eq!(count, 1, "never mutate an unrelated PR");
}

#[test]
fn denied_or_unconfirmed_quarantine_never_becomes_success() {
    let p = proof();
    let mut ready = pr(&p);
    ready["draft"] = false.into();
    let mut replies = vec![
        Ok(bytes(ready.clone())),
        Err(failure("permission denied")),
        Ok(bytes(ready)),
    ]
    .into_iter();
    let error = quarantine_pr(
        &mut |_| replies.next().unwrap(),
        "owner/repo",
        "feature",
        "master",
        7,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("PR_QUARANTINE_UNCONFIRMED"), "{error}");
    let mut merged = pr(&p);
    merged["state"] = "closed".into();
    merged["merged"] = true.into();
    let error = quarantine_pr(
        &mut |_| Ok(bytes(merged.clone())),
        "owner/repo",
        "feature",
        "master",
        7,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("PR_QUARANTINE_UNAVAILABLE_ALREADY_MERGED"),
        "{error}"
    );
}

#[test]
fn artifact_inventory_missing_and_out_of_diff_matrix() {
    for missing in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let mut p = proof();
        p.paths.remove(*missing);
        let error = require_bundle_paths(&p.paths).unwrap_err().to_string();
        assert!(error.contains(missing));
        assert!(error.contains("REMOTE_GOVERNANCE_DIFF_MISSING"));
    }
}

#[test]
fn tracked_but_unstaged_artifact_bytes_do_not_pass() {
    let dir = repo();
    bundle(dir.path());
    ensure_validation_artifacts_staged(dir.path()).unwrap();
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let old = std::fs::read(dir.path().join(path)).unwrap();
        write(dir.path(), path, "changed");
        assert!(
            ensure_validation_artifacts_staged(dir.path())
                .unwrap_err()
                .to_string()
                .contains("UNSTAGED_GOVERNANCE_ARTIFACT")
        );
        std::fs::write(dir.path().join(path), old).unwrap();
    }
}

#[test]
fn committed_bundle_requires_exact_bytes_and_clean_index() {
    let dir = repo();
    bundle(dir.path());
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    verify_committed_bundle(dir.path(), &head).unwrap();
    write(
        dir.path(),
        REQUIRED_PR_GOVERNANCE_ARTIFACTS[0],
        "different bytes",
    );
    git(dir.path(), &["add", "."]);
    assert!(
        verify_committed_bundle(dir.path(), &head)
            .unwrap_err()
            .to_string()
            .contains("UNCOMMITTED_GOVERNANCE_ARTIFACT")
    );
}

#[test]
fn actual_bare_remote_proves_artifact_bytes_and_detects_divergence() {
    let dir = repo();
    bundle(dir.path());
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-q"]);
    let target = remote.path().to_str().unwrap();
    git(dir.path(), &["push", target, "master"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    git(
        dir.path(),
        &[
            "push",
            "-u",
            "--",
            target,
            &format!("{head}:refs/heads/feature"),
        ],
    );
    // A fetch origin unrelated to the selected push endpoint is not evidence.
    git(
        dir.path(),
        &["remote", "add", "origin", "/nonexistent/wrong-fetch-target"],
    );
    let evidence = verify_remote(dir.path(), target, "feature", "master", &head).unwrap();
    assert_eq!(evidence.head, head);
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        assert_eq!(
            run(
                remote.path(),
                "git",
                &["show", &format!("{head}:{path}")],
                "remote bytes"
            )
            .unwrap(),
            std::fs::read(dir.path().join(path)).unwrap()
        );
    }
    assert!(
        verify_remote(dir.path(), target, "feature", "master", &"c".repeat(40))
            .unwrap_err()
            .to_string()
            .contains("REMOTE_HEAD_MISMATCH")
    );
    write(dir.path(), "other.txt", "remote moves");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "move"]);
    git(dir.path(), &["push", target, "feature"]);
    assert!(
        verify_remote_unchanged(dir.path(), target, "feature", "master", &evidence)
            .unwrap_err()
            .to_string()
            .contains("REMOTE_PUBLICATION_RACE")
    );
}

#[test]
fn existing_pr_is_reused_and_full_remote_diff_checked() {
    let p = proof();
    let mut calls = Vec::new();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    let result = ensure_and_verify_pr_with(
        &mut |args| {
            calls.push(args.join(" "));
            Ok(replies.next().unwrap())
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        &mut || Ok(()),
        "title",
        "body",
    )
    .unwrap();
    assert_eq!(result, "https://github.com/owner/repo/pull/7");
    assert!(!calls.iter().any(|call| call.contains("pr create")));
    assert!(calls.iter().any(|call| call.contains("--paginate --slurp")));
    assert!(!calls.iter().any(|call| call.contains("-C ")));
}

#[test]
fn new_pr_created_as_draft_and_read_back_after_create() {
    let p = proof();
    let mut calls = Vec::new();
    let mut replies = vec![
        bytes(serde_json::json!([[]])),
        b"https://github.com/owner/repo/pull/7".to_vec(),
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    ensure_and_verify_pr_with(
        &mut |args| {
            calls.push(args.join(" "));
            Ok(replies.next().unwrap())
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        &mut || Ok(()),
        "title",
        "body",
    )
    .unwrap();
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("pr create --draft --repo owner/repo"))
    );
}

#[test]
fn pr_create_failure_is_actionable_partial_publication() {
    let p = proof();
    let mut index = 0;
    let error = ensure_and_verify_pr_with(
        &mut |_| {
            index += 1;
            if index == 2 {
                Err(failure("GitHub PR creation failed"))
            } else {
                Ok(bytes(serde_json::json!([[]])))
            }
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        &mut || Ok(()),
        "title",
        "body",
    )
    .unwrap_err();
    let error = after_push(error).to_string();
    assert!(error.contains("branch was pushed"));
    assert!(error.contains("not verified"));
    assert!(error.contains("retry `decapod workspace publish`"));
}

#[test]
fn lost_create_response_is_recovered_by_exact_pr_readback() {
    let p = proof();
    let mut replies = vec![
        Ok(bytes(serde_json::json!([[]]))),
        Err(failure("connection lost")),
        Ok(bytes(serde_json::json!([[{"number":7}]]))),
        Ok(bytes(pr(&p))),
        Ok(bytes(files(&p))),
        Ok(bytes(pr(&p))),
    ]
    .into_iter();
    ensure_and_verify_pr_with(
        &mut |_| replies.next().unwrap(),
        "owner/repo",
        "feature",
        "master",
        &p,
        &mut || Ok(()),
        "title",
        "body",
    )
    .unwrap();
}

#[test]
fn wrong_target_head_base_state_or_repository_never_passes() {
    let p = proof();
    for (pointer, value) in [
        ("/head/sha", "c".repeat(40)),
        ("/base/sha", "c".repeat(40)),
        ("/head/ref", "wrong".into()),
        ("/base/ref", "wrong".into()),
        ("/head/repo/full_name", "attacker/repo".into()),
        ("/base/repo/full_name", "wrong/repo".into()),
        ("/state", "closed".into()),
    ] {
        let mut value_pr = pr(&p);
        *value_pr.pointer_mut(pointer).unwrap() = value.into();
        let parsed: PullRequest = serde_json::from_value(value_pr).unwrap();
        assert!(
            check_pr(&parsed, "owner/repo", "feature", "master", &p)
                .unwrap_err()
                .to_string()
                .contains("PR_TARGET_MISMATCH")
        );
    }
}

#[test]
fn missing_remote_artifact_and_truncated_pr_diff_fail() {
    let p = proof();
    let mut f = files(&p);
    f[0].as_array_mut().unwrap().pop();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(f),
        bytes(pr(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    let error = ensure_and_verify_pr_with(
        &mut |_| Ok(replies.next().unwrap()),
        "owner/repo",
        "feature",
        "master",
        &p,
        &mut || Ok(()),
        "title",
        "body",
    )
    .unwrap_err();
    assert!(error.to_string().contains("PR_DIFF_MISMATCH"));
}

#[test]
fn pr_race_after_diff_readback_fails() {
    let p = proof();
    let mut changed = pr(&p);
    changed["head"]["sha"] = "c".repeat(40).into();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(changed),
        bytes(pr(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    assert!(
        ensure_and_verify_pr_with(
            &mut |_| Ok(replies.next().unwrap()),
            "owner/repo",
            "feature",
            "master",
            &p,
            &mut || Ok(()),
            "title",
            "body"
        )
        .is_err()
    );
}

#[test]
fn diagnostics_and_success_remote_never_echo_credentials() {
    let secret = "top-secret-token";
    assert!(
        !redact_remote(&format!(
            "https://user:{secret}@github.com/owner/repo?token={secret}"
        ))
        .contains(secret)
    );
    assert!(
        !publish_push_failure(
            &format!("fatal https://user:{secret}@github.com/owner/repo"),
            "feature",
            "origin"
        )
        .contains(secret)
    );
}

fn plan_fixture(repo: &Path) {
    plan_governance::init_plan(
        repo,
        plan_governance::InitPlanInput {
            title: "Publication".into(),
            intent: "Verify current contracts".into(),
            todo_ids: vec!["bugs_current".into()],
            proof_hooks: vec![],
            unknowns: vec![],
            human_questions: vec![],
            stop_conditions: vec![],
            unresolved_contradictions: vec![],
            deferred_questions: vec![],
            constraints: Default::default(),
            phases: vec![],
        },
    )
    .unwrap();
    for path in plan_governance::PUBLICATION_REVIEW_SPECS {
        write(repo, path, "# Current contract\nStill accurate.\n");
    }
}
fn review_all(repo: &Path) {
    for path in plan_governance::PUBLICATION_REVIEW_SPECS {
        plan_governance::review_spec(
            repo,
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            "Inspected current implementation; existing contract still describes behavior",
        )
        .unwrap();
    }
}

#[test]
fn code_change_requires_current_review_not_unrelated_spec_churn() {
    let dir = repo();
    plan_fixture(dir.path());
    write(dir.path(), "api/onboarding.go", "new contract");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "api change"]);
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("SPEC_REVIEW_REQUIRED")
    );
    review_all(dir.path());
    verify_spec_reviews(dir.path(), "master").unwrap();
    write(dir.path(), "api/onboarding.go", "changed replay semantics");
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("STALE_SPEC_REVIEW")
    );
    review_all(dir.path());
    verify_spec_reviews(dir.path(), "master").unwrap();
    write(
        dir.path(),
        plan_governance::PUBLICATION_REVIEW_SPECS[0],
        "different authored contract",
    );
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("STALE_SPEC_REVIEW")
    );
}

#[test]
fn unchanged_review_requires_reason_and_decision_cannot_self_approve() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            " "
        )
        .is_err()
    );
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Human must choose replay contract",
    )
    .unwrap();
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            "Agent says approved"
        )
        .unwrap_err()
        .to_string()
        .contains("SPEC_REVIEW_DECISION_REQUIRED")
    );
    assert!(
        plan_governance::review_spec(
            dir.path(),
            "../outside",
            plan_governance::SpecReviewDisposition::Updated,
            "no"
        )
        .is_err()
    );
}

#[test]
fn legacy_plan_loads_without_reviews() {
    let dir = repo();
    plan_fixture(dir.path());
    let plan = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    let mut value = serde_json::to_value(plan).unwrap();
    value.as_object_mut().unwrap().remove("spec_reviews");
    value["schema_version"] = "1.0.0".into();
    crate::core::governance_document::write_section(dir.path(), "plan", &value).unwrap();
    assert!(
        plan_governance::load_plan(dir.path())
            .unwrap()
            .unwrap()
            .spec_reviews
            .is_empty()
    );
}

#[test]
fn epoch_excludes_receipt_but_binds_plan_claims_and_code_content() {
    use crate::core::validation_epoch::active_validation_epoch;
    let dir = repo();
    crate::core::governance_document::begin_pr(dir.path(), "epoch-test", "master").unwrap();
    write(dir.path(), "api/contract.go", "contract v1");
    let receipt = current_receipt_fixture(dir.path());
    let first = active_validation_epoch(dir.path()).unwrap();
    save_receipt(dir.path(), &receipt);
    assert_eq!(first, active_validation_epoch(dir.path()).unwrap());
    write(dir.path(), "api/contract.go", "contract v2");
    assert_ne!(first, active_validation_epoch(dir.path()).unwrap());
    write(dir.path(), "api/contract.go", "contract v1");
    assert_eq!(first, active_validation_epoch(dir.path()).unwrap());
    review_all(dir.path());
    assert_ne!(first, active_validation_epoch(dir.path()).unwrap());
    let reviewed = active_validation_epoch(dir.path()).unwrap();
    crate::core::governance_document::record_claim(
        dir.path(),
        "epoch-claim",
        "The epoch binds claims",
        "Changing a claim leaves the epoch unchanged",
        "open",
        vec![],
    )
    .unwrap();
    assert_ne!(reviewed, active_validation_epoch(dir.path()).unwrap());
}

#[test]
fn explicit_human_resolution_preserves_provenance_and_unchanged_bytes() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Choose replay policy",
    )
    .unwrap();
    let before = std::fs::read(dir.path().join(path)).unwrap();
    assert!(plan_governance::resolve_spec_review(dir.path(), path, "", "accepted").is_err());
    let plan = plan_governance::resolve_spec_review(
        dir.path(),
        path,
        "review-thread:human-answer-17",
        "Human accepted documented one-time replay behavior",
    )
    .unwrap();
    assert_eq!(before, std::fs::read(dir.path().join(path)).unwrap());
    assert_eq!(
        plan.spec_reviews[0].decision_ref.as_deref(),
        Some("review-thread:human-answer-17")
    );
    assert!(plan.spec_reviews[0].reason.contains("Choose replay policy"));
    assert_eq!(
        plan.spec_reviews[0].disposition,
        plan_governance::SpecReviewDisposition::UnchangedWithReason
    );
}

pub(crate) fn current_receipt_fixture(repo: &Path) -> crate::core::validate::ValidationReceipt {
    use crate::core::validate::ValidationReceipt;
    plan_fixture(repo);
    research_claims::ensure_template(repo, false).unwrap();
    let run = match trajectory::load_trajectory_cookie(repo).unwrap() {
        Some(run) if run.run_id == "publication-proof" => run,
        _ => trajectory::init_trajectory(
            repo,
            trajectory::TrajectoryInit {
                run_id: "publication-proof".into(),
                task_id: Some("bugs_current".into()),
                intent_id: None,
                original_intent: "test publication".into(),
                derived_intent: "test current proof".into(),
                active_boundaries: vec![],
                repo_scope: vec![".".into()],
                destination: None,
                current_phase: None,
                next_transitions: vec![],
                blockers: vec![],
            },
        )
        .unwrap(),
    };
    ValidationReceipt {
        schema_version: "1.0.0".into(),
        kind: "validation_receipt".into(),
        decapod_release: entrypoint_integrity::RELEASE_VERSION.into(),
        git_revision: git(repo, &["rev-parse", "HEAD"]),
        repo_signal_fingerprint: project_specs::repo_signal_fingerprint(repo).unwrap(),
        trajectory_run_id: Some(run.run_id),
        trajectory_artifact_hash: Some(run.artifact_hash),
        validation_epoch: crate::core::validation_epoch::active_validation_epoch(repo).unwrap(),
        status: "ok".into(),
        pass_count: 1,
        fail_count: 0,
        warn_count: 0,
        elapsed_ms: 1,
        drift_findings: vec![],
        temporary_artifacts_cleaned: 0,
        failures: None,
        warnings: None,
        gate_timings: None,
        parallelism: None,
        ci_prediction: None,
        receipt_hash: String::new(),
    }
    .with_recomputed_hash()
    .unwrap()
}
pub(crate) fn save_receipt(repo: &Path, receipt: &crate::core::validate::ValidationReceipt) {
    crate::core::governance_document::write_section(
        repo,
        "validation",
        &serde_json::to_value(receipt).unwrap(),
    )
    .unwrap();
}

#[test]
fn current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision() {
    let dir = repo();
    write(dir.path(), "api/contract.go", "current API");
    let receipt = current_receipt_fixture(dir.path());
    save_receipt(dir.path(), &receipt);
    verify_validation_artifacts_for_publish(dir.path()).unwrap();
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &["commit", "-qm", "validated content and proof"],
    );
    verify_validation_artifacts_for_publish(dir.path()).unwrap();
    write(dir.path(), "api/contract.go", "stale API");
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_VALIDATION")
    );
    write(dir.path(), "api/contract.go", "current API");
    let mut invalid = receipt.clone();
    invalid.git_revision = "c".repeat(40);
    save_receipt(dir.path(), &invalid.with_recomputed_hash().unwrap());
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_REVISION")
    );
    save_receipt(dir.path(), &receipt);
    let mut plan = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    plan.todo_ids = vec!["wrong_task".into()];
    plan_governance::save_plan(dir.path(), &plan).unwrap();
    // Re-hashing an epoch cannot hide a mismatched plan/trajectory subject.
    let mut wrong_task = receipt.clone();
    wrong_task.validation_epoch =
        crate::core::validation_epoch::active_validation_epoch(dir.path()).unwrap();
    save_receipt(dir.path(), &wrong_task.with_recomputed_hash().unwrap());
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_TASK")
    );
}

#[test]
fn current_governance_is_receipt_independent_but_preserves_task_and_decision_gates() {
    let dir = repo();
    // Build the real governed context, deliberately without storing a project
    // receipt: full readiness cannot require a receipt that mutates its subject.
    let receipt = current_receipt_fixture(dir.path());
    assert!(
        crate::core::governance_document::read_section(dir.path(), "validation")
            .unwrap()
            .is_none()
    );
    verify_current_governance_for_publish(dir.path()).unwrap();
    assert!(verify_validation_artifacts_for_publish(dir.path()).is_err());

    // Even an intact old project receipt does not resolve current human gates.
    save_receipt(dir.path(), &receipt);
    let original = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    let mut changed = original.clone();
    changed
        .human_questions
        .push("Choose the intended contract".into());
    plan_governance::save_plan(dir.path(), &changed).unwrap();
    assert!(
        verify_current_governance_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("PUBLICATION_DECISION_REQUIRED")
    );
    changed = original.clone();
    changed
        .unresolved_contradictions
        .push("Source and intent disagree".into());
    plan_governance::save_plan(dir.path(), &changed).unwrap();
    assert!(
        verify_current_governance_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("PUBLICATION_DECISION_REQUIRED")
    );
    changed = original;
    changed.todo_ids = vec!["wrong_task".into()];
    plan_governance::save_plan(dir.path(), &changed).unwrap();
    assert!(
        verify_current_governance_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_TASK")
    );
}

#[test]
fn invalid_and_missing_artifacts_fail_real_publication_gate() {
    let dir = repo();
    crate::core::governance_document::begin_pr(dir.path(), "invalid-artifact-test", "master")
        .unwrap();
    let receipt = current_receipt_fixture(dir.path());
    save_receipt(dir.path(), &receipt);
    git(dir.path(), &["add", "."]);
    crate::core::governance_document::checkpoint(
        dir.path(),
        "valid-bundle",
        "Valid current proof",
        vec!["test:publication".into()],
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "valid bundle"]);
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let original = std::fs::read(dir.path().join(path)).unwrap();
        write(dir.path(), path, "invalid JSON");
        assert!(ensure_required_governance_artifacts_in_pr(dir.path(), "master").is_err());
        std::fs::remove_file(dir.path().join(path)).unwrap();
        assert!(ensure_required_governance_artifacts_in_pr(dir.path(), "master").is_err());
        std::fs::write(dir.path().join(path), original).unwrap();
    }
    // Fully valid inherited artifacts still fail if not in the actual PR delta.
    git(dir.path(), &["branch", "current-base"]);
    write(dir.path(), "app.txt", "unrelated");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "no proof changes"]);
    assert!(
        ensure_required_governance_artifacts_in_pr(dir.path(), "current-base")
            .unwrap_err()
            .to_string()
            .contains("not included in the PR diff")
    );
}

#[test]
fn code_fingerprint_covers_root_and_other_implementation_surfaces() {
    for path in [
        "main.go",
        "assets/tools/check.rs",
        "app/route.ts",
        "api/auth.go",
        "backend/auth.py",
        "frontend/app.tsx",
        "web/main.js",
        "services/service.go",
    ] {
        let dir = repo();
        write(dir.path(), path, "before");
        let before = project_specs::repo_signal_fingerprint(dir.path()).unwrap();
        write(dir.path(), path, "after");
        assert_ne!(
            before,
            project_specs::repo_signal_fingerprint(dir.path()).unwrap(),
            "{path}"
        );
        assert!(project_specs::is_implementation_path(path));
    }
}

#[test]
fn spec_review_cli_requires_deliberate_resolution_arguments() {
    use clap::Parser;
    let path = ".decapod/managed/specs/INTERFACES.md";
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "review-spec",
            "--path",
            path,
            "--disposition",
            "unchanged-with-reason",
            "--reason",
            "Inspected current API"
        ])
        .is_ok()
    );
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "resolve-spec-review",
            "--path",
            path,
            "--reason",
            "approved"
        ])
        .is_err()
    );
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "resolve-spec-review",
            "--path",
            path,
            "--decision-ref",
            "human-thread-17",
            "--reason",
            "accepted"
        ])
        .is_ok()
    );
}

#[test]
fn code_changes_cannot_silently_erase_pending_human_decisions() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Choose contract",
    )
    .unwrap();
    write(dir.path(), "main.go", "new implementation");
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::Updated,
            "Unrelated code changed"
        )
        .is_err()
    );
    assert!(
        plan_governance::resolve_spec_review(
            dir.path(),
            path,
            "old-human-answer",
            "stale approval"
        )
        .is_err()
    );
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Human must review current implementation",
    )
    .unwrap();
    plan_governance::resolve_spec_review(
        dir.path(),
        path,
        "current-human-answer",
        "accepted current behavior",
    )
    .unwrap();
}

#[test]
fn unconfirmed_push_never_claims_rejection_or_rollback() {
    for diagnostic in [
        "",
        "connection reset after writing objects",
        "fatal: authentication failed",
        "remote hook declined; failed to push some refs",
    ] {
        let message = publish_push_failure(diagnostic, "feature", "origin");
        assert!(message.contains("PUBLICATION_OUTCOME_UNKNOWN"));
        assert!(message.contains("server may have accepted"));
        assert!(message.contains("git ls-remote"));
        assert!(!message.contains("remote branch has diverged"));
    }
}

#[test]
fn remote_publication_rejects_uncheckpointed_intermediate_work_even_with_valid_tip() {
    let dir = repo();
    bundle(dir.path());
    write(dir.path(), "main.go", "package main\nfunc main() {}\n");
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &["commit", "-qm", "authored work without checkpoint"],
    );
    let unproved = git(dir.path(), &["rev-parse", "HEAD"]);
    let receipt = current_receipt_fixture(dir.path());
    save_receipt(dir.path(), &receipt);
    git(dir.path(), &["add", "."]);
    crate::core::governance_document::checkpoint(
        dir.path(),
        "later-proof",
        "Proof recorded only after the authored commit",
        vec!["test:publication".into()],
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &["commit", "-qm", "later proof cannot repair historical gap"],
    );
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-q"]);
    let target = remote.path().to_str().unwrap();
    git(dir.path(), &["push", target, "master", "feature"]);
    let error = verify_remote(dir.path(), target, "feature", "master", &head)
        .expect_err("a valid document at the PR tip must not cover missing historical proof")
        .to_string();
    assert!(error.contains("CHECKPOINT"), "{error}");
    assert!(error.contains(&unproved), "{error}");
}

#[test]
fn staged_publication_requires_fresh_checkpoint_before_creating_history() {
    let dir = repo();
    bundle(dir.path());
    let original_head = git(dir.path(), &["rev-parse", "HEAD"]);
    write(dir.path(), "main.go", "package main\nfunc main() {}\n");
    git(dir.path(), &["add", "."]);
    let error = crate::core::governance_document::verify_staged_checkpoint(dir.path())
        .expect_err("staged authored work requires a new checkpoint before auto-commit")
        .to_string();
    assert!(error.contains("checkpoint"), "{error}");
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), original_head);
    crate::core::governance_document::checkpoint(
        dir.path(),
        "staged-publication",
        "Staged implementation proof",
        vec!["test:publication".into()],
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    crate::core::governance_document::verify_staged_checkpoint(dir.path()).unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), original_head);
    git(dir.path(), &["commit", "-qm", "checkpointed publication"]);
    crate::core::governance_document::verify_pr_checkpoints_for_target(
        dir.path(),
        "master",
        "HEAD",
        "master",
    )
    .unwrap();
}

#[test]
fn native_and_buildkite_publication_inventory_do_not_require_legacy_receipt() {
    let dir = repo();
    crate::core::governance_document::begin_pr(dir.path(), "provider-owned-proof", "master")
        .unwrap();
    let receipt = current_receipt_fixture(dir.path());
    let mut config = crate::cli::DecapodProjectConfig::default();
    config.proof.validation_buildkite_preflights = true;
    write(
        dir.path(),
        ".decapod/config.toml",
        &toml::to_string(&config).unwrap(),
    );
    assert!(
        crate::core::governance_document::read_section(dir.path(), "validation")
            .unwrap()
            .is_none()
    );
    git(dir.path(), &["add", "."]);
    crate::core::governance_document::checkpoint(
        dir.path(),
        "remote-preparation",
        "Prepare remote proof without a local receipt",
        vec!["test:remote-routing".into()],
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "provider-owned proof"]);
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();
    verify_current_governance_for_publish(dir.path()).unwrap();

    // An intact stale local receipt cannot become a prerequisite or proof.
    let mut stale = receipt;
    stale.trajectory_artifact_hash = Some("0".repeat(64));
    save_receipt(dir.path(), &stale.with_recomputed_hash().unwrap());
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();
    assert!(verify_validation_artifacts_for_publish(dir.path()).is_err());
    config.proof.validation_buildkite_preflights = false;
    write(
        dir.path(),
        ".decapod/config.toml",
        &toml::to_string(&config).unwrap(),
    );
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();

    config.proof.validation_buildkite_preflights = true;
    write(
        dir.path(),
        ".decapod/config.toml",
        &toml::to_string(&config).unwrap(),
    );
    let original = std::fs::read(
        dir.path()
            .join(crate::core::governance_document::GOVERNANCE_PATH),
    )
    .unwrap();
    for section in ["plan", "claims", "trajectory"] {
        let mut doc: serde_json::Value = serde_json::from_slice(&original).unwrap();
        if section == "claims" {
            doc["claims"] = serde_json::Value::Null;
        } else {
            doc["sections"].as_object_mut().unwrap().remove(section);
        }
        std::fs::write(
            dir.path()
                .join(crate::core::governance_document::GOVERNANCE_PATH),
            serde_json::to_vec(&doc).unwrap(),
        )
        .unwrap();
        assert!(
            ensure_required_governance_artifacts_in_pr(dir.path(), "master").is_err(),
            "{section}"
        );
    }
}

#[test]
fn publication_inventory_absent_config_is_native_but_malformed_config_fails() {
    let dir = repo();
    bundle(dir.path());
    let config_path = dir.path().join(".decapod/config.toml");
    assert!(!config_path.exists());
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();

    write(dir.path(), ".decapod/config.toml", "invalid = [");
    assert!(matches!(
        ensure_required_governance_artifacts_in_pr(dir.path(), "master"),
        Err(DecapodError::Config(_))
    ));
    std::fs::remove_file(config_path).unwrap();
    let mut stale = current_receipt_fixture(dir.path());
    stale.trajectory_artifact_hash = Some("0".repeat(64));
    save_receipt(dir.path(), &stale.with_recomputed_hash().unwrap());
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();
    assert!(verify_validation_artifacts_for_publish(dir.path()).is_err());
}

fn provider_material_fixture() -> TempDir {
    let dir = repo();
    let mut config = crate::cli::DecapodProjectConfig::default();
    config.proof.validation_buildkite_preflights = true;
    write(
        dir.path(),
        ".decapod/config.toml",
        &toml::to_string(&config).unwrap(),
    );
    crate::core::governance_document::begin_pr(dir.path(), "provider-material", "master").unwrap();
    let _ = current_receipt_fixture(dir.path());
    write(dir.path(), "api/contract.rs", "pub fn contract() {}\n");
    git(dir.path(), &["add", "."]);
    review_all(dir.path());
    git(dir.path(), &["add", "."]);
    crate::core::governance_document::checkpoint(
        dir.path(),
        "provider-material",
        "Current provider-owned material",
        vec!["test:provider-material".into()],
    )
    .unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "checkpointed material"]);
    dir
}

#[test]
fn accepted_identity_continuation_preserves_exact_base_and_current_candidate() {
    let dir = provider_material_fixture();
    let accepted = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["checkout", "-b", "continued-work"]);
    write(dir.path(), "api/contract.rs", "pub fn continued() {}\n");
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &["commit", "-qm", "continue same candidate work"],
    );
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(
        !crate::core::governance_document::inspect_checkpoint_history(
            dir.path(),
            &accepted,
            &head,
            Some("master")
        )
        .unwrap()
        .history_proven
    );
    assert!(
        crate::core::governance_document::verify_pr_continuation_for_target(
            dir.path(),
            &accepted,
            &head,
            "master"
        )
        .is_ok()
    );
}

#[test]
fn provider_material_checks_require_clean_exact_commits_and_current_spec_reviews() {
    let dir = provider_material_fixture();
    let base = git(dir.path(), &["rev-parse", "master"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    verify_provider_publication_material(dir.path(), &base, &head, "master").unwrap();
    assert!(verify_provider_publication_material(dir.path(), "master", &head, "master").is_err());
    assert!(verify_provider_publication_material(dir.path(), &base, "HEAD", "master").is_err());
    assert!(verify_provider_publication_material(dir.path(), &head, &head, "master").is_err());
    write(dir.path(), "uncommitted.txt", "material not in H");
    assert!(
        verify_provider_publication_material(dir.path(), &base, &head, "master")
            .unwrap_err()
            .to_string()
            .contains("UNCOMMITTED_PUBLICATION_CONTENT")
    );
    std::fs::remove_file(dir.path().join("uncommitted.txt")).unwrap();
    let mut plan = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    plan.spec_reviews.clear();
    plan_governance::save_plan(dir.path(), &plan).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "missing current reviews"]);
    let changed = git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(
        verify_provider_publication_material(dir.path(), &base, &changed, "master")
            .unwrap_err()
            .to_string()
            .contains("SPEC_REVIEW_REQUIRED")
    );
}

#[test]
fn transport_admits_unverified_history_but_remote_material_still_rejects_it() {
    let root = provider_material_fixture();
    let worktree = TempDir::new().unwrap();
    git(
        root.path(),
        &[
            "worktree",
            "add",
            "-b",
            "agent/test/bugs_current",
            worktree.path().to_str().unwrap(),
        ],
    );
    let dir = worktree.path();
    write(dir, "README.md", "An uncheckpointed authored commit\n");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "uncheckpointed history"]);
    let base = git(dir, &["rev-parse", "master"]);
    let head = git(dir, &["rev-parse", "HEAD"]);
    let before =
        std::fs::read(dir.join(crate::core::governance_document::GOVERNANCE_PATH)).unwrap();
    verify_readiness_transport_prerequisites(dir).unwrap();
    assert!(
        verify_current_publication_prerequisites(dir).is_err(),
        "native retains local verification"
    );
    assert!(
        verify_provider_publication_material(dir, &base, &head, "master").is_err(),
        "the existing remote governance obligation must reject uncheckpointed history"
    );
    assert_eq!(
        std::fs::read(dir.join(crate::core::governance_document::GOVERNANCE_PATH)).unwrap(),
        before
    );
    let mut plan = plan_governance::load_plan(dir).unwrap().unwrap();
    plan.human_questions
        .push("Choose the intended action".into());
    plan_governance::save_plan(dir, &plan).unwrap();
    assert!(
        verify_readiness_transport_prerequisites(dir)
            .unwrap_err()
            .to_string()
            .contains("PUBLICATION_DECISION_REQUIRED")
    );
}

#[test]
fn publication_material_cli_is_explicit_and_requires_target_binding() {
    use clap::Parser;
    let args = [
        "decapod",
        "govern",
        "artifacts",
        "verify-checkpoints",
        "--base-branch",
        "master",
        "--publication-material",
    ];
    assert!(crate::cli::Cli::try_parse_from(args).is_err());
    let parsed =
        crate::cli::Cli::try_parse_from(args.into_iter().chain(["--target-base", "master"]))
            .unwrap();
    assert!(matches!(
        parsed.command,
        crate::cli::Command::Govern(crate::cli::GovernCli {
            command: crate::cli::GovernCommand::Artifacts(crate::cli::ArtifactsCli {
                command: crate::cli::ArtifactsCommand::VerifyCheckpoints {
                    publication_material: true,
                    ..
                }
            })
        })
    ));
}
