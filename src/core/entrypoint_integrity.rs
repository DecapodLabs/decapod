//! Release-bound integrity for the generated agent entrypoints.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntrypointExpectation {
    pub surface: &'static str,
    pub fingerprint: &'static str,
}

// These values are the v0.76.0 release manifest. Keep them immutable for the
// lifetime of that release; a later release must update them deliberately and
// regenerate the four root entrypoints through Decapod.
pub const EXPECTED_ENTRYPOINTS: [EntrypointExpectation; 4] = [
    EntrypointExpectation {
        surface: "AGENTS.md",
        fingerprint: "0da8411d76cf9b18befaf0b5b5a28048026d3aca782da95155270e378373df75",
    },
    EntrypointExpectation {
        surface: "CLAUDE.md",
        fingerprint: "43d90a2e5d640ee5787cd03e70a24675d20559f9d5c37c5f951e8497804e7b6c",
    },
    EntrypointExpectation {
        surface: "GEMINI.md",
        fingerprint: "553d42331f1f9bbae02123bf1c43f4dcf085bfd66f76445f0391ad8c46ec9f08",
    },
    EntrypointExpectation {
        surface: "CODEX.md",
        fingerprint: "dfea936c740b1bb6ec5290b1fd8c9abaf3356708d505e4af795cd408190e0cf7",
    },
];

static COMPUTED_ENTRYPOINTS: OnceLock<[String; 4]> = OnceLock::new();

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

/// Refresh release metadata when the generated payload is still canonical.
///
/// A current-format marker is migrated only when its value is internally valid
/// for the release named by that marker. This keeps a hand-edited fingerprint
/// from being silently repaired. The legacy binary marker is accepted only as
/// a migration source for the pre-v0.70 entrypoint format.
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
        if canonical_template(surface).as_deref() != Some(payload) {
            continue;
        }

        match marker_value(second, FINGERPRINT_MARKER) {
            Some(Ok(declared_fingerprint))
                if fingerprint_for_payload(surface, &declared_release, payload)
                    == declared_fingerprint => {}
            Some(_) => continue,
            None if matches!(marker_value(second, LEGACY_BINARY_MARKER), Some(Ok(_))) => {}
            None => continue,
        }

        let Some(rendered) = render_entrypoint(surface) else {
            continue;
        };
        if existing != rendered {
            fs::write(&path, rendered).map_err(error::DecapodError::IoError)?;
            updated += 1;
        }
    }
    Ok(updated)
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
mod tests {
    use super::*;

    #[test]
    fn compiled_manifest_matches_canonical_templates() {
        assert_eq!(EXPECTED_ENTRYPOINTS.len(), ENTRYPOINT_FILES.len());
        for surface in ENTRYPOINT_FILES {
            let payload = canonical_template(surface).expect("canonical entrypoint");
            assert_eq!(
                fingerprint_for_payload(surface, RELEASE_VERSION, &payload),
                expected_fingerprint(surface).expect("compiled fingerprint"),
                "compiled release manifest drifted from canonical {surface}"
            );
            let manifest_entry = EXPECTED_ENTRYPOINTS
                .iter()
                .find(|entry| entry.surface == surface)
                .expect("release manifest entry");
            assert_eq!(
                manifest_entry.fingerprint,
                expected_fingerprint(surface).expect("computed fingerprint"),
                "release manifest SHA drifted from computed {surface}"
            );
        }
    }

    #[test]
    fn computed_manifest_is_filename_and_version_bound() {
        for surface in ENTRYPOINT_FILES {
            let payload = canonical_template(surface).expect("canonical entrypoint");
            let expected = expected_fingerprint(surface).expect("computed fingerprint");
            assert_eq!(
                fingerprint_for_payload(surface, RELEASE_VERSION, &payload),
                expected
            );
        }
    }
}
