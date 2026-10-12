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

mod signal;
pub use signal::SignalCancellationGuard;

pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

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
            {
                let parent_pid = std::process::id() as libc::pid_t;
                unsafe {
                    self.pre_exec(move || {
                        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        if libc::getppid() != parent_pid {
                            return Err(io::Error::new(
                                io::ErrorKind::Interrupted,
                                "subprocess parent exited",
                            ));
                        }
                        Ok(())
                    });
                }
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
