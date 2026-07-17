//! Release-bound integrity for the generated agent entrypoints.
//!
//! The expected fingerprints in this module are release artifact data. They
//! are deliberately not derived from the files being validated or from the
//! repository at runtime.

use crate::core::assets;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::Path;

pub const ENTRYPOINT_FILES: [&str; 4] = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];
pub const RELEASE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Release identity SHA compiled into the binary and copied into generated
/// entrypoints. This is the SHA of the immutable release contract, not a
/// runtime hash of a mutable repository file or self-declared marker.
pub const BINARY_SHA256: &str = "fa91eefdc903310d8fb9a760f7ae44fb4225afc1b4f8fe121674a8a5b3b43659";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntrypointExpectation {
    pub surface: &'static str,
    pub fingerprint: &'static str,
}

// These values are the v0.69.1 release manifest. Keep them immutable for the
// lifetime of that release; a later release must update them deliberately and
// regenerate the four root entrypoints through Decapod.
pub const EXPECTED_ENTRYPOINTS: [EntrypointExpectation; 4] = [
    EntrypointExpectation {
        surface: "AGENTS.md",
        fingerprint: "9210b4634ebc9d2dbc7e0fa56cc23277f74196d954d63dc931eed98bc015cb28",
    },
    EntrypointExpectation {
        surface: "CLAUDE.md",
        fingerprint: "54881ae547640ac5c829fd36639a443c679e40c012d2e46a24e1a6a0ab577a2a",
    },
    EntrypointExpectation {
        surface: "GEMINI.md",
        fingerprint: "b87541fe0f153f8dfb25f638f721e47e9d1fa586bff2ecc63fc8f874b7b81ad8",
    },
    EntrypointExpectation {
        surface: "CODEX.md",
        fingerprint: "d49882f00ac748c6fbe8d1c80c73cb556916353433e64b99b34c0b36786538ca",
    },
];

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
    pub expected_binary_sha256: String,
    pub observed_binary_sha256: Option<String>,
    pub expected_payload_fingerprint: String,
    pub observed_payload_fingerprint: Option<String>,
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
            "Expected binary SHA-256: {}",
            self.expected_binary_sha256
        )?;
        writeln!(
            f,
            "Observed binary SHA-256: {}",
            self.observed_binary_sha256
                .as_deref()
                .unwrap_or("<missing>")
        )?;
        writeln!(
            f,
            "Expected payload fingerprint: {}",
            self.expected_payload_fingerprint
        )?;
        writeln!(
            f,
            "Observed payload fingerprint: {}",
            self.observed_payload_fingerprint
                .as_deref()
                .unwrap_or("<missing>")
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

pub fn expected_fingerprint(surface: &str) -> Option<&'static str> {
    EXPECTED_ENTRYPOINTS
        .iter()
        .find(|entry| entry.surface == surface)
        .map(|entry| entry.fingerprint)
}

pub fn fingerprint(payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn render_entrypoint(surface: &str) -> Option<String> {
    let payload = canonical_template(surface)?;
    let _ = expected_fingerprint(surface)?;
    Some(format!(
        "<!-- decapod-release: {RELEASE_VERSION} -->\n<!-- decapod-binary-sha256: {BINARY_SHA256} -->\n{payload}"
    ))
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
    observed_binary_sha256: Option<String>,
    observed_payload_fingerprint: Option<String>,
) -> Box<Finding> {
    Box::new(Finding {
        surface: surface.to_string(),
        running_release: RELEASE_VERSION.to_string(),
        declared_release,
        expected_binary_sha256: BINARY_SHA256.to_string(),
        observed_binary_sha256,
        expected_payload_fingerprint: expected_fingerprint(surface)
            .unwrap_or("<unknown>")
            .to_string(),
        observed_payload_fingerprint,
        kind,
    })
}

pub fn validate_entrypoint(project_root: &Path, surface: &str) -> Result<(), Box<Finding>> {
    let Some(expected) = expected_fingerprint(surface) else {
        return Err(finding(surface, FindingKind::Missing, None, None, None));
    };
    let path = project_root.join(surface);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(finding(surface, FindingKind::Missing, None, None, None));
        }
        Err(_) => {
            return Err(finding(
                surface,
                FindingKind::UnsupportedFileType,
                None,
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
            None,
        ));
    }

    let bytes = fs::read(&path)
        .map_err(|_| finding(surface, FindingKind::UnsupportedFileType, None, None, None))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| finding(surface, FindingKind::PayloadModified, None, None, None))?;
    let mut lines = content.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let second = lines.next().unwrap_or("");
    let payload_offset = first.len() + second.len();
    let payload = &content[payload_offset..];

    let release = marker_value(first, "<!-- decapod-release:");
    let declared_fingerprint = marker_value(second, "<!-- decapod-binary-sha256:");
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
            None,
        ));
    }
    if declared_release.is_none() || declared_fingerprint.is_none() {
        return Err(finding(
            surface,
            FindingKind::MetadataMissing,
            declared_release,
            declared_fingerprint,
            None,
        ));
    }

    let declared_release = declared_release.unwrap();
    let declared_fingerprint = declared_fingerprint.unwrap();
    let observed_payload_fingerprint = fingerprint(payload);
    if declared_release != RELEASE_VERSION {
        return Err(finding(
            surface,
            FindingKind::ReleaseMismatch,
            Some(declared_release),
            Some(declared_fingerprint),
            Some(observed_payload_fingerprint),
        ));
    }
    if declared_fingerprint != BINARY_SHA256 {
        return Err(finding(
            surface,
            FindingKind::FingerprintMismatch,
            Some(declared_release),
            Some(declared_fingerprint),
            Some(observed_payload_fingerprint),
        ));
    }

    let canonical = canonical_template(surface).unwrap_or_default();
    if payload != canonical || observed_payload_fingerprint != expected {
        return Err(finding(
            surface,
            FindingKind::PayloadModified,
            Some(declared_release),
            Some(BINARY_SHA256.to_string()),
            Some(observed_payload_fingerprint),
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
                fingerprint(&payload),
                expected_fingerprint(surface).expect("compiled fingerprint"),
                "compiled release manifest drifted from canonical {surface}"
            );
        }
    }

    #[test]
    fn release_manifest_matches_package_release() {
        let mut release_contract = format!("decapod-release:{RELEASE_VERSION}\n");
        for entry in EXPECTED_ENTRYPOINTS {
            release_contract.push_str(entry.surface);
            release_contract.push(':');
            release_contract.push_str(entry.fingerprint);
            release_contract.push('\n');
        }
        assert_eq!(fingerprint(&release_contract), BINARY_SHA256);
    }
}
