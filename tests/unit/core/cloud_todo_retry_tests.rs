use super::*;
use crate::core::cloud_todo_operation::FailureKind;
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct RetryStore {
    tasks: Arc<Mutex<BTreeMap<String, StorageTask>>>,
    lose_ack: Arc<AtomicBool>,
    fail_reconciliation: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl TodoStore for RetryStore {
    async fn list_tasks(&self) -> anyhow::Result<Vec<StorageTask>> {
        Ok(self.tasks.lock().unwrap().values().cloned().collect())
    }
    async fn add_task(
        &self,
        task: StorageTask,
        _actor: String,
        intent: String,
    ) -> anyhow::Result<StorageTask> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(intent.starts_with(cloud_todo_operation::INTENT_PREFIX));
        if self.fail_reconciliation.swap(false, Ordering::SeqCst) {
            return Err(cloud_todo_operation::AdapterFailure {
                status: Outcome::BlockedBeforeSubmit,
                kind: FailureKind::Unavailable,
            }
            .into());
        }
        let task = self
            .tasks
            .lock()
            .unwrap()
            .entry(task.id.clone())
            .or_insert(task)
            .clone();
        if self.lose_ack.swap(false, Ordering::SeqCst) {
            return Err(DecapodError::from(dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Transport,
                code: None,
                message: "secret-bearer private-backend".into(),
            })
            .into());
        }
        Ok(task)
    }
    async fn claim_task(&self, _id: &str, _actor: String) -> anyhow::Result<StorageTask> {
        unreachable!()
    }
    async fn complete_task(
        &self,
        _id: &str,
        _actor: String,
        _resolution: String,
    ) -> anyhow::Result<StorageTask> {
        unreachable!()
    }
}
struct RetryFactory {
    store: RetryStore,
    blocked: Mutex<Option<CloudAuthStatus>>,
    builds: AtomicUsize,
}
impl RetryFactory {
    fn new() -> Self {
        Self {
            store: RetryStore::default(),
            blocked: Mutex::new(None),
            builds: AtomicUsize::new(0),
        }
    }
}
impl CloudTodoStoreFactory for RetryFactory {
    fn build(
        &self,
        _config: &CloudRuntimeConfig,
        _identity: &RepositoryIdentity,
    ) -> Result<Box<dyn TodoStore>, DecapodError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        if let Some(status) = *self.blocked.lock().unwrap() {
            return Err(DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                status,
                "secret-bearer private-backend",
                "secret-url",
            )));
        }
        Ok(Box::new(self.store.clone()))
    }
}
fn add() -> TodoCommand {
    TodoCli::try_parse_from([
        "todo",
        "add",
        "private-title",
        "--description",
        "private-payload",
    ])
    .unwrap()
    .command
}
fn identity() -> RepositoryIdentity {
    crate::core::repo_identity::resolve_repository_identity_from_remote(
        "https://github.com/example/project.git",
    )
    .unwrap()
}
fn diagnostic(error: DecapodError) -> CloudTodoDiagnostic {
    let DecapodError::CloudTodo(diagnostic) = error else {
        panic!("expected structured cloud outcome, got {error}");
    };
    diagnostic
}

#[test]
fn cloud_auth_blocked_add_resumes_after_onboarding_without_touching_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("protected-root");
    std::fs::create_dir(&root).unwrap();
    let journal = CloudTodoJournal::at(temp.path().join("machine"));
    let factory = RetryFactory::new();
    *factory.blocked.lock().unwrap() = Some(CloudAuthStatus::OnboardingPending);
    let blocked = diagnostic(
        run_cloud_todo_command_with_factory(
            &root,
            &add(),
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            None,
            Some(&journal),
        )
        .unwrap_err(),
    );
    assert_eq!(blocked.status, Outcome::BlockedBeforeSubmit);
    assert_eq!(blocked.failure_kind, FailureKind::OnboardingPending);
    assert_eq!(factory.store.calls.load(Ordering::SeqCst), 0);
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    let json = serde_json::to_string(&blocked).unwrap();
    for secret in [
        "secret-bearer",
        "private-backend",
        "private-title",
        "private-payload",
        "secret-url",
    ] {
        assert!(!json.contains(secret));
    }
    *factory.blocked.lock().unwrap() = None;
    let result = run_cloud_todo_command_with_factory(
        &root,
        &add(),
        &CloudRuntimeConfig::default(),
        &identity(),
        &factory,
        Some(&blocked.operation_id),
        Some(&journal),
    )
    .unwrap();
    let retry = run_cloud_todo_command_with_factory(
        &root,
        &add(),
        &CloudRuntimeConfig::default(),
        &identity(),
        &factory,
        Some(&blocked.operation_id),
        Some(&journal),
    )
    .unwrap();
    assert_eq!(result["id"], retry["id"]);
    assert_eq!(factory.store.tasks.lock().unwrap().len(), 1);
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    assert_eq!(journal.list().unwrap()[0].status, Outcome::Succeeded);
}

#[test]
fn unknown_commit_reuses_identity_while_a_fresh_identical_add_is_new_work() {
    let temp = tempfile::tempdir().unwrap();
    let journal = CloudTodoJournal::at(temp.path().join("machine"));
    let factory = RetryFactory::new();
    factory.store.lose_ack.store(true, Ordering::SeqCst);
    let failure = diagnostic(
        run_cloud_todo_command_with_factory(
            temp.path(),
            &add(),
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            None,
            Some(&journal),
        )
        .unwrap_err(),
    );
    assert_eq!(failure.status, Outcome::OutcomeUnknown);
    assert_eq!(failure.failure_kind, FailureKind::Transport);
    assert_eq!(journal.list().unwrap()[0].status, Outcome::OutcomeUnknown);
    assert_eq!(factory.store.tasks.lock().unwrap().len(), 1);
    *factory.blocked.lock().unwrap() = Some(CloudAuthStatus::Missing);
    let blocked_retry = diagnostic(
        run_cloud_todo_command_with_factory(
            temp.path(),
            &add(),
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            Some(&failure.operation_id),
            Some(&journal),
        )
        .unwrap_err(),
    );
    assert_eq!(blocked_retry.status, Outcome::OutcomeUnknown);
    assert_eq!(blocked_retry.attempt_status, Outcome::BlockedBeforeSubmit);
    assert_eq!(journal.list().unwrap()[0].status, Outcome::OutcomeUnknown);
    assert_eq!(factory.store.calls.load(Ordering::SeqCst), 1);
    *factory.blocked.lock().unwrap() = None;
    factory
        .store
        .fail_reconciliation
        .store(true, Ordering::SeqCst);
    let unreadable_retry = diagnostic(
        run_cloud_todo_command_with_factory(
            temp.path(),
            &add(),
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            Some(&failure.operation_id),
            Some(&journal),
        )
        .unwrap_err(),
    );
    assert_eq!(unreadable_retry.status, Outcome::OutcomeUnknown);
    assert_eq!(
        unreadable_retry.attempt_status,
        Outcome::BlockedBeforeSubmit
    );
    assert_eq!(unreadable_retry.failure_kind, FailureKind::Unavailable);
    assert_eq!(journal.list().unwrap()[0].status, Outcome::OutcomeUnknown);
    assert_eq!(factory.store.tasks.lock().unwrap().len(), 1);
    let recovered = run_cloud_todo_command_with_factory(
        temp.path(),
        &add(),
        &CloudRuntimeConfig::default(),
        &identity(),
        &factory,
        Some(&failure.operation_id),
        Some(&journal),
    )
    .unwrap();
    let fresh = run_cloud_todo_command_with_factory(
        temp.path(),
        &add(),
        &CloudRuntimeConfig::default(),
        &identity(),
        &factory,
        None,
        Some(&journal),
    )
    .unwrap();
    assert_ne!(recovered["id"], fresh["id"]);
    assert_eq!(factory.store.tasks.lock().unwrap().len(), 2);
}

#[test]
fn changed_payload_is_rejected_before_auth_and_cached_success_is_not_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let journal = CloudTodoJournal::at(temp.path().join("machine"));
    let factory = RetryFactory::new();
    let result = run_cloud_todo_command_with_factory(
        temp.path(),
        &add(),
        &CloudRuntimeConfig::default(),
        &identity(),
        &factory,
        None,
        Some(&journal),
    )
    .unwrap();
    let token = result["operation_id"].as_str().unwrap();
    let changed = TodoCli::try_parse_from(["todo", "add", "changed"])
        .unwrap()
        .command;
    assert!(
        run_cloud_todo_command_with_factory(
            temp.path(),
            &changed,
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            Some(token),
            Some(&journal)
        )
        .is_err()
    );
    assert_eq!(factory.builds.load(Ordering::SeqCst), 1);
    *factory.blocked.lock().unwrap() = Some(CloudAuthStatus::RepositoryDenied);
    let denied = diagnostic(
        run_cloud_todo_command_with_factory(
            temp.path(),
            &add(),
            &CloudRuntimeConfig::default(),
            &identity(),
            &factory,
            Some(token),
            Some(&journal),
        )
        .unwrap_err(),
    );
    assert_eq!(denied.failure_kind, FailureKind::Authorization);
    assert!(!denied.retryable);
    assert_eq!(factory.store.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn missing_offline_and_unauthorized_preflight_remain_distinct_and_never_submit() {
    for (status, expected) in [
        (CloudAuthStatus::Missing, FailureKind::Authentication),
        (CloudAuthStatus::Offline, FailureKind::Offline),
        (CloudAuthStatus::Unauthorized, FailureKind::Authorization),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let journal = CloudTodoJournal::at(temp.path().join("machine"));
        let factory = RetryFactory::new();
        *factory.blocked.lock().unwrap() = Some(status);
        let blocked = diagnostic(
            run_cloud_todo_command_with_factory(
                temp.path(),
                &add(),
                &CloudRuntimeConfig::default(),
                &identity(),
                &factory,
                None,
                Some(&journal),
            )
            .unwrap_err(),
        );
        assert_eq!(blocked.failure_kind, expected);
        assert_eq!(blocked.status, Outcome::BlockedBeforeSubmit);
        assert_eq!(factory.store.calls.load(Ordering::SeqCst), 0);
        assert!(!temp.path().join(".decapod").exists());
    }
}
