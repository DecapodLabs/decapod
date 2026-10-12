
use std::{ffi::OsString, fs, os::unix::ffi::OsStrExt, process::Command};
fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    assert_eq!(&args[..2], &[OsString::from("docs"), OsString::from("build")]);
    let mut log = Vec::new();
    for arg in &args { log.extend(arg.as_bytes()); log.push(0); }
    fs::write(".fixture-argv", log).unwrap();
    let mode = fs::read_to_string(".fixture-mode").unwrap_or_default();
    if mode == "fail" { std::process::exit(37); }
    let touched = if args.len() == 2 { &[][..] } else {
        assert_eq!(args[2], "--touched");
        &args[3..]
    };
    let contracts = touched.is_empty() || touched.iter().any(|p| {
        let p = p.to_string_lossy();
        p.contains("src/decapod/cli.rs") || p.contains("src/decapod/core/docs_cli.rs") || p.contains("assets/schemas/trajectory.schema.json")
    });
    let schema = touched.is_empty() || touched.iter().any(|p| p.to_string_lossy().contains("src/decapod/cli.rs"));
    if contracts { fs::write("docs/agent/command-contracts.md", "generated contracts\n").unwrap(); }
    if schema { fs::write("docs/agent/config-schema.md", "generated schema\n").unwrap(); }
    if mode == "drift" || mode == "stage" || mode == "stage-restore" {
        fs::write("docs/agent/command-contracts.md", "drift\n").unwrap();
    }
    if mode == "stage" || mode == "stage-restore" {
        assert!(Command::new("git").args(["add", "--", "docs/agent/command-contracts.md"]).status().unwrap().success());
    }
    if mode == "stage-restore" { fs::write("docs/agent/command-contracts.md", "generated contracts\n").unwrap(); }
    if mode == "switch-head" {
        assert!(Command::new("git").args(["checkout", "--detach", "fixture-base"]).status().unwrap().success());
    }
}
