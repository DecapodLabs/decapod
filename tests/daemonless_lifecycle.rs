use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn resolve_decapod_bin() -> std::path::PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(p) = std::path::Path::new(cargo_bin).canonicalize() {
        return p;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let p = std::path::Path::new(&runfiles_dir).join("_main").join("decapod");
        if p.exists() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("decapod");
            if p.exists() {
                return p;
            }
        }
    }
    std::path::PathBuf::from(cargo_bin)
}

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    let exe = resolve_decapod_bin();
    Command::new(exe)
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let init = run_decapod(&dir, &["init", "--force"]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    (tmp, dir)
}

fn running_decapod_pids(exe_path: &str) -> Vec<u32> {
    let out = Command::new("ps")
        .args(["-eo", "pid=,args="])
        .output()
        .expect("ps output");
    assert!(out.status.success(), "ps command failed");
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let mut parts = trimmed.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let args = parts.collect::<Vec<_>>().join(" ");
            if args.contains(exe_path) {
                Some(pid)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn decapod_has_no_lingering_background_process() {
    let (_tmp, dir) = setup_repo();
    let exe_path = env!("CARGO_BIN_EXE_decapod");

    let before = running_decapod_pids(exe_path);

    for args in [
        vec!["system", "version"],
        vec!["capabilities", "--format", "json"],
        vec!["docs", "show", "docs/agent/api-index.md"],
    ] {
        let out = run_decapod(&dir, &args);
        assert!(
            out.status.success(),
            "decapod command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    thread::sleep(Duration::from_millis(500));
    let after = running_decapod_pids(exe_path);

    if before != after {
        println!("before: {:?}", before);
        println!("after: {:?}", after);
        for pid in &after {
            if !before.contains(pid) {
                let out = Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "args="])
                    .output();
                if let Ok(o) = out {
                    println!(
                        "lingering pid {} args: {}",
                        pid,
                        String::from_utf8_lossy(&o.stdout).trim()
                    );
                }
            }
        }
    }

    assert_eq!(
        before, after,
        "daemonless contract violated: unexpected lingering decapod process(es)"
    );
}
