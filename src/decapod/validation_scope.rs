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
}
