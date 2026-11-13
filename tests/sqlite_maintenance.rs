#[cfg(any(target_os = "linux", target_os = "android"))]
use dactyl_db::RecoveryJournalMode;
use dactyl_db::{AccessMode, AdapterErrorKind};
use decapod::core::dactyl::DactylBridge;
use decapod::core::error::{DecapodError, StorageFailureKind};
use fs2::FileExt;
use std::fs::{self, OpenOptions};
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

// Private fixtures cover default storage. Shared recovery is exercised separately;
// production recovery never chmods an existing installation to proceed.
fn private_recovery_directory() -> TempDir {
    let directory = tempdir().expect("temporary recovery directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .expect("private recovery fixture");
    }
    directory
}

fn open_local(path: &Path, access_mode: AccessMode) -> DactylBridge {
    DactylBridge::open_local(path, access_mode)
        .expect("required native SQLite maintenance capability must be available")
}

#[test]
fn healthy_verification_reports_dactyl_metadata() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("healthy.db");
    let db = open_local(&path, AccessMode::ReadWrite);
    db.write(
        "create table records (id integer primary key, value text)",
        &[],
    )
    .expect("schema");
    db.write("pragma journal_mode = WAL", &[])
        .expect("WAL mode");
    db.write("pragma user_version = 37", &[])
        .expect("user version");
    db.write("pragma application_id = 4242", &[])
        .expect("application id");
    db.write("insert into records (value) values ('healthy')", &[])
        .expect("application row");

    let report = db.verify_integrity().expect("healthy verification");
    assert_eq!(report.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(report.user_version, 37);
    assert_eq!(report.application_id, 4242);
}

#[test]
fn online_backup_preserves_wal_data_metadata_and_has_no_sidecars() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("source.db");
    let backup_path = directory.path().join("backup.db");
    let source = open_local(&source_path, AccessMode::ReadWrite);
    source
        .write(
            "create table records (id integer primary key, payload blob)",
            &[],
        )
        .expect("schema");
    source
        .write("pragma journal_mode = WAL", &[])
        .expect("WAL mode");
    source
        .write("pragma user_version = 9", &[])
        .expect("user version");
    source
        .write("pragma application_id = 99", &[])
        .expect("application id");
    source
        .write(
            "insert into records (payload) values (?)",
            &[vec![1_u8, 2, 3].into()],
        )
        .expect("application row");

    let result = source.backup(&backup_path).expect("online backup");
    assert_eq!(result.source_journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(result.destination_journal_mode.to_ascii_lowercase(), "wal");
    assert!(result.bytes > 0);
    assert!(backup_path.is_file());
    assert!(!sidecar(&backup_path, "-wal").exists());
    assert!(!sidecar(&backup_path, "-shm").exists());

    drop(source);
    // A WAL database must be reopened read-write so SQLite can establish its
    // own current sidecars; the backup itself was published without copying
    // source `-wal`/`-shm` files.
    let backup = open_local(&backup_path, AccessMode::ReadWrite);
    let report = backup.verify_integrity().expect("backup verification");
    assert_eq!(report.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(report.user_version, 9);
    assert_eq!(report.application_id, 99);
    let rows = backup
        .read("select payload from records", &[])
        .expect("backup data");
    assert_eq!(rows.as_slice()[0].get_blob(0).expect("payload"), &[1, 2, 3]);
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn damaged_secondary_index_is_reported_and_explicit_recovery_preserves_data() {
    let directory = private_recovery_directory();
    let source_path = directory.path().join("corrupt.db");
    let archive_path = directory.path().join("corrupt.before-recovery.db");
    let (page_size, root_page) = {
        let db = open_local(&source_path, AccessMode::ReadWrite);
        db.write(
            "create table records (id integer primary key, name text not null, payload blob)",
            &[],
        )
        .expect("schema");
        db.write("create unique index records_name on records(name)", &[])
            .expect("secondary index");
        db.write("pragma journal_mode = WAL", &[])
            .expect("WAL mode");
        db.write("pragma user_version = 9", &[])
            .expect("user version");
        db.write("pragma application_id = 99", &[])
            .expect("application id");
        db.write(
            "insert into records (name, payload) values (?, ?)",
            &["one".into(), vec![1_u8].into()],
        )
        .expect("first row");
        db.write(
            "insert into records (name, payload) values (?, ?)",
            &["two".into(), vec![2_u8].into()],
        )
        .expect("second row");
        db.write("pragma wal_checkpoint(TRUNCATE)", &[])
            .expect("checkpoint");
        let page_size = db
            .read("pragma page_size", &[])
            .expect("page size")
            .as_slice()[0]
            .get_int(0)
            .expect("page size value");
        let root_page = db
            .read(
                "select rootpage from sqlite_schema where type = 'index' and name = 'records_name'",
                &[],
            )
            .expect("index root page")
            .as_slice()[0]
            .get_int(0)
            .expect("root page value");
        (page_size, root_page)
    };

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&source_path)
        .expect("corruption fixture");
    file.seek(SeekFrom::Start((root_page as u64 - 1) * page_size as u64))
        .expect("index page");
    file.write_all(&[0]).expect("damage index page");
    file.sync_all().expect("sync corruption fixture");
    let damaged_bytes = fs::read(&source_path).expect("read damaged fixture");

    let mut db = open_local(&source_path, AccessMode::ReadWrite);
    let error = db
        .verify_integrity()
        .expect_err("damaged index must fail verification");
    assert_eq!(
        fs::read(&source_path).expect("verification must not repair"),
        damaged_bytes,
        "integrity verification must be read-only"
    );
    assert_eq!(error.storage_failure_kind(), StorageFailureKind::Corrupt);
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Corrupt));
            assert_eq!(error.adapter_code(), Some("corrupt_database"));
        }
        other => panic!("unexpected corruption error: {other}"),
    }

    let recovered = db
        .recover_from_dump_reload(&archive_path)
        .expect("explicit Dactyl recovery");
    assert_eq!(recovered.journal_mode, RecoveryJournalMode::Delete);
    assert_eq!(recovered.user_version, 9);
    assert_eq!(recovered.application_id, 99);
    assert!(archive_path.is_file());

    let health = db.verify_integrity().expect("recovered verification");
    assert_eq!(health.journal_mode.to_ascii_lowercase(), "delete");
    assert_eq!(health.user_version, 9);
    assert_eq!(health.application_id, 99);
    let rows = db
        .read("select name, payload from records order by id", &[])
        .expect("recovered rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.as_slice()[0].get_str("name").expect("name"), "one");
    assert_eq!(
        rows.as_slice()[1].get_blob("payload").expect("payload"),
        &[2]
    );
    assert!(!sidecar(&source_path, "-wal").exists());
    assert!(!sidecar(&source_path, "-shm").exists());
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn recovery_requires_quiesced_connections_and_preserves_existing_archive() {
    let directory = private_recovery_directory();
    let source_path = directory.path().join("source.db");
    let archive_path = directory.path().join("archive.db");
    let mut primary = open_local(&source_path, AccessMode::ReadWrite);
    primary
        .write("create table records (id integer primary key)", &[])
        .expect("schema");
    let sibling = open_local(&source_path, AccessMode::ReadWrite);
    let error = primary
        .recover_from_dump_reload(&archive_path)
        .expect_err("open sibling must block recovery");
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Conflict));
            assert_eq!(error.adapter_code(), Some("open_connections"));
        }
        other => panic!("unexpected open-connection error: {other}"),
    }
    drop(sibling);

    fs::write(&archive_path, b"operator-owned archive").expect("archive sentinel");
    let error = primary
        .recover_from_dump_reload(&archive_path)
        .expect_err("existing archive must not be overwritten");
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Conflict));
            assert_eq!(error.adapter_code(), Some("path_exists"));
        }
        other => panic!("unexpected archive error: {other}"),
    }
    assert_eq!(
        fs::read(&archive_path).expect("archive contents"),
        b"operator-owned archive"
    );
    assert_eq!(
        primary
            .read("select count(*) from records", &[])
            .expect("source remains readable")
            .as_slice()[0]
            .get_int(0)
            .expect("count"),
        0
    );
}

#[test]
fn coordination_contention_is_bounded_and_never_cleans_the_sidecar() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("locked.db");
    let sidecar_path = sidecar(&source_path, ".lock");
    fs::File::create(&source_path).expect("database target");
    let external = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&sidecar_path)
        .expect("sidecar");
    external.lock_exclusive().expect("external lock");

    let error = match DactylBridge::open_local(&source_path, AccessMode::ReadWrite) {
        Ok(_) => panic!("coordination lock should block the bridge"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("STORAGE_LOCK_TIMEOUT"));
    assert_eq!(error.storage_failure_kind(), StorageFailureKind::Contention);
    assert!(sidecar_path.exists());
    external.unlock().expect("unlock sidecar");
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[test]
fn operator_cli_exposes_verify_backup_and_recovery_without_automatic_repair() {
    let tmp = TempDir::new().expect("temporary project");
    let project = tmp.path();
    let executable = env!("CARGO_BIN_EXE_decapod");
    let init = Command::new(executable)
        .args(["init", "--force", "--no-ci"])
        .current_dir(project)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = Command::new(executable)
        .args(["session", "acquire"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("session");
    assert!(
        session.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
        .expect("session password");

    // macOS temporary directories can have a `/var` -> `/private/var`
    // spelling difference. Dactyl requires the archive and active database to
    // have the same physical parent spelling, so use the resolved data path.
    let data_root = fs::canonicalize(project.join(".decapod/data")).expect("data directory");
    let source_path = data_root.join("decapod.db");
    let db = open_local(&source_path, AccessMode::ReadWrite);
    db.write(
        "create table records (id integer primary key, value text)",
        &[],
    )
    .expect("schema");
    db.write("insert into records (value) values ('operator')", &[])
        .expect("row");
    drop(db);

    let run = |args: &[&str]| {
        Command::new(executable)
            .args(args)
            .current_dir(project)
            .env("DECAPOD_AGENT_ID", "maintenance-test")
            .env("DECAPOD_SESSION_PASSWORD", &password)
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .output()
            .expect("maintenance command")
    };

    let verify = run(&["data", "db", "verify"]);
    assert!(
        verify.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let verify_json: serde_json::Value =
        serde_json::from_slice(&verify.stdout).expect("verify JSON");
    assert_eq!(verify_json["status"], "ok");
    assert_eq!(verify_json["diagnostic_status"], "healthy");
    assert_eq!(verify_json["automatic_repair"], false);

    let verify_alias = run(&["data", "database", "verify"]);
    assert!(
        verify_alias.status.success(),
        "database alias failed: {}",
        String::from_utf8_lossy(&verify_alias.stderr)
    );

    let backup_path = project.join("backup.db");
    let backup = run(&[
        "data",
        "db",
        "backup",
        "--destination",
        backup_path.to_str().expect("backup path"),
    ]);
    assert!(
        backup.status.success(),
        "backup failed: {}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup_json: serde_json::Value =
        serde_json::from_slice(&backup.stdout).expect("backup JSON");
    assert_eq!(backup_json["operation"], "backup");
    assert_eq!(backup_json["diagnostic_status"], "healthy");
    assert!(backup_path.is_file());

    let archive_path = data_root.join("recovery-original.db");
    let recovery = run(&[
        "data",
        "db",
        "recover",
        "--preserve-original-at",
        archive_path.to_str().expect("archive path"),
    ]);
    if !cfg!(any(target_os = "linux", target_os = "android")) {
        assert!(!recovery.status.success());
        let json: serde_json::Value =
            serde_json::from_slice(&recovery.stdout).expect("refusal JSON");
        assert_eq!(json["diagnostic_status"], "unsupported");
        assert_eq!(json["failure_code"], "secure_recovery_unsupported");
        assert!(!archive_path.exists());
        let db = open_local(&source_path, AccessMode::ReadOnly);
        assert_eq!(
            db.read("select value from records", &[])
                .unwrap()
                .as_slice()[0]
                .get_str(0)
                .unwrap(),
            "operator"
        );
        return;
    }
    assert!(
        recovery.status.success(),
        "recovery failed: {}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    let recovery_json: serde_json::Value =
        serde_json::from_slice(&recovery.stdout).expect("recovery JSON");
    assert_eq!(recovery_json["operation"], "recovery");
    assert_eq!(recovery_json["diagnostic_status"], "healthy");
    assert_eq!(recovery_json["recovery"]["journal_mode"], "delete");
    assert!(archive_path.is_file());
    assert!(!sidecar(&source_path, "-wal").exists());
    assert!(!sidecar(&source_path, "-shm").exists());
}

#[test]
fn operator_cli_reports_malformed_database_without_repairing_it() {
    let tmp = TempDir::new().expect("temporary project");
    let project = tmp.path();
    let executable = env!("CARGO_BIN_EXE_decapod");
    let init = Command::new(executable)
        .args(["init", "--force", "--no-ci"])
        .current_dir(project)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = Command::new(executable)
        .args(["session", "acquire"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("session");
    assert!(
        session.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
        .expect("session password");

    let data_root = fs::canonicalize(project.join(".decapod/data")).expect("data directory");
    let source_path = data_root.join("decapod.db");
    let malformed = b"not a SQLite database";
    fs::write(&source_path, malformed).expect("malformed database");

    let verify = Command::new(executable)
        .args(["data", "db", "verify"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("verify");
    assert!(!verify.status.success(), "malformed database must fail");
    let json: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("diagnostic JSON");
    assert_eq!(json["status"], "corrupt");
    assert_eq!(json["diagnostic_status"], "corrupt");
    assert_eq!(json["failure_code"], "malformed_database");
    assert_eq!(json["automatic_repair"], false);
    assert_eq!(json["operator_action_required"], true);
    assert_eq!(
        fs::read(&source_path).expect("database remains unchanged"),
        malformed
    );

    let validation = Command::new(executable)
        .args(["validate", "--format", "json"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("ordinary validation");
    assert!(
        !validation.status.success(),
        "ordinary validation must surface malformed storage rather than repair it"
    );
    assert_eq!(
        fs::read(&source_path).expect("validation must not repair the database"),
        malformed
    );
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
#[test]
fn unsupported_platform_recovery_is_typed_and_preserves_the_original() {
    let directory = private_recovery_directory();
    let source = directory.path().join("source.db");
    let archive = directory.path().join("archive.db");
    let mut db = open_local(&source, AccessMode::ReadWrite);
    db.write("create table keep_original (id integer)", &[])
        .unwrap();
    let before = fs::read(&source).unwrap();
    match db.recover_from_dump_reload(&archive).unwrap_err() {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Capability));
            assert_eq!(error.adapter_code(), Some("secure_recovery_unsupported"));
        }
        other => panic!("unexpected refusal: {other}"),
    }
    assert_eq!(fs::read(&source).unwrap(), before);
    assert!(!archive.exists());
}

struct GovernedRepoFixture {
    _temp: TempDir,
    repo_root: PathBuf,
    xdg_config: PathBuf,
    home_dir: PathBuf,
}

struct ClaimedAgentWorktree {
    agent_id: String,
    password: String,
    worktree_dir: PathBuf,
}

impl GovernedRepoFixture {
    fn new() -> Self {
        let temp = tempdir().expect("governed repo fixture");
        let repo_root = temp.path().join("repo");
        let xdg_config = temp.path().join("xdg");
        let home_dir = temp.path().join("home");
        fs::create_dir_all(&repo_root).unwrap();
        fs::create_dir_all(&xdg_config).unwrap();
        fs::create_dir_all(&home_dir).unwrap();

        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo_root)
                .output()
                .expect("git command");
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "master"]);
        git(&["config", "user.email", "test@decapod.local"]);
        git(&["config", "user.name", "Decapod Test"]);

        let init = Self::isolated_command()
            .args(["init", "--force", "--dir"])
            .arg(&repo_root)
            .env("XDG_CONFIG_HOME", &xdg_config)
            .env("HOME", &home_dir)
            .output()
            .expect("decapod init");
        assert!(
            init.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        git(&["add", "-A"]);
        git(&["commit", "-m", "chore: initial decapod bootstrap"]);

        Self {
            _temp: temp,
            repo_root,
            xdg_config,
            home_dir,
        }
    }

    // These children own disposable repositories. Provider context describes
    // the outer source job and must never impersonate a child checkout.
    fn isolated_command() -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
        for key in [
            "GITHUB_ACTIONS",
            "BUILDKITE",
            "GITHUB_EVENT_NAME",
            "GITHUB_WORKSPACE",
            "BUILDKITE_BUILD_CHECKOUT_PATH",
            "CHANGE_EVENT",
            "CHANGE_BASE",
            "CHANGE_HEAD",
            "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT",
            "DECAPOD_VALIDATE_SKIP_GIT_GATES",
            "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
            "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
            "DECAPOD_SESSION_PASSWORD",
            "DECAPOD_USER_ID",
            "DECAPOD_ROLE",
        ] {
            command.env_remove(key);
        }
        command
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1");
        command
    }

    fn decapod_cmd(&self, cwd: &Path, agent_id: &str, password: Option<&str>) -> Command {
        let mut cmd = Self::isolated_command();
        cmd.current_dir(cwd)
            .env("XDG_CONFIG_HOME", &self.xdg_config)
            .env("HOME", &self.home_dir)
            .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
            .env_remove("DECAPOD_SESSION_PASSWORD")
            .env_remove("DECAPOD_USER_ID")
            .env_remove("DECAPOD_ROLE");
        if !agent_id.is_empty() {
            cmd.env("DECAPOD_AGENT_ID", agent_id);
        } else {
            cmd.env_remove("DECAPOD_AGENT_ID");
        }
        if let Some(pw) = password {
            cmd.env("DECAPOD_SESSION_PASSWORD", pw);
        }
        cmd
    }

    fn spawn_claimed_agent(&self, agent_id: &str, topic: &str) -> ClaimedAgentWorktree {
        let session = self
            .decapod_cmd(&self.repo_root, agent_id, None)
            .args(["session", "acquire"])
            .output()
            .expect("session acquire");
        assert!(
            session.status.success(),
            "session acquire failed: {}",
            String::from_utf8_lossy(&session.stderr)
        );
        let password = String::from_utf8_lossy(&session.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
            .expect("session password");

        let add = self
            .decapod_cmd(&self.repo_root, agent_id, Some(&password))
            .args(["todo", "add", &format!("Task for {agent_id}")])
            .output()
            .expect("todo add");
        assert!(
            add.status.success(),
            "todo add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        );
        let add_json: serde_json::Value =
            serde_json::from_slice(&add.stdout).expect("todo add json");
        let task_id = add_json["id"].as_str().expect("task id").to_string();

        let claim = self
            .decapod_cmd(&self.repo_root, agent_id, Some(&password))
            .args(["todo", "claim", "--id", &task_id])
            .output()
            .expect("todo claim");
        assert!(
            claim.status.success(),
            "todo claim failed: {}",
            String::from_utf8_lossy(&claim.stderr)
        );

        let branch = format!("agent/{agent_id}/{task_id}-{topic}");
        let ensure = self
            .decapod_cmd(&self.repo_root, agent_id, Some(&password))
            .args(["workspace", "ensure", "--branch", &branch])
            .output()
            .expect("workspace ensure");
        assert!(
            ensure.status.success(),
            "workspace ensure failed: {}{}",
            String::from_utf8_lossy(&ensure.stdout),
            String::from_utf8_lossy(&ensure.stderr)
        );
        let ensure_json: serde_json::Value =
            serde_json::from_slice(&ensure.stdout).expect("workspace ensure json");
        let worktree_dir = PathBuf::from(
            ensure_json["worktree_path"]
                .as_str()
                .or_else(|| ensure_json["path"].as_str())
                .expect("worktree path"),
        );

        // Also acquire session scoped to the isolated worktree so worktree invocations verify cleanly.
        let wt_session = self
            .decapod_cmd(&worktree_dir, agent_id, None)
            .args(["session", "acquire"])
            .output()
            .expect("worktree session acquire");
        assert!(wt_session.status.success());
        let wt_password = String::from_utf8_lossy(&wt_session.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
            .expect("worktree session password");

        ClaimedAgentWorktree {
            agent_id: agent_id.to_string(),
            password: wt_password,
            worktree_dir,
        }
    }

    fn set_canonical_db_maintenance(&self, section: Option<&str>) {
        let cfg_path = self.repo_root.join(".decapod/config.toml");
        let current = fs::read_to_string(&cfg_path).expect("read canonical config");
        let base = current
            .split("\n[governance.db_maintenance]")
            .next()
            .unwrap()
            .to_string();
        let updated = match section {
            Some(sec) => format!(
                "{}\n\n[governance.db_maintenance]\n{}\n",
                base.trim_end(),
                sec.trim()
            ),
            None => format!("{}\n", base.trim_end()),
        };
        fs::write(&cfg_path, updated).expect("write canonical config");
    }
}

#[test]
fn governed_db_maintenance_enforces_default_deny_opt_in_anti_self_elevation_and_owner_boundary() {
    let fixture = GovernedRepoFixture::new();
    let alpha = fixture.spawn_claimed_agent("agent-alpha", "db-maint");

    // 1. Default-deny in isolated git worktree on both CLI and RPC, while legacy read-only
    //    `data database inspect` continues working without opt-in.
    let legacy_inspect = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "database", "inspect", "--format", "json"])
        .output()
        .expect("legacy inspect");
    assert!(legacy_inspect.status.success());

    let denied_verify = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "maintain", "verify"])
        .output()
        .expect("denied verify");
    assert!(!denied_verify.status.success());
    let stderr = String::from_utf8_lossy(&denied_verify.stderr);
    assert!(
        stderr.contains("POLICY_OPT_IN_DISABLED"),
        "expected POLICY_OPT_IN_DISABLED, got: {stderr}"
    );

    let denied_rpc = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["rpc", "--op", "data.database.verify"])
        .output()
        .expect("denied rpc verify");
    assert!(!denied_rpc.status.success());
    let rpc_json: serde_json::Value =
        serde_json::from_slice(&denied_rpc.stdout).expect("rpc error json");
    assert_eq!(rpc_json["success"], false);
    assert_eq!(rpc_json["error"]["code"], "POLICY_OPT_IN_DISABLED");

    // 2. Anti-self-elevation: agent edits `.decapod/config.toml` inside its own worktree to
    //    `enabled = true` — canonical main-repo policy MUST still deny!
    let wt_cfg_path = alpha.worktree_dir.join(".decapod/config.toml");
    let wt_cfg_orig = fs::read_to_string(&wt_cfg_path).unwrap();
    fs::write(
        &wt_cfg_path,
        format!("{wt_cfg_orig}\n[governance.db_maintenance]\nenabled = true\n"),
    )
    .unwrap();
    let self_elevate = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "maintain", "verify"])
        .output()
        .expect("worktree self-elevation attempt");
    assert!(!self_elevate.status.success());
    assert!(
        String::from_utf8_lossy(&self_elevate.stderr).contains("POLICY_OPT_IN_DISABLED"),
        "worktree config edit must never self-elevate"
    );
    fs::write(&wt_cfg_path, &wt_cfg_orig).unwrap();

    // 3. Unmerged branch on main_root cannot self-elevate either (`UNMERGED_BRANCH_POLICY_UNTRUSTED`).
    let checkout_unmerged = Command::new("git")
        .args(["checkout", "-b", "unreviewed-policy-branch"])
        .current_dir(&fixture.repo_root)
        .output()
        .unwrap();
    assert!(checkout_unmerged.status.success());
    fixture.set_canonical_db_maintenance(Some("enabled = true"));
    let unmerged_attempt = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "maintain", "verify"])
        .output()
        .unwrap();
    assert!(!unmerged_attempt.status.success());
    assert!(
        String::from_utf8_lossy(&unmerged_attempt.stderr)
            .contains("UNMERGED_BRANCH_POLICY_UNTRUSTED"),
        "unmerged branch on main repo must not grant maintenance authority"
    );
    fixture.set_canonical_db_maintenance(None);
    assert!(
        Command::new("git")
            .args(["checkout", "master"])
            .current_dir(&fixture.repo_root)
            .output()
            .unwrap()
            .status
            .success()
    );

    // 4. Enable canonical `[governance.db_maintenance] enabled = true` on `master`.
    fixture.set_canonical_db_maintenance(Some("enabled = true"));

    let caps_out = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "capabilities",
            "--probe",
            "db.sql",
            "--probe",
            "db.vacuum",
        ])
        .output()
        .expect("capabilities");
    assert!(
        caps_out.status.success(),
        "capabilities failed: {}",
        String::from_utf8_lossy(&caps_out.stderr)
    );
    let caps_json: serde_json::Value = serde_json::from_slice(&caps_out.stdout).unwrap();
    let policy_sha = caps_json["policy"]["canonical_policy_sha256"]
        .as_str()
        .expect("policy sha256")
        .to_string();
    assert_eq!(caps_json["implies_validation_ready"], false);
    let cap_entries = caps_json["capabilities"].as_array().unwrap();
    let find_cap = |name: &str| {
        cap_entries
            .iter()
            .find(|c| c["capability"] == name)
            .cloned()
            .expect(name)
    };
    assert_eq!(find_cap("db.verify")["decision"], "allowed");
    assert_eq!(find_cap("db.backup")["decision"], "allowed");
    assert_eq!(find_cap("db.recover")["decision"], "denied");
    assert_eq!(
        find_cap("db.recover")["reason_code"],
        "OWNER_REPAIR_AUTHORITY_REQUIRED"
    );
    assert_eq!(find_cap("db.sql")["decision"], "unsupported");
    assert_eq!(find_cap("db.sql")["reason_code"], "CAPABILITY_UNSUPPORTED");

    // 5. Authorized `db.verify` (CLI + RPC parity) and `db.backup` inside `.decapod/backups/`.
    let ok_verify = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "verify",
            "--expected-policy-sha256",
            &policy_sha,
        ])
        .output()
        .unwrap();
    assert!(
        ok_verify.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&ok_verify.stderr)
    );
    let verify_json: serde_json::Value = serde_json::from_slice(&ok_verify.stdout).unwrap();
    assert_eq!(verify_json["status"], "ok");
    assert_eq!(verify_json["canonical_policy_sha256"], policy_sha);
    assert_eq!(verify_json["implies_validation_ready"], false);

    let rpc_verify = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "rpc",
            "--op",
            "data.database.verify",
            "--params",
            &serde_json::json!({ "expected_policy_sha256": policy_sha }).to_string(),
        ])
        .output()
        .unwrap();
    assert!(rpc_verify.status.success());
    let rpc_verify_json: serde_json::Value = serde_json::from_slice(&rpc_verify.stdout).unwrap();
    assert_eq!(rpc_verify_json["success"], true);
    assert_eq!(rpc_verify_json["result"]["status"], "ok");

    // `db.backup` rejects paths outside `.decapod/backups/`, succeeds inside `.decapod/backups/`,
    // rejects `operation_id` replay, and does NOT unlock `db.recover`.
    let outside_backup = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            "../outside.db",
        ])
        .output()
        .unwrap();
    assert!(!outside_backup.status.success());
    assert!(
        String::from_utf8_lossy(&outside_backup.stderr)
            .contains("BACKUP_DESTINATION_OUTSIDE_BOUNDARY")
    );

    let valid_backup_rel = ".decapod/backups/snap-01.db";
    let ok_backup = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            valid_backup_rel,
            "--operation-id",
            "backup-op-001",
        ])
        .output()
        .unwrap();
    assert!(
        ok_backup.status.success(),
        "backup failed: {}",
        String::from_utf8_lossy(&ok_backup.stderr)
    );
    let backup_json: serde_json::Value = serde_json::from_slice(&ok_backup.stdout).unwrap();
    assert_eq!(backup_json["grants_recovery_approval"], false);
    assert!(alpha.worktree_dir.join(valid_backup_rel).is_file());

    let replay_backup = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-02.db",
            "--operation-id",
            "backup-op-001",
        ])
        .output()
        .unwrap();
    assert!(!replay_backup.status.success());
    assert!(String::from_utf8_lossy(&replay_backup.stderr).contains("OPERATION_REPLAY_REJECTED"));

    // Even if `allowed = ["db.recover", "db.verify"]` is written to canonical config, `db.recover`
    // remains denied (`OWNER_REPAIR_AUTHORITY_REQUIRED`).
    fixture.set_canonical_db_maintenance(Some(
        "enabled = true\nallowed = [\"db.verify\", \"db.recover\"]",
    ));
    let denied_recover = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "recover",
            "--preserve-original-at",
            ".decapod/backups/orig.db",
        ])
        .output()
        .unwrap();
    assert!(!denied_recover.status.success());
    assert!(
        String::from_utf8_lossy(&denied_recover.stderr).contains("OWNER_REPAIR_AUTHORITY_REQUIRED")
    );

    // Stale expected_policy_sha256 is rejected (`POLICY_SNAPSHOT_STALE`).
    let stale_sha = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "verify",
            "--expected-policy-sha256",
            &policy_sha,
        ])
        .output()
        .unwrap();
    assert!(!stale_sha.status.success());
    assert!(String::from_utf8_lossy(&stale_sha.stderr).contains("POLICY_SNAPSHOT_STALE"));

    // Forged actor / wrong password / secret redaction in audit log.
    let forged_actor = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "maintain",
            "verify",
            "--actor",
            "forged-owner-Bearer-supersecret123456",
        ])
        .output()
        .unwrap();
    assert!(!forged_actor.status.success());
    assert!(String::from_utf8_lossy(&forged_actor.stderr).contains("AGENT_IDENTITY_FORGED"));

    let wrong_pw = fixture
        .decapod_cmd(
            &alpha.worktree_dir,
            &alpha.agent_id,
            Some("wrong-password-value"),
        )
        .args(["data", "db", "maintain", "verify"])
        .output()
        .unwrap();
    assert!(!wrong_pw.status.success());
    assert!(String::from_utf8_lossy(&wrong_pw.stderr).contains("AGENT_IDENTITY_FORGED"));

    // Reverting canonical config to `enabled = false` immediately revokes access.
    fixture.set_canonical_db_maintenance(Some("enabled = false"));
    let revoked = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "maintain", "verify"])
        .output()
        .unwrap();
    assert!(!revoked.status.success());
    assert!(String::from_utf8_lossy(&revoked.stderr).contains("POLICY_OPT_IN_DISABLED"));

    // Audit records remain readable, redacted, and never imply validation readiness.
    let audit = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "audit", "--limit", "50"])
        .output()
        .unwrap();
    assert!(audit.status.success());
    let audit_str = String::from_utf8_lossy(&audit.stdout);
    assert!(!audit_str.contains(&alpha.password));
    assert!(
        !audit_str.contains(fixture.repo_root.to_string_lossy().as_ref()),
        "audit log must redact absolute host repo paths"
    );
    let audit_json: serde_json::Value = serde_json::from_slice(&audit.stdout).unwrap();
    assert!(audit_json["count"].as_u64().unwrap() >= 5);
    assert_eq!(audit_json["implies_validation_ready"], false);

    // Cloud backend fails closed even with `enabled = true` and spoofed env vars.
    let main_cfg_path = fixture.repo_root.join(".decapod/config.toml");
    let cloud_cfg = fs::read_to_string(&main_cfg_path)
        .unwrap()
        .replace("backend = \"local\"", "backend = \"cloud\"");
    fs::write(&main_cfg_path, &cloud_cfg).unwrap();
    fs::write(
        &wt_cfg_path,
        fs::read_to_string(&wt_cfg_path)
            .unwrap()
            .replace("backend = \"local\"", "backend = \"cloud\""),
    )
    .unwrap();
    fixture.set_canonical_db_maintenance(Some("enabled = true"));

    let cloud_denied = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .env("DECAPOD_USER_ID", "owner-user-123")
        .env("DECAPOD_ROLE", "owner")
        .args(["data", "db", "maintain", "verify"])
        .output()
        .unwrap();
    assert!(!cloud_denied.status.success());
    assert!(
        String::from_utf8_lossy(&cloud_denied.stderr).contains("CLOUD_PRINCIPAL_UNVERIFIED"),
        "cloud backend must fail closed until Propodus #88 principal delegation"
    );
}

#[test]
fn two_concurrent_agents_diagnose_and_resolve_lock_contention_with_custody_isolation() {
    let fixture = GovernedRepoFixture::new();
    let alpha = fixture.spawn_claimed_agent("agent-alpha", "lock-alpha");
    let beta = fixture.spawn_claimed_agent("agent-beta", "lock-beta");
    fixture.set_canonical_db_maintenance(Some("enabled = true"));

    // Custody isolation: agent-alpha cannot invoke governed maintenance from agent-beta's worktree!
    let cross_worktree = fixture
        .decapod_cmd(&beta.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "lock", "diagnose"])
        .output()
        .unwrap();
    assert!(!cross_worktree.status.success());
    let cross_err = String::from_utf8_lossy(&cross_worktree.stderr);
    assert!(
        cross_err.contains("WORKTREE_CUSTODY_MISMATCH")
            || cross_err.contains("AGENT_IDENTITY_UNAUTHENTICATED")
            || cross_err.contains("AGENT_IDENTITY_FORGED"),
        "unexpected cross-worktree error: {cross_err}"
    );

    let lock_path = fixture.repo_root.join(".decapod/data/decapod.db.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();

    // Hold an external exclusive advisory lock simulating transient contention from another process.
    lock_file.lock_exclusive().unwrap();

    // Agent alpha non-destructively diagnoses exclusive contention (`owner = unknown`) without deleting `.lock`.
    let diag_out = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args(["data", "db", "lock", "diagnose", "--timeout-ms", "40"])
        .output()
        .unwrap();
    assert!(
        diag_out.status.success(),
        "lock diagnose failed: {}",
        String::from_utf8_lossy(&diag_out.stderr)
    );
    let diag_json: serde_json::Value = serde_json::from_slice(&diag_out.stdout).unwrap();
    assert_eq!(
        diag_json["diagnosis"]["lock_state"],
        "exclusive_maintenance_active"
    );
    assert_eq!(diag_json["diagnosis"]["owner"], "unknown");
    assert_eq!(diag_json["diagnosis"]["timed_out"], true);
    assert!(lock_path.exists(), "diagnose must never delete .lock");

    // Bounded retry fails closed (`RETRY_DEADLINE_EXCEEDED`) when contention outlasts deadline.
    let timeout_retry = fixture
        .decapod_cmd(&beta.worktree_dir, &beta.agent_id, Some(&beta.password))
        .args([
            "rpc",
            "--op",
            "data.database.lock.retry",
            "--params",
            r#"{"target":"verify","max_attempts":2,"initial_backoff_ms":20,"deadline_ms":60}"#,
        ])
        .output()
        .unwrap();
    assert!(!timeout_retry.status.success());
    let timeout_json: serde_json::Value = serde_json::from_slice(&timeout_retry.stdout).unwrap();
    assert_eq!(timeout_json["success"], false);
    assert_eq!(timeout_json["error"]["code"], "RETRY_DEADLINE_EXCEEDED");

    // Release the transient exclusive lock after 120ms on a background thread while BOTH
    // agent-alpha and agent-beta run bounded `db.lock.retry` from their own isolated worktrees!
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let _ = FileExt::unlock(&lock_file);
    });

    let alpha_child = fixture
        .decapod_cmd(&alpha.worktree_dir, &alpha.agent_id, Some(&alpha.password))
        .args([
            "data",
            "db",
            "lock",
            "retry",
            "--target",
            "verify",
            "--max-attempts",
            "8",
            "--initial-backoff-ms",
            "40",
            "--deadline-ms",
            "3000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let beta_child = fixture
        .decapod_cmd(&beta.worktree_dir, &beta.agent_id, Some(&beta.password))
        .args([
            "rpc",
            "--op",
            "data.database.lock.retry",
            "--params",
            r#"{"target":"verify","max_attempts":8,"initial_backoff_ms":40,"deadline_ms":3000}"#,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let alpha_res = alpha_child.wait_with_output().unwrap();
    let beta_res = beta_child.wait_with_output().unwrap();
    releaser.join().unwrap();

    assert!(
        alpha_res.status.success(),
        "alpha retry failed: {}",
        String::from_utf8_lossy(&alpha_res.stderr)
    );
    assert!(
        beta_res.status.success(),
        "beta retry failed: {}{}",
        String::from_utf8_lossy(&beta_res.stdout),
        String::from_utf8_lossy(&beta_res.stderr)
    );

    let alpha_json: serde_json::Value = serde_json::from_slice(&alpha_res.stdout).unwrap();
    assert_eq!(alpha_json["status"], "ok");
    assert_eq!(alpha_json["target_result"]["integrity_check"], "ok");

    let beta_json: serde_json::Value = serde_json::from_slice(&beta_res.stdout).unwrap();
    assert_eq!(beta_json["success"], true);
    assert_eq!(
        beta_json["result"]["target_result"]["integrity_check"],
        "ok"
    );
}
