//! Gatling regression harness — runs all CLI code-path tests.
//!
//! This is a thin wrapper around `cargo test --test gatling`. The actual
//! test cases live in `tests/gatling.rs` as standard `#[test]` functions.

use crate::core::error::DecapodError;
use clap::Parser;
use std::process::Command;

#[derive(Parser, Debug)]
#[clap(
    name = "gatling",
    about = "Run gatling regression test across all CLI code paths"
)]
pub struct GatlingCli {
    /// Output format: 'text' (default) or 'json' (cargo test --format json).
    #[clap(long, default_value = "text")]
    pub format: String,

    /// Filter: only run tests matching this string.
    #[clap(long)]
    pub filter: Option<String>,

    /// Pass --nocapture to see test stdout.
    #[clap(long)]
    pub nocapture: bool,
}

pub fn run_gatling_cli(cli: &GatlingCli) -> Result<(), DecapodError> {
    // Find the project root by locating Cargo.toml relative to the running binary.
    // When installed via `cargo install`, we need the source checkout to run tests.
    // When running from the repo, current_exe's ancestor has Cargo.toml.
    let manifest_dir = find_manifest_dir()?;

    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .arg("--test")
        .arg("gatling")
        .arg("--manifest-path")
        .arg(manifest_dir.join("Cargo.toml"))
        .arg("--");

    if let Some(ref filter) = cli.filter {
        cmd.arg(filter);
    }

    if cli.nocapture {
        cmd.arg("--nocapture");
    }

    if cli.format == "json" {
        cmd.args(["--format", "json", "-Z", "unstable-options"]);
    }

    let status = cmd.status().map_err(DecapodError::IoError)?;

    if !status.success() {
        return Err(DecapodError::ValidationError(format!(
            "Gatling tests failed (exit {})",
            status.code().unwrap_or(-1)
        )));
    }

    Ok(())
}

fn find_manifest_dir() -> Result<std::path::PathBuf, DecapodError> {
    // Try current_exe's ancestors first (works when running from source)
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if d.join("Cargo.toml").exists() {
                return Ok(d);
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Fallback: walk up from cwd
    let mut cwd = std::env::current_dir().map_err(DecapodError::IoError)?;
    loop {
        if cwd.join("Cargo.toml").exists() {
            return Ok(cwd);
        }
        if !cwd.pop() {
            break;
        }
    }

    Err(DecapodError::NotFound(
        "Cannot find Cargo.toml — gatling tests require the decapod source checkout.".into(),
    ))
}
