//! Run a build or test with an invocation-owned temporary directory.
//!
//! Bazel uses this as its test `--run_under` executable. Cargo aliases use the
//! existing process temp root. Never clean Cargo targets, Bazel output bases, or
//! arbitrary pre-existing directories from inside a build/test action.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
const MAX_PERMISSION_RECOVERY_DEPTH: usize = 128;
#[cfg(unix)]
const MAX_PERMISSION_RECOVERY_ENTRIES: usize = 65_536;

struct Scratch {
    path: PathBuf,
    identity: fs::Metadata,
    parent_identity: fs::Metadata,
}

impl Scratch {
    fn create(parent: &Path) -> io::Result<Self> {
        fs::create_dir_all(parent)?;
        // Keep short sandbox/symlink aliases intact. Canonicalization can turn a
        // valid Unix-socket path into a hundreds-of-bytes Bazel output path.
        let parent = std::path::absolute(parent)?;
        let parent_identity = fs::metadata(&parent)?;
        // Atomic creation, rather than timestamps or a scan of old directories,
        // establishes ownership and keeps concurrent runs disjoint.
        for sequence in 0..1000 {
            let path = parent.join(format!(".d{:x}-{sequence:x}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    let identity = fs::symlink_metadata(&path)?;
                    return Ok(Self {
                        path,
                        identity,
                        parent_identity,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate an invocation-owned scratch directory",
        ))
    }

    fn verify_root(&self) -> io::Result<Option<fs::Metadata>> {
        let parent = fs::metadata(self.path.parent().expect("scratch has a parent"))?;
        if !same_directory(&self.parent_identity, &parent) {
            return Err(io::Error::other(
                "scratch parent was replaced; refusing cleanup",
            ));
        }
        let current = match fs::symlink_metadata(&self.path) {
            Ok(current) => current,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if current.file_type().is_symlink()
            || !current.is_dir()
            || !same_directory(&self.identity, &current)
        {
            return Err(io::Error::other(
                "scratch directory was replaced; refusing cleanup",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if current.uid() != self.identity.uid() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "scratch directory owner changed (expected uid={}, observed uid={})",
                        self.identity.uid(),
                        current.uid()
                    ),
                ));
            }
        }
        Ok(Some(current))
    }

    #[cfg(unix)]
    fn recover_owned_permissions(&self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        self.recover_owned_permissions_with_bounds(
            self.identity.uid(),
            self.identity.dev(),
            MAX_PERMISSION_RECOVERY_DEPTH,
            MAX_PERMISSION_RECOVERY_ENTRIES,
        )
    }

    #[cfg(unix)]
    fn recover_owned_permissions_with_bounds(
        &self,
        expected_uid: u32,
        expected_dev: u64,
        max_depth: usize,
        max_entries: usize,
    ) -> io::Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let Some(root_meta) = self.verify_root()? else {
            return Ok(());
        };
        // Lock the scratch root to owner-only 0o700 first so no other local
        // principal can traverse or mutate paths inside it during recovery.
        if (root_meta.mode() & 0o777) != 0o700 {
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o700))?;
            let Some(rechecked) = self.verify_root()? else {
                return Ok(());
            };
            if (rechecked.mode() & 0o700) != 0o700 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "could not restore owner permissions on scratch root",
                ));
            }
        }

        let mut stack = vec![(self.path.clone(), self.identity.ino(), 0usize)];
        let mut visited_entries = 0usize;

        while let Some((dir_path, expected_ino, depth)) = stack.pop() {
            let current = fs::symlink_metadata(&dir_path)?;
            if current.file_type().is_symlink() || !current.is_dir() {
                return Err(io::Error::other(format!(
                    "scratch directory {} was replaced by a non-directory; refusing cleanup",
                    dir_path.display()
                )));
            }
            let required_dev = if depth == 0 {
                self.identity.dev()
            } else {
                expected_dev
            };
            let required_uid = if depth == 0 {
                self.identity.uid()
            } else {
                expected_uid
            };
            if current.dev() != required_dev {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "scratch directory {} crossed filesystem device boundary; refusing cleanup",
                        dir_path.display()
                    ),
                ));
            }
            if current.ino() != expected_ino {
                return Err(io::Error::other(format!(
                    "scratch directory {} inode changed during cleanup; refusing cleanup",
                    dir_path.display()
                )));
            }
            if current.uid() != required_uid {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "scratch directory {} has foreign owner uid={} (expected uid={}); refusing cleanup",
                        dir_path.display(),
                        current.uid(),
                        required_uid
                    ),
                ));
            }
            if depth > 0 && (current.mode() & 0o777) != 0o700 {
                fs::set_permissions(&dir_path, fs::Permissions::from_mode(0o700))?;
                let after = fs::symlink_metadata(&dir_path)?;
                if after.file_type().is_symlink()
                    || !after.is_dir()
                    || after.dev() != required_dev
                    || after.ino() != expected_ino
                    || after.uid() != required_uid
                {
                    return Err(io::Error::other(format!(
                        "scratch directory {} changed identity during permission recovery",
                        dir_path.display()
                    )));
                }
            }

            for entry in fs::read_dir(&dir_path)? {
                visited_entries += 1;
                if visited_entries > max_entries {
                    return Err(io::Error::other(
                        "scratch permission recovery exceeded bounded entry limit",
                    ));
                }
                let entry = entry?;
                let entry_path = entry.path();
                // symlink_metadata inspects the link entry itself without following
                // it, keeping external caches and symlink targets untouched.
                let entry_meta = fs::symlink_metadata(&entry_path)?;
                if entry_meta.file_type().is_symlink() {
                    continue;
                }
                if entry_meta.dev() != expected_dev {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "scratch entry {} crossed filesystem device boundary; refusing cleanup",
                            entry_path.display()
                        ),
                    ));
                }
                if entry_meta.uid() != expected_uid {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!(
                            "scratch entry {} has foreign owner uid={} (expected uid={}); refusing cleanup",
                            entry_path.display(),
                            entry_meta.uid(),
                            expected_uid
                        ),
                    ));
                }
                if entry_meta.is_dir() {
                    if depth + 1 > max_depth {
                        return Err(io::Error::other(
                            "scratch permission recovery exceeded bounded directory depth",
                        ));
                    }
                    stack.push((entry_path, entry_meta.ino(), depth + 1));
                }
            }
        }

        Ok(())
    }

    fn clean(&self) -> io::Result<()> {
        if self.verify_root()?.is_none() {
            return Ok(());
        }
        // remove_dir_all does not follow symlinks within the owned tree.
        match fs::remove_dir_all(&self.path) {
            Ok(()) => Ok(()),
            #[cfg(unix)]
            Err(first_error) if first_error.kind() == io::ErrorKind::PermissionDenied => {
                // Rootless container storage (e.g., Podman overlay work/work under a
                // disposable HOME) can leave same-UID mode-000 or read-only nested
                // directories inside the invocation-owned scratch tree. Recover owner
                // rwx only on verified same-UID, same-device real directories and retry.
                if let Err(recovery_error) = self.recover_owned_permissions() {
                    return Err(io::Error::new(
                        first_error.kind(),
                        format!(
                            "{first_error} (permission recovery stopped: {recovery_error}); {}",
                            cleanup_diagnostics(&self.path)
                        ),
                    ));
                }
                if self.verify_root()?.is_none() {
                    return Ok(());
                }
                match fs::remove_dir_all(&self.path) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(io::Error::new(
                        error.kind(),
                        format!("{error}; {}", cleanup_diagnostics(&self.path)),
                    )),
                }
            }
            Err(error) => Err(io::Error::new(
                error.kind(),
                format!("{error}; {}", cleanup_diagnostics(&self.path)),
            )),
        }
    }
}

fn cleanup_diagnostics(root: &Path) -> String {
    // Read-only, bounded diagnostics for a failed removal. Never follow links
    // or restore permissions: foreign-owned output needs its owner's teardown.
    let mut pending = vec![root.to_path_buf()];
    let mut details = Vec::new();
    let mut inspected = 0;
    #[cfg(unix)]
    let root_uid = {
        use std::os::unix::fs::MetadataExt;
        fs::symlink_metadata(root)
            .ok()
            .map(|metadata| metadata.uid())
    };
    while let Some(path) = pending.pop() {
        if inspected >= 1024 || details.len() >= 8 {
            details.push("diagnostic traversal limit reached".to_string());
            break;
        }
        inspected += 1;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                details.push(format!("{}: metadata: {error}", path.display()));
                continue;
            }
        };
        if !metadata.is_dir() {
            continue;
        }
        #[cfg(unix)]
        let (description, noteworthy) = {
            use std::os::unix::fs::MetadataExt;
            (
                format!(
                    "{} (uid={}, gid={}, mode={:o})",
                    path.display(),
                    metadata.uid(),
                    metadata.gid(),
                    metadata.mode() & 0o7777
                ),
                path == root
                    || Some(metadata.uid()) != root_uid
                    || metadata.mode() & 0o700 != 0o700,
            )
        };
        #[cfg(not(unix))]
        let (description, noteworthy) = (
            format!(
                "{} (readonly={})",
                path.display(),
                metadata.permissions().readonly()
            ),
            path == root || metadata.permissions().readonly(),
        );
        // Include remaining directory ownership even if listing is allowed:
        // unlinking its children can fail because it is owned by a container.
        if noteworthy {
            details.push(description.clone());
        }
        match fs::read_dir(&path) {
            Ok(entries) => {
                for entry in entries.take(1024 - inspected) {
                    if pending.len() + inspected >= 1024 {
                        details.push("diagnostic traversal limit reached".to_string());
                        break;
                    }
                    match entry {
                        Ok(entry) => pending.push(entry.path()),
                        Err(error) => details.push(format!("{description}: listing: {error}")),
                    }
                }
            }
            Err(error) => details.push(format!("{description}: listing: {error}")),
        }
    }
    format!("remaining scratch diagnostics: {}", details.join("; "))
}

fn same_directory(original: &fs::Metadata, current: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        original.dev() == current.dev() && original.ino() == current.ino()
    }
    #[cfg(not(unix))]
    {
        matches!((original.created(), current.created()), (Ok(first), Ok(second)) if first == second)
    }
}

fn bazel_scratch_parent(test_tmpdir: &Path, process_tmpdir: &Path) -> io::Result<PathBuf> {
    let test_tmpdir = std::path::absolute(test_tmpdir)?;
    let process_tmpdir = std::path::absolute(process_tmpdir)?;
    // Bazel TEST_TMPDIR can live under a very long output base. Use the existing
    // process temp root when shorter, just as tempfile did before supervision.
    // Both roots are provided by the current execution environment; do not
    // hardcode /tmp, escape its sandbox, or sweep either shared parent.
    Ok(
        if process_tmpdir.as_os_str().as_encoded_bytes().len()
            < test_tmpdir.as_os_str().as_encoded_bytes().len()
        {
            process_tmpdir
        } else {
            test_tmpdir
        },
    )
}

#[cfg(unix)]
mod signals {
    use std::io;
    use std::sync::atomic::{AtomicI32, Ordering};

    static RECEIVED: AtomicI32 = AtomicI32::new(0);
    const HANDLED: [i32; 3] = [1, 2, 15]; // SIGHUP, SIGINT, SIGTERM on Unix.

    unsafe extern "C" {
        fn signal(number: i32, handler: usize) -> usize;
        fn kill(pid: i32, number: i32) -> i32;
    }

    extern "C" fn receive(number: i32) {
        // Signal handlers must not allocate, wait, log, or touch the filesystem.
        RECEIVED.store(number, Ordering::Relaxed);
    }

    pub struct Guard(Vec<(i32, usize)>);

    impl Guard {
        pub fn install() -> io::Result<Self> {
            RECEIVED.store(0, Ordering::Relaxed);
            let mut guard = Self(Vec::new());
            for number in HANDLED {
                // SAFETY: receive is a static C ABI handler using only an atomic.
                let previous = unsafe { signal(number, receive as *const () as usize) };
                if previous == usize::MAX {
                    return Err(io::Error::last_os_error());
                }
                guard.0.push((number, previous));
            }
            Ok(guard)
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            for &(number, previous) in &self.0 {
                // SAFETY: restore exactly the disposition returned by signal.
                unsafe { signal(number, previous) };
            }
        }
    }

    pub fn take() -> i32 {
        RECEIVED.swap(0, Ordering::Relaxed)
    }

    pub fn forward(group: u32, number: i32) {
        if let Ok(group) = i32::try_from(group) {
            // SAFETY: a negative child PID addresses only its new process group.
            unsafe { kill(-group, number) };
        }
    }

    pub fn group_exists(group: u32) -> bool {
        let Ok(group) = i32::try_from(group) else {
            return true;
        };
        // SAFETY: signal zero probes existence; it does not send a signal.
        if unsafe { kill(-group, 0) } == 0 {
            return true;
        }
        // ESRCH is the only result proving absence. EPERM and other errors are
        // not evidence that it is safe to remove scratch.
        io::Error::last_os_error().raw_os_error() != Some(3)
    }
}

fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        128 + status.signal().unwrap_or(1)
    }
    #[cfg(not(unix))]
    {
        1
    }
}

fn supervise(parent: &Path, command: &mut Command) -> io::Result<i32> {
    #[cfg(unix)]
    let _signals = signals::Guard::install()?;
    let scratch = Scratch::create(parent)?;
    command
        .env("TMPDIR", &scratch.path)
        .env("TMP", &scratch.path)
        .env("TEMP", &scratch.path);
    if let Some(test_tmpdir) = env::var_os("TEST_TMPDIR") {
        command.env("TEST_TMPDIR", &scratch.path);
        // Bazel normally gives a test a disposable HOME. Keep that disposable
        // state inside this run too, but never override a real/custom HOME.
        if env::var_os("HOME").as_ref() == Some(&test_tmpdir) {
            command.env("HOME", &scratch.path);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if let Err(cleanup) = scratch.clean() {
                eprintln!("build cleanup: {}: {cleanup}", scratch.path.display());
            }
            return Err(error);
        }
    };
    #[cfg(unix)]
    let mut interrupted = 0;
    #[cfg(not(unix))]
    let interrupted = 0;
    #[cfg(unix)]
    let mut interrupt_time = None;
    let status = loop {
        #[cfg(unix)]
        {
            let signal = signals::take();
            if signal != 0 {
                interrupted = signal;
                interrupt_time.get_or_insert_with(Instant::now);
                signals::forward(child.id(), signal);
            }
            if interrupt_time.is_some_and(|start| start.elapsed() >= Duration::from_secs(5)) {
                // A child that ignores termination cannot keep the wrapper alive
                // indefinitely. Only this invocation's process group is killed.
                signals::forward(child.id(), 9);
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                // Do not delete a directory while its child may still use it.
                eprintln!(
                    "build cleanup: retained {}: {error}",
                    scratch.path.display()
                );
                return Err(error);
            }
        }
    };
    let code = if interrupted == 0 {
        exit_code(status)
    } else {
        128 + interrupted
    };
    #[cfg(unix)]
    if signals::group_exists(child.id()) {
        // try_wait has reaped the leader, so its PID/PGID could be reused. Never
        // signal that numeric group again. A positive probe only justifies
        // retaining scratch; it does not establish ownership of those processes.
        eprintln!(
            "build cleanup: retained {} because process-group exit is unconfirmed",
            scratch.path.display()
        );
        return Ok(if code == 0 { 1 } else { code });
    }
    if let Err(error) = scratch.clean() {
        eprintln!("build cleanup: {}: {error}", scratch.path.display());
        return Ok(if code == 0 { 1 } else { code });
    }
    Ok(code)
}

fn run(arguments: Vec<OsString>) -> io::Result<i32> {
    if arguments
        .first()
        .is_some_and(|argument| argument == "--help")
    {
        println!(
            "decapod-build [--scratch-parent DIR] [--] COMMAND [ARGS...]\n\
                  Run a build/test and remove only its owned temporary files.\n\
                  Bazel uses TEST_TMPDIR or its shorter process temp root;\n\
                  otherwise the process temp root is used. --scratch-parent overrides it."
        );
        return Ok(0);
    }
    let mut arguments = arguments.into_iter().peekable();
    let parent = if arguments
        .peek()
        .is_some_and(|argument| argument == "--scratch-parent")
    {
        arguments.next();
        PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| io::Error::other("missing scratch parent"))?,
        )
    } else if let Some(test_tmpdir) = env::var_os("TEST_TMPDIR") {
        let test_tmpdir = PathBuf::from(test_tmpdir);
        bazel_scratch_parent(&test_tmpdir, &env::temp_dir())?
    } else {
        env::temp_dir()
    };
    if arguments.peek().is_some_and(|argument| argument == "--") {
        arguments.next();
    }
    let executable = arguments
        .next()
        .ok_or_else(|| io::Error::other("missing command"))?;
    let mut command = Command::new(executable);
    command.args(arguments);
    supervise(&parent, &mut command)
}

fn main() {
    let result = run(env::args_os().skip(1).collect());
    let code = result.unwrap_or_else(|error| {
        eprintln!("build cleanup: {error}");
        1
    });
    std::process::exit(code);
}

#[cfg(test)]
#[path = "build_lifecycle_tests.rs"]
mod tests;
