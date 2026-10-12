//! One explicitly scoped process-wide signal bridge. The handler touches only
//! static lock-free atomics; no heap pointer, allocation, lock, thread or daemon.
//! Applications install this at a public invocation boundary, never in a generic
//! subprocess helper. Concurrent/nested guard creation is rejected.
use super::Cancellation;
use std::io;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(unix)]
static OWNED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static SIGNALLED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static GENERATION: AtomicUsize = AtomicUsize::new(0);

pub(super) fn is_cancelled(generation: usize) -> bool {
    #[cfg(unix)]
    {
        GENERATION.load(Ordering::SeqCst) == generation && SIGNALLED.load(Ordering::SeqCst)
    }
    #[cfg(not(unix))]
    {
        let _ = generation;
        false
    }
}
#[cfg(unix)]
extern "C" fn cancel_signal(_: libc::c_int) {
    SIGNALLED.store(true, Ordering::SeqCst);
}
/// Owns the prior SIGINT/SIGTERM dispositions until explicitly restored or
/// dropped. Keep this alive through collection and any bounded cleanup. It does
/// not authorize any action. Drop cancels retained clones and restores handlers;
/// `restore()` additionally reports a rare OS restoration error to the caller.
pub struct SignalCancellationGuard {
    cancellation: Cancellation,
    #[cfg(unix)]
    previous_int: libc::sigaction,
    #[cfg(unix)]
    previous_term: libc::sigaction,
    #[cfg(unix)]
    installed_int: bool,
    #[cfg(unix)]
    installed_term: bool,
    #[cfg(unix)]
    owns_scope: bool,
}
impl SignalCancellationGuard {
    pub fn new() -> io::Result<Self> {
        #[cfg(not(unix))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "scoped signal cancellation requires Unix",
            ))
        }
        #[cfg(unix)]
        {
            OWNED
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "signal cancellation scope already active",
                    )
                })?;
            let generation =
                match GENERATION
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                {
                    Ok(previous) => previous + 1,
                    Err(_) => {
                        OWNED.store(false, Ordering::SeqCst);
                        return Err(io::Error::other("signal cancellation generation exhausted"));
                    }
                };
            SIGNALLED.store(false, Ordering::SeqCst);
            // Zero initialization is valid for sigaction storage and is replaced
            // by the kernel's complete prior disposition on successful install.
            let mut guard = Self {
                cancellation: Cancellation {
                    local: Default::default(),
                    signal_generation: Some(generation),
                },
                previous_int: unsafe { std::mem::zeroed() },
                previous_term: unsafe { std::mem::zeroed() },
                installed_int: false,
                installed_term: false,
                owns_scope: true,
            };
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = cancel_signal as *const () as usize;
            action.sa_flags = 0;
            if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0
                || unsafe { libc::sigaddset(&mut action.sa_mask, libc::SIGINT) } != 0
                || unsafe { libc::sigaddset(&mut action.sa_mask, libc::SIGTERM) } != 0
            {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::sigaction(libc::SIGINT, &action, &mut guard.previous_int) } != 0 {
                return Err(io::Error::last_os_error());
            }
            guard.installed_int = true;
            if unsafe { libc::sigaction(libc::SIGTERM, &action, &mut guard.previous_term) } != 0 {
                return Err(io::Error::last_os_error());
            }
            guard.installed_term = true;
            Ok(guard)
        }
    }
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }
    pub fn restore(&mut self) -> io::Result<()> {
        self.cancellation.cancel();
        #[cfg(unix)]
        {
            let mut error = None;
            if self.installed_term {
                if unsafe {
                    libc::sigaction(libc::SIGTERM, &self.previous_term, std::ptr::null_mut())
                } == 0
                {
                    self.installed_term = false;
                } else {
                    error = Some(io::Error::last_os_error());
                }
            }
            if self.installed_int {
                if unsafe {
                    libc::sigaction(libc::SIGINT, &self.previous_int, std::ptr::null_mut())
                } == 0
                {
                    self.installed_int = false;
                } else if error.is_none() {
                    error = Some(io::Error::last_os_error());
                }
            }
            if self.owns_scope && !self.installed_int && !self.installed_term {
                self.owns_scope = false;
                OWNED.store(false, Ordering::SeqCst);
            }
            if let Some(error) = error {
                return Err(error);
            }
        }
        Ok(())
    }
}
impl Drop for SignalCancellationGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::{BoundedCommand, ProcessLimits, Termination};
    use super::*;
    use std::{
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };
    const ROLE: &str = "DECAPOD_SCOPED_SIGNAL_TEST_ROLE";
    const DIR: &str = "DECAPOD_SCOPED_SIGNAL_TEST_DIR";
    fn command(role: &str, dir: &Path) -> Command {
        let mut c = Command::new(std::env::current_exe().unwrap());
        let test_name = format!(
            "{}::isolated_signal_bridge_kills_and_reaps_owned_group",
            module_path!().split_once("::").unwrap().1
        );
        c.args(["--exact", test_name.as_str(), "--nocapture"])
            .env(ROLE, role)
            .env(DIR, dir);
        c
    }
    fn pid(path: &Path) -> i32 {
        std::fs::read_to_string(path)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
    fn wait_file(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(Instant::now() < deadline, "fixture failed to start");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn disposition(signal: i32) -> libc::sigaction {
        let mut old = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::sigaction(signal, std::ptr::null(), &mut old) },
            0
        );
        old
    }
    #[test]
    fn isolated_signal_bridge_kills_and_reaps_owned_group() {
        if let Ok(role) = std::env::var(ROLE) {
            let dir = std::path::PathBuf::from(std::env::var_os(DIR).unwrap());
            if role == "sleeper" {
                std::thread::sleep(Duration::from_secs(60));
                return;
            }
            if role == "leader" {
                let mut descendant = command("sleeper", &dir).spawn().unwrap();
                std::fs::write(dir.join("descendant"), descendant.id().to_string()).unwrap();
                std::fs::write(dir.join("leader"), std::process::id().to_string()).unwrap();
                // Park instead of racing the controller to reap the descendant
                // when the process group is killed. If cancellation regresses,
                // the outer isolated controller still has a finite deadline.
                std::thread::sleep(Duration::from_secs(60));
                let _ = descendant.wait();
                return;
            }
            assert_eq!(role, "controller");
            // Linux's isolated fixture adopts the descendant so both statuses
            // can be reaped and checked, without touching the main test process.
            #[cfg(target_os = "linux")]
            assert_eq!(
                unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
                0
            );
            let before_int = disposition(libc::SIGINT);
            let before_term = disposition(libc::SIGTERM);
            let mut guard = SignalCancellationGuard::new().unwrap();
            assert_eq!(
                SignalCancellationGuard::new().err().unwrap().kind(),
                io::ErrorKind::AlreadyExists
            );
            let clone = guard.cancellation().clone();
            let out = command("leader", &dir)
                .bounded_capture(
                    ProcessLimits {
                        timeout: Duration::from_secs(10),
                        max_output_bytes: 1024 * 1024,
                    },
                    guard.cancellation(),
                )
                .unwrap();
            assert_eq!(out.termination, Termination::Cancelled);
            assert!(clone.is_cancelled());
            let leader = pid(&dir.join("leader"));
            let mut status = 0;
            assert_eq!(
                unsafe { libc::waitpid(leader, &mut status, libc::WNOHANG) },
                -1
            );
            assert_eq!(
                io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
            #[cfg(target_os = "linux")]
            {
                let descendant = pid(&dir.join("descendant"));
                assert_eq!(
                    unsafe { libc::waitpid(descendant, &mut status, 0) },
                    descendant
                );
                assert!(libc::WIFSIGNALED(status));
                assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
            }
            guard.restore().unwrap();
            drop(guard);
            assert!(clone.is_cancelled());
            assert_eq!(
                disposition(libc::SIGINT).sa_sigaction,
                before_int.sa_sigaction
            );
            assert_eq!(
                disposition(libc::SIGTERM).sa_sigaction,
                before_term.sa_sigaction
            );
            let fresh = SignalCancellationGuard::new().unwrap();
            assert!(!fresh.cancellation().is_cancelled());
            drop(fresh);
            std::fs::write(
                dir.join("finished"),
                "group killed, leader reaped, handlers restored",
            )
            .unwrap();
            return;
        }
        for signal in [libc::SIGINT, libc::SIGTERM] {
            let dir = tempfile::tempdir().unwrap();
            let mut controller = command("controller", dir.path()).spawn().unwrap();
            wait_file(&dir.path().join("leader"));
            wait_file(&dir.path().join("descendant"));
            assert_eq!(unsafe { libc::kill(controller.id() as i32, signal) }, 0);
            let deadline = Instant::now() + Duration::from_secs(8);
            let status = loop {
                if let Some(status) = controller.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    // Only PIDs of this freshly spawned fixture are used here.
                    unsafe {
                        libc::kill(-pid(&dir.path().join("leader")), libc::SIGKILL);
                    }
                    let _ = controller.kill();
                    let _ = controller.wait();
                    panic!("signal bridge fixture timed out");
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(status.success());
            assert!(dir.path().join("finished").exists());
        }
    }
}
