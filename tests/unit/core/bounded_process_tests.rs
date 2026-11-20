use super::*;

#[cfg(unix)]
#[test]
fn drains_large_stdout_and_stderr_without_pipe_deadlock() {
    let output = Command::new("sh")
        .args([
            "-c",
            "head -c 2097152 /dev/zero; head -c 2097152 /dev/zero >&2",
        ])
        .bounded_output(Duration::from_secs(5))
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 2097152);
    assert_eq!(output.stderr.len(), 2097152);
}

#[cfg(unix)]
#[test]
fn timeout_kills_and_reaps_owned_child() {
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("pid");
    let started = Instant::now();
    let error = Command::new("sh")
        .arg("-c")
        .arg("echo $$ > \"$1\"; exec sleep 30")
        .arg("sh")
        .arg(&pid_file)
        .bounded_output(Duration::from_millis(150))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<libc::pid_t>()
        .unwrap();
    // ESRCH proves the child is gone; ECHILD proves it was reaped as well.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    assert_eq!(
        unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) },
        -1
    );
    assert_eq!(
        io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}

#[cfg(unix)]
#[test]
fn inherited_output_handles_do_not_extend_deadline() {
    let started = Instant::now();
    let output = Command::new("sh")
        .args(["-c", "sleep 30 & echo complete"])
        .bounded_output(Duration::from_secs(2))
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"complete\n");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn preserves_failure_status_and_diagnostics() {
    let output = Command::new("sh")
        .args(["-c", "echo failed >&2; exit 7"])
        .bounded_output(Duration::from_secs(2))
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"failed\n");
}

#[test]
fn missing_executable_returns_without_child() {
    let error = Command::new("/nonexistent/decapod-lifecycle-test")
        .bounded_output(Duration::from_secs(1))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn output_limit_terminates_noisy_helper() {
    let start = Instant::now();
    let error = Command::new("sh")
        .args(["-c", "exec head -c 70000000 /dev/zero"])
        .bounded_output(Duration::from_secs(5))
        .unwrap_err();
    assert!(error.to_string().contains("output exceeded"));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn arguments_are_not_interpreted_as_shell_commands() {
    let argument = "; echo unexpected; $(touch /nonexistent/decapod-test)";
    let output = Command::new("printf")
        .arg("%s")
        .arg(argument)
        .bounded_output(Duration::from_secs(2))
        .unwrap();
    assert_eq!(output.stdout, argument.as_bytes());
}

#[cfg(unix)]
#[test]
fn target_nofile_soft_limit_respects_hard_and_infinity_ceilings() {
    assert_eq!(
        target_nofile_soft_limit(
            NofileLimits {
                soft: 256,
                hard: Some(8192),
            },
            RECOMMENDED_NOFILE_CEILING,
        ),
        Some(8192)
    );
    assert_eq!(
        target_nofile_soft_limit(
            NofileLimits {
                soft: 256,
                hard: Some(131_072),
            },
            RECOMMENDED_NOFILE_CEILING,
        ),
        Some(RECOMMENDED_NOFILE_CEILING)
    );
    assert_eq!(
        target_nofile_soft_limit(
            NofileLimits {
                soft: 256,
                hard: None,
            },
            RECOMMENDED_NOFILE_CEILING,
        ),
        Some(RECOMMENDED_NOFILE_CEILING)
    );
    assert_eq!(
        target_nofile_soft_limit(
            NofileLimits {
                soft: 65_536,
                hard: Some(65_536),
            },
            RECOMMENDED_NOFILE_CEILING,
        ),
        None
    );
}

#[cfg(unix)]
#[test]
fn bounded_output_elevates_child_nofile_limit_at_least_to_floor() {
    let limits = current_nofile_limits().unwrap();
    let output = Command::new("sh")
        .args(["-c", "ulimit -Sn"])
        .bounded_output(Duration::from_secs(2))
        .unwrap();
    assert!(output.status.success());
    let observed = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .unwrap();
    let expected_min = limits
        .hard
        .map(|h| h.min(4096))
        .unwrap_or(4096)
        .max(limits.soft.min(RECOMMENDED_NOFILE_CEILING));
    assert!(
        observed >= expected_min,
        "expected child soft RLIMIT_NOFILE >= {expected_min}, got {observed} (parent={limits:?})"
    );
}
