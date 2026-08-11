//! Release-bound integrity for the generated agent entrypoints.
//!
//! `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and `CODEX.md` are Decapod-owned
//! templates. Agents and humans must not hand-edit them. The only permitted
//! on-disk form is the exact template body plus release/fingerprint headers
//! for the evaluating Decapod binary. Feature-branch commits that touch these
//! files for any other reason (payload edits, fingerprint tweaks, mode-only
//! changes) fail validation via the entrypoint commit-discipline gate.
//!
//! The expected fingerprints in this module are release artifact data. They
//! are deliberately not derived from the files being validated or from the
//! repository at runtime.

use crate::core::{assets, error};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

pub const ENTRYPOINT_FILES: [&str; 4] = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];
pub const RELEASE_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASE_MARKER: &str = "<!-- decapod-release:";
const FINGERPRINT_MARKER: &str = "<!-- decapod-fingerprint:";
const LEGACY_BINARY_MARKER: &str = "<!-- decapod-binary-sha256:";
const FINGERPRINT_PLACEHOLDER: &str = "<fingerprint>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrypointExpectation {
    pub surface: &'static str,
    pub fingerprint: String,
}

// Release fingerprints are computed from the installed binary's templates and
// CARGO_PKG_VERSION. A hand-maintained SHA table was a release footgun: every
// version bump required four manual updates or unit tests failed while root
// .md markers lagged. Runtime validate still hard-fails when on-disk markers
// do not match the evaluating release (#1154).
static COMPUTED_ENTRYPOINTS: OnceLock<[String; 4]> = OnceLock::new();

/// Deterministic release manifest for the evaluating Decapod binary.
pub fn expected_entrypoints() -> [EntrypointExpectation; 4] {
    let fps = computed_entrypoints();
    [
        EntrypointExpectation {
            surface: "AGENTS.md",
            fingerprint: fps[0].clone(),
        },
        EntrypointExpectation {
            surface: "CLAUDE.md",
            fingerprint: fps[1].clone(),
        },
        EntrypointExpectation {
            surface: "GEMINI.md",
            fingerprint: fps[2].clone(),
        },
        EntrypointExpectation {
            surface: "CODEX.md",
            fingerprint: fps[3].clone(),
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    Missing,
    MetadataMissing,
    MetadataMalformed,
    ReleaseMismatch,
    FingerprintMismatch,
    PayloadModified,
    UnsupportedFileType,
}

impl FindingKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Missing => "entrypoint_missing",
            Self::MetadataMissing => "entrypoint_metadata_missing",
            Self::MetadataMalformed => "entrypoint_metadata_malformed",
            Self::ReleaseMismatch => "entrypoint_release_mismatch",
            Self::FingerprintMismatch => "entrypoint_fingerprint_mismatch",
            Self::PayloadModified => "entrypoint_payload_modified",
            Self::UnsupportedFileType => "entrypoint_unsupported_file_type",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub surface: String,
    pub running_release: String,
    pub declared_release: Option<String>,
    pub expected_fingerprint: String,
    pub observed_fingerprint: Option<String>,
    pub kind: FindingKind,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Governed agent entrypoint integrity failure")?;
        writeln!(f, "Surface: {}", self.surface)?;
        writeln!(f, "Running Decapod release: {}", self.running_release)?;
        writeln!(
            f,
            "Declared Decapod release: {}",
            self.declared_release.as_deref().unwrap_or("<missing>")
        )?;
        writeln!(
            f,
            "Expected entrypoint fingerprint: {}",
            self.expected_fingerprint
        )?;
        writeln!(
            f,
            "Observed entrypoint fingerprint: {}",
            self.observed_fingerprint.as_deref().unwrap_or("<missing>")
        )?;
        writeln!(f, "Finding: {}", self.kind.code())?;
        write!(
            f,
            "Remediation: regenerate the governed entrypoints using the installed Decapod release"
        )
    }
}

pub fn canonical_template(surface: &str) -> Option<String> {
    assets::canonical_template(surface)
}

fn computed_entrypoints() -> &'static [String; 4] {
    COMPUTED_ENTRYPOINTS.get_or_init(|| {
        ENTRYPOINT_FILES.map(|surface| {
            let payload = canonical_template(surface)
                .expect("every governed entrypoint must have a canonical template");
            fingerprint_for_payload(surface, RELEASE_VERSION, &payload)
        })
    })
}

pub fn expected_fingerprint(surface: &str) -> Option<&'static str> {
    let index = ENTRYPOINT_FILES
        .iter()
        .position(|entry| *entry == surface)?;
    Some(computed_entrypoints()[index].as_str())
}

pub fn fingerprint(payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn fingerprint_input(surface: &str, release: &str, payload: &str) -> String {
    format!(
        "decapod-entrypoint:{surface}\n<!-- decapod-release: {release} -->\n<!-- decapod-fingerprint: {FINGERPRINT_PLACEHOLDER} -->\n{payload}"
    )
}

fn fingerprint_for_payload(surface: &str, release: &str, payload: &str) -> String {
    fingerprint(&fingerprint_input(surface, release, payload))
}

pub fn render_entrypoint(surface: &str) -> Option<String> {
    let payload = canonical_template(surface)?;
    let expected = expected_fingerprint(surface)?;
    Some(format!(
        "<!-- decapod-release: {RELEASE_VERSION} -->\n<!-- decapod-fingerprint: {expected} -->\n{payload}"
    ))
}

/// Refresh release-bound entrypoints when they do not match the evaluating binary.
///
/// **Always verify; write only on mismatch.**
///
/// - If on-disk content is already byte-identical to [`render_entrypoint`] for
///   this binary's [`RELEASE_VERSION`], do nothing (subsequent project PRs on
///   the same Decapod version produce no entrypoint churn).
/// - If the release pin or fingerprint is stale (project or CI moved to a newer
///   Decapod) **and** the body is still the canonical template, rewrite to the
///   evaluating template so the agent can commit a real pin/fingerprint bump.
/// - Hand-edited (non-canonical) bodies are left alone so validation can hard-fail
///   them via [`validate_entrypoint`] instead of silently overwriting project text.
///
/// Agents must call this path early (validate / workspace entry) so Decapod
/// version alignment is established before implementation work.
pub fn refresh_entrypoint_metadata(project_root: &Path) -> Result<usize, error::DecapodError> {
    let mut updated = 0;
    for surface in ENTRYPOINT_FILES {
        let path = project_root.join(surface);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error::DecapodError::IoError(error)),
        };
        if !metadata.file_type().is_file() {
            continue;
        }

        let existing = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
        let Some(rendered) = render_entrypoint(surface) else {
            continue;
        };
        // Fast path: already matches evaluating release → verify-only, no write.
        if existing == rendered {
            continue;
        }

        let mut lines = existing.splitn(3, '\n');
        let Some(first) = lines.next() else {
            continue;
        };
        let Some(second) = lines.next() else {
            continue;
        };
        let Some(payload) = lines.next() else {
            continue;
        };
        let Some(Ok(declared_release)) = marker_value(first, RELEASE_MARKER) else {
            continue;
        };
        // Only auto-heal files whose body is still the governed template.
        if canonical_template(surface).as_deref() != Some(payload) {
            continue;
        }

        let expected = expected_fingerprint(surface).unwrap_or("");
        let declared_fingerprint = marker_value(second, FINGERPRINT_MARKER)
            .and_then(|result| result.ok())
            .unwrap_or_default();
        let fingerprint_valid_for_declared = !declared_fingerprint.is_empty()
            && fingerprint_for_payload(surface, &declared_release, payload) == declared_fingerprint;
        let legacy_binary_marker =
            matches!(marker_value(second, LEGACY_BINARY_MARKER), Some(Ok(_)));
        let release_mismatch = declared_release != RELEASE_VERSION;
        let fingerprint_mismatch =
            !declared_fingerprint.is_empty() && declared_fingerprint != expected;

        // Rewrite when on-disk pin is stale relative to the evaluating binary:
        // - release pin differs (project upgraded / CI evaluates newer Decapod), or
        // - fingerprint is stale for the current release, or
        // - legacy binary marker still present, or
        // - declared fingerprint is consistent with the declared (older) release
        //   so a pure header migration to the evaluating release is safe.
        let should_rewrite = release_mismatch
            || fingerprint_mismatch
            || legacy_binary_marker
            || fingerprint_valid_for_declared;
        if !should_rewrite {
            continue;
        }

        fs::write(&path, rendered).map_err(error::DecapodError::IoError)?;
        updated += 1;
    }
    Ok(updated)
}

/// Classify on-disk entrypoint alignment with the evaluating binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrypointAlignment {
    /// Byte-identical to [`render_entrypoint`] for [`RELEASE_VERSION`].
    MatchesEvaluatingRelease,
    /// Canonical body but stale release pin and/or fingerprint (heal will bump).
    StalePin,
    /// Missing, non-canonical, or otherwise not auto-healable.
    Divergent,
}

/// Inspect whether a surface already matches the evaluating Decapod release.
pub fn classify_entrypoint_alignment(
    project_root: &Path,
    surface: &str,
) -> Result<EntrypointAlignment, error::DecapodError> {
    let path = project_root.join(surface);
    let existing = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(EntrypointAlignment::Divergent);
        }
        Err(error) => return Err(error::DecapodError::IoError(error)),
    };
    let Some(rendered) = render_entrypoint(surface) else {
        return Ok(EntrypointAlignment::Divergent);
    };
    if existing == rendered {
        return Ok(EntrypointAlignment::MatchesEvaluatingRelease);
    }
    let mut lines = existing.splitn(3, '\n');
    let Some(_first) = lines.next() else {
        return Ok(EntrypointAlignment::Divergent);
    };
    let Some(_second) = lines.next() else {
        return Ok(EntrypointAlignment::Divergent);
    };
    let Some(payload) = lines.next() else {
        return Ok(EntrypointAlignment::Divergent);
    };
    if canonical_template(surface).as_deref() == Some(payload) {
        Ok(EntrypointAlignment::StalePin)
    } else {
        Ok(EntrypointAlignment::Divergent)
    }
}

fn marker_value(line: &str, prefix: &str) -> Option<Result<String, ()>> {
    let line = line.trim_end_matches(['\r', '\n']).trim();
    if !line.starts_with(prefix) {
        return None;
    }
    Some(
        line.strip_prefix(prefix)
            .and_then(|value| value.strip_suffix("-->"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or(()),
    )
}

fn finding(
    surface: &str,
    kind: FindingKind,
    declared_release: Option<String>,
    observed_fingerprint: Option<String>,
) -> Box<Finding> {
    Box::new(Finding {
        surface: surface.to_string(),
        running_release: RELEASE_VERSION.to_string(),
        declared_release,
        expected_fingerprint: expected_fingerprint(surface)
            .unwrap_or("<unknown>")
            .to_string(),
        observed_fingerprint,
        kind,
    })
}

pub fn validate_entrypoint(project_root: &Path, surface: &str) -> Result<(), Box<Finding>> {
    let Some(expected) = expected_fingerprint(surface) else {
        return Err(finding(surface, FindingKind::Missing, None, None));
    };
    let path = project_root.join(surface);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(finding(surface, FindingKind::Missing, None, None));
        }
        Err(_) => {
            return Err(finding(
                surface,
                FindingKind::UnsupportedFileType,
                None,
                None,
            ));
        }
    };
    if !metadata.file_type().is_file() {
        return Err(finding(
            surface,
            FindingKind::UnsupportedFileType,
            None,
            None,
        ));
    }

    let bytes = fs::read(&path)
        .map_err(|_| finding(surface, FindingKind::UnsupportedFileType, None, None))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| finding(surface, FindingKind::PayloadModified, None, None))?;
    let mut lines = content.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let second = lines.next().unwrap_or("");
    let payload_offset = first.len() + second.len();
    let payload = &content[payload_offset..];

    let release = marker_value(first, RELEASE_MARKER);
    let declared_fingerprint = marker_value(second, FINGERPRINT_MARKER);
    let malformed = release.as_ref().is_some_and(Result::is_err)
        || declared_fingerprint.as_ref().is_some_and(Result::is_err);
    let declared_release = release.and_then(Result::ok);
    let declared_fingerprint = declared_fingerprint.and_then(Result::ok);
    if malformed {
        return Err(finding(
            surface,
            FindingKind::MetadataMalformed,
            declared_release,
            declared_fingerprint,
        ));
    }
    if declared_release.is_none() || declared_fingerprint.is_none() {
        return Err(finding(
            surface,
            FindingKind::MetadataMissing,
            declared_release,
            declared_fingerprint,
        ));
    }

    let declared_release = declared_release.unwrap();
    let declared_fingerprint = declared_fingerprint.unwrap();
    let observed_fingerprint = fingerprint_for_payload(surface, &declared_release, payload);
    if declared_release != RELEASE_VERSION {
        return Err(finding(
            surface,
            FindingKind::ReleaseMismatch,
            Some(declared_release),
            Some(observed_fingerprint),
        ));
    }
    if declared_fingerprint != expected {
        return Err(finding(
            surface,
            FindingKind::FingerprintMismatch,
            Some(declared_release),
            Some(observed_fingerprint),
        ));
    }

    let canonical = canonical_template(surface).unwrap_or_default();
    if payload != canonical || observed_fingerprint != expected {
        return Err(finding(
            surface,
            FindingKind::PayloadModified,
            Some(declared_release),
            Some(observed_fingerprint),
        ));
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/unit/core/entrypoint_integrity_tests.rs"]
mod tests;
