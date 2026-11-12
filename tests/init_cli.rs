//! Exercise the real first-run journey through a controlling terminal.
#![cfg(unix)]

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn binary() -> PathBuf {
    let cargo = PathBuf::from(env!("CARGO_BIN_EXE_decapod"));
    if let Ok(path) = cargo.canonicalize() {
        return path;
    }
    if let Ok(runfiles) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles).join("_main/decapod");
        if path.exists() {
            return path;
        }
    }
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("decapod")
}

struct SetupTerminal {
    child: Child,
    master: File,
    slave: File,
    original_flags: libc::tcflag_t,
    output: String,
    _home: tempfile::TempDir,
}

impl SetupTerminal {
    fn start(directory: &Path, args: &[&str]) -> Self {
        let (mut master, mut slave) = (-1, -1);
        let size = libc::winsize {
            ws_row: 30,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let mut termios = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut termios) },
            0
        );
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert_ne!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            -1
        );
        let home = tempfile::tempdir().unwrap();
        let mut command = Command::new(binary());
        command
            .args(args)
            .current_dir(directory)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", home.path())
            .env("TERM", "xterm-256color")
            .env("NO_COLOR", "1")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self {
            child: command.spawn().unwrap(),
            master,
            slave,
            original_flags: termios.c_lflag,
            output: String::new(),
            _home: home,
        }
    }

    fn pump(&mut self) {
        let mut bytes = [0; 16384];
        loop {
            match self.master.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => self
                    .output
                    .push_str(&String::from_utf8_lossy(&bytes[..count])),
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.raw_os_error() == Some(libc::EIO) =>
                {
                    break;
                }
                Err(error) => panic!("PTY read: {error}"),
            }
        }
    }

    fn expect(&mut self, text: &str) {
        let start = self.output.len();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if self.output[start..].contains(text) {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "setup exited waiting for {text:?}: {}",
                self.output
            );
            assert!(
                Instant::now() < deadline,
                "setup stalled waiting for {text:?}: {}",
                self.output
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn send(&mut self, input: &str) {
        self.master.write_all(input.as_bytes()).unwrap();
    }

    fn finish(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if let Some(status) = self.child.try_wait().unwrap() {
                self.pump();
                let mut termios = unsafe { std::mem::zeroed() };
                assert_eq!(
                    unsafe { libc::tcgetattr(self.slave.as_raw_fd(), &mut termios) },
                    0
                );
                assert_eq!(
                    termios.c_lflag & (libc::ICANON | libc::ECHO),
                    self.original_flags & (libc::ICANON | libc::ECHO),
                    "terminal mode leaked"
                );
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "setup failed to terminate: {}",
                self.output
            );
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn recommended(&mut self) {
        self.expect("What are you building?");
        self.send("A quiet place to plan good work.\n");
        self.expect("How would you like to set up?");
        self.send("\r");
        self.expect("Ready when you are");
    }
}

impl Drop for SetupTerminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn recommended_setup_has_three_decisions_and_an_agent_handoff() {
    let project = tempfile::tempdir().unwrap();
    let mut terminal = SetupTerminal::start(project.path(), &["init", "--dir", "."]);
    terminal.recommended();
    assert!(
        !project.path().join(".git").exists(),
        "Git changed before review"
    );
    assert!(
        !project.path().join(".decapod").exists(),
        "governance changed before review"
    );
    terminal.send("\r");
    assert!(terminal.finish().success(), "{}", terminal.output);
    assert!(
        terminal
            .output
            .contains("Your project is ready for your agent.")
    );
    assert!(
        terminal.output.contains("Required validation has not run")
            || terminal.output.contains("does not run validation")
    );
    assert!(
        !terminal.output.contains("\x1b[1;36m"),
        "NO_COLOR was ignored"
    );
    assert!(!terminal.output.contains("DEBUG:"));
    let config = fs::read_to_string(project.path().join(".decapod/config.toml")).unwrap();
    assert!(config.contains("A quiet place to plan good work."));
    assert!(config.contains("backend = \"local\""));
    assert!(project.path().join("AGENTS.md").exists());
}

#[test]
fn edit_then_cancel_leaves_a_new_project_uncreated() {
    let root = tempfile::tempdir().unwrap();
    let mut terminal = SetupTerminal::start(root.path(), &["init"]);
    terminal.expect("Where would you like to begin?");
    terminal.send("Create a project\r");
    terminal.expect("Project directory");
    terminal.send("new-project\n");
    terminal.recommended();
    terminal.send("Edit choices\r");
    terminal.expect("Make it yours");
    terminal.send("Workspace\r");
    terminal.expect("Where should agent work run?");
    terminal.send("Git worktrees\r");
    terminal.expect("Make it yours");
    terminal.send("\r");
    terminal.expect("Ready when you are");
    terminal.send("Cancel\r");
    assert!(terminal.finish().success(), "{}", terminal.output);
    assert!(!root.path().join("new-project").exists());
    assert!(!root.path().join(".git").exists());
    assert!(terminal.output.contains("No setup changes were applied."));
}

#[test]
fn lone_escape_cancels_without_hanging_or_leaking_raw_mode() {
    let project = tempfile::tempdir().unwrap();
    let mut terminal = SetupTerminal::start(project.path(), &["init", "--dir", "."]);
    terminal.expect("What are you building?");
    terminal.send("\n");
    terminal.expect("How would you like to set up?");
    terminal.send("\x1b");
    assert!(!terminal.finish().success());
    assert!(terminal.output.contains("Setup cancelled"));
    assert!(!project.path().join(".decapod").exists());
    assert!(!project.path().join(".git").exists());
}

#[test]
fn preview_creates_neither_git_nor_governance_state() {
    let project = tempfile::tempdir().unwrap();
    let mut terminal = SetupTerminal::start(project.path(), &["init", "--dir", ".", "--dry-run"]);
    terminal.recommended();
    terminal.send("\r");
    assert!(terminal.finish().success(), "{}", terminal.output);
    assert!(terminal.output.contains("Preview complete."));
    assert_eq!(
        fs::read_dir(project.path()).unwrap().count(),
        0,
        "preview wrote project state"
    );
}
