//! Capture accepted execution authority before any provider dispatch.
//!
//! This is the existing reviewed-transitive-code model. Separate roots prevent
//! accidental candidate self-authorization; they do not sandbox hostile build
//! scripts running as the same provider account. The final controller still
//! recompiles policy and independently checks source, target and provider proof.
use super::*;

const PROTOCOL_DECLARATION: &str =
    "pub const DUAL_ROOT_SCHEMA: &str = \"decapod.source-ci.dual-root.v1\";";

fn is_valid_definition_entry(path: &str, entry: &[u8]) -> bool {
    let valid_mode = entry.starts_with(b"100644 blob ")
        || (path.ends_with(".sh") && entry.starts_with(b"100755 blob "));
    valid_mode && entry.iter().filter(|v| **v == b'\n').count() == 1
}

pub(super) fn capture(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    base: &str,
    enabled: bool,
) -> io::Result<Option<profile::AcceptedExecutor>> {
    // The native adapter retains its existing strict compiled contract. This
    // protocol is implemented by the configured source-CI provider carrier.
    if !enabled {
        return Ok(None);
    }
    let marker_path = "assets/tools/source_ci_custody.rs";
    let marker_entry = git(options, budget, &["ls-tree", base, "--", marker_path])?;
    if marker_entry.is_empty() {
        return Ok(None);
    }
    if !marker_entry.starts_with(b"100644 blob ")
        || marker_entry.iter().filter(|v| **v == b'\n').count() != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            profile::PlanningFailure::ControllerContractUnsupported(
                "accepted executor protocol mode".into(),
            ),
        ));
    }
    let marker = git(options, budget, &["show", &format!("{base}:{marker_path}")])?;
    if !marker
        .split(|b| *b == b'\n')
        .any(|line| line == PROTOCOL_DECLARATION.as_bytes())
    {
        // A one-time protocol rollout can still use the legacy contract. The
        // caller must independently admit every H helper against this binary;
        // absence of R support never authorizes a changed candidate recipe.
        return Ok(None);
    }
    let tree = text(git(
        options,
        budget,
        &["rev-parse", "--verify", &format!("{base}^{{tree}}")],
    )?)?;
    let mut files = BTreeMap::new();
    for (path, _) in profile::definition_files() {
        let entry = git(options, budget, &["ls-tree", base, "--", path])?;
        if !is_valid_definition_entry(path, &entry) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                profile::PlanningFailure::ControllerContractUnsupported(format!(
                    "accepted executor mode: {path}"
                )),
            ));
        }
        let bytes = git(options, budget, &["show", &format!("{base}:{path}")])?;
        use sha2::{Digest, Sha256};
        files.insert(path.into(), format!("sha256:{:x}", Sha256::digest(&bytes)));
    }
    profile::AcceptedExecutor::from_captured(base.into(), tree, SourceInventory { files })
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Carrier files are also bound to ordinary tracked modes. Byte-only source
/// discovery cannot admit a symlink or an executable-mode change as equivalent.
pub(super) fn verify_carrier_modes(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    head: &str,
) -> io::Result<()> {
    for (path, _) in profile::carrier_files() {
        let entry = git(options, budget, &["ls-tree", head, "--", path])?;
        if !entry.starts_with(b"100644 blob ") || entry.iter().filter(|v| **v == b'\n').count() != 1
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                profile::PlanningFailure::ControllerContractUnsupported(format!(
                    "carrier mode: {path}"
                )),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_run(root: &Path, args: &[&str]) -> String {
        let out = Command::new("/usr/bin/git")
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn definition_entry_allows_100755_only_for_shell_scripts() {
        let sh = "assets/tools/source_ci/pre-build-validate.sh";
        let rs = "assets/tools/source_ci_profile.rs";
        assert!(is_valid_definition_entry(
            sh,
            b"100644 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci/pre-build-validate.sh\n"
        ));
        assert!(is_valid_definition_entry(
            sh,
            b"100755 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci/pre-build-validate.sh\n"
        ));
        assert!(!is_valid_definition_entry(
            sh,
            b"120000 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci/pre-build-validate.sh\n"
        ));
        assert!(is_valid_definition_entry(
            rs,
            b"100644 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci_profile.rs\n"
        ));
        assert!(!is_valid_definition_entry(
            rs,
            b"100755 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci_profile.rs\n"
        ));
        assert!(!is_valid_definition_entry(
            sh,
            b"100754 blob 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci/pre-build-validate.sh\n"
        ));
        assert!(!is_valid_definition_entry(
            sh,
            b"160000 commit 0123456789abcdef0123456789abcdef01234567\tassets/tools/source_ci/pre-build-validate.sh\n"
        ));
        assert!(!is_valid_definition_entry(sh, b""));
    }

    #[test]
    fn capture_admits_executable_sh_definition_scripts_and_rejects_executable_rust() {
        let tmp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let root = tmp.path().join("repo");
        let evidence = tmp.path().join("evidence");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&evidence).unwrap();

        git_run(&root, &["init", "-b", "master"]);
        git_run(&root, &["config", "user.name", "Executor Contract Test"]);
        git_run(&root, &["config", "user.email", "test@example.invalid"]);
        for (path, bytes) in profile::definition_files() {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, bytes).unwrap();
        }
        for (path, bytes) in profile::carrier_files() {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, bytes).unwrap();
        }
        git_run(&root, &["add", "-A"]);
        git_run(
            &root,
            &[
                "update-index",
                "--chmod=+x",
                "assets/tools/source_ci/pre-build-validate.sh",
                "assets/tools/source_ci/shard-status.sh",
            ],
        );
        git_run(&root, &["commit", "-m", "init with executable sh helpers"]);
        let base_ok = git_run(&root, &["rev-parse", "HEAD"]);

        let options = Options {
            root: root.clone(),
            git: PathBuf::from("/usr/bin/git").canonicalize().unwrap(),
            repository: "DecapodLabs/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: evidence,
            timeout: Duration::from_secs(30),
            max_input_bytes: 32 * 1024 * 1024,
            max_files: 5000,
            max_output_bytes: 512 * 1024,
            target: None,
        };
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + options.timeout,
            cancel: &cancel,
            max_output_bytes: options.max_output_bytes,
            bytes_left: options.max_input_bytes,
        };
        let captured = capture(&options, &budget, &base_ok, true)
            .expect("capture should admit 100755 .sh scripts")
            .expect("dual-root executor should be captured");
        assert_eq!(captured.revision(), base_ok);
        verify_carrier_modes(&options, &budget, &base_ok)
            .expect("carrier modes at 100644 should verify");

        // Self-hosting check: all `assets/tools/source_ci/*.sh` scripts as 100755 are admitted.
        let mut all_sh_chmod = vec!["update-index", "--chmod=+x"];
        let sh_paths: Vec<&str> = profile::definition_files()
            .into_iter()
            .map(|(p, _)| p)
            .filter(|p| p.ends_with(".sh"))
            .collect();
        assert!(!sh_paths.is_empty());
        all_sh_chmod.extend(sh_paths.iter().copied());
        git_run(&root, &all_sh_chmod);
        git_run(&root, &["commit", "-m", "all source_ci sh executable"]);
        let base_all_sh = git_run(&root, &["rev-parse", "HEAD"]);
        let captured_all = capture(&options, &budget, &base_all_sh, true)
            .expect("capture should admit all 100755 .sh definition scripts")
            .expect("dual-root executor should be captured");
        assert_eq!(captured_all.revision(), base_all_sh);

        // Unauthorized mutation of a definition script fails closed.
        let mutated_path = root.join("assets/tools/source_ci/pre-build-validate.sh");
        let original_sh = fs::read(&mutated_path).unwrap();
        fs::write(&mutated_path, b"#!/bin/sh\necho tampered\n").unwrap();
        git_run(
            &root,
            &["add", "assets/tools/source_ci/pre-build-validate.sh"],
        );
        git_run(&root, &["commit", "-m", "mutated sh definition"]);
        let base_mutated = git_run(&root, &["rev-parse", "HEAD"]);
        let err = capture(&options, &budget, &base_mutated, true)
            .expect_err("capture must reject unauthorized definition mutation");
        assert!(
            err.to_string().contains("accepted executor closure"),
            "unexpected error: {err}"
        );
        fs::write(&mutated_path, &original_sh).unwrap();
        git_run(
            &root,
            &["add", "assets/tools/source_ci/pre-build-validate.sh"],
        );
        git_run(&root, &["commit", "-m", "restore sh definition"]);

        // Symlink (120000) on a .sh definition file fails closed.
        let blob_oid = git_run(
            &root,
            &[
                "rev-parse",
                "HEAD:assets/tools/source_ci/pre-build-validate.sh",
            ],
        );
        git_run(
            &root,
            &[
                "update-index",
                "--cacheinfo",
                &format!("120000,{blob_oid},assets/tools/source_ci/pre-build-validate.sh"),
            ],
        );
        git_run(&root, &["commit", "-m", "symlink sh definition"]);
        let base_symlink = git_run(&root, &["rev-parse", "HEAD"]);
        let err = capture(&options, &budget, &base_symlink, true)
            .expect_err("capture must reject symlink 120000 on .sh definition");
        assert!(
            err.to_string()
                .contains("accepted executor mode: assets/tools/source_ci/pre-build-validate.sh"),
            "unexpected error: {err}"
        );
        git_run(
            &root,
            &[
                "update-index",
                "--cacheinfo",
                &format!("100755,{blob_oid},assets/tools/source_ci/pre-build-validate.sh"),
            ],
        );
        git_run(&root, &["commit", "-m", "restore 100755 sh definition"]);

        // Executable protocol marker (.rs) fails closed.
        git_run(
            &root,
            &[
                "update-index",
                "--chmod=+x",
                "assets/tools/source_ci_custody.rs",
            ],
        );
        git_run(&root, &["commit", "-m", "executable protocol marker"]);
        let base_bad_marker = git_run(&root, &["rev-parse", "HEAD"]);
        let err = capture(&options, &budget, &base_bad_marker, true)
            .expect_err("capture must reject 100755 protocol marker");
        assert!(
            err.to_string().contains("accepted executor protocol mode"),
            "unexpected error: {err}"
        );
        git_run(
            &root,
            &[
                "update-index",
                "--chmod=-x",
                "assets/tools/source_ci_custody.rs",
            ],
        );
        git_run(&root, &["commit", "-m", "restore protocol marker"]);

        // Executable .rs definition file fails closed.
        git_run(
            &root,
            &[
                "update-index",
                "--chmod=+x",
                "assets/tools/source_ci_profile.rs",
            ],
        );
        git_run(&root, &["commit", "-m", "executable rust definition"]);
        let base_bad = git_run(&root, &["rev-parse", "HEAD"]);
        let err = capture(&options, &budget, &base_bad, true)
            .expect_err("capture must still reject 100755 .rs files");
        assert!(
            err.to_string()
                .contains("accepted executor mode: assets/tools/source_ci_profile.rs"),
            "unexpected error: {err}"
        );

        // Executable carrier (.sh in .github/scripts) and symlink carrier fail closed.
        git_run(
            &root,
            &[
                "update-index",
                "--chmod=+x",
                ".github/scripts/release-build.sh",
            ],
        );
        git_run(&root, &["commit", "-m", "executable carrier script"]);
        let head_bad_carrier = git_run(&root, &["rev-parse", "HEAD"]);
        let err = verify_carrier_modes(&options, &budget, &head_bad_carrier)
            .expect_err("verify_carrier_modes must reject 100755 carrier");
        assert!(
            err.to_string()
                .contains("carrier mode: .github/scripts/release-build.sh"),
            "unexpected error: {err}"
        );

        let carrier_oid = git_run(
            &root,
            &["rev-parse", "HEAD:.github/actions/ci-context/action.yml"],
        );
        git_run(
            &root,
            &[
                "update-index",
                "--cacheinfo",
                &format!("120000,{carrier_oid},.github/actions/ci-context/action.yml"),
            ],
        );
        git_run(&root, &["commit", "-m", "symlink carrier action"]);
        let head_symlink_carrier = git_run(&root, &["rev-parse", "HEAD"]);
        let err = verify_carrier_modes(&options, &budget, &head_symlink_carrier)
            .expect_err("verify_carrier_modes must reject symlink carrier");
        assert!(
            err.to_string()
                .contains("carrier mode: .github/actions/ci-context/action.yml"),
            "unexpected error: {err}"
        );
    }
}
