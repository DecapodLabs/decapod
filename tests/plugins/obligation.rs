#[cfg(test)]
mod tests {
    use decapod::core::bounded_process::BoundedCommand;
    use decapod::core::db::{Connection, params};
    use decapod::core::obligation::{
        ObligationStatus, add_obligation, complete_obligation, derive_obligation_status,
        get_dependencies, get_obligation, initialize_obligation_db, list_obligations,
        obligation_db_path, validate_obligation_graph,
    };
    use decapod::core::store::{Store, StoreKind};
    use decapod::plugins::health::{add_claim, initialize_health_db, record_proof};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    fn test_store() -> (Store, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = Store {
            root: PathBuf::from(temp_dir.path()),
            kind: StoreKind::Repo,
        };
        initialize_obligation_db(&store.root).unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_add_obligation() {
        let (store, _temp) = test_store();
        let result = add_obligation(&store, "test-intent", "medium", "", "");
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_list_obligations() {
        let (store, _temp) = test_store();
        add_obligation(&store, "test-1", "medium", "", "").unwrap();
        add_obligation(&store, "test-2", "high", "", "").unwrap();

        let obligations = list_obligations(&store).unwrap();
        assert_eq!(obligations.len(), 2);
    }

    #[test]
    fn test_get_obligation() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        let obligation = get_obligation(&store, &id).unwrap();
        assert_eq!(obligation.id, id);
        assert_eq!(obligation.intent_ref, "test-intent");
        assert_eq!(obligation.status, ObligationStatus::Open);
    }

    #[test]
    fn test_derive_status_no_dependencies_no_proofs() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        let result = derive_obligation_status(&store, &id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Open);
        assert!(!result.commit_present);
        assert!(
            result
                .validation_errors
                .iter()
                .any(|e| e.contains("STATE_COMMIT"))
        );
    }

    #[test]
    fn test_derive_status_without_commit() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "test-intent", "medium", "", "").unwrap();

        // Without state commit, status should be Open
        let result = derive_obligation_status(&store, &id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Open);
    }

    #[test]
    fn test_validate_graph_empty() {
        let (store, _temp) = test_store();
        let result = validate_obligation_graph(&store).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.total_nodes, 0);
    }

    #[test]
    fn test_validate_graph_with_obligations() {
        let (store, _temp) = test_store();
        add_obligation(&store, "test-1", "medium", "", "").unwrap();
        add_obligation(&store, "test-2", "high", "", "").unwrap();

        let result = validate_obligation_graph(&store).unwrap();
        // Graph is valid (no cycles), but obligations are not all satisfied
        assert!(!result.has_cycles);
        assert_eq!(result.total_nodes, 2);
    }

    #[test]
    fn test_dependency_chain() {
        let (store, _temp) = test_store();

        // Create obligation 1 (no dependencies)
        let id1 = add_obligation(&store, "test-1", "medium", "", "").unwrap();

        // Create obligation 2 that depends on 1
        let _id2 = add_obligation(&store, "test-2", "medium", &id1, "").unwrap();

        // Validate graph - should have valid structure (no cycles)
        let result = validate_obligation_graph(&store).unwrap();
        assert!(!result.has_cycles);
        assert_eq!(result.total_nodes, 2);
        assert_eq!(result.total_edges, 1);
    }

    fn fixture_db(store: &Store) -> Connection {
        Connection::open(obligation_db_path(&store.root)).unwrap()
    }

    fn set_cached_status(store: &Store, id: &str, status: &str) {
        fixture_db(store)
            .execute(
                "UPDATE obligations SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .unwrap();
    }

    fn add_completed(store: &Store, intent: &str, dependencies: &str, proofs: &str) -> String {
        let id = add_obligation(store, intent, "medium", dependencies, proofs).unwrap();
        complete_obligation(store, &id, "state-commit-fixture").unwrap();
        id
    }

    fn add_proof_claim(store: &Store, claim: &str) {
        initialize_health_db(&store.root).unwrap();
        add_claim(store, claim, claim, "FACT", "obligation regression fixture").unwrap();
    }

    fn insert_proof_at(store: &Store, claim: &str, result: &str, ts: i64, sla: i64) {
        fixture_db(store)
            .execute(
                "INSERT INTO proof_events(event_id, claim_id, ts, surface, result, sla_seconds)
             VALUES(?1, ?2, ?3, 'obligation regression', ?4, ?5)",
                params![
                    decapod::core::ulid::new_ulid(),
                    claim,
                    format!("{ts}Z"),
                    result,
                    sla
                ],
            )
            .unwrap();
    }

    fn now_seconds() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn poison_verified_cache(store: &Store, claim: &str) {
        fixture_db(store)
            .execute(
                "INSERT OR REPLACE INTO health_cache(claim_id, computed_state, reason, updated_at)
             VALUES(?1, 'VERIFIED', 'untrusted stale projection', '1Z')",
                params![claim],
            )
            .unwrap();
    }

    fn assert_unmet(store: &Store, id: &str) {
        let result = derive_obligation_status(store, id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Open, "{result:?}");
        assert!(!result.validation_errors.is_empty(), "{result:?}");
    }

    #[test]
    fn fresh_passing_proof_is_accepted_without_populating_health_cache() {
        let (store, _temp) = test_store();
        add_proof_claim(&store, "fresh-proof");
        record_proof(&store, "fresh-proof", "fixture", "pass", 3600).unwrap();
        let id = add_completed(&store, "fresh-proof-obligation", "", "fresh-proof");
        let before = serde_json::to_value(get_obligation(&store, &id).unwrap()).unwrap();
        let count = || {
            fixture_db(&store)
                .query_row("SELECT COUNT(*) FROM health_cache", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap()
        };
        assert_eq!(count(), 0);

        let result = derive_obligation_status(&store, &id).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Met);
        assert!(result.proofs_satisfied);
        assert!(result.validation_errors.is_empty());
        assert_eq!(
            count(),
            0,
            "derivation must not materialize a health projection"
        );
        assert_eq!(
            serde_json::to_value(get_obligation(&store, &id).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn verified_cache_cannot_replace_missing_expired_or_contradicted_proof() {
        for evidence in ["none", "expired", "new_failure"] {
            let (store, _temp) = test_store();
            add_proof_claim(&store, "poisoned-proof");
            let now = now_seconds();
            match evidence {
                "expired" => insert_proof_at(&store, "poisoned-proof", "pass", now - 7200, 60),
                "new_failure" => {
                    insert_proof_at(&store, "poisoned-proof", "pass", now - 10, 3600);
                    insert_proof_at(&store, "poisoned-proof", "fail", now - 1, 3600);
                }
                _ => {}
            }
            poison_verified_cache(&store, "poisoned-proof");
            let id = add_completed(&store, evidence, "", "poisoned-proof");
            let result = derive_obligation_status(&store, &id).unwrap();
            assert_ne!(
                result.derived_status,
                ObligationStatus::Met,
                "{evidence}: {result:?}"
            );
            assert!(!result.proofs_satisfied, "{evidence}: {result:?}");
            let graph = validate_obligation_graph(&store).unwrap();
            assert!(!graph.is_valid, "{evidence}");
            assert!(graph.missing_proofs.contains(&id), "{evidence}: {graph:?}");
        }
    }

    #[test]
    fn unknown_proof_claim_is_unmet_even_with_a_commit() {
        let (store, _temp) = test_store();
        initialize_health_db(&store.root).unwrap();
        let id = add_completed(&store, "unknown-claim", "", "missing-claim");
        let result = derive_obligation_status(&store, &id).unwrap();
        assert!(!result.proofs_satisfied);
        assert_ne!(result.derived_status, ObligationStatus::Met);
    }

    #[test]
    fn exact_dependency_chain_derives_without_writing_cached_statuses() {
        let (store, _temp) = test_store();
        add_proof_claim(&store, "leaf-proof");
        record_proof(&store, "leaf-proof", "fixture", "pass", 3600).unwrap();
        let leaf = add_completed(&store, "leaf", "", "leaf-proof");
        let middle = add_completed(&store, "middle", &leaf, "");
        let root = add_completed(&store, "root", &middle, "");
        let before = serde_json::to_value(list_obligations(&store).unwrap()).unwrap();
        assert!(
            list_obligations(&store)
                .unwrap()
                .iter()
                .all(|node| node.status == ObligationStatus::Open)
        );

        // Derive the outermost node first: verifying a dependency separately
        // must not be a prerequisite for transitive readiness.
        let result = derive_obligation_status(&store, &root).unwrap();
        assert_eq!(result.derived_status, ObligationStatus::Met);
        assert!(result.dependencies_satisfied);
        let graph = validate_obligation_graph(&store).unwrap();
        assert!(graph.is_valid, "{graph:?}");
        assert_eq!(graph.total_nodes, 3);
        assert_eq!(graph.total_edges, 2);
        assert_eq!(
            serde_json::to_value(list_obligations(&store).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn stored_met_dependency_cannot_hide_invalid_current_evidence() {
        let (store, _temp) = test_store();
        add_proof_claim(&store, "unproved");
        poison_verified_cache(&store, "unproved");
        let leaf = add_completed(&store, "cached-leaf", "", "unproved");
        set_cached_status(&store, &leaf, "met");
        let root = add_completed(&store, "cached-parent", &leaf, "");
        let result = derive_obligation_status(&store, &root).unwrap();
        assert!(!result.dependencies_satisfied);
        assert_ne!(result.derived_status, ObligationStatus::Met);
        assert_eq!(
            get_obligation(&store, &leaf).unwrap().status,
            ObligationStatus::Met
        );
    }

    #[test]
    fn diamond_graph_derives_shared_dependency_and_propagates_later_failure() {
        let (store, _temp) = test_store();
        add_proof_claim(&store, "shared-proof");
        insert_proof_at(&store, "shared-proof", "pass", now_seconds() - 10, 3600);
        let leaf = add_completed(&store, "shared-leaf", "", "shared-proof");
        let left = add_completed(&store, "left", &leaf, "");
        let right = add_completed(&store, "right", &leaf, "");
        let root = add_completed(&store, "diamond-root", &format!("{left},{right}"), "");
        let graph = validate_obligation_graph(&store).unwrap();
        assert!(graph.is_valid, "{graph:?}");
        assert!(!graph.has_cycles);
        assert_eq!(graph.total_nodes, 4);
        assert_eq!(graph.total_edges, 4);
        assert_eq!(
            derive_obligation_status(&store, &root)
                .unwrap()
                .derived_status,
            ObligationStatus::Met
        );

        insert_proof_at(&store, "shared-proof", "fail", now_seconds(), 3600);
        poison_verified_cache(&store, "shared-proof");
        for id in [&leaf, &left, &right, &root] {
            assert_unmet(&store, id);
        }
        assert_eq!(
            validate_obligation_graph(&store)
                .unwrap()
                .unsatisfied_obligations
                .len(),
            4
        );
    }

    #[test]
    fn deep_chain_uses_current_transitive_evidence() {
        let (store, _temp) = test_store();
        let leaf = add_obligation(&store, "unfinished-leaf", "medium", "", "").unwrap();
        let mut root = leaf.clone();
        for depth in 1..64 {
            root = add_completed(&store, &format!("chain-{depth}"), &root, "");
        }
        assert_unmet(&store, &root);
        complete_obligation(&store, &leaf, "leaf-state-commit").unwrap();
        assert_eq!(
            derive_obligation_status(&store, &root)
                .unwrap()
                .derived_status,
            ObligationStatus::Met
        );
        let graph = validate_obligation_graph(&store).unwrap();
        assert!(graph.is_valid, "{graph:?}");
        assert_eq!(graph.total_nodes, 64);
        assert_eq!(graph.total_edges, 63);
        assert!(
            list_obligations(&store)
                .unwrap()
                .iter()
                .all(|node| node.status == ObligationStatus::Open)
        );
    }

    #[test]
    fn missing_dependency_addition_is_atomic() {
        let (store, _temp) = test_store();
        let existing = add_completed(&store, "existing", "", "");
        let before = serde_json::to_value(list_obligations(&store).unwrap()).unwrap();
        for dependencies in ["missing".to_string(), format!("{existing},missing")] {
            assert!(
                add_obligation(&store, "must-not-survive", "medium", &dependencies, "").is_err()
            );
            assert_eq!(
                serde_json::to_value(list_obligations(&store).unwrap()).unwrap(),
                before
            );
            let edges: i64 = fixture_db(&store)
                .query_row("SELECT COUNT(*) FROM obligation_edges", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(edges, 0, "partial dependency edges must roll back");
        }
    }

    #[test]
    fn completion_rejects_blank_commit_and_missing_obligation_without_mutation() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "completion-input", "medium", "", "").unwrap();
        let before = serde_json::to_value(get_obligation(&store, &id).unwrap()).unwrap();
        for commit in ["", " ", "\t\n"] {
            assert!(
                complete_obligation(&store, &id, commit).is_err(),
                "accepted {commit:?}"
            );
            assert_eq!(
                serde_json::to_value(get_obligation(&store, &id).unwrap()).unwrap(),
                before
            );
        }
        assert!(complete_obligation(&store, "unknown-obligation", "state-commit").is_err());
        assert_eq!(list_obligations(&store).unwrap().len(), 1);
    }

    #[test]
    fn blank_legacy_commit_is_missing_and_reported_without_required_proofs() {
        let (store, _temp) = test_store();
        let id = add_obligation(&store, "legacy-empty-commit", "medium", "", "").unwrap();
        fixture_db(&store)
            .execute(
                "UPDATE obligations SET state_commit_root = '  ' WHERE id = ?1",
                params![id],
            )
            .unwrap();
        let result = derive_obligation_status(&store, &id).unwrap();
        assert!(!result.commit_present);
        assert_ne!(result.derived_status, ObligationStatus::Met);
        assert!(
            validate_obligation_graph(&store)
                .unwrap()
                .missing_commits
                .contains(&id)
        );
    }

    #[test]
    fn malformed_required_proofs_fail_every_reader_closed() {
        let (store, _temp) = test_store();
        let leaf = add_completed(&store, "corrupt-leaf", "", "");
        let parent = add_completed(&store, "corrupt-parent", &leaf, "");
        for malformed in [
            "{",
            "{}",
            "null",
            "[null]",
            "[1]",
            "[\"valid\",false]",
            "[\"\"]",
            "[\"  \"]",
        ] {
            fixture_db(&store)
                .execute(
                    "UPDATE obligations SET required_proofs = ?1 WHERE id = ?2",
                    params![malformed, leaf],
                )
                .unwrap();
            assert!(
                get_obligation(&store, &leaf).is_err(),
                "get accepted {malformed}"
            );
            assert!(
                list_obligations(&store).is_err(),
                "list accepted {malformed}"
            );
            assert!(
                get_dependencies(&store, &parent).is_err(),
                "dependencies accepted {malformed}"
            );
            assert!(
                derive_obligation_status(&store, &leaf).is_err(),
                "derive accepted {malformed}"
            );
            assert!(
                derive_obligation_status(&store, &parent).is_err(),
                "transitive derive accepted {malformed}"
            );
            assert!(
                validate_obligation_graph(&store).is_err(),
                "graph accepted {malformed}"
            );
        }
    }

    fn legacy_edges_fixture() -> (Store, TempDir) {
        let (store, temp) = test_store();
        // Negative legacy fixture only: recreate the edge table without FK
        // constraints to model historical/corrupt rows that normal writes reject.
        // Production schema and write paths are never weakened.
        let conn = fixture_db(&store);
        conn.execute("DROP TABLE obligation_edges", []).unwrap();
        conn.execute(
            "CREATE TABLE obligation_edges (
                edge_id TEXT PRIMARY KEY, from_id TEXT NOT NULL, to_id TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'depends_on', created_at TEXT NOT NULL,
                UNIQUE(from_id, to_id)
             )",
            [],
        )
        .unwrap();
        drop(conn);
        (store, temp)
    }

    fn insert_legacy_edge(store: &Store, from: &str, to: &str) {
        fixture_db(store)
            .execute(
                "INSERT INTO obligation_edges(edge_id, from_id, to_id, kind, created_at)
             VALUES(?1, ?2, ?3, 'depends_on', '1Z')",
                params![decapod::core::ulid::new_ulid(), from, to],
            )
            .unwrap();
    }

    #[test]
    fn dangling_targets_and_orphan_sources_are_reported_and_counted() {
        let (store, _temp) = legacy_edges_fixture();
        let id = add_completed(&store, "live-node", "", "");
        insert_legacy_edge(&store, &id, "missing-target");
        insert_legacy_edge(&store, "orphan-source", &id);
        insert_legacy_edge(&store, "orphan-both", "missing-both");
        assert!(get_dependencies(&store, &id).is_err());
        assert_unmet(&store, &id);
        let graph = validate_obligation_graph(&store).unwrap();
        assert!(!graph.is_valid);
        assert!(!graph.has_cycles);
        assert_eq!(graph.total_nodes, 1);
        assert_eq!(
            graph.total_edges, 3,
            "raw edge count must include orphan sources"
        );
        let errors = graph.dependency_errors.join(" ");
        for missing in [
            "missing-target",
            "orphan-source",
            "orphan-both",
            "missing-both",
        ] {
            assert!(
                errors.contains(missing),
                "missing endpoint {missing}: {graph:?}"
            );
        }
    }

    #[test]
    fn orphan_source_alone_invalidates_an_otherwise_met_graph() {
        let (store, _temp) = legacy_edges_fixture();
        let id = add_completed(&store, "met-node", "", "");
        insert_legacy_edge(&store, "absent-source", &id);
        let graph = validate_obligation_graph(&store).unwrap();
        assert!(!graph.is_valid);
        assert_eq!(graph.total_edges, 1);
        assert!(
            graph
                .dependency_errors
                .iter()
                .any(|error| error.contains("absent-source"))
        );
    }

    #[test]
    fn completed_cycles_and_self_edges_never_derive_met() {
        for self_cycle in [false, true] {
            let (store, _temp) = legacy_edges_fixture();
            let first = add_completed(&store, "cycle-first", "", "");
            let second = if self_cycle {
                first.clone()
            } else {
                add_completed(&store, "cycle-second", &first, "")
            };
            insert_legacy_edge(&store, &first, &second);
            set_cached_status(&store, &first, "met");
            set_cached_status(&store, &second, "met");
            assert_unmet(&store, &first);
            assert_unmet(&store, &second);
            let graph = validate_obligation_graph(&store).unwrap();
            assert!(!graph.is_valid);
            assert!(graph.has_cycles);
            assert!(!graph.cycle_errors.is_empty());
            assert_eq!(graph.total_edges, if self_cycle { 1 } else { 2 });
        }
    }

    fn resolve_decapod_bin() -> PathBuf {
        let cargo_bin = env!("CARGO_BIN_EXE_decapod");
        if let Ok(path) = Path::new(cargo_bin).canonicalize() {
            return path;
        }
        if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
            let path = Path::new(&runfiles_dir).join("_main").join("decapod");
            if path.exists() {
                return path;
            }
        }
        if let Some(parent) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
        {
            let path = parent.join("decapod");
            if path.exists() {
                return path;
            }
        }
        PathBuf::from(cargo_bin)
    }

    struct CliFixture {
        temp: TempDir,
        worktree: PathBuf,
        store: Store,
    }

    impl CliFixture {
        fn command(&self, program: impl AsRef<std::ffi::OsStr>, cwd: &Path) -> Command {
            let mut command = Command::new(program);
            command.current_dir(cwd).stdin(Stdio::null());
            // The regression exercises real worktree and session enforcement.
            // Never inherit a developer's gate-bypass or controller environment.
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("DECAPOD_") {
                    command.env_remove(key);
                }
            }
            command
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env("HOME", self.temp.path().join("home"))
                .env("XDG_CONFIG_HOME", self.temp.path().join("config"))
                .env("XDG_DATA_HOME", self.temp.path().join("data"))
                .env("DECAPOD_AGENT_ID", "obligation-cli-regression")
                // Claim the real task without optional container preparation.
                .env("DECAPOD_CLAIM_AUTORUN", "0")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    self.temp.path().join("empty-gitconfig"),
                )
                .env("GIT_AUTHOR_NAME", "Obligation Regression")
                .env("GIT_AUTHOR_EMAIL", "obligation@example.invalid")
                .env("GIT_COMMITTER_NAME", "Obligation Regression")
                .env("GIT_COMMITTER_EMAIL", "obligation@example.invalid");
            command
        }

        fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let repo = temp.path().join("repo");
            std::fs::create_dir_all(&repo).unwrap();
            std::fs::create_dir_all(temp.path().join("home")).unwrap();
            let mut fixture = Self {
                store: Store {
                    root: repo.join(".decapod/data"),
                    kind: StoreKind::Repo,
                },
                temp,
                worktree: repo.clone(),
            };
            let output = fixture
                .command("git", &repo)
                .args(["init", "-b", "main"])
                .bounded_output(Duration::from_secs(30))
                .unwrap();
            assert!(output.status.success(), "git init: {output:?}");
            let init = fixture
                .command(resolve_decapod_bin(), &repo)
                .args(["init", "--force"])
                .bounded_output(Duration::from_secs(60))
                .unwrap();
            assert!(init.status.success(), "fixture init: {init:?}");
            for args in [
                vec!["add", "-A"],
                vec!["commit", "-m", "Initialize obligation fixture"],
            ] {
                let output = fixture
                    .command("git", &repo)
                    .args(args)
                    .bounded_output(Duration::from_secs(60))
                    .unwrap();
                assert!(output.status.success(), "fixture commit: {output:?}");
            }
            for args in [&["docs", "ingest"][..], &["session", "acquire"][..]] {
                let output = fixture.run(args);
                assert!(
                    output.status.success(),
                    "fixture bootstrap {args:?}: {output:?}"
                );
            }
            let task = fixture.json(
                &[
                    "todo",
                    "--format",
                    "json",
                    "add",
                    "Exercise obligation CLI readiness",
                    "--scope",
                    "workspace",
                ],
                true,
            );
            let task_id = task["id"].as_str().expect("real fixture todo ID");
            let claimed = fixture.run(&["todo", "--format", "json", "claim", "--id", task_id]);
            assert!(claimed.status.success(), "fixture task claim: {claimed:?}");
            let workspace = fixture.json(&["workspace", "ensure"], true);
            fixture.worktree = PathBuf::from(
                workspace["worktree_path"]
                    .as_str()
                    .expect("governed isolated fixture path"),
            );
            assert_ne!(fixture.worktree, repo);
            assert!(fixture.worktree.is_dir());
            let initialized = fixture.run(&["obligation", "list"]);
            assert!(
                initialized.status.success(),
                "fixture obligations: {initialized:?}"
            );
            fixture
        }

        fn run(&self, args: &[&str]) -> Output {
            self.command(resolve_decapod_bin(), &self.worktree)
                .args(args)
                .bounded_output(Duration::from_secs(60))
                .expect("run obligation CLI")
        }

        fn json(&self, args: &[&str], success: bool) -> serde_json::Value {
            let output = self.run(args);
            assert_eq!(output.status.success(), success, "{args:?}: {output:?}");
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!("{args:?} did not emit machine-readable JSON: {error}: {output:?}")
            })
        }
    }

    #[test]
    fn cli_verify_and_validate_graph_report_unmet_and_met_with_json_exit_contract() {
        let fixture = CliFixture::new();
        let added = fixture.run(&["obligation", "add", "--intent", "cli-exit-contract"]);
        assert!(added.status.success(), "{added:?}");
        let nodes = fixture.json(&["obligation", "list"], true);
        let id = nodes[0]["id"].as_str().expect("added obligation id");
        let verify = fixture.json(&["obligation", "verify", "--id", id], false);
        assert_eq!(verify["derived_status"], "open");
        assert_eq!(verify["commit_present"], false);
        let graph = fixture.json(&["obligation", "validate-graph"], false);
        assert_eq!(graph["is_valid"], false);
        assert_eq!(graph["total_nodes"], 1);
        assert!(
            graph["missing_commits"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node.as_str() == Some(id))
        );

        let completed = fixture.run(&[
            "obligation",
            "complete",
            "--id",
            id,
            "--commit",
            "cli-state-commit",
        ]);
        assert!(completed.status.success(), "{completed:?}");
        let verify = fixture.json(&["obligation", "verify", "--id", id], true);
        assert_eq!(verify["derived_status"], "met");
        let graph = fixture.json(&["obligation", "validate-graph"], true);
        assert_eq!(graph["is_valid"], true);
        assert_eq!(graph["total_edges"], 0);
        assert_eq!(
            get_obligation(&fixture.store, id).unwrap().status,
            ObligationStatus::Open
        );
    }

    #[test]
    fn cli_verify_rejects_poisoned_health_cache_after_completion() {
        let fixture = CliFixture::new();
        add_proof_claim(&fixture.store, "cli-poison");
        poison_verified_cache(&fixture.store, "cli-poison");
        let id = add_completed(&fixture.store, "cli-invalid-proof", "", "cli-poison");
        let verify = fixture.json(&["obligation", "verify", "--id", &id], false);
        assert_eq!(verify["proofs_satisfied"], false);
        assert_ne!(verify["derived_status"], "met");
        let graph = fixture.json(&["obligation", "validate-graph"], false);
        assert_eq!(graph["is_valid"], false);
        assert!(
            graph["missing_proofs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node.as_str() == Some(id.as_str()))
        );
    }
}
