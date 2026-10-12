//! Pure planning and verification for a derived release. No commands execute here.
//!
//! The caller must capture complete snapshots, resolve production tool identities,
//! prove history/registry inputs and authenticate command receipts with the native
//! runner. This module does not prove those facts from caller-supplied strings.
//! Unsupported production rules fail closed instead of falling back to no-release.

use super::release_metadata;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn require(ok: bool, why: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(why.into()) }
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|b| b.is_ascii_hexdigit())
        && value.bytes().any(|b| b != b'0')
}

fn object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value.bytes().all(|b| b.is_ascii_hexdigit())
        && value.bytes().any(|b| b != b'0')
}

fn stable_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
                && p.parse::<u64>().is_ok()
        })
}

fn date(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let (y, m, d) = (
        parts[0].parse::<u32>().unwrap(),
        parts[1].parse::<u32>().unwrap(),
        parts[2].parse::<u32>().unwrap(),
    );
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if y.is_multiple_of(400) || (y.is_multiple_of(4) && !y.is_multiple_of(100)) => 29,
        2 => 28,
        _ => return false,
    };
    y > 0 && (1..=max).contains(&d)
}

fn safe_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseConfig {
    workspace: ReleaseWorkspace,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseWorkspace {
    changelog_update: bool,
    changelog_path: String,
    git_tag_name: String,
    pr_labels: Vec<String>,
    allow_dirty: bool,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DistWorkspace {
    workspace: Members,
    dist: DistConfig,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Members {
    members: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct DistConfig {
    cargo_dist_version: String,
    ci: String,
    installers: Vec<String>,
    targets: Vec<String>,
    allow_dirty: Vec<String>,
    github_custom_runners: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DistPackage {
    package: Binaries,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binaries {
    binaries: Vec<String>,
}

/// Construct from the source checkout's actual files, never a parallel policy file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseProfile {
    pub package: String,
    pub source_version: String,
    pub rust_version: String,
    pub dist_version: String,
    pub binaries: BTreeSet<String>,
    pub targets: BTreeSet<String>,
    pub runners: BTreeMap<String, String>,
    pub configuration_digest: String,
    repository: String,
}

pub fn profile(
    cargo: &str,
    release: &str,
    workspace: &str,
    package: &str,
) -> Result<ReleaseProfile> {
    let mut input_hash = Sha256::new();
    for bytes in [cargo, release, workspace, package] {
        input_hash.update((bytes.len() as u64).to_be_bytes());
        input_hash.update(bytes);
    }
    let configuration_digest = format!("{:x}", input_hash.finalize());
    let cargo: toml::Value = toml::from_str(cargo).map_err(|e| format!("Cargo.toml: {e}"))?;
    let release: ReleaseConfig =
        toml::from_str(release).map_err(|e| format!("release.toml: {e}"))?;
    let dist: DistWorkspace =
        toml::from_str(workspace).map_err(|e| format!("dist-workspace.toml: {e}"))?;
    let package: DistPackage = toml::from_str(package).map_err(|e| format!("dist.toml: {e}"))?;
    let string = |key: &str| {
        cargo
            .get("package")
            .and_then(|p| p.get(key))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("missing package.{key}"))
    };
    let name = string("name")?;
    let version = string("version")?;
    let rust_version = string("rust-version")?;
    let repository = string("repository")?;
    require(
        name == "decapod" && stable_version(&version) && stable_version(&rust_version),
        "unsupported root package identity/version",
    )?;
    require(
        repository.starts_with("https://github.com/")
            && !repository.ends_with('/')
            && !repository
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | '?' | '#')),
        "unsupported repository URL",
    )?;
    let r = release.workspace;
    require(
        r.changelog_update
            && r.changelog_path == "CHANGELOG.md"
            && r.git_tag_name == "v{{ version }}"
            && r.pr_labels == ["release"]
            && r.allow_dirty,
        "release rules require an explicit profile implementation",
    )?;
    require(
        dist.workspace.members == ["cargo:."],
        "unsupported distribution workspace",
    )?;
    require(
        dist.dist.cargo_dist_version == "0.22.1",
        "cargo-dist schema must be resolved for the configured version",
    )?;
    require(
        dist.dist.ci == "github"
            && dist.dist.installers.is_empty()
            && dist.dist.allow_dirty == ["ci"],
        "unsupported distribution settings",
    )?;
    require(
        cargo
            .get("workspace")
            .and_then(|w| w.get("metadata"))
            .and_then(|m| m.get("dist"))
            .is_none(),
        "unexpected second Cargo distribution policy",
    )?;
    let targets: BTreeSet<_> = dist.dist.targets.iter().cloned().collect();
    require(
        !targets.is_empty()
            && targets.len() == dist.dist.targets.len()
            && targets.iter().all(|t| {
                t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            }),
        "missing, duplicate or invalid distribution targets",
    )?;
    let binaries: BTreeSet<_> = package.package.binaries.iter().cloned().collect();
    require(
        !binaries.is_empty()
            && binaries.len() == package.package.binaries.len()
            && binaries.iter().all(|b| safe_path(b) && !b.contains('/')),
        "invalid distribution binaries",
    )?;
    let bins = cargo
        .get("bin")
        .and_then(toml::Value::as_array)
        .ok_or("missing Cargo binaries")?;
    for name in &binaries {
        let matches: Vec<_> = bins
            .iter()
            .filter(|b| b.get("name").and_then(toml::Value::as_str) == Some(name))
            .collect();
        require(
            matches.len() == 1 && matches[0].get("required-features").is_none(),
            "selected binary is absent, duplicated or feature gated",
        )?;
    }
    let required_runner_keys: BTreeSet<_> =
        targets.iter().cloned().chain(["global".into()]).collect();
    require(
        dist.dist
            .github_custom_runners
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            == required_runner_keys
            && dist
                .dist
                .github_custom_runners
                .values()
                .all(|v| !v.is_empty()),
        "missing or extraneous distribution runner",
    )?;
    Ok(ReleaseProfile {
        package: name,
        source_version: version,
        rust_version,
        dist_version: dist.dist.cargo_dist_version,
        binaries,
        targets,
        runners: dist.dist.github_custom_runners,
        configuration_digest,
        repository,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Cargo,
    Rustc,
    ReleasePlz,
    CargoDist,
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResolution {
    pub actual_version: String,
    pub executable_sha256: String,
    /// One resolved version for each applicable production workflow/runtime.
    pub production_versions: BTreeMap<String, String>,
    /// Digest of the source workflows and immutable resolutions supporting above.
    pub production_resolution_sha256: String,
}

pub fn verify_tools(
    profile: &ReleaseProfile,
    tools: &BTreeMap<Tool, ToolResolution>,
) -> Result<()> {
    for key in [
        Tool::Cargo,
        Tool::Rustc,
        Tool::ReleasePlz,
        Tool::CargoDist,
        Tool::Git,
    ] {
        let tool = tools
            .get(&key)
            .ok_or_else(|| format!("missing tool: {key:?}"))?;
        require(
            !tool.actual_version.is_empty()
                && digest(&tool.executable_sha256)
                && digest(&tool.production_resolution_sha256)
                && !tool.production_versions.is_empty()
                && tool
                    .production_versions
                    .values()
                    .all(|v| v == &tool.actual_version),
            "unresolved or divergent production tool identity",
        )?;
        if key == Tool::Rustc {
            require(
                tool.actual_version == profile.rust_version,
                "Rust compiler differs from release profile",
            )?;
        }
        if key == Tool::CargoDist {
            require(
                tool.actual_version == profile.dist_version,
                "cargo-dist differs from repository pin",
            )?;
        }
    }
    Ok(())
}

/// Mandatory obligations determined before generation or command results exist.
/// The surrounding planner maps these to its own CheckKey/evidence model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequiredCheck {
    pub id: &'static str,
    pub target: Option<String>,
    pub runner: Option<String>,
}

impl ReleaseProfile {
    pub fn required_checks(&self) -> Vec<RequiredCheck> {
        let mut checks: Vec<_> = [
            "generate",
            "verify-delta",
            "dist-plan",
            "cargo-package",
            "candidate-health",
        ]
        .into_iter()
        .map(|id| RequiredCheck {
            id,
            target: None,
            runner: if id == "candidate-health" {
                self.runners.get("global").cloned()
            } else {
                None
            },
        })
        .collect();
        for target in &self.targets {
            checks.push(RequiredCheck {
                id: "dist-local",
                target: Some(target.clone()),
                runner: self.runners.get(target).cloned(),
            });
        }
        checks.push(RequiredCheck {
            id: "dist-global",
            target: None,
            runner: self.runners.get("global").cloned(),
        });
        checks.push(RequiredCheck {
            id: "verify-artifacts",
            target: None,
            runner: None,
        });
        checks
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    pub tool: Tool,
    pub args: Vec<String>,
}

impl ReleaseProfile {
    pub fn generation_command(&self, release_date: &str) -> Result<CommandSpec> {
        require(date(release_date), "invalid release date")?;
        Ok(CommandSpec {
            tool: Tool::ReleasePlz,
            args: [
                "update",
                "--config",
                ".github/release.toml",
                "--manifest-path",
                "Cargo.toml",
                "--release-date",
                release_date,
            ]
            .map(str::to_owned)
            .to_vec(),
        })
    }

    /// Single argv authority shared by advertised proof recipes and actual
    /// production/nonpublishing cargo-dist artifact execution.
    pub fn artifact_build_arguments(
        &self,
        target: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut args = vec!["dist".into(), "build".into()];
        if let Some(tag) = tag {
            require(
                tag.strip_prefix('v').is_some_and(stable_version),
                "invalid explicit release tag",
            )?;
            args.extend(["--tag".into(), tag.into()]);
        }
        args.push("--output-format=json".into());
        if let Some(target) = target {
            require(
                self.targets.contains(target),
                "unconfigured distribution target",
            )?;
            args.extend([
                "--print=linkage".into(),
                "--artifacts=local".into(),
                "--target".into(),
                target.into(),
            ]);
        } else {
            args.push("--artifacts=global".into());
        }
        Ok(args)
    }

    /// Available targets come from verified runner/platform capabilities, not the
    /// targets requested by a caller. Partial coverage cannot yield full readiness.
    pub fn verification_commands(
        &self,
        version: &str,
        available_targets: &BTreeSet<String>,
    ) -> Result<Vec<CommandSpec>> {
        require(
            stable_version(version) && version != self.source_version,
            "unproven derived version",
        )?;
        require(
            self.targets.is_subset(available_targets),
            "missing release target/platform capability",
        )?;
        let tag = format!("v{version}");
        let cargo = |args: Vec<String>| CommandSpec {
            tool: Tool::Cargo,
            args,
        };
        let mut commands = vec![
            cargo(vec![
                "dist".into(),
                "plan".into(),
                "--tag".into(),
                tag.clone(),
                "--output-format=json".into(),
            ]),
            cargo(
                ["package", "--locked", "--package", &self.package]
                    .map(str::to_owned)
                    .to_vec(),
            ),
        ];
        for target in &self.targets {
            commands.push(cargo(
                self.artifact_build_arguments(Some(target), Some(&tag))?,
            ));
        }
        commands.push(cargo(self.artifact_build_arguments(None, Some(&tag))?));
        Ok(commands)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMaterial {
    pub mode: u32,
    pub bytes: Vec<u8>,
}
pub type Snapshot = BTreeMap<String, FileMaterial>;

pub fn snapshot_digest(files: &Snapshot) -> Result<String> {
    require(!files.is_empty(), "empty source snapshot")?;
    let mut hash = Sha256::new();
    for (path, f) in files {
        require(safe_path(path), "unsafe source path")?;
        hash.update((path.len() as u64).to_be_bytes());
        hash.update(path);
        hash.update(f.mode.to_be_bytes());
        hash.update((f.bytes.len() as u64).to_be_bytes());
        hash.update(&f.bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationBinding {
    pub source_commit: String,
    pub source_git_tree: String,
    pub source_material_sha256: String,
    pub derived_commit: String,
    pub derived_git_tree: String,
    pub derived_material_sha256: String,
    pub accepted_base_commit: String,
    /// Complete parent graph, commit messages and tags consulted by release-plz.
    pub history_sha256: String,
    pub baseline_tag: String,
    pub baseline_commit: String,
    pub baseline_version: String,
    /// Registry URL/index snapshot plus fetched release metadata/crate checksums.
    pub registry_inputs_sha256: String,
    pub registry_identity: String,
    pub registry_observed_at: String,
    pub registry_version: String,
    pub registry_crate_sha256: String,
    pub configuration_sha256: String,
    pub tools_sha256: String,
    pub release_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedRelease {
    pub old_version: String,
    pub version: String,
    pub tag: String,
    pub changelog_sha256: String,
    pub lock_sha256: String,
    pub binding_sha256: String,
}

fn file_text<'a>(snapshot: &'a Snapshot, path: &str) -> Result<&'a str> {
    let file = snapshot
        .get(path)
        .ok_or_else(|| format!("missing {path}"))?;
    require(
        file.mode == 0o100644,
        "release files must be non-executable regular blobs",
    )?;
    std::str::from_utf8(&file.bytes).map_err(|_| format!("non-UTF-8 {path}"))
}

pub fn verify_generation(
    profile: &ReleaseProfile,
    before: &Snapshot,
    after: &Snapshot,
    binding: &GenerationBinding,
    tools: &BTreeMap<Tool, ToolResolution>,
) -> Result<GeneratedRelease> {
    verify_tools(profile, tools)?;
    require(
        *profile
            == self::profile(
                file_text(before, "Cargo.toml")?,
                file_text(before, ".github/release.toml")?,
                file_text(before, "dist-workspace.toml")?,
                file_text(before, "dist.toml")?,
            )?,
        "profile is not bound to source configuration",
    )?;
    for id in [
        &binding.source_commit,
        &binding.source_git_tree,
        &binding.derived_commit,
        &binding.derived_git_tree,
        &binding.accepted_base_commit,
        &binding.baseline_commit,
    ] {
        require(
            object_id(id),
            "missing immutable source/derived/base/history identity",
        )?;
    }
    require(
        binding.source_commit != binding.derived_commit
            && binding.source_git_tree != binding.derived_git_tree,
        "derived tree must have its own local-only commit",
    )?;
    for h in [
        &binding.history_sha256,
        &binding.registry_inputs_sha256,
        &binding.registry_crate_sha256,
    ] {
        require(digest(h), "unresolved history/registry baseline")?;
    }
    require(
        date(&binding.release_date)
            && !binding.registry_observed_at.is_empty()
            && !binding.registry_identity.is_empty()
            && stable_version(&binding.baseline_version)
            && binding.baseline_tag == format!("v{}", binding.baseline_version)
            && binding.registry_version == binding.baseline_version,
        "unresolved or inconsistent release baseline/date",
    )?;
    require(
        binding.source_material_sha256 == snapshot_digest(before)?
            && binding.derived_material_sha256 == snapshot_digest(after)?
            && binding.configuration_sha256 == profile.configuration_digest
            && binding.tools_sha256
                == sha256(&serde_json::to_vec(tools).map_err(|e| e.to_string())?),
        "stale generation binding",
    )?;
    let delta = verify_delta(profile, before, after, &binding.release_date)?;
    Ok(GeneratedRelease {
        tag: format!("v{}", delta.version),
        old_version: delta.old_version,
        version: delta.version,
        changelog_sha256: delta.changelog_sha256,
        lock_sha256: delta.lock_sha256,
        binding_sha256: sha256(&serde_json::to_vec(binding).map_err(|e| e.to_string())?),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDelta {
    pub old_version: String,
    pub version: String,
    pub changelog_sha256: String,
    pub lock_sha256: String,
}
/// Verify the pure generated delta before a derivative Git commit is created.
pub fn verify_delta(
    profile: &ReleaseProfile,
    before: &Snapshot,
    after: &Snapshot,
    release_date: &str,
) -> Result<VerifiedDelta> {
    require(date(release_date), "invalid generated release date")?;
    require(
        before.keys().eq(after.keys()),
        "generator added/deleted files outside the supported release delta",
    )?;
    let changed: BTreeSet<_> = before
        .iter()
        .filter_map(|(p, b)| (after.get(p) != Some(b)).then_some(p.as_str()))
        .collect();
    require(
        changed == BTreeSet::from(["Cargo.toml", "Cargo.lock", "CHANGELOG.md"]),
        "generator did not produce the exact proven root Cargo/lock/changelog delta; no-release is not inferred",
    )?;
    let (old, new) = release_metadata::version_only_manifest(
        file_text(before, "Cargo.toml")?,
        file_text(after, "Cargo.toml")?,
    )
    .ok_or("generated Cargo.toml is not a production-policy version-only increase")?;
    require(
        old == profile.source_version,
        "profile source version changed",
    )?;
    require(
        release_metadata::version_only_lock(
            file_text(before, "Cargo.lock")?,
            file_text(after, "Cargo.lock")?,
            &old,
            &new,
        ),
        "generated Cargo.lock changes dependency resolution or fails production root-version rules",
    )?;
    let old_log = file_text(before, "CHANGELOG.md")?;
    let new_log = file_text(after, "CHANGELOG.md")?;
    let marker = "## [Unreleased]\n\n";
    let (prefix, history) = old_log
        .split_once(marker)
        .ok_or("unsupported changelog header")?;
    require(
        history.starts_with("## ["),
        "existing unreleased content needs explicit support",
    )?;
    let start = format!("{prefix}{marker}");
    let inserted = new_log
        .strip_prefix(&start)
        .and_then(|s| s.strip_suffix(history))
        .ok_or("generator rewrote existing changelog history")?;
    let heading = format!(
        "## [{new}]({}/compare/v{old}...v{new}) - {}\n\n",
        profile.repository, release_date
    );
    let body = inserted
        .strip_prefix(&heading)
        .ok_or("generated changelog version/date/link mismatch")?;
    require(
        !body.trim().is_empty() && !body.lines().any(|l| l.starts_with("## ")),
        "empty or extra generated changelog release section",
    )?;
    Ok(VerifiedDelta {
        old_version: old,
        version: new,
        changelog_sha256: sha256(new_log.as_bytes()),
        lock_sha256: sha256(file_text(after, "Cargo.lock")?.as_bytes()),
    })
}

/// cargo-dist does not pass --locked; every plan/build must preserve this hash.
pub fn verify_lock_unchanged(release: &GeneratedRelease, observed: &[u8]) -> Result<()> {
    require(
        sha256(observed) == release.lock_sha256,
        "cargo-dist/package changed the generated lockfile",
    )
}

// serde_json::Value silently overwrites duplicate object keys. Reject them at
// every nesting level before applying cargo-dist's versioned schema checks.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                x: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, x: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, x: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, x: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(x)
                    .map(|n| UniqueJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                x: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                x: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(x.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(UniqueJson(value)) = a.next_element()? {
                    items.push(value);
                }
                Ok(UniqueJson(Value::Array(items)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut items = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = a.next_entry::<String, UniqueJson>()? {
                    if items.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(UniqueJson(Value::Object(items)))
            }
        }
        d.deserialize_any(Visitor)
    }
}

fn json_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing/invalid manifest field {key}"))
}
fn json_strings(value: &Value, key: &str, optional: bool) -> Result<BTreeSet<String>> {
    let Some(field) = value.get(key) else {
        return if optional {
            Ok(BTreeSet::new())
        } else {
            Err(format!("missing manifest field {key}"))
        };
    };
    let array = field
        .as_array()
        .ok_or_else(|| format!("invalid manifest list {key}"))?;
    let strings: BTreeSet<_> = array
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("non-string {key}"))
        })
        .collect::<Result<_>>()?;
    require(
        strings.len() == array.len(),
        "duplicate manifest list entry",
    )?;
    Ok(strings)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DistInventory {
    pub manifest_sha256: String,
    pub target_archives: BTreeMap<String, String>,
    pub source_archive: String,
    pub artifact_names: BTreeSet<String>,
    pub checksums: BTreeSet<String>,
}

/// Validate the complete cargo-dist plan against actual repository configuration.
/// Extra JSON metadata is retained in the bound raw digest but cannot introduce
/// additional releases, artifact kinds, targets, binaries or path traversals.
pub fn verify_dist_plan(
    profile: &ReleaseProfile,
    generated: &GeneratedRelease,
    raw: &[u8],
) -> Result<DistInventory> {
    let UniqueJson(plan) =
        serde_json::from_slice(raw).map_err(|e| format!("invalid dist manifest: {e}"))?;
    require(plan.is_object(), "dist manifest must be one object")?;
    require(
        plan.get("announcement_tag_is_implicit")
            .and_then(Value::as_bool)
            == Some(false)
            && plan
                .get("announcement_is_prerelease")
                .and_then(Value::as_bool)
                == Some(false),
        "plan must bind an explicit stable release tag",
    )?;
    require(
        plan.get("upload_files").is_none(),
        "plan must use cargo-dist's no-local-paths contract",
    )?;
    let matrix = plan
        .pointer("/ci/github/artifacts_matrix/include")
        .and_then(Value::as_array)
        .ok_or("missing cargo-dist target matrix")?;
    let mut matrix_targets = BTreeSet::new();
    for row in matrix {
        let targets = json_strings(row, "targets", false)?;
        require(!targets.is_empty(), "empty target matrix row")?;
        let runner = json_string(row, "runner")?;
        for target in targets {
            require(
                profile.runners.get(&target).map(String::as_str) == Some(runner)
                    && matrix_targets.insert(target),
                "unexpected/duplicate matrix target or runner",
            )?;
        }
    }
    require(
        matrix_targets == profile.targets,
        "incomplete cargo-dist target matrix",
    )?;
    require(
        json_string(&plan, "dist_version")? == profile.dist_version
            && json_string(&plan, "announcement_tag")? == generated.tag,
        "dist manifest version/tag mismatch",
    )?;
    let releases = plan
        .get("releases")
        .and_then(Value::as_array)
        .ok_or("missing dist releases")?;
    require(
        releases.len() == 1,
        "expected exactly the configured root release",
    )?;
    let release = &releases[0];
    require(
        json_string(release, "app_name")? == profile.package
            && json_string(release, "app_version")? == generated.version,
        "dist package/version mismatch",
    )?;
    let names = json_strings(release, "artifacts", false)?;
    let artifacts = plan
        .get("artifacts")
        .and_then(Value::as_object)
        .ok_or("missing dist artifacts")?;
    require(
        !names.is_empty() && names == artifacts.keys().cloned().collect(),
        "orphaned, missing or additional release artifacts",
    )?;
    let mut target_archives = BTreeMap::new();
    let mut checksums = BTreeSet::new();
    let mut checksum_references = BTreeSet::new();
    let mut source_archive = None;
    for (id, artifact) in artifacts {
        require(
            safe_path(id) && !id.contains('/'),
            "unsafe artifact identifier",
        )?;
        require(
            json_string(artifact, "name")? == id,
            "artifact ID/name mismatch",
        )?;
        require(
            artifact.get("path").is_none(),
            "plan artifact contains a local path",
        )?;
        let targets = json_strings(artifact, "target_triples", true)?;
        require(
            targets.is_subset(&profile.targets),
            "unconfigured artifact target",
        )?;
        let assets = match artifact.get("assets") {
            None => &[][..],
            Some(value) => value.as_array().ok_or("invalid archive assets")?.as_slice(),
        };
        match json_string(artifact, "kind")? {
            "executable-zip" => {
                require(
                    targets.len() == 1,
                    "local archive must bind exactly one configured target",
                )?;
                let target = targets.first().unwrap();
                let extension = if target.contains("windows") {
                    ".zip"
                } else {
                    ".tar.xz"
                };
                require(
                    id == &format!("{}-{target}{extension}", profile.package),
                    "archive name differs from pinned cargo-dist defaults",
                )?;
                require(
                    target_archives.insert(target.clone(), id.clone()).is_none(),
                    "duplicate local archive for target",
                )?;
                let mut executables = BTreeSet::new();
                let mut asset_paths = BTreeSet::new();
                for asset in assets {
                    let name = json_string(asset, "name")?;
                    let path = json_string(asset, "path")?;
                    require(
                        safe_path(name) && safe_path(path) && asset_paths.insert(path),
                        "unsafe/duplicate archive asset",
                    )?;
                    match json_string(asset, "kind")? {
                        "executable" => {
                            require(
                                executables.insert(name.to_owned()),
                                "duplicate executable asset",
                            )?;
                        }
                        "readme" | "license" | "changelog" => {}
                        _ => return Err("unsupported archive asset kind".into()),
                    }
                }
                require(
                    executables == profile.binaries,
                    "archive runtime binary inventory differs from dist.toml",
                )?;
                let checksum = json_string(artifact, "checksum")?;
                require(
                    checksum == format!("{id}.sha256") && names.contains(checksum),
                    "archive checksum artifact missing/misnamed",
                )?;
                checksum_references.insert(checksum.to_owned());
            }
            "source-tarball" => {
                require(
                    id == "source.tar.gz"
                        && targets.is_empty()
                        && assets.is_empty()
                        && source_archive.replace(id.clone()).is_none(),
                    "invalid/duplicate global source archive",
                )?;
                let checksum = json_string(artifact, "checksum")?;
                require(
                    checksum == "source.tar.gz.sha256" && names.contains(checksum),
                    "source checksum artifact missing/misnamed",
                )?;
                checksum_references.insert(checksum.to_owned());
            }
            "checksum" => {
                require(
                    assets.is_empty() && artifact.get("checksum").is_none(),
                    "invalid checksum artifact",
                )?;
                let for_id = id
                    .strip_suffix(".sha256")
                    .ok_or("unexpected checksum algorithm")?;
                let target_artifact = artifacts.get(for_id).ok_or("orphan checksum artifact")?;
                require(
                    targets == json_strings(target_artifact, "target_triples", true)?,
                    "checksum target mismatch",
                )?;
                checksums.insert(id.clone());
            }
            _ => return Err("unsupported/unexpected distribution artifact kind".into()),
        }
    }
    require(
        target_archives.keys().cloned().collect::<BTreeSet<_>>() == profile.targets,
        "missing distribution target archive",
    )?;
    require(
        !checksums.is_empty() && checksum_references == checksums,
        "invalid checksum relationships",
    )?;
    Ok(DistInventory {
        manifest_sha256: sha256(raw),
        target_archives,
        source_archive: source_archive.ok_or("missing global source archive")?,
        artifact_names: names,
        checksums,
    })
}

/// After all target/global builds, compare exact artifact inventory and actual
/// file hashes. Extraction/compile checks and source git-archive provenance are
/// independent runner gates, not inferred from these inventory receipts.
pub fn verify_built_artifacts(
    plan: &DistInventory,
    observed: &BTreeMap<String, String>,
) -> Result<()> {
    require(
        observed.keys().cloned().collect::<BTreeSet<_>>() == plan.artifact_names,
        "missing or extra built artifact",
    )?;
    require(
        observed.values().all(|h| digest(h)),
        "missing built artifact digest",
    )
}

/// Validate cargo-dist's actual per-artifact sha256 checksum file format.
/// Digests must be independently computed by the runner from physical files.
pub fn verify_checksum_files(
    plan: &DistInventory,
    observed: &BTreeMap<String, String>,
    checksum_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    verify_built_artifacts(plan, observed)?;
    require(
        checksum_contents.keys().cloned().collect::<BTreeSet<_>>() == plan.checksums,
        "missing or extra checksum files",
    )?;
    for (checksum, bytes) in checksum_contents {
        let artifact = checksum
            .strip_suffix(".sha256")
            .ok_or("invalid checksum filename")?;
        let hash = observed.get(artifact).ok_or("checksum target missing")?;
        require(
            bytes == format!("{hash} *{artifact}\n").as_bytes()
                && observed.get(checksum) == Some(&sha256(bytes)),
            "checksum content/hash mismatch",
        )?;
    }
    Ok(())
}

pub(crate) fn unique_json(raw: &[u8]) -> Result<Value> {
    let UniqueJson(value) = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    Ok(value)
}

#[cfg(test)]
#[path = "derived_release_profile_tests.rs"]
mod tests;
