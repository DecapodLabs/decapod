//! Gatekeeper Safety Gates
//!
//! Provides validation gates for workspace safety:
//! - Path allowlist/blocklist enforcement
//! - Diff size ceiling
//! - Secret scanning
//! - Dangerous pattern detection

use regex::Regex;
use std::path::{Path, PathBuf};
use crate::core::error;

/// Gatekeeper configuration
#[derive(Debug, Clone)]
pub struct GatekeeperConfig {
    /// Maximum allowed diff size in bytes
    pub max_diff_bytes: u64,
    /// Paths that are allowed
    pub allow_paths: Vec<String>,
    /// Paths that are blocked
    pub block_paths: Vec<String>,
    /// Enable secret scanning
    pub scan_secrets: bool,
    /// Enable dangerous pattern detection
    pub scan_dangerous_patterns: bool,
}

impl Default for GatekeeperConfig {
    fn default() -> Self {
        Self {
            max_diff_bytes: 10 * 1024 * 1024, // 10MB default
            allow_paths: vec!["*".to_string()], // Allow all by default
            block_paths: vec![
                ".env".to_string(),
                ".env.*".to_string(),
                "**/secrets/**".to_string(),
                "**/.credentials".to_string(),
            ],
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
}

impl std::fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathBlocked => write!(f, "Path blocked"),
            Self::DiffTooLarge => write!(f, "Diff too large"),
            Self::SecretDetected => write!(f, "Secret detected"),
            Self::DangerousPattern => write!(f, "Dangerous pattern"),
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
        
        // Check blocklist first
        for pattern in &config.block_paths {
            if glob_match(pattern, &path_str) {
                violations.push(Violation {
                    kind: ViolationKind::PathBlocked,
                    path: path.clone(),
                    line: None,
                    message: format!("Path matches blocked pattern: {}", pattern),
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
        violations.extend(scan_for_dangerous_patterns(repo_root, paths)?);
    }

    let passed = violations.is_empty();
    Ok(GateResult { passed, violations })
}

/// Scan files for secrets
fn scan_for_secrets(
    repo_root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<Violation>, error::DecapodError> {
    let patterns = secret_patterns();
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

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                if pattern.is_match(line) {
                    violations.push(Violation {
                        kind: ViolationKind::SecretDetected,
                        path: path.clone(),
                        line: Some(line_num + 1),
                        message: format!("Potential secret detected: {}", pattern),
                    });
                }
            }
        }
    }

    Ok(violations)
}

/// Scan files for dangerous patterns
fn scan_for_dangerous_patterns(
    repo_root: &Path,
    paths: &[PathBuf],
) -> Result<Vec<Violation>, error::DecapodError> {
    let patterns = dangerous_patterns();
    let mut violations = Vec::new();

    // Only scan code files
    let code_extensions = ["rs", "py", "js", "ts", "sh", "bash", "zsh"];
    
    for path in paths {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !code_extensions.contains(&ext) {
            continue;
        }

        let full_path = repo_root.join(path);
        if !full_path.exists() || !full_path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            for pattern in &patterns {
                if pattern.is_match(line) {
                    violations.push(Violation {
                        kind: ViolationKind::DangerousPattern,
                        path: path.clone(),
                        line: Some(line_num + 1),
                        message: format!("Dangerous pattern detected: {}", pattern),
                    });
                }
            }
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
        // Bearer tokens
        Regex::new(r#"(?i)bearer\s+[a-zA-Z0-9_\-\.]+"#).unwrap(),
        // GitHub tokens
        Regex::new(r#"(ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9_]{36,255}"#).unwrap(),
        // Generic secrets
        Regex::new(r#"(?i)(password|passwd|pwd)['"]?\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#).unwrap(),
        // Private keys
        Regex::new(r#"-----BEGIN (RSA |DSA |EC |OPENSSH )?PRIVATE KEY-----"#).unwrap(),
        // Connection strings
        Regex::new(r#"(?i)(postgres|mysql|mongodb|redis)://[^\s'"]+:[^\s'"]+@[^\s'"]+"#).unwrap(),
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
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "foo"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("**/.credentials", "foo/bar/.credentials"));
        assert!(glob_match("src/**", "src/lib.rs"));
        assert!(glob_match(".env*", ".env.local"));
    }

    #[test]
    fn test_secret_patterns() {
        let patterns = secret_patterns();
        
        // AWS key
        let line = "AWS_KEY=AKIAIOSFODNN7EXAMPLE";
        assert!(patterns.iter().any(|p| p.is_match(line)));
        
        // GitHub token
        let line = "token=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        assert!(patterns.iter().any(|p| p.is_match(line)));
        
        // Private key
        let line = "-----BEGIN PRIVATE KEY-----";
        assert!(patterns.iter().any(|p| p.is_match(line)));
    }

    #[test]
    fn test_dangerous_patterns() {
        let patterns = dangerous_patterns();
        
        // eval with variable
        let line = "eval $CMD";
        assert!(patterns.iter().any(|p| p.is_match(line)));
        
        // shell=True
        let line = "subprocess.run(cmd, shell=True)";
        assert!(patterns.iter().any(|p| p.is_match(line)));
    }

    #[test]
    fn test_gatekeeper_default_config() {
        let config = GatekeeperConfig::default();
        assert!(config.scan_secrets);
        assert!(config.scan_dangerous_patterns);
        assert!(!config.block_paths.is_empty());
    }
}
