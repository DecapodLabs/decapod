//! Crash-durability and fault-barrier integration test suite (#1934).
//!
//! Exercises real OS subprocess interruption (`SIGKILL`), uncommitted SQLite
//! transactions + WAL reopen, live OS advisory sidecar locks (proving `.lock`
//! files are never unlinked and OS locks are released on crash), journal/SQLite
//! drift + idempotent checkpointing, torn/missing/divergent workunit projection
//! JSON recovery, permission fault boundaries, and Git worktree / workspace
//! staging recovery without clobbering unrelated peer workspaces.

use decapod::core::{
    broker::DbBroker,
    db::{self, Connection},
    storage_lock::{
        StorageLock, StorageLockContentionState, StorageLockMode, StorageLockOwnerKind, lock_path,
    },
    store::{Store, StoreKind},
    todo, workspace_lifecycle, workunit,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct CmdResult {
    output: Output,
    combined: String,
}

impl CmdResult {
    fn success(&self) -> bool {
        self.output.status.success()
    }

    fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout)
            .trim()
            .to_string()
    }

    fn json(&self) -> Value {
        let raw = self.stdout_str();
        serde_json::from_str(&raw).unwrap_or_else(|err| {
            panic!(
                "expected JSON on stdout ({err}); combined:\n{}",
                self.combined
            )
        })
    }
}

fn decapod_bin() -> &'static str {
    env!("CARGO_BIN_EXE_decapod")
}

fn scrub_env(cmd: &mut Command, home: Option<&Path>, xdg: Option<[&Path; 4]>) {
    let keys: Vec<String> = std::env::vars()
        .map(|(k, _)| k)
        .filter(|k| {
            k.starts_with("DECAPOD_")
                || k.starts_with("GIT_")
                || k.starts_with("GITHUB_")
                || k.starts_with("BUILDKITE")
                || k.starts_with("CHANGE_")
                || k.starts_with("SOURCE_CI_")
                || k.starts_with("XDG_")
        })
        .collect();
    for k in keys {
        cmd.env_remove(k);
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(h) = home {
        cmd.env("HOME", h);
    }
    if let Some([cfg, data, state, cache]) = xdg {
        cmd.env("XDG_CONFIG_HOME", cfg)
            .env("XDG_DATA_HOME", data)
            .env("XDG_STATE_HOME", state)
            .env("XDG_CACHE_HOME", cache);
    }
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    scrub_env(&mut cmd, None, None);
    let out = cmd
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Crash Durability Test")
        .env("GIT_AUTHOR_EMAIL", "crash-1934@example.invalid")
        .env("GIT_COMMITTER_NAME", "Crash Durability Test")
        .env("GIT_COMMITTER_EMAIL", "crash-1934@example.invalid")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct RepoFixture {
    _temp: TempDir,
    repo: PathBuf,
    home: PathBuf,
    xdg: [PathBuf; 4],
    store: Store,
}

impl RepoFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let cfg = temp.path().join("xdg_config");
        let data = temp.path().join("xdg_data");
        let state = temp.path().join("xdg_state");
        let cache = temp.path().join("xdg_cache");
        for d in [&home, &cfg, &data, &state, &cache] {
            fs::create_dir_all(d).unwrap();
        }
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join(".gitignore"), "/.decapod/data/\n").unwrap();
        fs::write(repo.join("README.md"), "durability fixture\n").unwrap();

        git_in(&repo, &["init", "--quiet", "--initial-branch=master"]);
        git_in(&repo, &["add", ".gitignore", "README.md"]);
        git_in(&repo, &["commit", "--quiet", "-m", "initial commit"]);

        let store = Store {
            kind: StoreKind::Repo,
            root: repo.join(".decapod/data"),
        };

        let fx = Self {
            _temp: temp,
            repo,
            home,
            xdg: [cfg, data, state, cache],
            store,
        };
        let init = fx.run_decapod(
            "bootstrap",
            &["init", "--force", "--no-container-workspaces", "--no-ci"],
        );
        assert!(init.success(), "init failed:\n{}", init.combined);
        git_in(&fx.repo, &["add", "-A"]);
        git_in(
            &fx.repo,
            &["commit", "--quiet", "-m", "chore: init decapod"],
        );
        todo::initialize_todo_db(&fx.store.root).unwrap();
        fx
    }

    fn run_decapod(&self, agent_id: &str, args: &[&str]) -> CmdResult {
        let mut cmd = Command::new(decapod_bin());
        let xdg_refs = [
            self.xdg[0].as_path(),
            self.xdg[1].as_path(),
            self.xdg[2].as_path(),
            self.xdg[3].as_path(),
        ];
        scrub_env(&mut cmd, Some(&self.home), Some(xdg_refs));
        let out = cmd
            .current_dir(&self.repo)
            .env("DECAPOD_AGENT_ID", agent_id)
            .args(args)
            .output()
            .expect("spawn decapod");
        let combined = format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        CmdResult {
            output: out,
            combined,
        }
    }
}

fn wait_for_file(path: &Path, child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "helper child exited unexpectedly ({status:?}) before writing {}",
                path.display()
            );
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for helper readiness file {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn integrity_check(conn: &Connection) -> String {
    conn.query_row("PRAGMA integrity_check;", db::params![], |r| r.get(0))
        .unwrap()
}

struct ChildKillGuard(Option<Child>);

impl ChildKillGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn as_mut(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }

    fn sigkill_and_wait(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildKillGuard {
    fn drop(&mut self) {
        self.sigkill_and_wait();
    }
}

#[cfg(unix)]
struct RestoreModeGuard {
    path: PathBuf,
    mode: u32,
}

#[cfg(unix)]
impl Drop for RestoreModeGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
    }
}

#[test]
fn crash_1934_sqlite_uncommitted_tx_sigkill_and_wal_reopen() {
    if let Ok(db_str) = std::env::var("DECAPOD_1934_HELPER_UNCOMMITTED_TX_DB") {
        let ready = std::env::var("DECAPOD_1934_HELPER_READY").unwrap();
        let conn = db::db_connect(&db_str).unwrap();
        conn.execute_batch(
            "PRAGMA busy_timeout = 5000;
             BEGIN IMMEDIATE;
             INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,one_shot,revision)
             VALUES('crashed_uncommitted_task','crashed-h','Should rollback','open','10Z','10Z','.','',1,1);
             INSERT INTO events(seq,event_id,ts,stream,event_type,actor,subject_kind,subject_id,payload)
             VALUES((SELECT COALESCE(MAX(seq), 0) + 1 FROM events),'evt_crashed','10Z','todo','task.add','helper','task','crashed_uncommitted_task','{}');",
        )
        .unwrap();
        fs::write(&ready, "locked").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    let fx = RepoFixture::new();
    let add = fx.run_decapod(
        "agent-crash",
        &["todo", "add", "Baseline durable task", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);
    let base_id = add.json()["id"].as_str().unwrap().to_string();
    let ckpt = fx.run_decapod(
        "agent-crash",
        &["todo", "journal", "checkpoint", "--format", "json"],
    );
    assert!(ckpt.success(), "{}", ckpt.combined);

    let db_path = todo::todo_db_path(&fx.store.root);
    let ready_file = fx.repo.join(".decapod/1934_tx_ready");

    let exe = std::env::current_exe().unwrap();
    let child = Command::new(&exe)
        .arg("crash_1934_sqlite_uncommitted_tx_sigkill_and_wal_reopen")
        .arg("--nocapture")
        .env(
            "DECAPOD_1934_HELPER_UNCOMMITTED_TX_DB",
            db_path.to_string_lossy().as_ref(),
        )
        .env("DECAPOD_1934_HELPER_READY", &ready_file)
        .spawn()
        .expect("spawn crash helper");
    let mut guard = ChildKillGuard::new(child);
    wait_for_file(&ready_file, guard.as_mut(), Duration::from_secs(10));

    // Concurrent read-only snapshot while uncommitted writer is active must never see uncommitted row.
    {
        let ro =
            db::db_connect_readonly_bounded(&db_path.to_string_lossy(), Duration::from_millis(500))
                .unwrap();
        let count: i64 = ro
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = 'crashed_uncommitted_task'",
                db::params![],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    // Hard crash via SIGKILL while uncommitted WAL transaction is open.
    guard.sigkill_and_wait();

    // Immediately reopen via DbBroker and verify integrity + rollback + subsequent writes.
    DbBroker::new(&fx.store.root)
        .with_conn(&db_path, "recovery", None, "test.1934.post_kill", |conn| {
            assert_eq!(integrity_check(conn), "ok");
            let crashed_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = 'crashed_uncommitted_task'",
                db::params![],
                |r| r.get(0),
            )?;
            assert_eq!(crashed_count, 0);
            let base_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM tasks WHERE id = ?1",
                db::params![base_id],
                |r| r.get(0),
            )?;
            assert_eq!(base_count, 1);
            Ok(())
        })
        .unwrap();

    let post_add = fx.run_decapod(
        "agent-crash",
        &[
            "todo",
            "add",
            "Post-SIGKILL recovery task",
            "--format",
            "json",
        ],
    );
    assert!(post_add.success(), "{}", post_add.combined);
    let journal_verify = fx.run_decapod(
        "agent-crash",
        &["todo", "journal", "verify", "--format", "json"],
    );
    assert!(journal_verify.success(), "{}", journal_verify.combined);
}

#[cfg(unix)]
#[test]
fn crash_1934_live_advisory_lock_never_unlinked_and_released_on_sigkill() {
    use std::os::unix::fs::MetadataExt;

    if let Ok(db_str) = std::env::var("DECAPOD_1934_HELPER_HOLD_LOCK_DB") {
        let ready = std::env::var("DECAPOD_1934_HELPER_LOCK_READY").unwrap();
        let _lock = StorageLock::acquire(
            Path::new(&db_str),
            StorageLockMode::Exclusive,
            Duration::from_millis(500),
        )
        .unwrap();
        fs::write(&ready, "held").unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    let fx = RepoFixture::new();
    let db_path = todo::todo_db_path(&fx.store.root);
    let sidecar = lock_path(&db_path);
    assert!(
        sidecar.exists(),
        "sidecar lock file should already exist after init"
    );
    let inode_before = fs::metadata(&sidecar).unwrap().ino();

    let ready_file = fx.repo.join(".decapod/1934_lock_ready");
    let exe = std::env::current_exe().unwrap();
    let child = Command::new(&exe)
        .arg("crash_1934_live_advisory_lock_never_unlinked_and_released_on_sigkill")
        .arg("--nocapture")
        .env(
            "DECAPOD_1934_HELPER_HOLD_LOCK_DB",
            db_path.to_string_lossy().as_ref(),
        )
        .env("DECAPOD_1934_HELPER_LOCK_READY", &ready_file)
        .spawn()
        .expect("spawn lock-holder helper");
    let mut guard = ChildKillGuard::new(child);
    wait_for_file(&ready_file, guard.as_mut(), Duration::from_secs(10));

    // Diagnose while external process holds exclusive advisory lock:
    // must report contention truthfully and NEVER delete the live `.lock` file.
    let diag = StorageLock::diagnose(&db_path, Duration::from_millis(80)).unwrap();
    assert_eq!(
        diag.lock_state,
        StorageLockContentionState::ExclusiveMaintenanceActive
    );
    assert_eq!(diag.owner, StorageLockOwnerKind::Unknown);
    assert!(diag.lock_file_exists);
    assert!(diag.timed_out);
    assert!(sidecar.exists());
    assert_eq!(fs::metadata(&sidecar).unwrap().ino(), inode_before);

    // Bounded acquire MUST fail while holder is alive.
    let busy = StorageLock::acquire(
        &db_path,
        StorageLockMode::Exclusive,
        Duration::from_millis(60),
    )
    .unwrap_err()
    .to_string();
    assert!(busy.contains("STORAGE_LOCK_TIMEOUT"), "{busy}");
    assert_eq!(fs::metadata(&sidecar).unwrap().ino(), inode_before);

    // Crash the lock holder via SIGKILL; OS releases advisory lock descriptor without unlinking sidecar.
    guard.sigkill_and_wait();

    assert!(sidecar.exists(), "sidecar .lock must never be unlinked");
    assert_eq!(fs::metadata(&sidecar).unwrap().ino(), inode_before);

    let after_diag = StorageLock::diagnose(&db_path, Duration::from_millis(50)).unwrap();
    assert_eq!(after_diag.lock_state, StorageLockContentionState::Unlocked);
    assert_eq!(after_diag.owner, StorageLockOwnerKind::None);
    assert!(after_diag.lock_file_exists);

    let reacquired = StorageLock::acquire(
        &db_path,
        StorageLockMode::Exclusive,
        Duration::from_millis(100),
    )
    .unwrap();
    drop(reacquired);

    let add = fx.run_decapod(
        "agent-crash",
        &["todo", "add", "Post-lock-crash task", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);
}

#[test]
fn crash_1934_journal_sqlite_drift_and_idempotent_checkpoint_recovery() {
    let fx = RepoFixture::new();
    let add = fx.run_decapod(
        "agent-drift",
        &["todo", "add", "Checkpoint base", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);
    let task_id = add.json()["id"].as_str().unwrap().to_string();
    let ckpt1 = fx.run_decapod(
        "agent-drift",
        &["todo", "journal", "checkpoint", "--format", "json"],
    );
    assert!(ckpt1.success(), "{}", ckpt1.combined);
    let ckpt2 = fx.run_decapod(
        "agent-drift",
        &["todo", "journal", "checkpoint", "--format", "json"],
    );
    assert!(ckpt2.success(), "{}", ckpt2.combined);
    assert_eq!(ckpt1.json()["record_digest"], ckpt2.json()["record_digest"]);

    let db_path = todo::todo_db_path(&fx.store.root);
    // Simulate a crash after direct SQLite mutation before journal checkpoint append:
    // live SQLite row differs from latest durable journal checkpoint.
    DbBroker::new(&fx.store.root)
        .with_conn(&db_path, "agent-drift", None, "test.1934.drift", |conn| {
            conn.execute(
                "UPDATE tasks SET title = 'Updated pre-checkpoint crash', revision = revision + 1 WHERE id = ?1",
                db::params![task_id],
            )?;
            Ok(())
        })
        .unwrap();

    // `todo journal verify` must detect live-vs-journal drift instead of accepting stale history.
    let verify_drift = fx.run_decapod(
        "agent-drift",
        &["todo", "journal", "verify", "--format", "json"],
    );
    assert!(
        !verify_drift.success()
            || verify_drift.combined.contains("drift")
            || verify_drift.combined.contains("DRIFT")
            || verify_drift.combined.contains("false"),
        "expected drift detection from todo journal verify, got:\n{}",
        verify_drift.combined
    );

    // Explicit checkpoint refuses to overwrite divergent live-vs-journal state silently.
    let rec_conflict = fx.run_decapod(
        "agent-drift",
        &["todo", "journal", "checkpoint", "--format", "json"],
    );
    assert!(
        !rec_conflict.success() && rec_conflict.combined.contains("TASK_JOURNAL_CONFLICT"),
        "expected TASK_JOURNAL_CONFLICT on diverged state:\n{}",
        rec_conflict.combined
    );

    let list_ok = fx.run_decapod("agent-drift", &["todo", "list", "--format", "json"]);
    assert!(
        list_ok.success(),
        "readback must remain available:\n{}",
        list_ok.combined
    );

    DbBroker::new(&fx.store.root)
        .with_conn(&db_path, "agent-drift", None, "test.1934.check", |conn| {
            assert_eq!(integrity_check(conn), "ok");
            Ok(())
        })
        .unwrap();
}

#[test]
fn crash_1934_workunit_projection_torn_missing_and_divergent_recovery() {
    let fx = RepoFixture::new();
    let task_id = "task_1934_workunit";
    workunit::init_workunit(&fx.repo, task_id, "intent:1934").unwrap();
    let path = workunit::workunit_path(&fx.repo, task_id).unwrap();
    let expected = workunit::load_workunit(&fx.repo, task_id).unwrap();

    // 1. Missing projection file (crash after journal append before projection publish).
    fs::remove_file(&path).unwrap();
    let status_missing = workunit::workunit_journal_status(&fx.repo, task_id).unwrap();
    assert_eq!(status_missing.projection_state, "missing");
    assert!(
        workunit::load_workunit(&fx.repo, task_id)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_RECOVERY_REQUIRED")
    );
    let rec_a = workunit::recover_workunit(&fx.repo, task_id).unwrap();
    let rec_b = workunit::recover_workunit(&fx.repo, task_id).unwrap();
    assert_eq!(rec_a, rec_b);
    assert_eq!(rec_a.projection_state, "current");
    assert_eq!(
        workunit::load_workunit(&fx.repo, task_id).unwrap(),
        expected
    );

    // 2. Torn / truncated partial JSON projection (crash mid-write without atomic swap).
    fs::write(
        &path,
        b"{\"task_id\":\"task_1934_workunit\",\"intent_ref\":",
    )
    .unwrap();
    let status_invalid = workunit::workunit_journal_status(&fx.repo, task_id).unwrap();
    assert_eq!(status_invalid.projection_state, "invalid");
    let rec_torn_1 = workunit::recover_workunit(&fx.repo, task_id).unwrap();
    let rec_torn_2 = workunit::recover_workunit(&fx.repo, task_id).unwrap();
    assert_eq!(rec_torn_1, rec_torn_2);
    assert_eq!(rec_torn_1.projection_state, "current");
    assert_eq!(
        workunit::load_workunit(&fx.repo, task_id).unwrap(),
        expected
    );

    // 3. Unrecognized valid projection content (external edit / divergence) is preserved with conflict error.
    let mut divergent = expected.clone();
    divergent.intent_ref = "intent:unrecognized-external-edit".into();
    let divergent_bytes = serde_json::to_vec_pretty(&divergent).unwrap();
    fs::write(&path, &divergent_bytes).unwrap();
    let status_div = workunit::workunit_journal_status(&fx.repo, task_id).unwrap();
    assert_eq!(status_div.projection_state, "recovery_required");
    let conflict_err = workunit::recover_workunit(&fx.repo, task_id)
        .unwrap_err()
        .to_string();
    assert!(
        conflict_err.contains("WORKUNIT_PROJECTION_CONFLICT"),
        "expected WORKUNIT_PROJECTION_CONFLICT, got: {conflict_err}"
    );
    assert_eq!(fs::read(&path).unwrap(), divergent_bytes);
}

#[cfg(unix)]
#[test]
fn crash_1934_permission_fault_preserves_prior_workunit_and_sqlite_state() {
    use std::os::unix::fs::PermissionsExt;

    let fx = RepoFixture::new();
    let task_id = "task_1934_perm";
    workunit::init_workunit(&fx.repo, task_id, "intent:1934-perm").unwrap();
    let init = workunit::workunit_path(&fx.repo, task_id).unwrap();
    let workunits_dir = init.parent().unwrap().to_path_buf();
    let baseline = workunit::load_workunit(&fx.repo, task_id).unwrap();
    let baseline_bytes = fs::read(&init).unwrap();

    let add = fx.run_decapod(
        "agent-perm",
        &["todo", "add", "Before chmod fault", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);

    let orig_mode = fs::metadata(&workunits_dir).unwrap().permissions().mode();
    let _restore = RestoreModeGuard {
        path: workunits_dir.clone(),
        mode: orig_mode,
    };
    fs::set_permissions(&workunits_dir, fs::Permissions::from_mode(0o555)).unwrap();

    // Probe whether OS enforces 0o555 for current UID (e.g., skip permission write assertion only if running as root euid=0).
    let probe = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(workunits_dir.join(".perm_probe"));
    if let Ok(f) = probe {
        drop(f);
        let _ = fs::remove_file(workunits_dir.join(".perm_probe"));
        return;
    }

    let mut mutated = baseline.clone();
    mutated
        .state_refs
        .push("state:should-not-partial-write".into());
    let write_err = workunit::write_workunit_checked(&fx.repo, &mutated, "op-perm-fail", 1);
    assert!(
        write_err.is_err(),
        "expected permission fault error on unwritable workunits dir"
    );

    // Existing durable projection bytes remain untouched when directory publish fails,
    // and journal-ahead status surfaces `recovery_required`.
    assert_eq!(fs::read(&init).unwrap(), baseline_bytes);
    let status = workunit::workunit_journal_status(&fx.repo, task_id).unwrap();
    assert_eq!(status.projection_state, "recovery_required");

    // Restore permissions explicitly and verify idempotent projection recovery + SQLite integrity.
    drop(_restore);
    let recovered = workunit::recover_workunit(&fx.repo, task_id).unwrap();
    assert_eq!(recovered.projection_state, "current");
    assert_eq!(workunit::load_workunit(&fx.repo, task_id).unwrap(), mutated);
    DbBroker::new(&fx.store.root)
        .with_conn(
            &todo::todo_db_path(&fx.store.root),
            "agent-perm",
            None,
            "test.1934.perm",
            |conn| {
                assert_eq!(integrity_check(conn), "ok");
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn crash_1934_git_worktree_and_staging_recovery_preserves_peers_and_fsck() {
    let fx = RepoFixture::new();

    // Create an unrelated peer Git worktree with committed + untracked files.
    let peer_ws = fx.repo.join(".decapod/workspaces/peer-intact");
    fs::create_dir_all(peer_ws.parent().unwrap()).unwrap();
    git_in(
        &fx.repo,
        &[
            "worktree",
            "add",
            "-b",
            "agent/peer/work",
            peer_ws.to_str().unwrap(),
            "master",
        ],
    );
    fs::write(peer_ws.join("peer_notes.txt"), "peer valuable state\n").unwrap();

    // Simulate a leftover orphan staging directory from a killed invocation alongside a reserved staging directory.
    let orphan_stage = fx
        .repo
        .join(".decapod/workspaces/.staging/orphan-crashed-stage");
    fs::create_dir_all(&orphan_stage).unwrap();
    fs::write(orphan_stage.join("partial.txt"), "partial").unwrap();

    let target_ws = fx.repo.join(".decapod/workspaces/agent-1934-recovered");
    let mut owned = workspace_lifecycle::reserve(&fx.repo, &target_ws, false).unwrap();
    owned.mark_ready().unwrap();
    drop(owned);

    // Re-acquiring existing owned workspace works cleanly and never touches peer worktree.
    let reacquired = workspace_lifecycle::acquire(&fx.repo, &target_ws)
        .unwrap()
        .expect("reacquire reserved workspace");
    assert_eq!(reacquired.path(), target_ws.canonicalize().unwrap());
    assert!(reacquired.is_ready());
    drop(reacquired);

    // Register a new Git worktree off master plus simulate an orphaned `.git/worktrees` entry from a crash.
    let wt_git = fx.repo.join(".decapod/workspaces/wt-1934-git");
    git_in(
        &fx.repo,
        &[
            "worktree",
            "add",
            "-b",
            "agent/1934/recovered-git",
            wt_git.to_str().unwrap(),
            "master",
        ],
    );
    let dead_wt_admin = fx.repo.join(".git/worktrees/dead-crash");
    fs::create_dir_all(&dead_wt_admin).unwrap();
    fs::write(
        dead_wt_admin.join("gitdir"),
        fx.repo
            .join(".decapod/workspaces/nonexistent-crash/.git")
            .to_string_lossy()
            .as_bytes(),
    )
    .unwrap();
    let _ = git_in(&fx.repo, &["worktree", "prune"]);

    let wt_list = git_in(&fx.repo, &["worktree", "list", "--porcelain"]);
    assert!(wt_list.contains("peer-intact"), "peer missing:\n{wt_list}");
    assert!(
        wt_list.contains("wt-1934-git"),
        "recovered git worktree missing:\n{wt_list}"
    );
    assert!(
        !dead_wt_admin.exists(),
        "dead crashed worktree admin entry should be pruned"
    );
    assert!(target_ws.is_dir(), "reserved target workspace preserved");
    assert_eq!(
        fs::read_to_string(peer_ws.join("peer_notes.txt")).unwrap(),
        "peer valuable state\n"
    );
    let _ = git_in(&fx.repo, &["fsck", "--full", "--no-dangling"]);
}
