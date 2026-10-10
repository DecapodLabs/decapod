//! Bounded, synchronous subprocess ownership without pipe-drain deadlocks.
//!
//! Output goes to anonymous files rather than pipes: neither a full pipe nor an
//! inherited writer can keep the caller waiting. Every spawned child is reaped.
use std::io::{self, Read, Seek, SeekFrom};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

pub trait BoundedCommand {
    fn bounded_output(&mut self, timeout: Duration) -> io::Result<Output>;
}

struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn terminate(&mut self) {
        // The leader has not been reaped, so its PID (and process group ID)
        // cannot have been recycled. Never signal a PID read from disk.
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
            let _ = self.child.wait();
        }
    }
}

impl BoundedCommand for Command {
    fn bounded_output(&mut self, timeout: Duration) -> io::Result<Output> {
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        self.stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            self.process_group(0);
            #[cfg(target_os = "linux")]
            {
                let parent_pid = std::process::id() as libc::pid_t;
                // Linux terminates the direct helper if its invoking process
                // disappears, including SIGKILL. No detached watchdog is used.
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
        }
        let mut child = OwnedChild {
            child: self.spawn()?,
            reaped: false,
        };
        let started = Instant::now();
        let status = loop {
            #[cfg(unix)]
            let exited = {
                let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
                // Observe without reaping, preserving process-group ownership
                // until descendants are terminated as well.
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        child.child.id(),
                        info.as_mut_ptr(),
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result != 0 {
                    let err = io::Error::last_os_error();
                    if err.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    if err.raw_os_error() == Some(libc::ECHILD) {
                        // A process-wide child reaper has already consumed it;
                        // its numeric PID is no longer ours to signal.
                        child.reaped = true;
                    }
                    return Err(err);
                }
                unsafe { info.assume_init().si_pid() != 0 }
            };
            #[cfg(not(unix))]
            let exited = child.child.try_wait()?.is_some();
            if exited {
                child.terminate();
                let status = child.child.wait()?;
                // Drop must not signal a PID after it has been reaped.
                child.reaped = true;
                break status;
            }
            if started.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "subprocess deadline exceeded after {}s; owned child terminated and reaped",
                        timeout.as_secs_f64()
                    ),
                ));
            }
            if stdout.metadata()?.len() > MAX_OUTPUT_BYTES
                || stderr.metadata()?.len() > MAX_OUTPUT_BYTES
            {
                return Err(io::Error::other(
                    "subprocess output exceeded 64 MiB per stream; owned child terminated and reaped",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        fn collect(file: &mut std::fs::File) -> io::Result<Vec<u8>> {
            file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            file.take(MAX_OUTPUT_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_OUTPUT_BYTES {
                return Err(io::Error::other(
                    "subprocess output exceeded 64 MiB per stream",
                ));
            }
            Ok(bytes)
        }
        Ok(Output {
            status,
            stdout: collect(&mut stdout)?,
            stderr: collect(&mut stderr)?,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/bounded_process_tests.rs"]
mod tests;
