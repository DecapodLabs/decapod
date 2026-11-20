//! One synchronous subprocess owner, with bounded nonblocking output capture.
//!
//! No pipe-drain thread, background watchdog, shell expansion or global signal
//! handler by default. SignalCancellationGuard opts one public invocation into
//! a scoped SIGINT/SIGTERM bridge. Unix
//! process groups are mandatory: unsupported platforms fail before spawning.
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

// This module is also included by the separate native Laya workspace.
#[path = "bounded_process/signal.rs"]
mod signal;
pub use signal::SignalCancellationGuard;

pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const RECOMMENDED_NOFILE_CEILING: u64 = 65_536;
pub(crate) const LOW_NOFILE_WARNING_THRESHOLD: u64 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NofileLimits {
    pub soft: u64,
    pub hard: Option<u64>,
}

impl NofileLimits {
    pub(crate) fn hard_display(&self) -> String {
        self.hard
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unlimited".to_string())
    }
}

pub(crate) fn current_nofile_limits() -> io::Result<NofileLimits> {
    #[cfg(unix)]
    {
        let mut rlim = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) } != 0 {
            return Err(io::Error::last_os_error());
        }
        #[allow(clippy::unnecessary_cast)]
        let hard = if rlim.rlim_max == libc::RLIM_INFINITY {
            None
        } else {
            Some(rlim.rlim_max as u64)
        };
        #[allow(clippy::unnecessary_cast)]
        Ok(NofileLimits {
            soft: rlim.rlim_cur as u64,
            hard,
        })
    }
    #[cfg(not(unix))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "RLIMIT_NOFILE inspection requires a Unix adapter",
        ))
    }
}

#[cfg(unix)]
pub(crate) fn target_nofile_soft_limit(current: NofileLimits, ceiling: u64) -> Option<u64> {
    let bounded = match current.hard {
        Some(hard) => ceiling.min(hard),
        None => ceiling,
    };
    (bounded > current.soft).then_some(bounded)
}

#[cfg(unix)]
pub(crate) fn raise_nofile_soft_limit_in_child(ceiling: u64) {
    let mut rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) } != 0 {
        return;
    }
    #[allow(clippy::unnecessary_cast)]
    let hard = if rlim.rlim_max == libc::RLIM_INFINITY {
        None
    } else {
        Some(rlim.rlim_max as u64)
    };
    #[allow(clippy::unnecessary_cast)]
    let current = NofileLimits {
        soft: rlim.rlim_cur as u64,
        hard,
    };
    let Some(primary) = target_nofile_soft_limit(current, ceiling) else {
        return;
    };
    // Try target first; on macOS with RLIM_INFINITY hard limit and lower
    // kern.maxfilesperproc/OPEN_MAX caps, fall back gracefully without failing spawn.
    for candidate in [primary, 10_240, 4_096] {
        let clamped = match hard {
            Some(h) => candidate.min(h),
            None => candidate,
        };
        if clamped <= current.soft {
            continue;
        }
        let desired = libc::rlimit {
            rlim_cur: clamped as libc::rlim_t,
            rlim_max: rlim.rlim_max,
        };
        if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &desired) } == 0 {
            break;
        }
    }
}

/// Configure a child process to raise its soft `RLIMIT_NOFILE` toward
/// `min(hard_limit, 65536)` on Unix before exec so concurrent test harnesses
/// do not fail under low default soft limits (e.g., macOS default 256).
pub(crate) fn configure_child_nofile_limit(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                raise_nofile_soft_limit_in_child(RECOMMENDED_NOFILE_CEILING);
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation {
    local: Arc<AtomicBool>,
    signal_generation: Option<usize>,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.local.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.local.load(Ordering::Acquire)
            || self.signal_generation.is_some_and(signal::is_cancelled)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessLimits {
    pub timeout: Duration,
    /// Per stream. Exceeding the limit is failure, never successful truncation.
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Exited,
    TimedOut,
    Cancelled,
    OutputLimit,
}

#[derive(Debug)]
pub struct ProcessCapture {
    pub output: Output,
    pub termination: Termination,
    pub duration: Duration,
}

pub trait BoundedCommand {
    fn bounded_output(&mut self, timeout: Duration) -> io::Result<Output>;
    fn bounded_capture(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
    ) -> io::Result<ProcessCapture>;
    /// Separate bounded allowance for a declared, authenticated artifact body.
    /// Callers must validate the fixed transport request and expected byte count.
    /// Ordinary subprocess capture retains its original 64 MiB ceiling.
    fn bounded_artifact_capture(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
    ) -> io::Result<ProcessCapture>;
}

struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn reap(&mut self) -> io::Result<ExitStatus> {
        match self.child.wait() {
            Ok(status) => {
                self.reaped = true;
                Ok(status)
            }
            Err(error) => {
                #[cfg(unix)]
                if error.raw_os_error() == Some(libc::ECHILD) {
                    self.reaped = true;
                }
                Err(error)
            }
        }
    }
    fn terminate(&mut self) {
        // WNOWAIT retains ownership of the leader PID until this signal. Never
        // use a PID loaded from disk, or signal after reaping it.
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            self.terminate();
            let _ = self.reap();
        }
    }
}

#[cfg(unix)]
fn nonblocking(fd: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let raw = fd.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// Bounded work per poll prevents an infinite writer starving cancellation/deadlines.
#[cfg(unix)]
fn drain(
    pipe: &mut impl Read,
    bytes: &mut Vec<u8>,
    limit: usize,
    reads: usize,
) -> io::Result<(bool, bool)> {
    let mut buffer = [0; 8192];
    for _ in 0..reads {
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok((false, true)),
            Ok(n) => {
                let keep = n.min(limit.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&buffer[..keep]);
                if keep < n {
                    return Ok((true, false));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok((false, true)),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok((false, false))
}

#[cfg(unix)]
fn drain_final(pipe: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(200);
    loop {
        let (overflow, drained) = drain(pipe, bytes, limit, 8)?;
        if overflow || drained {
            return Ok(overflow);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "bounded final output capture did not finish",
            ));
        }
    }
}

impl BoundedCommand for Command {
    fn bounded_output(&mut self, timeout: Duration) -> io::Result<Output> {
        let capture = self.bounded_capture(
            ProcessLimits {
                timeout,
                max_output_bytes: MAX_OUTPUT_BYTES,
            },
            &Cancellation::default(),
        )?;
        match capture.termination {
            Termination::Exited => Ok(capture.output),
            Termination::TimedOut => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "subprocess deadline exceeded; owned group terminated and child reaped",
            )),
            Termination::Cancelled => Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "subprocess cancelled; owned group terminated and child reaped",
            )),
            Termination::OutputLimit => Err(io::Error::other(
                "subprocess output exceeded limit; owned group terminated and child reaped",
            )),
        }
    }

    fn bounded_capture(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
    ) -> io::Result<ProcessCapture> {
        self.capture_with_ceiling(limits, cancel, MAX_OUTPUT_BYTES)
    }
    fn bounded_artifact_capture(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
    ) -> io::Result<ProcessCapture> {
        self.capture_with_ceiling(limits, cancel, 256 * 1024 * 1024 + 64 * 1024)
    }
}
trait CaptureWithCeiling {
    fn capture_with_ceiling(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
        ceiling: usize,
    ) -> io::Result<ProcessCapture>;
}
impl CaptureWithCeiling for Command {
    fn capture_with_ceiling(
        &mut self,
        limits: ProcessLimits,
        cancel: &Cancellation,
        ceiling: usize,
    ) -> io::Result<ProcessCapture> {
        if limits.timeout.is_zero()
            || limits.max_output_bytes == 0
            || limits.max_output_bytes > ceiling
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid subprocess budget",
            ));
        }
        if cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "cancelled before spawn",
            ));
        }
        #[cfg(not(unix))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "bounded process-group execution requires a Unix adapter",
            ))
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            self.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0);
            #[cfg(target_os = "linux")]
            let parent_pid = std::process::id() as libc::pid_t;
            unsafe {
                self.pre_exec(move || {
                    raise_nofile_soft_limit_in_child(RECOMMENDED_NOFILE_CEILING);
                    #[cfg(target_os = "linux")]
                    {
                        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        if libc::getppid() != parent_pid {
                            return Err(io::Error::new(
                                io::ErrorKind::Interrupted,
                                "subprocess parent exited",
                            ));
                        }
                    }
                    Ok(())
                });
            }
            let started = Instant::now();
            let mut child = OwnedChild {
                child: self.spawn()?,
                reaped: false,
            };
            let mut stdout = child
                .child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("stdout unavailable"))?;
            let mut stderr = child
                .child
                .stderr
                .take()
                .ok_or_else(|| io::Error::other("stderr unavailable"))?;
            nonblocking(&stdout)?;
            nonblocking(&stderr)?;
            let mut out = Vec::new();
            let mut err = Vec::new();
            let termination = loop {
                if cancel.is_cancelled() {
                    break Termination::Cancelled;
                }
                if started.elapsed() >= limits.timeout {
                    break Termination::TimedOut;
                }
                let previous_bytes = out.len() + err.len();
                let (noisy_out, _) = drain(&mut stdout, &mut out, limits.max_output_bytes, 8)?;
                let (noisy_err, _) = drain(&mut stderr, &mut err, limits.max_output_bytes, 8)?;
                if noisy_out || noisy_err {
                    break Termination::OutputLimit;
                }
                let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        child.child.id(),
                        info.as_mut_ptr(),
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result != 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    if error.raw_os_error() == Some(libc::ECHILD) {
                        child.reaped = true;
                    }
                    return Err(error);
                }
                if unsafe { info.assume_init().si_pid() != 0 } {
                    break Termination::Exited;
                }
                if out.len() + err.len() == previous_bytes {
                    std::thread::sleep(Duration::from_millis(2));
                }
            };
            child.terminate();
            let status = child.reap()?;
            // At most a pipe's buffered data remains after group termination. We
            // never wait for EOF, so inherited writers cannot hold up cleanup.
            let noisy_out = drain_final(&mut stdout, &mut out, limits.max_output_bytes)?;
            let noisy_err = drain_final(&mut stderr, &mut err, limits.max_output_bytes)?;
            let termination = if termination == Termination::Exited && (noisy_out || noisy_err) {
                Termination::OutputLimit
            } else {
                termination
            };
            Ok(ProcessCapture {
                output: Output {
                    status,
                    stdout: out,
                    stderr: err,
                },
                termination,
                duration: started.elapsed(),
            })
        }
    }
}

#[cfg(test)]
mod artifact_budget_tests {
    use super::*;
    #[test]
    fn artifact_allowance_does_not_expand_ordinary_process_capture() {
        let limits = ProcessLimits {
            timeout: Duration::from_secs(1),
            max_output_bytes: MAX_OUTPUT_BYTES + 1,
        };
        let cancel = Cancellation::default();
        assert_eq!(
            Command::new("/no-spawn")
                .bounded_capture(limits, &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        cancel.cancel();
        // Validation accepts the declared artifact allowance, then cancellation
        // prevents spawn. No large allocation or external program is required.
        assert_eq!(
            Command::new("/no-spawn")
                .bounded_artifact_capture(limits, &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        let too_large = ProcessLimits {
            max_output_bytes: 256 * 1024 * 1024 + 64 * 1024 + 1,
            ..limits
        };
        assert_eq!(
            Command::new("/no-spawn")
                .bounded_artifact_capture(too_large, &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/bounded_process_tests.rs"]
mod tests;
