//! Dependency-free CPU budget for local release artifact builds only.
//!
//! Rust's available_parallelism accounts for affinity and accessible Linux
//! cgroup CPU quotas, unlike the host's raw logical CPU count. It is still an
//! estimate: hidden container limits and current memory pressure are not known.
//! A runner can retain a smaller, positive CARGO_BUILD_JOBS memory budget.
use std::num::NonZeroUsize;

pub fn select_jobs(
    available: Option<NonZeroUsize>,
    requested: Option<&str>,
) -> Result<usize, String> {
    // Never guess the host count when resource discovery is unavailable.
    let capacity = available.map(NonZeroUsize::get).unwrap_or(1);
    match requested {
        None => Ok(capacity),
        Some(value) => {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("CARGO_BUILD_JOBS must be a positive integer".into());
            }
            let requested = value
                .parse::<NonZeroUsize>()
                .map_err(|_| "CARGO_BUILD_JOBS must be a positive integer")?;
            Ok(capacity.min(requested.get()))
        }
    }
}

fn main() {
    let available = std::thread::available_parallelism().ok();
    if available.is_none() {
        eprintln!("release CPU discovery unavailable; using at most one Cargo job");
    }
    let requested = match std::env::var("CARGO_BUILD_JOBS") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => {
            eprintln!("release CPU budget: {error}");
            std::process::exit(1);
        }
    };
    match select_jobs(available, requested.as_deref()) {
        Ok(jobs) => {
            eprintln!(
                "release artifact Cargo jobs={jobs} available={} requested={}",
                available.map(NonZeroUsize::get).unwrap_or(1),
                requested.as_deref().unwrap_or("auto")
            );
            println!("{jobs}");
        }
        Err(error) => {
            eprintln!("release CPU budget: {error}");
            std::process::exit(1);
        }
    }
}
