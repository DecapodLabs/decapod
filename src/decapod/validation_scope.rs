//! Public validation dispatch only. Environment flags may select a partial
//! CI-local result; they never establish provider identity or publication proof.
//! The publisher calls the full readiness boundary directly, without this selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationScope {
    PublicationReadiness,
    ProjectPrerequisites,
    CiLocal,
}

impl ValidationScope {
    pub(crate) fn public_validate() -> Self {
        Self::from_provider_flags(
            std::env::var("GITHUB_ACTIONS").ok().as_deref(),
            std::env::var("BUILDKITE").ok().as_deref(),
        )
    }

    fn from_provider_flags(github: Option<&str>, buildkite: Option<&str>) -> Self {
        if github == Some("true") || buildkite == Some("true") {
            Self::CiLocal
        } else {
            Self::PublicationReadiness
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::PublicationReadiness => "publication_readiness",
            Self::ProjectPrerequisites => "project_prerequisites",
            Self::CiLocal => "ci_local",
        }
    }

    pub(crate) fn requires_publication_proof(self) -> bool {
        self == Self::PublicationReadiness
    }

    /// Pick the execution owner before invoking either path. Provider flags only
    /// select partial CI scope; they cannot mint full publication readiness.
    pub(crate) fn dispatch<T, R>(
        self,
        buildkite_enabled: bool,
        input: T,
        local: impl FnOnce(T) -> R,
        remote: impl FnOnce(T) -> R,
    ) -> R {
        if self.requires_publication_proof() && buildkite_enabled {
            remote(input)
        } else {
            local(input)
        }
    }

    pub(crate) fn records_receipt(self) -> bool {
        self == Self::ProjectPrerequisites
    }
}

#[cfg(test)]
mod tests {
    use super::ValidationScope as Scope;

    #[test]
    fn provider_context_selects_only_partial_ci_local_scope() {
        for (github, buildkite) in [
            (Some("true"), None),
            (None, Some("true")),
            (Some("true"), Some("true")),
            (Some("false"), Some("true")),
        ] {
            let scope = Scope::from_provider_flags(github, buildkite);
            assert_eq!(scope.label(), "ci_local");
            assert!(!scope.requires_publication_proof());
            assert!(!scope.records_receipt());
        }
    }

    #[test]
    fn missing_false_or_unrecognized_provider_flags_keep_full_validation() {
        for value in [None, Some("false"), Some(""), Some("1"), Some("TRUE")] {
            assert_eq!(
                Scope::from_provider_flags(value, value),
                Scope::PublicationReadiness
            );
        }
    }

    #[test]
    fn partial_project_scope_remains_explicit_and_separate() {
        assert_eq!(Scope::ProjectPrerequisites.label(), "project_prerequisites");
        assert!(Scope::ProjectPrerequisites.records_receipt());
        assert!(!Scope::ProjectPrerequisites.requires_publication_proof());
        assert!(Scope::PublicationReadiness.requires_publication_proof());
        assert!(!Scope::PublicationReadiness.records_receipt());
    }

    #[test]
    fn environment_probe() {
        if let Ok(expected) = std::env::var("DECAPOD_SCOPE_TEST_EXPECTED") {
            assert_eq!(Scope::public_validate().label(), expected);
        }
    }

    #[test]
    fn actual_environment_dispatch_is_exact_and_ignores_generic_ci() {
        for (github, buildkite, expected) in [
            ("true", "false", "ci_local"),
            ("false", "true", "ci_local"),
            ("false", "false", "publication_readiness"),
            ("", "", "publication_readiness"),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    &format!(
                        "{}::environment_probe",
                        module_path!().split_once("::").unwrap().1
                    ),
                ])
                .env("GITHUB_ACTIONS", github)
                .env("BUILDKITE", buildkite)
                .env("CI", "true")
                .env("DECAPOD_SCOPE_TEST_EXPECTED", expected)
                .output()
                .unwrap();
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }
    #[test]
    fn full_buildkite_dispatch_submits_without_local_prerequisites() {
        let events = std::cell::RefCell::new(Vec::new());
        let ready = Scope::PublicationReadiness.dispatch(
            true,
            (),
            |_| panic!("local 203-gate suite must not run before remote submission"),
            |_| {
                events.borrow_mut().extend(["submit", "wait", "collect"]);
                false // Missing authenticated execution remains non-ready.
            },
        );
        assert!(!ready);
        assert_eq!(*events.borrow(), ["submit", "wait", "collect"]);
    }

    #[test]
    fn native_project_and_ci_dispatch_keep_actual_local_checks() {
        for (scope, enabled) in [
            (Scope::PublicationReadiness, false),
            (Scope::ProjectPrerequisites, true),
            (Scope::ProjectPrerequisites, false),
            (Scope::CiLocal, true),
            (Scope::CiLocal, false),
        ] {
            let calls = std::cell::Cell::new(0);
            scope.dispatch(
                enabled,
                (),
                |_| calls.set(calls.get() + 1),
                |_| panic!("partial/native route must not submit a snapshot"),
            );
            assert_eq!(calls.get(), 1);
        }
        for flags in [(Some("true"), None), (None, Some("true"))] {
            let scope = Scope::from_provider_flags(flags.0, flags.1);
            assert!(!scope.requires_publication_proof());
            scope.dispatch(true, (), |_| (), |_| panic!("CI must not recurse"));
        }
    }

    #[test]
    fn cli_rpc_and_publish_wiring_leave_local_suite_outside_true_path() {
        let source = include_str!("lib.rs");
        for (entry, end) in [
            ("fn run_validate_command(", "/// Buildkite owns"),
            (
                "pub(crate) fn handle_validate_run(",
                "fn handle_validate_run_remote(",
            ),
        ] {
            let body = source
                .split_once(entry)
                .unwrap()
                .1
                .split_once(end)
                .unwrap()
                .0;
            assert!(body.contains("scope.dispatch("));
            assert!(!body.contains("run_validation_bounded_with_receipt("));
        }
        for (entry, end) in [
            (
                "fn run_validate_command_remote(",
                "fn run_validate_command_local(",
            ),
            (
                "fn handle_validate_run_remote(",
                "fn handle_validate_run_local(",
            ),
        ] {
            let body = source
                .split_once(entry)
                .unwrap()
                .1
                .split_once(end)
                .unwrap()
                .0;
            assert!(body.contains("run_current_workbranch_readiness_with("));
            assert!(body.contains("not_run_provider_owned"));
            assert!(!body.contains("run_validation_bounded_with_receipt("));
            assert!(!body.contains("heal_"));
        }
        let publish = source
            .split_once("pub(crate) fn enforce_workbranch_readiness_before_push(")
            .unwrap()
            .1
            .split_once("/// Called only after source publication")
            .unwrap()
            .0;
        assert!(publish.contains("if !crate::cli::DecapodProjectConfig::load(workspace_root)?"));
        assert!(publish.contains(".validation_buildkite_preflights"));
        assert!(publish.contains("run_current_workbranch_readiness_with("));
        let command = source
            .split_once("fn run_workspace_command(")
            .unwrap()
            .1
            .split_once("WorkspaceCommand::Publish {")
            .unwrap()
            .1
            .split_once("let invocation = ReadinessInvocation {")
            .unwrap()
            .0;
        assert!(command.contains("if !crate::cli::DecapodProjectConfig::load(workspace_root)?"));
        assert!(command.contains(".validation_buildkite_preflights"));
    }
}
