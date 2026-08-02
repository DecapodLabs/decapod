// Moved from src/decapod/cli.rs
use super::{BackendType, RepoContext};

#[test]
fn backend_field_selects_the_repository_backend() {
    let mut context = RepoContext {
        backend: Some(BackendType::Cloud),
        ..RepoContext::default()
    };
    assert_eq!(context.effective_backend(), BackendType::Cloud);

    context.backend = Some(BackendType::Local);
    assert_eq!(context.effective_backend(), BackendType::Local);
}

#[test]
fn setting_backend_selects_the_canonical_config_field() {
    let mut context = RepoContext::default();
    context.set_backend(BackendType::Cloud);
    assert_eq!(context.backend, Some(BackendType::Cloud));
    assert_eq!(context.effective_backend(), BackendType::Cloud);
}
