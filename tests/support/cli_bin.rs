//! Shared decapod test binary and fixture helpers for CLI/entrypoint integration targets.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

pub fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = std::path::Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = std::path::Path::new(&runfiles_dir)
            .join("_main")
            .join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|path| path.to_path_buf()))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

pub fn resolve_repo_root() -> PathBuf {
    let cargo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if cargo_root.join("AGENTS.md").is_file() {
        return cargo_root;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let runfiles_root = std::path::Path::new(&runfiles_dir).join("_main");
        if runfiles_root.join("AGENTS.md").is_file() {
            return runfiles_root;
        }
    }
    cargo_root
}

pub fn contract_fixture() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("decapod-cli-contract-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().expect("private CLI contract fixture")
}

pub fn run_decapod(temp_dir: &PathBuf, args: &[&str]) -> (bool, String) {
    run_decapod_with_env(temp_dir, args, &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")])
}

pub fn run_decapod_with_env(
    temp_dir: &PathBuf,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (bool, String) {
    let output = run_raw(temp_dir, args, envs);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{stdout}\n{stderr}"))
}

pub fn run_raw(temp_dir: &PathBuf, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(resolve_decapod_bin());
    cmd.env_remove("GITHUB_ACTIONS").env_remove("BUILDKITE");
    cmd.current_dir(temp_dir).args(args);
    let mut has_xdg = false;
    for (k, v) in envs {
        cmd.env(k, v);
        if *k == "XDG_CONFIG_HOME" {
            has_xdg = true;
        }
    }
    if !has_xdg {
        cmd.env("XDG_CONFIG_HOME", temp_dir.join(".config"));
    }
    cmd.output().expect("Failed to execute decapod")
}

pub fn acquire_session(temp_path: &PathBuf) {
    let (success, output) = run_decapod(temp_path, &["session", "acquire"]);
    assert!(
        success,
        "decapod session acquire should succeed. Output:\n{output}"
    );
}

pub fn extract_password(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Password: ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}
