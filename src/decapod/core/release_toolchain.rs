//! Shared source-controlled setup requirements, not an executable receipt.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseToolchain {
    pub rust: String,
    pub release_plz: String,
    pub cargo_dist: String,
    pub release_plz_action: String,
    pub semver_checks_constraint: String,
    pub semver_checks_installer: String,
}
impl ReleaseToolchain {
    pub fn parse(text: &str) -> Result<Self, String> {
        let result: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        for version in [&result.rust, &result.release_plz, &result.cargo_dist] {
            if version.split('.').count() != 3
                || version
                    .split('.')
                    .any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err("release tools require exact stable versions".into());
            }
        }
        for (reference, expected_repo) in [
            (&result.release_plz_action, "release-plz/action"),
            (&result.semver_checks_installer, "taiki-e/install-action"),
        ] {
            let Some((repo, revision)) = reference.split_once('@') else {
                return Err("missing installer revision".into());
            };
            if repo != expected_repo
                || revision.len() != 40
                || !revision.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("installer must use a reviewed immutable action revision".into());
            }
        }
        if result.semver_checks_constraint != "0.51" {
            return Err("unsupported upstream semver-checks setup constraint".into());
        }
        Ok(result)
    }
    pub fn accepts_semver_checks(&self, measured: &str) -> bool {
        measured
            .strip_prefix(&(self.semver_checks_constraint.clone() + "."))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }
}
