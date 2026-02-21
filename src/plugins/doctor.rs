//! Doctor: Read-only preflight health checks.
//!
//! Performs non-destructive diagnostic checks on the Decapod workspace:
//! - Git status and configuration
//! - Required files and directories
//! - Database presence and accessibility
//! - Configuration validation
//! - Version and toolchain checks

use crate::core::error::DecapodError;
use crate::core::migration;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::path::Path;

#[derive(Parser, Debug)]
pub struct DoctorCli {
    #[clap(subcommand)]
    pub command: DoctorCommand,
}

#[derive(Subcommand, Debug)]
pub enum DoctorCommand {
    /// Run all preflight checks
    Check {
        /// Output format: 'text' or 'json'
        #[clap(long, default_value = "text")]
        format: String,
    },
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
    pub passed: usize,
    pub failed: usize,
    pub warnings: usize,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Warn,
}

pub fn run_doctor_cli(
    store: &Store,
    project_root: &Path,
    cli: DoctorCli,
) -> Result<(), DecapodError> {
    match cli.command {
        DoctorCommand::Check { format } => {
            let report = run_preflight_checks(store, project_root)?;

            if format == "json" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .map_err(|e| DecapodError::ValidationError(e.to_string()))?
                );
            } else {
                println!("Decapod Doctor — Preflight Checks\n");
                for check in &report.checks {
                    let icon = match check.status {
                        CheckStatus::Pass => "PASS",
                        CheckStatus::Fail => "FAIL",
                        CheckStatus::Warn => "WARN",
                    };
                    println!("  [{}] {}: {}", icon, check.name, check.message);
                }
                println!(
                    "\nSummary: {} passed, {} failed, {} warnings",
                    report.passed, report.failed, report.warnings
                );
            }

            if report.failed > 0 {
                return Err(DecapodError::ValidationError(format!(
                    "Doctor: {} check(s) failed",
                    report.failed
                )));
            }
        }
    }
    Ok(())
}

fn run_preflight_checks(store: &Store, project_root: &Path) -> Result<DoctorReport, DecapodError> {
    let mut checks = Vec::new();

    // 1. Git status
    checks.push(check_git_status(project_root));

    // 2. Required files
    checks.extend(check_required_files(project_root));

    // 3. .decapod directory
    checks.push(check_decapod_dir(project_root));

    // 4. Database files
    checks.extend(check_databases(&store.root));

    // 5. Version check
    checks.push(check_version());

    // 6. Rust toolchain
    checks.push(check_rust_toolchain(project_root));

    // 7. Config validation
    checks.push(check_config(project_root));

    let passed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Pass)
        .count();
    let failed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Fail)
        .count();
    let warnings = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Warn)
        .count();

    Ok(DoctorReport {
        checks,
        passed,
        failed,
        warnings,
    })
}

fn check_git_status(project_root: &Path) -> CheckResult {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_root)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let changed_files = stdout.lines().count();
            if changed_files == 0 {
                CheckResult {
                    name: "Git Status".to_string(),
                    status: CheckStatus::Pass,
                    message: "Working tree clean".to_string(),
                }
            } else {
                CheckResult {
                    name: "Git Status".to_string(),
                    status: CheckStatus::Warn,
                    message: format!("{} uncommitted change(s)", changed_files),
                }
            }
        }
        Ok(o) => CheckResult {
            name: "Git Status".to_string(),
            status: CheckStatus::Fail,
            message: format!(
                "git status failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
        },
        Err(e) => CheckResult {
            name: "Git Status".to_string(),
            status: CheckStatus::Fail,
            message: format!("git not available: {}", e),
        },
    }
}

fn check_required_files(project_root: &Path) -> Vec<CheckResult> {
    let required = [
        ("AGENTS.md", true),
        ("CLAUDE.md", true),
        ("Cargo.toml", false), // Only required for Rust projects
    ];

    required
        .iter()
        .map(|(file, is_required)| {
            let path = project_root.join(file);
            if path.is_file() {
                CheckResult {
                    name: format!("File: {}", file),
                    status: CheckStatus::Pass,
                    message: "Present".to_string(),
                }
            } else if *is_required {
                CheckResult {
                    name: format!("File: {}", file),
                    status: CheckStatus::Fail,
                    message: "Missing (required)".to_string(),
                }
            } else {
                CheckResult {
                    name: format!("File: {}", file),
                    status: CheckStatus::Warn,
                    message: "Missing (optional)".to_string(),
                }
            }
        })
        .collect()
}

fn check_decapod_dir(project_root: &Path) -> CheckResult {
    let decapod_dir = project_root.join(".decapod");
    if decapod_dir.is_dir() {
        let data_dir = decapod_dir.join("data");
        if data_dir.is_dir() {
            CheckResult {
                name: ".decapod".to_string(),
                status: CheckStatus::Pass,
                message: ".decapod/data directory present".to_string(),
            }
        } else {
            CheckResult {
                name: ".decapod".to_string(),
                status: CheckStatus::Fail,
                message: ".decapod exists but data/ subdirectory missing".to_string(),
            }
        }
    } else {
        CheckResult {
            name: ".decapod".to_string(),
            status: CheckStatus::Fail,
            message: "Not initialized (run `decapod init`)".to_string(),
        }
    }
}

fn check_databases(data_root: &Path) -> Vec<CheckResult> {
    let expected_dbs = [
        ("todo.db", true),
        ("governance.db", true),
        ("memory.db", false),
        ("automation.db", false),
    ];

    expected_dbs
        .iter()
        .map(|(db_name, is_required)| {
            let db_path = data_root.join(db_name);
            if db_path.is_file() {
                // Try opening the DB to verify it's accessible
                match crate::db::db_connect_for_validate(&db_path.to_string_lossy()) {
                    Ok(_) => CheckResult {
                        name: format!("DB: {}", db_name),
                        status: CheckStatus::Pass,
                        message: "Present and accessible".to_string(),
                    },
                    Err(e) => CheckResult {
                        name: format!("DB: {}", db_name),
                        status: CheckStatus::Fail,
                        message: format!("Present but not accessible: {}", e),
                    },
                }
            } else if *is_required {
                CheckResult {
                    name: format!("DB: {}", db_name),
                    status: CheckStatus::Warn,
                    message: "Not found (will be created on first use)".to_string(),
                }
            } else {
                CheckResult {
                    name: format!("DB: {}", db_name),
                    status: CheckStatus::Pass,
                    message: "Not found (optional)".to_string(),
                }
            }
        })
        .collect()
}

fn check_version() -> CheckResult {
    CheckResult {
        name: "Version".to_string(),
        status: CheckStatus::Pass,
        message: format!("decapod v{}", migration::DECAPOD_VERSION),
    }
}

fn check_rust_toolchain(project_root: &Path) -> CheckResult {
    if !project_root.join("Cargo.toml").exists() {
        return CheckResult {
            name: "Rust Toolchain".to_string(),
            status: CheckStatus::Pass,
            message: "Not a Rust project (skipped)".to_string(),
        };
    }

    let output = std::process::Command::new("rustc")
        .arg("--version")
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let version = String::from_utf8_lossy(&o.stdout).trim().to_string();
            CheckResult {
                name: "Rust Toolchain".to_string(),
                status: CheckStatus::Pass,
                message: version,
            }
        }
        _ => CheckResult {
            name: "Rust Toolchain".to_string(),
            status: CheckStatus::Fail,
            message: "rustc not available".to_string(),
        },
    }
}

fn check_config(project_root: &Path) -> CheckResult {
    let config_path = project_root.join(".decapod").join("config.toml");
    if config_path.is_file() {
        match std::fs::read_to_string(&config_path) {
            Ok(content) => match content.parse::<toml::Table>() {
                Ok(_) => CheckResult {
                    name: "Config".to_string(),
                    status: CheckStatus::Pass,
                    message: ".decapod/config.toml is valid TOML".to_string(),
                },
                Err(e) => CheckResult {
                    name: "Config".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Invalid TOML: {}", e),
                },
            },
            Err(e) => CheckResult {
                name: "Config".to_string(),
                status: CheckStatus::Fail,
                message: format!("Cannot read config: {}", e),
            },
        }
    } else {
        CheckResult {
            name: "Config".to_string(),
            status: CheckStatus::Pass,
            message: "No config file (using defaults)".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_check_version() {
        let result = check_version();
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.starts_with("decapod v"));
    }

    #[test]
    fn test_check_decapod_dir_missing() {
        let tmp = tempdir().unwrap();
        let result = check_decapod_dir(tmp.path());
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_check_decapod_dir_present() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".decapod/data")).unwrap();
        let result = check_decapod_dir(tmp.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_required_files() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "# Agents").unwrap();
        let results = check_required_files(tmp.path());
        assert_eq!(results[0].status, CheckStatus::Pass); // AGENTS.md present
        assert_eq!(results[1].status, CheckStatus::Fail); // CLAUDE.md missing
    }
}
