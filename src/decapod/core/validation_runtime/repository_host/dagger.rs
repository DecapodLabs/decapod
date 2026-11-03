//! Narrow adapter for an already provisioned local Dagger engine. No container
//! creation, pulls, credentials, remote hosts, engine lifecycle or shell occurs.
//! The local Docker service and reviewed module are trusted execution boundaries;
//! this does not infer container-image dependency pins or claim hosted CI ran.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineObservation {
    id: String,
    image: String,
    running: bool,
    started_at: String,
    readonly_rootfs: bool,
    mount_destinations: Vec<String>,
}

const INSPECT_FORMAT: &str = r#"{"id":{{json .Id}},"image":{{json .Image}},"running":{{json .State.Running}},"started_at":{{json .State.StartedAt}},"readonly_rootfs":{{json .HostConfig.ReadonlyRootfs}},"mount_destinations":[{{range $i, $m := .Mounts}}{{if $i}},{{end}}{{json $m.Destination}}{{end}}]}"#;

fn prerequisite_error(code: &str, cause: io::Error) -> io::Error {
    io::Error::new(cause.kind(), format!("{code}: {cause}"))
}

fn decode_observation(
    bytes: &[u8],
    policy: &repository_profile::DaggerExecution,
) -> io::Result<EngineObservation> {
    let observation: EngineObservation = serde_json::from_slice(bytes).map_err(|cause| {
        prerequisite_error("DAGGER_ENGINE_IDENTITY_MISMATCH", io::Error::other(cause))
    })?;
    if observation.id != policy.engine_container
        || observation.image != policy.engine_image
        || !observation.running
        || observation.started_at.is_empty()
    {
        return Err(invalid(
            "DAGGER_ENGINE_IDENTITY_MISMATCH: expected running explicitly trusted pinned engine",
        ));
    }
    Ok(observation)
}

pub(super) fn inspect_engines(
    project: &CurrentProject,
    authorize: &mut NativeAuthorizer<'_>,
    budget: &RuntimeBudget<'_>,
) -> io::Result<BTreeMap<String, EngineObservation>> {
    let mut observations = BTreeMap::new();
    let Some(contract) = project.configuration.repository_contract.as_ref() else {
        return Ok(observations);
    };
    for (name, execution) in &contract.executions {
        let Some(policy) = &execution.dagger else {
            continue;
        };
        let docker = installed_file(
            execution
                .toolchain_files
                .get("dagger-docker")
                .ok_or_else(|| invalid("DAGGER_DOCKER_TOOL_MISSING"))?,
            &project.identity_inputs.root,
        )
        .map_err(|cause| prerequisite_error("DAGGER_DOCKER_TOOL_INVALID", cause))?;
        if docker.file_name().and_then(|n| n.to_str()) != Some("docker") {
            return Err(invalid(
                "DAGGER_DOCKER_TOOL_INVALID: explicit canonical Docker executable required",
            ));
        }
        let socket = Path::new(&policy.docker_socket);
        if socket.starts_with(&project.identity_inputs.root)
            || socket
                .canonicalize()
                .map_err(|cause| prerequisite_error("DAGGER_SOCKET_INVALID", cause))?
                != socket
        {
            return Err(invalid(
                "DAGGER_SOCKET_INVALID: canonical installed local socket required",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if !fs::symlink_metadata(socket)
                .map_err(|cause| prerequisite_error("DAGGER_SOCKET_INVALID", cause))?
                .file_type()
                .is_socket()
            {
                return Err(invalid("DAGGER_SOCKET_INVALID: socket required"));
            }
        }
        #[cfg(not(unix))]
        return Err(invalid(
            "DAGGER_PLATFORM_UNSUPPORTED: local Unix Docker socket required",
        ));
        authorize(NativeAction::DaggerEngineInspect {
            program: &docker,
            container: &policy.engine_container,
        })?;
        // No HOME, DOCKER_CONTEXT, credential helper, ambient PATH or config is
        // inherited. Only the explicitly reviewed local service is queried.
        let mut command = Command::new(&docker);
        command
            .env_clear()
            .current_dir(&project.identity_inputs.root)
            .args([
                "--host",
                &format!("unix://{}", policy.docker_socket),
                "container",
                "inspect",
                "--format",
                INSPECT_FORMAT,
                &policy.engine_container,
            ]);
        let capture = command
            .bounded_capture(
                ProcessLimits {
                    timeout: budget.remaining()?.min(Duration::from_secs(15)),
                    max_output_bytes: budget.output_limit().min(64 * 1024),
                },
                budget.cancel,
            )
            .map_err(|cause| prerequisite_error("DAGGER_ENGINE_INSPECTION_FAILED", cause))?;
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(invalid(
                "DAGGER_ENGINE_INSPECTION_FAILED: explicit local engine unavailable",
            ));
        }
        observations.insert(
            name.clone(),
            decode_observation(&capture.output.stdout, policy)?,
        );
    }
    Ok(observations)
}

/// Preserve both observations even when a restart invalidates the proof. The
/// diagnostic pair is never a successful custody witness or readiness receipt.
pub(super) fn record_custody(
    directory: &Path,
    before: &BTreeMap<String, EngineObservation>,
    after: &BTreeMap<String, EngineObservation>,
) -> io::Result<()> {
    if !before.is_empty() || !after.is_empty() {
        fs::write(
            directory.join("dagger-engine-observations.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "before": before, "after": after, "unchanged": before == after
            }))
            .map_err(io::Error::other)?,
        )?;
    }
    if before != after {
        return Err(invalid(
            "DAGGER_ENGINE_CHANGED: local engine changed during proof",
        ));
    }
    if !after.is_empty() {
        fs::write(
            directory.join("dagger-engine-custody.json"),
            serde_json::to_vec_pretty(after).map_err(io::Error::other)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> repository_profile::DaggerExecution {
        repository_profile::DaggerExecution {
            cli_contract: "dagger-1.0-workspace-check.v1".into(),
            engine_container: "a".repeat(64),
            engine_image: format!("sha256:{}", "b".repeat(64)),
            docker_socket: "/var/run/docker.sock".into(),
        }
    }
    fn observed() -> serde_json::Value {
        serde_json::json!({"id":"a".repeat(64),"image":format!("sha256:{}","b".repeat(64)),"running":true,"started_at":"2026-11-02T00:00:00Z","readonly_rootfs":false,"mount_destinations":["/var/lib/dagger"]})
    }
    #[test]
    fn prerequisite_failures_retain_cause_kind_and_recovery_context() {
        let error = prerequisite_error(
            "DAGGER_SOCKET_INVALID",
            io::Error::new(io::ErrorKind::PermissionDenied, "socket access denied"),
        );
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            error.to_string(),
            "DAGGER_SOCKET_INVALID: socket access denied"
        );
        assert!(
            decode_observation(b"not json", &policy())
                .unwrap_err()
                .to_string()
                .starts_with("DAGGER_ENGINE_IDENTITY_MISMATCH:")
        );
    }

    #[test]
    fn engine_witness_requires_exact_live_identity() {
        let valid = observed();
        assert!(decode_observation(&serde_json::to_vec(&valid).unwrap(), &policy()).is_ok());
        for (key, value) in [
            ("id", serde_json::json!("c".repeat(64))),
            (
                "image",
                serde_json::json!(format!("sha256:{}", "c".repeat(64))),
            ),
            ("running", serde_json::json!(false)),
            ("started_at", serde_json::json!("")),
        ] {
            let mut changed = valid.clone();
            changed[key] = value;
            assert!(
                decode_observation(&serde_json::to_vec(&changed).unwrap(), &policy()).is_err(),
                "{key}"
            );
        }
        assert!(decode_observation(b"{}", &policy()).is_err());
    }
    #[test]
    fn restart_changes_engine_witness_even_when_image_is_unchanged() {
        let first =
            decode_observation(&serde_json::to_vec(&observed()).unwrap(), &policy()).unwrap();
        let mut changed = observed();
        changed["started_at"] = serde_json::json!("2026-11-02T01:00:00Z");
        assert_ne!(
            first,
            decode_observation(&serde_json::to_vec(&changed).unwrap(), &policy()).unwrap()
        );
    }
    #[test]
    fn changed_engine_preserves_diagnostics_without_successful_custody() {
        let first =
            decode_observation(&serde_json::to_vec(&observed()).unwrap(), &policy()).unwrap();
        let before = BTreeMap::from([("dagger-check".into(), first.clone())]);
        for field in [
            "id",
            "image",
            "started_at",
            "mounts",
            "readonly",
            "running",
            "missing",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut changed = first.clone();
            match field {
                "id" => changed.id = "c".repeat(64),
                "image" => changed.image = format!("sha256:{}", "c".repeat(64)),
                "started_at" => changed.started_at = "2026-11-02T01:00:00Z".into(),
                "mounts" => changed.mount_destinations.push("/changed".into()),
                "readonly" => changed.readonly_rootfs = !changed.readonly_rootfs,
                "running" => changed.running = false,
                "missing" => {}
                _ => unreachable!(),
            }
            let after = if field == "missing" {
                BTreeMap::new()
            } else {
                BTreeMap::from([("dagger-check".into(), changed)])
            };
            let error = record_custody(directory.path(), &before, &after).unwrap_err();
            assert!(error.to_string().contains("DAGGER_ENGINE_CHANGED"));
            assert!(!directory.path().join("dagger-engine-custody.json").exists());
            let diagnostic: serde_json::Value = serde_json::from_slice(
                &fs::read(directory.path().join("dagger-engine-observations.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(diagnostic["unchanged"], false);
            assert_eq!(diagnostic["before"], serde_json::to_value(&before).unwrap());
            assert_eq!(diagnostic["after"], serde_json::to_value(&after).unwrap());
        }
    }

    #[test]
    fn unchanged_engine_retains_both_observations_and_successful_custody() {
        let directory = tempfile::tempdir().unwrap();
        let observation =
            decode_observation(&serde_json::to_vec(&observed()).unwrap(), &policy()).unwrap();
        let engines = BTreeMap::from([("dagger-check".into(), observation)]);
        record_custody(directory.path(), &engines, &engines).unwrap();
        assert!(
            directory
                .path()
                .join("dagger-engine-observations.json")
                .is_file()
        );
        let custody: BTreeMap<String, EngineObservation> = serde_json::from_slice(
            &fs::read(directory.path().join("dagger-engine-custody.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(custody, engines);
        let dormant = tempfile::tempdir().unwrap();
        record_custody(dormant.path(), &BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert_eq!(fs::read_dir(dormant.path()).unwrap().count(), 0);
    }

    #[test]
    fn inspection_template_does_not_request_container_environment_or_credentials() {
        assert!(!INSPECT_FORMAT.contains(".Config.Env"));
        assert!(!INSPECT_FORMAT.contains("json .}}"));
    }
}
