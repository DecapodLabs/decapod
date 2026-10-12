//! Finite preparation policy for the shared source executor. Provider custody
//! authenticates bytes; this module independently decides whether those measured
//! bytes implement the frozen source contract before reading execution success.
use super::*;
use provider::source_execution::{MeasuredPreparation, SourceExpectation};
use serde_json::Value;

fn ensure(ok: bool, reason: &str) -> io::Result<()> {
    if ok { Ok(()) } else { Err(invalid(reason)) }
}
fn field<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("missing measured {key}")))
}
fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn regular_source_hash(root: &Path, path: &str) -> io::Result<String> {
    ensure(
        !path.starts_with('/') && !path.split('/').any(|c| matches!(c, "" | "." | "..")),
        "invalid source configuration path",
    )?;
    let mut full = root.join(path);
    let mut metadata = fs::symlink_metadata(&full)?;
    if metadata.file_type().is_symlink() {
        // The repository's tracked Rust-toolchain alias is a reviewed source
        // symlink. Both link bytes and target bytes are bound by H/material.
        ensure(
            path == "rust-toolchain.toml"
                && fs::read_link(&full)? == Path::new(".config/build/rust-toolchain.toml"),
            "unreviewed source configuration symlink",
        )?;
        full = root.join(".config/build/rust-toolchain.toml");
        metadata = fs::symlink_metadata(&full)?;
    }
    ensure(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 4 * 1024 * 1024,
        "source configuration is not a bounded regular file",
    )?;
    Ok(hash(&fs::read(full)?))
}
fn version_output(measured: &MeasuredPreparation<'_>, program: &str) -> io::Result<String> {
    let raw = measured
        .files
        .file(&format!("{}tool-logs.json", measured.member_prefix))?;
    let logs: Value = serde_json::from_slice(raw).map_err(io::Error::other)?;
    let rows: Vec<_> = logs
        .as_array()
        .ok_or_else(|| invalid("tool log inventory missing"))?
        .iter()
        .filter(|v| v["program"] == program)
        .collect();
    ensure(rows.len() == 1, "expected unique actual tool version log")?;
    let mut bytes = Vec::new();
    for side in ["stdout", "stderr"] {
        let name = field(&rows[0][side], "path")?;
        ensure(
            !name.contains('/') && !name.contains('\\'),
            "invalid version log member",
        )?;
        bytes.extend_from_slice(
            measured
                .files
                .file(&format!("{}{name}", measured.member_prefix))?,
        );
    }
    ensure(
        bytes.len() <= 128 * 1024,
        "version log exceeds finite policy bound",
    )?;
    String::from_utf8(bytes).map_err(io::Error::other)
}

/// The known input map is built only from independently authenticated earlier
/// producer artifacts. A receipt cannot supply its own expected binary hashes.
pub(super) fn verify(
    project: &CurrentProject,
    check: &source::Check,
    expected: &SourceExpectation,
    measured: MeasuredPreparation<'_>,
    known_inputs: &integration::Dependencies,
) -> io::Result<()> {
    ensure(
        expected.profile_digest == source::custody::profile_digest()?,
        "prepared executor differs from compiled source profile",
    )?;
    let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
    let recipe = source::commands(check, &paths)?;
    ensure(
        expected.invocations == source::custody::recipe_json(&recipe).as_bytes(),
        "preparation recipe differs from independent source",
    )?;
    ensure(
        expected.workflow == check.workflow
            && expected.job == check.job
            && expected.check == check.id
            && expected.matrix.as_deref() == check.matrix
            && expected.platform == check.platform.name(),
        "preparation differs from source check/matrix/platform",
    )?;
    let context = measured
        .context
        .as_object()
        .ok_or_else(|| invalid("effective context missing"))?;
    for (key, value) in [
        ("SOURCE_CI_EVENT", project.facts.event.as_str()),
        ("SOURCE_CI_BASE", expected.base.as_str()),
        ("SOURCE_CI_HEAD", expected.head.as_str()),
        ("SOURCE_CI_MERGE_BASE", expected.base.as_str()),
    ] {
        ensure(
            context.get(key).and_then(Value::as_str) == Some(value),
            "effective source context differs from current B/H/event",
        )?;
    }
    let release_only = !source::workflow_owned::artifact_worker(check)
        && source::workflow_owned::release_only(
            &project.identity_inputs.root,
            &project.facts.event,
            &expected.base,
            &expected.head,
        );
    ensure(
        context
            .get("SOURCE_CI_RELEASE_ONLY")
            .and_then(Value::as_str)
            == Some(if release_only { "true" } else { "false" }),
        "release-only context differs from independent current classification",
    )?;
    if check.id.starts_with("integration-") {
        integration::preparation(project, check, expected, &measured, known_inputs)?;
    } else {
        ensure(context.len() == 5, "undeclared effective source context")?;
    }
    let rows = measured
        .toolchain
        .as_array()
        .ok_or_else(|| invalid("declared toolchain inventory missing"))?;
    let mut programs = BTreeSet::new();
    let mut selected = BTreeSet::new();
    let mut sysroots = BTreeSet::new();
    let mut configs = BTreeSet::new();
    let mut recipes = BTreeSet::new();
    let mut inputs = BTreeSet::new();
    let mut images = BTreeSet::new();
    let mut native_linkers = BTreeSet::new();
    let mut environment = false;
    let mut bazel = false;
    let mut external_config = false;
    let mut native_compiler = false;
    let mut submodules = false;
    let mut cross_revision = None;
    let mut node_package = false;
    for row in rows {
        match row.get("kind").and_then(Value::as_str) {
            None => {
                let program = field(row, "program")?;
                ensure(programs.insert(program), "duplicate version probe")?;
                ensure(
                    digest(field(row, "executable_sha256")?),
                    "unmeasured direct executable",
                )?;
            }
            Some("selected_tool") => {
                ensure(
                    selected.insert(field(row, "program")?) && digest(field(row, "sha256")?),
                    "duplicate/unmeasured selected tool",
                )?;
            }
            Some("rust_sysroot") => {
                ensure(
                    sysroots.insert(field(row, "target")?),
                    "duplicate Rust target closure",
                )?;
                let files = row["files"]
                    .as_array()
                    .ok_or_else(|| invalid("Rust target files missing"))?;
                let mut names = BTreeSet::new();
                ensure(
                    !files.is_empty() && files.len() <= 512,
                    "empty/oversized Rust target closure",
                )?;
                for file in files {
                    let name = field(file, "path")?;
                    ensure(
                        !name.contains('/') && names.insert(name) && digest(field(file, "sha256")?),
                        "invalid Rust target library identity",
                    )?;
                }
                ensure(
                    names.iter().any(|n| n.starts_with("libstd-")),
                    "Rust standard library absent",
                )?;
            }
            Some("source_config") => {
                let path = field(row, "path")?;
                ensure(
                    CONFIG_PATHS.contains(&path)
                        && configs.insert(path)
                        && field(row, "sha256")?
                            == regular_source_hash(&project.identity_inputs.root, path)?,
                    "source configuration hash mismatch",
                )?;
            }
            Some("source_recipe") => {
                let path = field(row, "path")?;
                let expected = source::workflow_owned::script_sources(check)
                    .into_iter()
                    .find(|(name, _)| *name == path)
                    .ok_or_else(|| invalid("undeclared source recipe input"))?;
                ensure(
                    recipes.insert(path)
                        && field(row, "sha256")? == hash(expected.1)
                        && regular_source_hash(&project.identity_inputs.root, path)?
                            == hash(expected.1),
                    "fixed source recipe hash mismatch",
                )?;
            }
            Some("input_artifact") => {
                let path = field(row, "path")?;
                ensure(
                    inputs.insert(path)
                        && known_inputs.inputs.get(path).map(String::as_str)
                            == Some(field(row, "sha256")?)
                        && row["bytes"].as_u64().is_some_and(|n| n > 0),
                    "missing independently authenticated producer input",
                )?;
            }
            Some("external_config") => {
                ensure(
                    !external_config && row["cargo"] == "absent",
                    "ambient Cargo configuration not excluded",
                )?;
                external_config = true;
            }
            Some("native_compiler") => {
                ensure(
                    !native_compiler
                        && row["program"] == "cc"
                        && digest(field(row, "sha256")?)
                        && digest(field(row, "linker_sha256")?),
                    "native compiler/linker closure incomplete",
                )?;
                native_compiler = true;
            }
            Some("recursive_submodules") => {
                // Material capture rejects gitlink mode160000. This finite
                // repository currently has no submodules; never drop an added
                // or changed recursive input from its source identity.
                ensure(
                    !submodules
                        && !project.identity_inputs.root.join(".gitmodules").exists()
                        && row["clean"] == true
                        && row["modules"].as_array().is_some_and(Vec::is_empty),
                    "unsupported/missing recursive submodule closure",
                )?;
                submodules = true;
            }
            Some("cross_revision") => {
                let revision = field(row, "revision")?;
                ensure(
                    cross_revision.is_none()
                        && revision.len() == 40
                        && revision.bytes().all(|b| b.is_ascii_hexdigit())
                        && digest(field(row, "executable_sha256")?)
                        && field(row, "version")?.contains(&revision[..7]),
                    "cross selected revision unmeasured",
                )?;
                cross_revision = Some(revision);
            }
            Some("node_package") => {
                ensure(
                    !node_package
                        && row["name"] == "ajv-cli"
                        && digest(field(row, "entry_sha256")?),
                    "AJV package closure invalid",
                )?;
                ensure(
                    !field(row, "version")?.is_empty(),
                    "AJV resolved version absent",
                )?;
                let files = row["files"]
                    .as_array()
                    .ok_or_else(|| invalid("AJV package files missing"))?;
                ensure(
                    !files.is_empty() && files.len() <= 20000,
                    "AJV dependency closure empty/oversized",
                )?;
                let mut paths = BTreeSet::new();
                for file in files {
                    let path = field(file, "path")?;
                    ensure(
                        !path.starts_with('/')
                            && !path.split('/').any(|c| matches!(c, "" | "." | ".."))
                            && paths.insert(path)
                            && digest(field(file, "sha256")?),
                        "invalid AJV dependency file identity",
                    )?;
                }
                ensure(
                    paths.contains("package.json"),
                    "AJV package manifest not measured",
                )?;
                node_package = true;
            }
            Some("effective_environment") => {
                ensure(!environment, "duplicate effective environment")?;
                environment = true;
                verify_environment(&row["values"], &recipe)?;
            }
            Some("bazel_binary") => {
                ensure(
                    !bazel && digest(field(row, "sha256")?),
                    "duplicate/unmeasured selected Bazel",
                )?;
                bazel = true;
                let selected = field(row, "version")?;
                ensure(
                    version_output(&measured, "bazelisk")?
                        .lines()
                        .any(|line| line == format!("bazel {selected}")),
                    "selected Bazel version lacks actual launcher output",
                )?;
                let pin = project.identity_inputs.root.join(".bazelversion");
                if pin.try_exists()? {
                    let version = fs::read_to_string(pin)?;
                    ensure(
                        selected == version.trim(),
                        "selected Bazel version differs from source pin",
                    )?;
                }
                // Source CI pins the installer recipe, not a .bazelversion.
                // Freeze its actual resolved bytes/version before checks.
            }
            Some("docker_image") => {
                let reference = field(row, "reference")?;
                ensure(
                    images.insert(reference) && digest(field(row, "id")?),
                    "invalid duplicate Docker image",
                )?;
                let digests = row["repo_digests"]
                    .as_array()
                    .ok_or_else(|| invalid("Docker repository digests missing"))?;
                ensure(
                    !digests.is_empty()
                        && digests.iter().all(|v| {
                            v.as_str()
                                .and_then(|s| s.rsplit_once('@'))
                                .is_some_and(|(_, d)| digest(d))
                        }),
                    "Docker image repository identity missing",
                )?;
                if reference.contains('@') {
                    ensure(
                        digests.iter().any(|v| v == reference),
                        "pinned Docker digest differs",
                    )?;
                }
            }
            Some("native_linker") => {
                ensure(
                    native_linkers.insert(field(row, "program")?)
                        && digest(field(row, "sha256")?)
                        && digest(field(row, "specs_sha256")?),
                    "invalid declared native linker",
                )?;
            }
            _ => return Err(invalid("unknown declared toolchain record")),
        }
    }
    ensure(environment, "effective compiler environment not measured")?;
    ensure(external_config, "ambient Cargo configuration not excluded")?;
    let expected_configs: BTreeSet<_> = if source::workflow_owned::artifact_worker(check) {
        BTreeSet::new()
    } else {
        CONFIG_PATHS
            .iter()
            .copied()
            .filter(|p| project.identity_inputs.root.join(p).is_file())
            .collect()
    };
    ensure(
        configs == expected_configs,
        "source toolchain configuration inventory incomplete",
    )?;
    let expected_recipes: BTreeSet<_> = source::workflow_owned::script_sources(check)
        .iter()
        .map(|(p, _)| *p)
        .collect();
    ensure(
        recipes == expected_recipes,
        "fixed recipe dependency inventory incomplete",
    )?;
    let expected_inputs = integration::required_inputs(project, check, expected);
    ensure(
        inputs
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
            == expected_inputs,
        "required producer input inventory incomplete",
    )?;
    let rust = check
        .capabilities
        .iter()
        .any(|c| *c == "cargo" || c.starts_with("rustc:"));
    if rust {
        ensure(
            selected.contains("rustc") && selected.contains("cargo"),
            "selected compiler/Cargo byte closure missing",
        )?;
        let host = match check.platform {
            source::Platform::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            source::Platform::MacosAarch64 => "aarch64-apple-darwin",
        };
        ensure(
            sysroots.contains(host),
            "selected host stdlib closure missing",
        )?;
        if check.id != "fmt" {
            ensure(
                native_compiler,
                "selected native compiler/linker bytes missing",
            )?;
        }
    }
    for capability in check.capabilities {
        match capability.split_once(':') {
            Some(("rustc", version)) => {
                ensure(
                    version_output(&measured, "rustc")?.starts_with(&format!("rustc {version} ")),
                    "actual rustc version differs from source requirement",
                )?;
                if check.capabilities.contains(&"cargo") { ensure(version_output(&measured,"cargo")?.starts_with(&format!("cargo {version} ")),"actual Cargo version differs from selected Rust toolchain")?; }
            }
            Some(("node", version)) => {
                ensure(
                    version_output(&measured, "node")?
                        .trim()
                        .starts_with(&format!("v{version}.")),
                    "actual Node version differs from source requirement",
                )?;
            }
            Some(("mdbook", version)) => {
                ensure(
                    version_output(&measured, "mdbook")?.trim() == format!("mdbook v{version}"),
                    "actual mdbook version differs from source requirement",
                )?;
            }
            Some(("rust-target", target)) => ensure(
                sysroots.contains(target),
                "required actual target stdlib missing",
            )?,
            Some(("image", image)) => {
                ensure(images.contains(image), "declared Docker image not measured")?
            }
            Some(("cross-rev", revision)) => ensure(cross_revision==Some(revision),"actual cross revision differs from reviewed source")?,
            Some(("submodules", "recursive")) => ensure(submodules,"recursive source closure absent")?,
            Some(_) => return Err(invalid("unsupported declared source capability")),
            None => match *capability {
                "docker-daemon" => {
                    ensure(programs.contains("docker"), "Docker daemon probe missing")?
                }
                "qemu-aarch64-in-pinned-image" => ensure(images.contains("ghcr.io/cross-rs/aarch64-unknown-linux-musl@sha256:ca0f4a798b756bba34c9cd2f7ceafc1d665daa6c70349a95e2eb06fb687a1ea6"),"required pinned cross image absent")?,
                "ajv"=>ensure(programs.contains("ajv")&&node_package,"AJV transitive package closure absent")?,
                "bazelisk" => ensure(
                    bazel && programs.contains("bazelisk"),
                    "selected Bazel missing",
                )?,
                "musl-gcc" => ensure(
                    programs.contains("musl-gcc")
                        && native_linkers.contains("cc1")
                        && native_linkers.contains("ld"),
                    "musl compiler/linker closure incomplete",
                )?,
                program => ensure(
                    programs.contains(program),
                    "required direct executable probe missing",
                )?,
            },
        }
    }
    for program in ["rustfmt", "cargo-clippy"] {
        if check.capabilities.contains(&program) {
            ensure(
                selected.contains(program),
                "selected Rust component byte closure missing",
            )?;
        }
    }
    if check.capabilities.contains(&"cargo-clippy") {
        ensure(
            selected.contains("clippy-driver"),
            "clippy-driver closure missing",
        )?;
    }
    Ok(())
}
use source::custody::declared_toolchain::SOURCE_CONFIG_PATHS as CONFIG_PATHS;
fn verify_environment(values: &Value, recipe: &[source::Invocation]) -> io::Result<()> {
    let values: BTreeMap<String, Option<String>> =
        serde_json::from_value(values.clone()).map_err(io::Error::other)?;
    source::custody::declared_toolchain::validate_environment(&values, recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_effective_environment_rejects_unmeasured_wrappers_and_missing_keys() {
        let mut values: serde_json::Map<String, Value> =
            source::custody::declared_toolchain::ENVIRONMENT_KEYS
                .iter()
                .map(|key| (key.to_string(), Value::Null))
                .collect();
        let recipe = source::commands(source::find_check("fmt").unwrap(), &[]).unwrap();
        assert!(verify_environment(&Value::Object(values.clone()), &recipe).is_ok());
        values.insert(
            "RUSTC_WRAPPER".into(),
            Value::String("unmeasured-wrapper".into()),
        );
        assert!(verify_environment(&Value::Object(values.clone()), &recipe).is_err());
        values.insert("RUSTC_WRAPPER".into(), Value::Null);
        values.remove("RUSTC");
        assert!(verify_environment(&Value::Object(values), &recipe).is_err());
    }
}
