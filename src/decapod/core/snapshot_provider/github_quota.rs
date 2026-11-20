//! Advisory pacing for the explicitly anonymous GitHub reader. Only successful
//! response headers inform it; a denied request still returns immediately.
//! These in-process hints never cache provider records or authorize readiness.
use super::{Budget, Result, invalid};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(super) struct GithubQuota {
    requests: u64,
    remaining: Option<u64>,
    reset_unix: Option<u64>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn wait_error(error: std::io::Error, reset: u64) -> std::io::Error {
    std::io::Error::new(
        error.kind(),
        format!(
            "public GitHub quota wait ended: {error}; rate_limit_reset_unix={reset}; no request was made during the wait"
        ),
    )
}

// Accept one bounded decimal header; duplicate or malformed hints are unknown.
// Never retain arbitrary headers, cookies, response bodies or diagnostics.
fn decimal_header(bytes: &[u8], name: &str) -> Option<u64> {
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n")?;
    if end > 64 * 1024 {
        return None;
    }
    let headers = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut values = headers.split("\r\n").skip(1).filter_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then_some(value.trim())
    });
    let value = values.next()?;
    if values.next().is_some()
        || value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

impl GithubQuota {
    fn observe(&mut self, bytes: &[u8]) {
        self.requests = self.requests.saturating_add(1);
        self.remaining = decimal_header(bytes, "x-ratelimit-remaining");
        self.reset_unix = decimal_header(bytes, "x-ratelimit-reset");
    }

    fn request_delay(&self, now: u64) -> Result<Duration> {
        if self.remaining != Some(0) {
            return Ok(Duration::ZERO);
        }
        let reset = self.reset_unix.ok_or_else(|| {
            invalid("public GitHub quota exhausted with no unambiguous reset header; no further request was attempted")
        })?;
        Ok(Duration::from_secs(reset.saturating_sub(now)))
    }

    fn poll_interval(&self, before: u64, interval: Duration, now: u64) -> Duration {
        let requests = self.requests.saturating_sub(before);
        let (Some(remaining), Some(reset)) = (self.remaining, self.reset_unix) else {
            return interval;
        };
        if requests == 0 || reset <= now {
            return interval;
        }
        // Estimate further discovery polls against half the current allowance
        // to leave headroom for collection and fresh final identity checks.
        // Other users/regions can change the counter: it is advice, not a grant.
        let polls = ((remaining / 2) / requests).max(1);
        interval.max(Duration::from_secs((reset - now).div_ceil(polls)))
    }
}

impl Budget<'_> {
    pub(super) fn public_github_requests(&self) -> Result<u64> {
        self.remaining()?;
        Ok(self
            .github_quota
            .lock()
            .map_err(|_| invalid("public GitHub quota tracking unavailable"))?
            .requests)
    }

    pub(super) fn observe_public_github_quota(&self, bytes: &[u8]) -> Result<()> {
        self.github_quota
            .lock()
            .map_err(|_| invalid("public GitHub quota tracking unavailable"))?
            .observe(bytes);
        Ok(())
    }

    pub(super) fn wait_for_public_github_quota(&self) -> Result<()> {
        self.remaining()?;
        let (delay, reset) = {
            let quota = self
                .github_quota
                .lock()
                .map_err(|_| invalid("public GitHub quota tracking unavailable"))?;
            (quota.request_delay(now_unix())?, quota.reset_unix)
        };
        // Untrusted epoch values cannot overflow Instant or extend the original
        // invocation deadline. The existing wait also observes cancellation.
        let result = self
            .remaining()
            .and_then(|remaining| self.wait(delay.min(remaining)));
        match reset {
            Some(reset) if !delay.is_zero() => result.map_err(|error| wait_error(error, reset)),
            _ => result,
        }
    }

    pub(super) fn wait_for_public_github_poll(
        &self,
        before: u64,
        interval: Duration,
    ) -> Result<()> {
        let (delay, reset) = {
            let quota = self
                .github_quota
                .lock()
                .map_err(|_| invalid("public GitHub quota tracking unavailable"))?;
            (
                quota.poll_interval(before, interval, now_unix()),
                quota.reset_unix,
            )
        };
        let result = self
            .remaining()
            .and_then(|remaining| self.wait(delay.min(remaining)));
        match reset {
            Some(reset) if delay > interval => result.map_err(|error| wait_error(error, reset)),
            _ => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Cancellation;
    use super::*;

    fn response(remaining: &str, reset: &str) -> Vec<u8> {
        format!(
            "HTTP/2 200\r\nX-RateLimit-Remaining: {remaining}\r\nX-RateLimit-Reset: {reset}\r\n\r\n{{}}"
        )
        .into_bytes()
    }

    #[test]
    fn discovery_reserves_headroom_but_direct_buildkite_polls_are_not_delayed() {
        let mut quota = GithubQuota::default();
        quota.observe(&response("59", "3700"));
        quota.observe(&response("58", "3700"));
        assert_eq!(
            quota.poll_interval(0, Duration::from_secs(5), 100),
            Duration::from_secs(258)
        );
        assert_eq!(
            quota.poll_interval(2, Duration::from_secs(5), 100),
            Duration::from_secs(5)
        );
        assert_eq!(
            quota.poll_interval(0, Duration::from_secs(5), 3700),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn only_exhausted_success_waits_for_reset_and_unknown_reset_blocks() {
        let mut quota = GithubQuota::default();
        quota.observe(&response("1", "3700"));
        assert_eq!(quota.request_delay(100).unwrap(), Duration::ZERO);
        quota.observe(&response("0", "3700"));
        assert_eq!(quota.request_delay(100).unwrap(), Duration::from_secs(3600));
        assert_eq!(quota.request_delay(3701).unwrap(), Duration::ZERO);
        quota.observe(&response("0", "invalid"));
        assert!(quota.request_delay(100).is_err());
    }

    #[test]
    fn malformed_duplicate_missing_and_overflowing_hints_remain_unknown() {
        for value in ["", "-1", "+1", "1.5", "secret", "18446744073709551616"] {
            let mut quota = GithubQuota::default();
            quota.observe(&response(value, "3700"));
            assert!(quota.remaining.is_none(), "{value}");
            assert_eq!(
                quota.poll_interval(0, Duration::from_secs(5), 100),
                Duration::from_secs(5)
            );
        }
        let mut quota = GithubQuota::default();
        quota.observe(
            b"HTTP/2 200\r\nX-RateLimit-Remaining: 0\r\nx-ratelimit-remaining: 60\r\nX-RateLimit-Reset: 3700\r\n\r\n{}",
        );
        assert!(quota.remaining.is_none());
        quota.observe(
            b"HTTP/2 200\r\nX-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 3700\r\nx-ratelimit-reset: 3800\r\n\r\n{}",
        );
        assert!(quota.request_delay(100).is_err());
        quota.observe(b"HTTP/2 200\r\n\r\n{}");
        assert!(quota.remaining.is_none() && quota.reset_unix.is_none());
    }

    #[test]
    fn quota_wait_preserves_the_original_deadline_and_cancellation() {
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_millis(10), 4096, &cancel).unwrap();
        budget
            .observe_public_github_quota(&response("0", &u64::MAX.to_string()))
            .unwrap();
        let error = budget.wait_for_public_github_quota().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("rate_limit_reset_unix="));
        let budget = Budget::new(Duration::from_millis(10), 4096, &cancel).unwrap();
        budget
            .observe_public_github_quota(&response("58", &u64::MAX.to_string()))
            .unwrap();
        let error = budget
            .wait_for_public_github_poll(0, Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("rate_limit_reset_unix="));
        let budget = Budget::new(Duration::from_secs(5), 4096, &cancel).unwrap();
        budget
            .observe_public_github_quota(&response("0", "invalid"))
            .unwrap();
        cancel.cancel();
        assert_eq!(
            budget.wait_for_public_github_quota().unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
    }

    #[cfg(unix)]
    mod transport {
        use super::super::super::{
            CommandSpec, PublicGithubReads, Reply, Repository, Runner, provider,
        };
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        struct NoFallback;
        impl Runner for NoFallback {
            fn execute(&mut self, _: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
                panic!("anonymous quota handling must not select another transport");
            }
        }

        fn fixture(
            status: u16,
            exit: i32,
            reset: &str,
        ) -> (tempfile::TempDir, PublicGithubReads<NoFallback>) {
            let temp = tempfile::tempdir().unwrap();
            let curl = temp.path().join("synthetic-curl");
            std::fs::write(
                &curl,
                format!(
                    "#!/bin/sh\nprintf x >> requests\nprintf 'HTTP/2 {status}\\r\\nX-RateLimit-Remaining: 0\\r\\nX-RateLimit-Reset: {reset}\\r\\n\\r\\n{{}}'\nexit {exit}\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o700)).unwrap();
            let reader = PublicGithubReads::new(
                curl,
                Repository::new("org", "repo").unwrap(),
                NoFallback,
                |spec, _| {
                    spec.authorize_provider_read()?;
                    Ok(())
                },
            )
            .unwrap();
            (temp, reader)
        }

        #[test]
        fn exhausted_success_prevents_a_second_request_when_reset_is_unknown_or_budget_expires() {
            for reset in ["invalid", "18446744073709551615"] {
                let (temp, mut reader) = fixture(200, 0, reset);
                let cancel = Cancellation::default();
                let mut budget = Budget::new(Duration::from_secs(5), 4096, &cancel).unwrap();
                let spec = provider::gh_spec(temp.path(), "repos/org/repo/actions/runs/7");
                assert_eq!(reader.execute(&spec, &budget).unwrap().stdout, b"{}");
                budget.deadline = std::time::Instant::now() + Duration::from_millis(10);
                let error = reader.execute(&spec, &budget).unwrap_err();
                if reset == "invalid" {
                    assert!(
                        error
                            .to_string()
                            .contains("no further request was attempted")
                    );
                } else {
                    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                }
                assert_eq!(std::fs::read(temp.path().join("requests")).unwrap(), b"x");
            }
        }

        #[test]
        fn denied_responses_stop_once_without_waiting_for_their_future_reset() {
            for status in [403, 429] {
                let (temp, mut reader) = fixture(status, 22, "18446744073709551615");
                let cancel = Cancellation::default();
                let budget = Budget::new(Duration::from_secs(5), 4096, &cancel).unwrap();
                let spec = provider::gh_spec(temp.path(), "repos/org/repo/actions/runs/7");
                let error = reader.execute(&spec, &budget).unwrap_err().to_string();
                assert!(error.contains(&format!("http_status={status}")));
                assert!(error.contains("no automatic retry or alternate authentication route"));
                assert_eq!(std::fs::read(temp.path().join("requests")).unwrap(), b"x");
            }
        }
    }
}
