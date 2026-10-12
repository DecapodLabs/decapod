//! Production cargo-dist build entrypoint; shared with derivative proof.
use decapod::core::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    derived_release_driver::{Stage, build_artifacts},
    derived_release_profile,
    release_toolchain::ReleaseToolchain,
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::PathBuf,
    process::Command,
    time::Duration,
};
fn fail(s: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s.into())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Release artifact build failed: {e}");
        std::process::exit(1);
    }
}
fn run() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let scope = args
        .next()
        .ok_or_else(|| fail("expected local or global"))?;
    let (mut target, mut tag) = (None, None);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| fail("missing argument value"))?;
        match flag.as_str() {
            "--target" if target.is_none() => target = Some(value),
            "--tag" if tag.is_none() => {
                if !value.is_empty() {
                    tag = Some(value);
                }
            }
            _ => return Err(fail("unexpected/duplicate argument")),
        }
    }
    let stage = match (scope.as_str(), target) {
        ("local", Some(target)) => Stage::Local { target },
        ("global", None) => Stage::Global,
        _ => return Err(fail("local requires one target; global accepts none")),
    };
    let root = std::env::current_dir()?.canonicalize()?;
    let read = |p: &str| std::fs::read_to_string(root.join(p));
    let profile = derived_release_profile::profile(
        &read("Cargo.toml")?,
        &read(".github/release.toml")?,
        &read("dist-workspace.toml")?,
        &read("dist.toml")?,
    )
    .map_err(fail)?;
    let pins = ReleaseToolchain::parse(&read(".github/release-tools.toml")?).map_err(fail)?;
    if pins.rust != profile.rust_version || pins.cargo_dist != profile.dist_version {
        return Err(fail("release setup/profile pin drift"));
    }
    let cargo = std::env::var_os("CARGO")
        .map(PathBuf::from)
        .ok_or_else(|| {
            fail("invoke this helper with cargo run so CARGO is the exact reviewed compiler driver")
        })?
        .canonicalize()?;
    let env: BTreeMap<_, _> = [
        "PATH",
        "HOME",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "SDKROOT",
        "DEVELOPER_DIR",
        "MACOSX_DEPLOYMENT_TARGET",
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER",
        "CC",
        "CXX",
        "AR",
    ]
    .into_iter()
    .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_owned(), v)))
    .collect();
    let cancel = Cancellation::default();
    let execute = |args: &[String]| -> io::Result<Vec<u8>> {
        let capture = Command::new(&cargo)
            .args(args)
            .current_dir(&root)
            .env_clear()
            .envs(&env)
            .bounded_capture(
                ProcessLimits {
                    timeout: Duration::from_secs(1800),
                    max_output_bytes: 16 * 1024 * 1024,
                },
                &cancel,
            )?;
        io::stderr().write_all(&capture.output.stderr)?;
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(fail("bounded cargo-dist command failed"));
        }
        Ok(capture.output.stdout)
    };
    let version = String::from_utf8(execute(&["dist".into(), "--version".into()])?)
        .map_err(|_| fail("invalid cargo-dist version"))?;
    if version.split_whitespace().nth(1) != Some(&pins.cargo_dist) {
        return Err(fail("installed cargo-dist differs from shared pin"));
    }
    let output = build_artifacts(&root, &profile, &stage, tag.as_deref(), execute)?;
    io::stdout().write_all(&output)
}
