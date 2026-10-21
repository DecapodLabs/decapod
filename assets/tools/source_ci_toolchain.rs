//! Concrete declared toolchain closure. No installation, arbitrary commands, or
//! whole-machine inventory; source setup must make required tools/images ready.
use super::*;

pub const ENVIRONMENT_KEYS: &[&str] = &[
    "RUSTUP_TOOLCHAIN",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_RUSTC",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_TARGET",
    "CARGO_TARGET_DIR",
    "CARGO_INCREMENTAL",
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER",
    "CROSS_CONTAINER_ENGINE",
    "CROSS_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_IMAGE",
    "USE_BAZEL_VERSION",
    "BAZELISK_HOME",
    "CC",
    "CXX",
    "AR",
    "LD",
    "CFLAGS",
    "CXXFLAGS",
    "LDFLAGS",
];

pub const SOURCE_CONFIG_PATHS: &[&str] = &[
    ".cargo/config",
    ".cargo/config.toml",
    "Cross.toml",
    ".bazelversion",
    ".bazeliskrc",
    ".bazelrc",
    ".config/build/bazelrc",
    ".config/build/rust-toolchain.toml",
    "rust-toolchain.toml",
    "rust-toolchain",
];

/// Validate the complete finite environment map, including recipe-owned overrides.
/// Unknown keys and executable/wrapper substitutions are never silently accepted.
pub fn validate_environment(
    environment: &BTreeMap<String, Option<String>>,
    recipe: &[Invocation],
) -> io::Result<()> {
    if environment.len() != ENVIRONMENT_KEYS.len()
        || ENVIRONMENT_KEYS
            .iter()
            .any(|key| !environment.contains_key(*key))
    {
        return Err(io::Error::other(
            "missing or unknown effective environment key",
        ));
    }
    for (key, value) in environment {
        if [
            "CC",
            "CXX",
            "AR",
            "LD",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_TARGET",
        ]
        .contains(&key.as_str())
            && value.as_deref().is_some_and(|v| !v.is_empty())
        {
            return Err(io::Error::other(format!(
                "undeclared source toolchain override: {key}"
            )));
        }
        if key == "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"
            && value.as_deref().is_some_and(|v| v != "musl-gcc")
        {
            return Err(io::Error::other("undeclared musl linker"));
        }
        if key == "CROSS_CONTAINER_ENGINE" && value.as_deref().is_some_and(|v| v != "docker") {
            return Err(io::Error::other("undeclared cross engine"));
        }
        if key == "CROSS_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_IMAGE"
            && value.as_deref().is_some_and(|v| v != ARM_IMAGE)
        {
            return Err(io::Error::other("undeclared cross image"));
        }
        if value
            .as_deref()
            .is_some_and(|v| v.len() > 4096 || v.contains(['\n', '\r', '\0']))
        {
            return Err(io::Error::other("invalid declared toolchain environment"));
        }

        for invocation in recipe {
            for (name, required) in &invocation.env {
                if name == key && value.as_deref() != *required {
                    return Err(io::Error::other(
                        "effective environment differs from reviewed recipe",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn selected_rust_programs(check: &Check) -> BTreeSet<&'static str> {
    if !check
        .capabilities
        .iter()
        .any(|c| *c == "cargo" || c.starts_with("rustc"))
    {
        return BTreeSet::new();
    }
    let mut programs = BTreeSet::from(["cargo", "rustc"]);
    for program in ["rustfmt", "cargo-clippy"] {
        if check.capabilities.contains(&program) {
            programs.insert(program);
        }
    }
    if check.capabilities.contains(&"cargo-clippy") {
        programs.insert("clippy-driver");
    }
    programs
}

fn probe(
    repo: &Path,
    dir: &Path,
    name: &str,
    program: &Path,
    args: &[&str],
) -> io::Result<(String, String)> {
    let mut command = Command::new(program);
    command.args(args).current_dir(repo);
    let out = execute(command, dir, name, false)?;
    if !out.status.success()
        || out.stdout.total != out.stdout.stored
        || out.stderr.total != out.stderr.stored
    {
        return Err(io::Error::other(format!(
            "declared toolchain probe failed: {name}"
        )));
    }
    let bytes = fs::read(dir.join(&out.stdout.path))?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("toolchain response exceeds bound"));
    }
    Ok((
        String::from_utf8(bytes).map_err(io::Error::other)?,
        format!(
            "{{\"probe\":{},\"stdout\":{},\"stderr\":{}}}",
            json_string(name),
            out.stdout.json(),
            out.stderr.json()
        ),
    ))
}
fn selected_rust(
    program: &str,
    repo: &Path,
    dir: &Path,
    prefix: &str,
    logs: &mut Vec<String>,
) -> io::Result<PathBuf> {
    let launcher = resolve(program, repo)?;
    let actual = fs::canonicalize(&launcher)?;
    if actual
        .file_name()
        .is_some_and(|name| name == "rustup" || name == "rustup-init")
    {
        // rustup dispatches by argv[0]. macOS launchers can resolve to
        // rustup-init, which must not be invoked as if it were rustc or rustup.
        let rustup = resolve("rustup", repo)?;
        if fs::canonicalize(&rustup)? != actual {
            return Err(io::Error::other(
                "Rust launcher differs from selected rustup",
            ));
        }
        let (path, log) = probe(
            repo,
            dir,
            &format!("{prefix}rustup-{program}"),
            &rustup,
            &["which", program],
        )?;
        logs.push(log);
        let path = PathBuf::from(path.trim());
        if !path.is_absolute() || !path.is_file() {
            return Err(io::Error::other(
                "rustup did not identify an installed tool",
            ));
        }
        fs::canonicalize(path)
    } else {
        Ok(actual)
    }
}
fn file_inventory(root: &Path, extension: Option<&[&str]>) -> io::Result<Vec<String>> {
    let mut entries = fs::read_dir(root)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();
    if entries.len() > 512 {
        return Err(io::Error::other(
            "declared toolchain file inventory exceeds bound",
        ));
    }
    let mut result = Vec::new();
    let mut size = 0u64;
    for path in entries {
        if !path.is_file() {
            continue;
        }
        if extension.is_some_and(|exts| {
            !path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| exts.contains(&ext))
        }) {
            continue;
        }
        size = size
            .checked_add(path.metadata()?.len())
            .ok_or_else(|| io::Error::other("toolchain length overflow"))?;
        if size > 2 * 1024 * 1024 * 1024 {
            return Err(io::Error::other("declared toolchain bytes exceed bound"));
        }
        result.push(format!(
            "{{\"path\":{},\"sha256\":{}}}",
            json_string(&path.file_name().unwrap().to_string_lossy()),
            json_string(&file_sha(&path)?)
        ));
    }
    if result.is_empty() {
        return Err(io::Error::other(
            "declared toolchain library directory is empty",
        ));
    }
    Ok(result)
}

fn package_files(
    root: &Path,
    current: &Path,
    rows: &mut Vec<String>,
    size: &mut u64,
) -> io::Result<()> {
    let mut entries = fs::read_dir(current)?
        .map(|v| v.map(|v| v.path()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();
    for path in entries {
        let metadata = path.symlink_metadata()?;
        if metadata.file_type().is_symlink() {
            // npm's .bin launch links are not consumed by the Ajv entrypoint.
            if path.components().any(|p| p.as_os_str() == ".bin") {
                continue;
            }
            return Err(io::Error::other("external dependency link in Ajv package"));
        }
        if metadata.is_dir() {
            package_files(root, &path, rows, size)?;
        } else if metadata.is_file() {
            *size += metadata.len();
            if rows.len() >= 10000 || *size > 128 * 1024 * 1024 {
                return Err(io::Error::other("Ajv dependency closure exceeds bound"));
            }
            rows.push(format!(
                "{{\"path\":{},\"sha256\":{}}}",
                json_string(&path.strip_prefix(root).unwrap().to_string_lossy()),
                json_string(&file_sha(&path)?)
            ));
        } else {
            return Err(io::Error::other("special file in Ajv package"));
        }
    }
    Ok(())
}

pub fn measure_declared(
    check: &Check,
    recipe: &[Invocation],
    repo: &Path,
    dir: &Path,
    prefix: &str,
) -> io::Result<(Vec<String>, Vec<String>, bool)> {
    let mut records = Vec::new();
    let mut logs = Vec::new();
    // Reject undeclared executable wrappers and code-generation overrides. Do
    // not dump credentials or silently sanitize an environment into a different
    // source check. Fixed recipe env mutations take precedence exactly as run.
    let mut environment = BTreeMap::new();
    for &key in ENVIRONMENT_KEYS {
        let mut value = env::var(key).ok();
        for invocation in recipe {
            for (name, replacement) in &invocation.env {
                if *name == key {
                    value = replacement.map(str::to_string);
                }
            }
        }
        environment.insert(key.to_string(), value);
    }
    validate_environment(&environment, recipe)?;
    for (key, value) in env::vars() {
        if key.starts_with("CARGO_TARGET_")
            && (key.ends_with("_RUSTFLAGS") || key.ends_with("_LINKER"))
            && key != "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"
            && !value.is_empty()
        {
            return Err(io::Error::other("undeclared target compiler override"));
        }
    }
    records.push(format!(
        "{{\"kind\":\"effective_environment\",\"values\":{{{}}}}}",
        environment
            .iter()
            .map(|(key, value)| format!(
                "{}:{}",
                json_string(key),
                value.as_deref().map_or_else(|| "null".into(), json_string)
            ))
            .collect::<Vec<_>>()
            .join(",")
    ));
    for path in SOURCE_CONFIG_PATHS {
        let file = repo.join(path);
        if file.is_file() {
            records.push(format!(
                "{{\"kind\":\"source_config\",\"path\":{},\"sha256\":{}}}",
                json_string(path),
                json_string(&file_sha(&file)?)
            ));
        }
    }
    // Cargo reads user and ancestor configuration as well as source config.
    // Source CI declares none externally; fail closed instead of exporting
    // potentially credential-bearing config into an evidence artifact.
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")))
        .ok_or_else(|| io::Error::other("Cargo home unavailable"))?;
    let mut roots = vec![cargo_home];
    roots.extend(repo.ancestors().skip(1).map(|p| p.join(".cargo")));
    if roots
        .iter()
        .any(|p| p.join("config").exists() || p.join("config.toml").exists())
    {
        return Err(io::Error::other("undeclared external Cargo configuration"));
    }
    records.push("{\"kind\":\"external_config\",\"cargo\":\"absent\"}".into());
    let rust = check
        .capabilities
        .iter()
        .any(|c| *c == "cargo" || c.starts_with("rustc"));
    if rust {
        let programs = selected_rust_programs(check);
        let mut rustc = None;
        for program in programs {
            let selected = selected_rust(program, repo, dir, prefix, &mut logs)?;
            records.push(format!(
                "{{\"kind\":\"selected_tool\",\"program\":{},\"sha256\":{}}}",
                json_string(program),
                json_string(&file_sha(&selected)?)
            ));
            if program == "rustc" {
                rustc = Some(selected);
            }
        }
        let rustc = rustc.unwrap();
        let (version, log) = probe(
            repo,
            dir,
            &format!("{prefix}rustc-verbose"),
            &rustc,
            &["-vV"],
        )?;
        logs.push(log);
        let host = version
            .lines()
            .find_map(|s| s.strip_prefix("host: "))
            .ok_or_else(|| io::Error::other("rustc omitted target host"))?;
        let (sysroot, log) = probe(
            repo,
            dir,
            &format!("{prefix}rust-sysroot"),
            &rustc,
            &["--print", "sysroot"],
        )?;
        logs.push(log);
        let sysroot = PathBuf::from(sysroot.trim());
        if !sysroot.is_absolute() {
            return Err(io::Error::other("compiler sysroot is not absolute"));
        }
        let mut targets = BTreeSet::from([host]);
        for capability in check.capabilities {
            if let Some(target) = capability.strip_prefix("rust-target:") {
                targets.insert(target);
            }
        }
        for target in targets {
            if !token(target) || target.contains('/') {
                return Err(io::Error::other("invalid compiler target"));
            }
            let files = file_inventory(
                &sysroot.join("lib/rustlib").join(target).join("lib"),
                Some(&["rlib", "so", "dylib", "a", "o"]),
            )?;
            records.push(format!(
                "{{\"kind\":\"rust_sysroot\",\"target\":{},\"files\":[{}]}}",
                json_string(target),
                files.join(",")
            ));
        }
    }
    if rust && check.id != "fmt" {
        let cc = resolve("cc", repo)?;
        let (_, log) = probe(
            repo,
            dir,
            &format!("{prefix}native-cc-version"),
            &cc,
            &["--version"],
        )?;
        logs.push(log);
        let (ld, log) = probe(
            repo,
            dir,
            &format!("{prefix}native-ld-path"),
            &cc,
            &["-print-prog-name=ld"],
        )?;
        logs.push(log);
        let linker = resolve(ld.trim(), repo)?;
        records.push(format!("{{\"kind\":\"native_compiler\",\"program\":\"cc\",\"sha256\":{},\"linker_sha256\":{}}}", json_string(&file_sha(&cc)?), json_string(&file_sha(&linker)?)));
    }
    if check.capabilities.contains(&"submodules:recursive") {
        let git = resolve("git", repo)?;
        let (status, log) = probe(
            repo,
            dir,
            &format!("{prefix}submodule-status"),
            &git,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "submodule",
                "status",
                "--recursive",
            ],
        )?;
        logs.push(log);
        let mut modules = Vec::new();
        for (index, line) in status.lines().enumerate() {
            if !line.starts_with(' ') {
                return Err(io::Error::other("submodule differs from source gitlink"));
            }
            let mut fields = line[1..].split_whitespace();
            let revision = fields.next().unwrap_or("");
            let path = fields.next().unwrap_or("");
            if !immutable_sha(revision)
                || path.is_empty()
                || path.starts_with('/')
                || path.split('/').any(|v| v == "..")
            {
                return Err(io::Error::other("invalid recursive submodule identity"));
            }
            let root = repo.join(path);
            let (tree, log) = probe(
                &root,
                dir,
                &format!("{prefix}submodule-{index}-tree"),
                &git,
                &["--no-replace-objects", "rev-parse", "HEAD^{tree}"],
            )?;
            logs.push(log);
            let (dirty, log) = probe(
                &root,
                dir,
                &format!("{prefix}submodule-{index}-clean"),
                &git,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )?;
            logs.push(log);
            if !dirty.is_empty() || !immutable_sha(tree.trim()) {
                return Err(io::Error::other("recursive submodule is not clean"));
            }
            modules.push(format!(
                "{{\"path\":{},\"revision\":{},\"tree\":{}}}",
                json_string(path),
                json_string(revision),
                json_string(tree.trim())
            ));
        }
        records.push(format!(
            "{{\"kind\":\"recursive_submodules\",\"clean\":true,\"modules\":[{}]}}",
            modules.join(",")
        ));
    }
    for capability in check.capabilities {
        if let Some(revision) = capability.strip_prefix("cross-rev:") {
            let cross = resolve("cross", repo)?;
            let (version, log) = probe(
                repo,
                dir,
                &format!("{prefix}cross-revision"),
                &cross,
                &["--version"],
            )?;
            logs.push(log);
            if !immutable_sha(revision) || !version.contains(&revision[..7]) {
                return Err(io::Error::other(
                    "selected cross revision differs from source policy",
                ));
            }
            records.push(format!("{{\"kind\":\"cross_revision\",\"revision\":{},\"version\":{},\"executable_sha256\":{}}}", json_string(revision), json_string(version.trim()), json_string(&file_sha(&cross)?)));
        }
    }
    if check.capabilities.contains(&"ajv") {
        let entry = fs::canonicalize(resolve("ajv", repo)?)?;
        let node = resolve("node", repo)?;
        // Fixed introspection code, never caller-supplied JavaScript or eval.
        let script = "const fs=require('fs'),p=require('path');let d=p.dirname(fs.realpathSync(process.argv[1]));for(let i=0;i<10;i++,d=p.dirname(d)){let f=p.join(d,'package.json');if(fs.existsSync(f)){let j=JSON.parse(fs.readFileSync(f,'utf8'));if(j.name==='ajv-cli'){console.log(d);console.log(j.version);process.exit(0)}}}process.exit(1)";
        let (package, log) = probe(
            repo,
            dir,
            &format!("{prefix}ajv-package"),
            &node,
            &[
                "-e",
                script,
                entry
                    .to_str()
                    .ok_or_else(|| io::Error::other("Ajv path encoding"))?,
            ],
        )?;
        logs.push(log);
        let mut lines = package.lines();
        let root = PathBuf::from(lines.next().unwrap_or(""));
        let version = lines.next().unwrap_or("");
        if !root.is_absolute() || !entry.starts_with(&root) || !token(version) {
            return Err(io::Error::other("invalid selected Ajv package"));
        }
        let mut files = Vec::new();
        package_files(&root, &root, &mut files, &mut 0)?;
        records.push(format!("{{\"kind\":\"node_package\",\"name\":\"ajv-cli\",\"version\":{},\"entry_sha256\":{},\"files\":[{}]}}", json_string(version), json_string(&file_sha(&entry)?), files.join(",")));
    }
    if check.capabilities.contains(&"bazelisk") {
        let launcher = resolve("bazelisk", repo)?;
        let (version, log) = probe(
            repo,
            dir,
            &format!("{prefix}bazel-selected-version"),
            &launcher,
            &["--version"],
        )?;
        logs.push(log);
        let version = version
            .trim()
            .strip_prefix("bazel ")
            .filter(|v| {
                v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            })
            .ok_or_else(|| io::Error::other("unrecognized Bazel version"))?;
        let cache = env::var_os("BAZELISK_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("XDG_CACHE_HOME").map(|p| PathBuf::from(p).join("bazelisk")))
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache/bazelisk")))
            .ok_or_else(|| io::Error::other("Bazelisk cache unavailable"))?;
        // Exact pinned-launcher cache contract, not a search for any green binary.
        let metadata = cache.join(format!(
            "downloads/metadata/bazelbuild/bazel-{version}-linux-x86_64"
        ));
        let hash = fs::read_to_string(metadata)?;
        if !digest(&format!("sha256:{}", hash.trim())) {
            return Err(io::Error::other("invalid selected Bazel cache identity"));
        }
        let selected = cache
            .join("downloads/sha256")
            .join(hash.trim())
            .join("bin/bazel");
        let actual = file_sha(&selected)?;
        if actual != format!("sha256:{}", hash.trim()) {
            return Err(io::Error::other(
                "selected Bazel bytes differ from launch metadata",
            ));
        }
        records.push(format!(
            "{{\"kind\":\"bazel_binary\",\"sha256\":{},\"version\":{}}}",
            json_string(&actual),
            json_string(version)
        ));
    }
    if check.capabilities.contains(&"musl-gcc") {
        let compiler = resolve("musl-gcc", repo)?;
        let wrapper = fs::read_to_string(&compiler)?;
        let words: Vec<_> = wrapper.split_whitespace().collect();
        let specs = words
            .windows(2)
            .find_map(|w| (w[0] == "-specs").then_some(w[1]))
            .or_else(|| words.iter().find_map(|w| w.strip_prefix("-specs=")))
            .ok_or_else(|| io::Error::other("cannot bind musl compiler specs"))?;
        let specs = PathBuf::from(specs.trim_matches(['\'', '"']));
        if !specs.is_absolute() || !specs.is_file() {
            return Err(io::Error::other("musl specs unavailable"));
        }
        for program in ["cc1", "ld"] {
            let (path, log) = probe(
                repo,
                dir,
                &format!("{prefix}musl-{program}"),
                &compiler,
                &[&format!("-print-prog-name={program}")],
            )?;
            logs.push(log);
            let selected = resolve(path.trim(), repo)?;
            records.push(format!(
                "{{\"kind\":\"native_linker\",\"program\":{},\"sha256\":{},\"specs_sha256\":{}}}",
                json_string(program),
                json_string(&file_sha(&selected)?),
                json_string(&file_sha(&specs)?)
            ));
        }
    }
    for reference in [
        check
            .capabilities
            .iter()
            .find_map(|c| c.strip_prefix("image:")),
        check
            .capabilities
            .contains(&"qemu-aarch64-in-pinned-image")
            .then_some(ARM_IMAGE),
    ]
    .into_iter()
    .flatten()
    {
        let docker = resolve("docker", repo)?;
        let name = if reference == ARM_IMAGE {
            "cross"
        } else {
            "fixture"
        };
        let (inspection, log) = probe(
            repo,
            dir,
            &format!("{prefix}docker-{name}-image"),
            &docker,
            &[
                "image",
                "inspect",
                "--format",
                DOCKER_IMAGE_FORMAT,
                reference,
            ],
        )?;
        logs.push(log);
        records.push(docker_image_record(reference, &inspection)?);
    }
    Ok((records, logs, true))
}

const DOCKER_IMAGE_FORMAT: &str = "{{.Id}}\n{{range .RepoDigests}}{{println .}}{{end}}";

fn docker_image_record(reference: &str, inspection: &str) -> io::Result<String> {
    let mut lines = inspection.lines();
    let id = lines.next().unwrap_or("");
    if !digest(id) {
        return Err(io::Error::other(
            "Docker image ID unavailable; source setup must pull declared image first",
        ));
    }
    let mut repo_digests = lines.filter(|line| !line.is_empty()).collect::<Vec<_>>();
    repo_digests.sort();
    if repo_digests.is_empty()
        || repo_digests
            .iter()
            .any(|value| value.rsplit_once('@').is_none_or(|(_, hash)| !digest(hash)))
        || (reference.contains('@') && !repo_digests.contains(&reference))
    {
        return Err(io::Error::other(
            "Docker repository digest does not prove declared image",
        ));
    }
    Ok(format!(
        "{{\"kind\":\"docker_image\",\"reference\":{},\"id\":{},\"repo_digests\":{}}}",
        json_string(reference),
        json_string(id),
        json_strings(repo_digests)
    ))
}

#[cfg(test)]
mod live_runner_regressions {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn temporary() -> PathBuf {
        let root = env::temp_dir().join(format!(
            "source-toolchain-live-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        // Tool resolution records canonical paths. Keep this newly owned
        // fixture in the same namespace when TMPDIR is a symlink on macOS.
        root.canonicalize().unwrap()
    }
    #[cfg(unix)]
    fn executable(path: &Path, source: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    fn isolated(mode: &str) {
        let root = temporary();
        let listed = Command::new(env::current_exe().unwrap())
            .arg("--list")
            .output()
            .unwrap();
        let listed = String::from_utf8(listed.stdout).unwrap();
        let name = listed
            .lines()
            .find(|line| {
                line.ends_with(
                    "declared_toolchain::live_runner_regressions::isolated_toolchain_child: test",
                )
            })
            .unwrap()
            .strip_suffix(": test")
            .unwrap();
        let out = Command::new(env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("SOURCE_CI_TOOLCHAIN_CHILD", &root)
            .env("SOURCE_CI_TOOLCHAIN_MODE", mode)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        if mode == "preparation" {
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("\"preparation_complete\":false")
            );
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("source-ci-profile: preparation failed for golden-vectors-")
            );
            assert!(String::from_utf8_lossy(&out.stderr).contains("required platform"));
        }
        assert!(
            root.join("completed").is_file(),
            "child test was not selected"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn cold_bazel_probe_preserves_logs_and_rejects_changed_selected_bytes() {
        isolated("bazel");
    }
    #[cfg(unix)]
    #[test]
    fn macos_style_rustup_init_symlink_preserves_dispatch_and_selected_binary() {
        isolated("rustup-init");
    }
    #[cfg(unix)]
    #[test]
    fn unavailable_preparation_surfaces_the_actual_failure() {
        isolated("preparation");
    }
    #[cfg(unix)]
    #[test]
    fn isolated_toolchain_child() {
        let Some(root) = env::var_os("SOURCE_CI_TOOLCHAIN_CHILD") else {
            return;
        };
        let root = PathBuf::from(root);
        let mode = env::var("SOURCE_CI_TOOLCHAIN_MODE").unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        fs::create_dir(root.join("cargo-home")).unwrap();
        // This mutation is confined to this exact one-test child process.
        unsafe {
            if mode == "preparation" {
                env::set_var(
                    "PATH",
                    format!("{}:/usr/bin:/bin", root.join("bin").display()),
                );
            } else {
                env::set_var("PATH", root.join("bin"));
            }
            env::set_var("HOME", &root);
            env::set_var("CARGO_HOME", root.join("cargo-home"));
            env::set_var("BAZELISK_HOME", root.join("bazel-cache"));
            for key in ENVIRONMENT_KEYS {
                if *key != "BAZELISK_HOME" {
                    env::remove_var(key);
                }
            }
        }
        if mode == "preparation" {
            let git = |args: &[&str]| {
                let out = Command::new("git")
                    .current_dir(&root)
                    .args([
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.invalid",
                        "-c",
                        "commit.gpgsign=false",
                        "-c",
                        "core.hooksPath=/dev/null",
                    ])
                    .args(args)
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_COUNT", "0")
                    .env_remove("GIT_CONFIG_PARAMETERS")
                    .output()
                    .unwrap();
                assert!(out.status.success(), "git {args:?}: {out:?}");
                String::from_utf8(out.stdout).unwrap().trim().to_owned()
            };
            git(&["init", "-q", "-b", "master"]);
            fs::write(root.join("source"), "base").unwrap();
            git(&["add", "source"]);
            git(&["commit", "-qm", "base"]);
            let base = git(&["rev-parse", "HEAD"]);
            fs::write(root.join("source"), "head").unwrap();
            git(&["add", "source"]);
            git(&["commit", "-qm", "head"]);
            let head = git(&["rev-parse", "HEAD"]);
            let check = find_check(if env::consts::OS == "macos" {
                "golden-vectors-linux"
            } else {
                "golden-vectors-macos"
            })
            .unwrap();
            let recipe = commands(check, &[]).unwrap();
            let ctx = Context {
                snapshot: head.clone(),
                validation_head: head.clone(),
                tree: git(&["rev-parse", "HEAD^{tree}"]),
                provider: "github".into(),
                run_id: "12".into(),
                run_attempt: "1".into(),
                job_id: check.job.into(),
                step_key: String::new(),
                expected_profile: profile_digest().unwrap(),
                expected_invocation: invocation_digest(&recipe).unwrap(),
                expected_toolchain: None,
                preparation_file: None,
                evidence_dir: root.join("preparation"),
                executor: None,
            };
            assert!(!super::super::run(&root, check, &[], &base, &head, &ctx, true).unwrap());
            assert!(
                fs::read_to_string(root.join("preparation/preparation.json"))
                    .unwrap()
                    .contains("\"preparation_complete\":false")
            );
        } else if mode == "bazel" {
            executable(
                &root.join("bin/python3"),
                "#!/bin/sh\nprintf 'Python 3.12.3\\n'\n",
            );
            executable(
                &root.join("bin/bazelisk"),
                "#!/bin/sh\nif [ ! -e \"$HOME/warmed\" ]; then\n printf 'Downloading declared Bazel on cold cache\\n' >&2\n : > \"$HOME/warmed\"\nfi\nprintf 'bazel 9.3.0\\n'\n",
            );
            let bytes = b"selected Bazel fixture bytes";
            let hash = sha(bytes)
                .unwrap()
                .strip_prefix("sha256:")
                .unwrap()
                .to_owned();
            let cache = root.join("bazel-cache/downloads");
            fs::create_dir_all(cache.join("metadata/bazelbuild")).unwrap();
            fs::create_dir_all(cache.join("sha256").join(&hash).join("bin")).unwrap();
            fs::write(
                cache.join("metadata/bazelbuild/bazel-9.3.0-linux-x86_64"),
                &hash,
            )
            .unwrap();
            let selected = cache.join("sha256").join(&hash).join("bin/bazel");
            fs::write(&selected, bytes).unwrap();
            let check = find_check("setup-bazel").unwrap();
            let recipe = commands(check, &[]).unwrap();
            let first = root.join("first");
            let second = root.join("second");
            fs::create_dir(&first).unwrap();
            fs::create_dir(&second).unwrap();
            let a = super::super::measure_tools(check, &recipe, &root, &first, "", &root).unwrap();
            let b = super::super::measure_tools(check, &recipe, &root, &second, "", &root).unwrap();
            assert_eq!(
                a.0, b.0,
                "cold setup must not change stable toolchain identity"
            );
            assert!(a.2 && b.2);
            assert!(
                fs::read_to_string(first.join("bazel-selected-version.stderr.log"))
                    .unwrap()
                    .contains("Downloading declared Bazel")
            );
            assert!(a.1.contains("bazel-selected-version.stderr.log"));
            assert!(a.0.contains(&hash), "selected bytes must remain bound");
            fs::write(selected, "different executable bytes").unwrap();
            let changed = root.join("changed");
            fs::create_dir(&changed).unwrap();
            let error = super::super::measure_tools(check, &recipe, &root, &changed, "", &root)
                .unwrap_err();
            assert!(
                error.to_string().contains("selected Bazel bytes differ"),
                "{error}"
            );
        } else {
            use std::os::unix::fs::symlink;
            fs::create_dir(root.join("selected")).unwrap();
            executable(
                &root.join("selected/rustc"),
                "#!/bin/sh\nprintf 'rustc 1.96.1\\nhost: aarch64-apple-darwin\\n'\n",
            );
            executable(
                &root.join("bin/rustup-init"),
                "#!/bin/sh\ncase \"${0##*/}\" in\n rustup) [ \"$1\" = which ] || exit 2; printf '%s/selected/%s\\n' \"$HOME\" \"$2\";;\n *) printf 'rustup-init 1.29.0\\n';;\nesac\n",
            );
            symlink("rustup-init", root.join("bin/rustc")).unwrap();
            symlink("rustup-init", root.join("bin/rustup")).unwrap();
            let evidence = root.join("evidence");
            fs::create_dir(&evidence).unwrap();
            let mut logs = Vec::new();
            let selected = selected_rust("rustc", &root, &evidence, "", &mut logs).unwrap();
            assert_eq!(selected, root.join("selected/rustc"));
            let (version, _) = probe(&root, &evidence, "compiler", &selected, &["-vV"]).unwrap();
            assert!(version.contains("host: aarch64-apple-darwin"));
            assert!(logs[0].contains("rustup-rustc"));
            // A different rustup on PATH cannot claim the selected launcher.
            fs::remove_file(root.join("bin/rustup")).unwrap();
            executable(&root.join("bin/rustup"), "#!/bin/sh\nexit 0\n");
            assert!(selected_rust("rustc", &root, &evidence, "other-", &mut logs).is_err());
        }
        fs::write(root.join("completed"), mode).unwrap();
    }
    #[test]
    fn docker_range_format_accepts_json_array_digests_and_keeps_strict_identity() {
        assert!(DOCKER_IMAGE_FORMAT.contains("range .RepoDigests"));
        assert!(!DOCKER_IMAGE_FORMAT.contains("join"));
        let id = format!("sha256:{}", "1".repeat(64));
        let reference = format!("registry/fixture@sha256:{}", "2".repeat(64));
        assert!(docker_image_record(&reference, &format!("{id}\n{reference}\n")).is_ok());
        for bad in [
            format!("\n{reference}\n"),
            format!("{id}\n"),
            format!("{id}\nregistry/fixture@invalid\n"),
            format!("{id}\nregistry/other@sha256:{}\n", "3".repeat(64)),
        ] {
            assert!(docker_image_record(&reference, &bad).is_err(), "{bad}");
        }
    }
}
