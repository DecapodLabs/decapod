//! Gatekeeper Safety Gates
//!
//! Provides validation gates for workspace safety:
//! - Path allowlist/blocklist enforcement
//! - Diff size ceiling
//! - Secret scanning
//! - Dangerous pattern detection

use crate::core::error;
use fancy_regex::Regex;
use std::path::{Path, PathBuf};

mod runtime_credentials;
mod rust_context;
mod shell_context;
mod source_dependencies;
mod sql_context;
mod text_context;
use rust_context::{PasswordContext, RustContext};

/// Gatekeeper configuration
#[derive(Debug, Clone)]
pub struct GatekeeperConfig {
    /// Maximum allowed diff size in bytes
    pub max_diff_bytes: u64,
    /// Paths that are allowed
    pub allow_paths: Vec<String>,
    /// Paths that are blocked
    pub block_paths: Vec<String>,
    /// Repository-relative paths that require a protected-path finding
    pub protected_paths: Vec<String>,
    /// Enable secret scanning
    pub scan_secrets: bool,
    /// Enable dangerous pattern detection
    pub scan_dangerous_patterns: bool,
}

impl Default for GatekeeperConfig {
    fn default() -> Self {
        Self {
            max_diff_bytes: 10 * 1024 * 1024,   // 10MB default
            allow_paths: vec!["*".to_string()], // Allow all by default
            block_paths: vec![
                ".env".to_string(),
                ".env.*".to_string(),
                "**/secrets/**".to_string(),
                "**/.credentials".to_string(),
            ],
            protected_paths: Vec::new(),
            scan_secrets: true,
            scan_dangerous_patterns: true,
        }
    }
}

/// Gatekeeper check result
#[derive(Debug)]
pub struct GateResult {
    pub passed: bool,
    pub violations: Vec<Violation>,
}

/// Individual violation
#[derive(Debug)]
pub struct Violation {
    pub kind: ViolationKind,
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    PathBlocked,
    DiffTooLarge,
    SecretDetected,
    DangerousPattern,
    ProtectedPath,
}

impl std::fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathBlocked => write!(f, "Path blocked"),
            Self::DiffTooLarge => write!(f, "Diff too large"),
            Self::SecretDetected => write!(f, "Secret detected"),
            Self::DangerousPattern => write!(f, "Dangerous pattern"),
            Self::ProtectedPath => write!(f, "Protected path"),
        }
    }
}

/// Run all gatekeeper checks
pub fn run_gatekeeper(
    repo_root: &Path,
    paths: &[PathBuf],
    diff_bytes: u64,
    config: &GatekeeperConfig,
) -> Result<GateResult, error::DecapodError> {
    // Embedded native source is part of an explicit Rust scan. A missing or
    // dynamically constructed include must never turn that scan into a pass.
    let expanded_paths = source_dependencies::expand(repo_root, paths)?;
    let paths = expanded_paths.paths.as_slice();
    let mut violations = Vec::new();

    // Check diff size
    if diff_bytes > config.max_diff_bytes {
        violations.push(Violation {
            kind: ViolationKind::DiffTooLarge,
            path: PathBuf::from("."),
            line: None,
            message: format!(
                "Diff size {} bytes exceeds limit of {} bytes",
                diff_bytes, config.max_diff_bytes
            ),
        });
    }

    // Check paths
    for path in paths {
        let path_str = path.to_string_lossy();

        for pattern in &config.protected_paths {
            if glob_match(pattern, &path_str) {
                violations.push(Violation {
                    kind: ViolationKind::ProtectedPath,
                    path: path.clone(),
                    line: None,
                    message: format!("Path is protected by guided init policy: {pattern}"),
                });
            }
        }

        // Check blocklist first
        for pattern in &config.block_paths {
            if glob_match(pattern, &path_str) {
                violations.push(Violation {
                    kind: ViolationKind::PathBlocked,
                    path: path.clone(),
                    line: None,
                    message: format!("Path matches blocked pattern: {pattern}"),
                });
            }
        }
    }

    // Secret scanning
    if config.scan_secrets {
        violations.extend(scan_for_secrets(repo_root, paths)?);
    }

    // Dangerous pattern detection
    if config.scan_dangerous_patterns {
        violations.extend(scan_for_dangerous_patterns(
            repo_root,
            paths,
            &expanded_paths.included,
        )?);
    }

    let passed = violations.is_empty();
    Ok(GateResult { passed, violations })
}

impl GatekeeperConfig {
    pub fn from_repo_config(repo_root: &Path) -> Self {
        let mut config = Self::default();
        if let Ok(project) = crate::cli::DecapodProjectConfig::load(repo_root) {
            config.protected_paths = project.governance.protected_paths;
        }
        config
    }
}

/// Scan files for secrets
fn scan_for_secrets(
    repo_root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<Violation>, error::DecapodError> {
    let patterns = secret_patterns();
    let typed_password_patterns = typed_password_patterns();
    let mut violations = Vec::new();

    for path in paths {
        let full_path = repo_root.join(path);
        if !full_path.exists() || !full_path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse at most once, and only if a password-value candidate needs
        // context. Other secret families retain their independent matching.
        let mut context = None;
        let mut offset = 0;
        for (line_num, source_line) in content.split_inclusive('\n').enumerate() {
            let line = source_line.trim_end_matches(['\r', '\n']);
            for pattern in &patterns {
                // Evaluate every occurrence independently: a runtime field
                // cannot suppress a literal credential on the same line.
                let mut unresolved_format = false;
                let mut incomplete_match = false;
                let finding = pattern.captures_iter(line).any(|captures| {
                    let Ok(captures) = captures else {
                        incomplete_match = true;
                        return true;
                    };
                    let Some(value) = captures.name("password_value") else {
                        return true;
                    };
                    let context = context.get_or_insert_with(|| {
                        if path.extension().is_some_and(|ext| ext == "rs") {
                            RustContext::parse_file(repo_root, path, &content)
                        } else {
                            RustContext::default()
                        }
                    });
                    match context.password_value(offset + value.start()..offset + value.end()) {
                        PasswordContext::RuntimeEnvironment => false,
                        PasswordContext::UnresolvedFormat => {
                            unresolved_format = true;
                            true
                        }
                        PasswordContext::LiteralOrUnknown => true,
                    }
                });
                if finding {
                    violations.push(Violation {
                        kind: ViolationKind::SecretDetected,
                        path: path.clone(),
                        line: Some(line_num + 1),
                        message: if incomplete_match {
                            "Secret scan incomplete: pattern evaluation failed; explicit review is required".to_string()
                        } else if unresolved_format {
                            format!("Potential secret detected: {pattern}; format interpolation has unresolved credential provenance")
                        } else {
                            format!("Potential secret detected: {pattern}")
                        },
                    });
                }
            }
            offset += source_line.len();
        }

        // A Rust type annotation separates the binding name from its value,
        // and may span lines. Match these declarations over the complete
        // source without making successful Rust parsing a detection precondition.
        // Provenance can discharge only the exact initializer candidate, just
        // as for the ordinary assignment patterns above.
        for pattern in &typed_password_patterns {
            for captures in pattern.captures_iter(&content) {
                let captures = match captures {
                    Ok(captures) => captures,
                    Err(_) => {
                        violations.push(Violation {
                            kind: ViolationKind::SecretDetected,
                            path: path.clone(),
                            line: None,
                            message: "Secret scan incomplete: typed password pattern evaluation failed; explicit review is required".to_string(),
                        });
                        break;
                    }
                };
                let Some(value) = captures.name("password_value") else {
                    continue;
                };
                let context = context.get_or_insert_with(|| {
                    if path.extension().is_some_and(|ext| ext == "rs") {
                        RustContext::parse_file(repo_root, path, &content)
                    } else {
                        RustContext::default()
                    }
                });
                if context.password_value(value.start()..value.end())
                    == PasswordContext::RuntimeEnvironment
                {
                    continue;
                }
                violations.push(Violation {
                    kind: ViolationKind::SecretDetected,
                    path: path.clone(),
                    line: Some(
                        content[..value.start()]
                            .bytes()
                            .filter(|byte| *byte == b'\n')
                            .count()
                            + 1,
                    ),
                    message: format!(
                        "Potential secret detected in typed Rust password binding: {pattern}"
                    ),
                });
            }
        }
    }

    Ok(violations)
}

/// Scan files for dangerous patterns
fn scan_for_dangerous_patterns(
    repo_root: &Path,
    paths: &[PathBuf],
    included: &std::collections::BTreeSet<PathBuf>,
) -> Result<Vec<Violation>, error::DecapodError> {
    let patterns = dangerous_patterns();
    let mut violations = Vec::new();

    // Only scan code files
    let code_extensions = ["rs", "py", "js", "ts", "sh", "bash", "zsh"];

    for path in paths {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let dockerfile = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "Dockerfile" || name.starts_with("Dockerfile."));
        let full_path = repo_root.join(path);
        if !full_path.exists() || !full_path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // A dependency is source regardless of its suffix. Extensionless
        // shebang files are executable inputs too. Only a recognized native
        // grammar may discharge a match; other included text is scanned raw.
        if !code_extensions.contains(&ext)
            && !dockerfile
            && !included.contains(path)
            && !content.starts_with("#!")
        {
            continue;
        }

        let sql = if ext == "rs" {
            sql_context::SqlContext::parse_file(repo_root, path, &content)
        } else {
            sql_context::SqlContext::default()
        };
        let shell = shell_context::ShellContext::parse(path, &content);
        let inert_text = if ext == "rs" {
            text_context::ranges(repo_root, path, &content)
        } else {
            Vec::new()
        };
        let rust_execution = if ext == "rs" {
            rust_context::shell_execution_boundaries(&content)
        } else {
            Vec::new()
        };
        for boundary in shell.execution_boundaries().iter().chain(&rust_execution) {
            violations.push(Violation {
                kind: ViolationKind::DangerousPattern,
                path: path.clone(),
                line: content
                    .get(..boundary.start)
                    .map(|prefix| prefix.bytes().filter(|byte| *byte == b'\n').count() + 1),
                message: "Shell or unresolved interpreter boundary requires explicit review"
                    .to_string(),
            });
        }
        let mut offset = 0;
        for (line_num, source_line) in content.split_inclusive('\n').enumerate() {
            let line = source_line.trim_end_matches(['\r', '\n']);
            for pattern in &patterns {
                let finding = pattern.find_iter(line).any(|matched| {
                    let Ok(matched) = matched else { return true };
                    let source = offset + matched.start()..offset + matched.end();
                    !sql.is_safe(source.clone())
                        && !shell.is_safe(source.clone())
                        && !inert_text.iter().any(|literal| {
                            literal.start <= source.start && source.end <= literal.end
                        })
                });
                if finding {
                    violations.push(Violation {
                        kind: ViolationKind::DangerousPattern,
                        path: path.clone(),
                        line: Some(line_num + 1),
                        message: format!("Dangerous pattern detected: {pattern}"),
                    });
                }
            }
            offset += source_line.len();
        }
    }

    Ok(violations)
}

/// Secret detection patterns
fn secret_patterns() -> Vec<Regex> {
    vec![
        // AWS Access Key ID
        Regex::new(r#"(?i)(A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[0-9A-Z]{16}"#).unwrap(),
        // AWS Secret Access Key
        Regex::new(r#"(?i)aws(.{0,20})?['"][0-9a-zA-Z/+=]{40}['"]"#).unwrap(),
        // Generic API key patterns
        Regex::new(r#"(?i)(api[_-]?key|apikey|api_secret|secret[_-]?key)['"]?\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{20,}['"]?"#).unwrap(),
        // Bearer values start an authentication scheme, rather than appearing
        // as an arbitrary word inside prose. Preserve header/assignment,
        // quoted value, and standalone/comment forms, including short values.
        // Do not infer safety from a token's spelling, entropy, or file path.
        Regex::new(r#"(?i)(?:[:=]\s*["'`]?\s*|["'`]\s*|(?://[/!]?|/\*+)\s*|^\s*(?:[#*]\s*)?)bearer[ \t]+[a-zA-Z0-9_~+./-]+=*"#).unwrap(),
        // GitHub tokens
        Regex::new(r#"(ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9_]{36,255}"#).unwrap(),
        // A balanced quoted password is explicit credential syntax even if
        // its value is one character. Escapes cannot terminate the value and
        // hide a second same-line credential; genuinely empty values do not
        // establish a credential. Keep the broader legacy unquoted detector.
        Regex::new(r#"(?i)(?:password|passwd|pwd)(?:\\?['"])?\s*[:=]\s*(?P<password_quote>\\?['"])(?P<password_value>(?:(?!\k<password_quote>)(?:\\[^\r\n]|[^\\\r\n]))+)\k<password_quote>"#).unwrap(),
        // Unquoted equal-sign assignments are explicit value syntax too.
        // Keep comparisons/arrows and empty/quoted values out of this branch;
        // Rust type annotations use a colon and are not shortened by this rule.
        Regex::new(r#"(?i)(?:password|passwd|pwd)\s*=(?!=|>)\s*(?!\\?['"])(?P<password_value>[^\s'";,]+)"#).unwrap(),
        // Generic secrets
        Regex::new(r#"(?i)(password|passwd|pwd)['"]?\s*[:=]\s*['"]?(?P<password_value>[^\s'"]{8,})['"]?"#).unwrap(),
        // Private keys
        Regex::new(r#"-----BEGIN (RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----"#).unwrap(),
        // Connection strings
        Regex::new(r#"(?i)(postgres|mysql|mongodb|redis)://[^\s'"]+:[^\s'"]+@[^\s'"]+"#).unwrap(),
    ]
}

/// Additive detection of explicit Rust bindings, including newline-separated
/// type annotations. Do not consume another statement or a braced type body;
/// this is a bounded textual recognizer, not a replacement for the Rust parser.
fn typed_password_patterns() -> Vec<Regex> {
    // An array's length separator is not a statement separator. Recognize
    // simple arrays/slices as a balanced unit, with a literal or named length,
    // rather than allowing arbitrary semicolons in the annotation.
    let annotation = r"(?:[^\[\]=;{}]|\[[^\[\]=;{}]+(?:;\s*[A-Za-z0-9_:]+\s*)?\])+?";
    let binding = format!(
        r"(?i)\b(?:let\s+(?:ref\s+)?(?:mut\s+)?|const\s+|static\s+(?:mut\s+)?)(?:r#)?(?:password|passwd|pwd)\s*:\s*{annotation}\s*=(?!=|>)\s*"
    );
    vec![
        Regex::new(&format!(
            r#"{binding}(?P<password_quote>\\?['"])(?P<password_value>(?:(?!\k<password_quote>)(?:\\[\s\S]|[^\\]))+)\k<password_quote>"#
        )).unwrap(),
        Regex::new(&format!(
            r#"{binding}(?!\\?['"])(?P<password_value>[^\s'";,]+)"#
        )).unwrap(),
    ]
}

/// Dangerous code patterns
fn dangerous_patterns() -> Vec<Regex> {
    vec![
        // eval in shell
        Regex::new(r#"\beval\s+\$"#).unwrap(),
        // exec in Python
        Regex::new(r#"\bexec\s*\("#).unwrap(),
        // subprocess shell=True
        Regex::new(r#"subprocess\.[a-z]+\([^)]*shell\s*=\s*True"#).unwrap(),
        // Command injection patterns
        Regex::new(r#"\$\{[^}]+\}|\$\([^)]+\)"#).unwrap(),
        // Unquoted variables in shell commands (best effort)
        Regex::new(r#"\$\w+[^\s"']"#).unwrap(),
    ]
}

/// Simple glob match implementation
fn glob_match(pattern: &str, text: &str) -> bool {
    // Handle ** wildcard
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return (suffix.is_empty() || text.ends_with(suffix))
                && (prefix.is_empty() || text.starts_with(prefix));
        }
    }

    // Handle * wildcard (single level)
    if pattern.contains('*') && !pattern.contains("**") {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return text.starts_with(prefix) && text.ends_with(suffix);
        }
    }

    // Exact match
    pattern == text
}
#[cfg(test)]
#[path = "../../../tests/unit/core/gatekeeper_tests.rs"]
mod tests;
