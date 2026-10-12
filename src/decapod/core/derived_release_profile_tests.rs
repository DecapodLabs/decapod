use super::super::derived_release_driver;
use super::*;
use serde_json::json;

fn source() -> Snapshot {
    [
        (
            "Cargo.toml",
            include_str!("../../../tests/fixtures/derived-release/Cargo.toml.fixture"),
        ),
        (
            "Cargo.lock",
            include_str!("../../../tests/fixtures/derived-release/Cargo.lock"),
        ),
        (
            "CHANGELOG.md",
            include_str!("../../../tests/fixtures/derived-release/CHANGELOG.md"),
        ),
        (
            ".github/release.toml",
            include_str!("../../../tests/fixtures/derived-release/.github_release.toml"),
        ),
        (
            "dist-workspace.toml",
            include_str!("../../../tests/fixtures/derived-release/dist-workspace.toml"),
        ),
        (
            "dist.toml",
            include_str!("../../../tests/fixtures/derived-release/dist.toml"),
        ),
        ("src/main.rs", "// fixture source, no execution\n"),
    ]
    .into_iter()
    .map(|(path, text)| {
        (
            path.to_owned(),
            FileMaterial {
                mode: 0o100644,
                bytes: text.as_bytes().to_vec(),
            },
        )
    })
    .collect()
}
fn get_profile() -> ReleaseProfile {
    let s = source();
    profile(
        file_text(&s, "Cargo.toml").unwrap(),
        file_text(&s, ".github/release.toml").unwrap(),
        file_text(&s, "dist-workspace.toml").unwrap(),
        file_text(&s, "dist.toml").unwrap(),
    )
    .unwrap()
}
fn tools() -> BTreeMap<Tool, ToolResolution> {
    [
        (Tool::Cargo, "1.96.1"),
        (Tool::Rustc, "1.96.1"),
        (Tool::ReleasePlz, "0.3.146"),
        (Tool::CargoDist, "0.22.1"),
        (Tool::Git, "2.50.0"),
    ]
    .into_iter()
    .map(|(key, version)| {
        (
            key,
            ToolResolution {
                actual_version: version.into(),
                executable_sha256: "a".repeat(64),
                production_versions: BTreeMap::from([
                    ("github-fixture".into(), version.into()),
                    ("buildkite-fixture".into(), version.into()),
                ]),
                production_resolution_sha256: "b".repeat(64),
            },
        )
    })
    .collect()
}
fn change(s: &mut Snapshot, path: &str, f: impl FnOnce(String) -> String) {
    let file = s.get_mut(path).unwrap();
    file.bytes = f(String::from_utf8(file.bytes.clone()).unwrap()).into_bytes();
}
fn derived() -> Snapshot {
    let mut s = source();
    change(&mut s, "Cargo.toml", |s| {
        s.replacen("version = \"0.107.5\"", "version = \"0.107.6\"", 1)
    });
    change(&mut s, "Cargo.lock", |s| {
        s.replace(
            "name = \"decapod\"\nversion = \"0.107.5\"",
            "name = \"decapod\"\nversion = \"0.107.6\"",
        )
    });
    change(&mut s, "CHANGELOG.md", |s| {
        s.replacen("## [Unreleased]\n\n", concat!("## [Unreleased]\n\n",
        "## [0.107.6](https://github.com/DecapodLabs/decapod/compare/v0.107.5...v0.107.6) - 2026-10-11\n\n",
        "### Fixed\n\n- fixture generated change\n\n"), 1)
    });
    s
}
fn binding(before: &Snapshot, after: &Snapshot) -> GenerationBinding {
    GenerationBinding {
        source_commit: "1".repeat(40),
        source_git_tree: "2".repeat(40),
        source_material_sha256: snapshot_digest(before).unwrap(),
        derived_commit: "3".repeat(40),
        derived_git_tree: "4".repeat(40),
        derived_material_sha256: snapshot_digest(after).unwrap(),
        accepted_base_commit: "5".repeat(40),
        history_sha256: "a".repeat(64),
        baseline_tag: "v0.107.5".into(),
        baseline_commit: "6".repeat(40),
        baseline_version: "0.107.5".into(),
        registry_inputs_sha256: "b".repeat(64),
        registry_identity: "https://index.crates.io/".into(),
        registry_observed_at: "2026-10-11T23:00:00Z".into(),
        registry_version: "0.107.5".into(),
        registry_crate_sha256: "c".repeat(64),
        configuration_sha256: get_profile().configuration_digest,
        tools_sha256: sha256(&serde_json::to_vec(&tools()).unwrap()),
        release_date: "2026-10-11".into(),
    }
}
fn generated() -> GeneratedRelease {
    verify_generation(
        &get_profile(),
        &source(),
        &derived(),
        &binding(&source(), &derived()),
        &tools(),
    )
    .unwrap()
}
// This is a schema test double, not cargo-dist output or live acceptance evidence.
fn manifest() -> Value {
    let p = get_profile();
    let mut artifacts = serde_json::Map::new();
    for target in &p.targets {
        let archive = format!("decapod-{target}.tar.xz");
        let checksum = format!("{archive}.sha256");
        artifacts.insert(
            archive.clone(),
            json!({"name":archive,"kind":"executable-zip", "target_triples":[target],
            "checksum":checksum,
            "assets":[{"name":"decapod","path":"decapod","kind":"executable"},
                {"name":"LICENSE","path":"LICENSE","kind":"license"}]}),
        );
        artifacts.insert(
            checksum.clone(),
            json!({"name":checksum,"kind":"checksum", "target_triples":[target]}),
        );
    }
    artifacts.insert(
        "source.tar.gz".into(),
        json!({"name":"source.tar.gz","kind":"source-tarball","checksum":"source.tar.gz.sha256"}),
    );
    artifacts.insert(
        "source.tar.gz.sha256".into(),
        json!({"name":"source.tar.gz.sha256","kind":"checksum"}),
    );
    let names: Vec<_> = artifacts.keys().cloned().collect();
    let matrix: Vec<_> = p
        .targets
        .iter()
        .map(|t| json!({"targets":[t],"runner":p.runners[t]}))
        .collect();
    json!({"dist_version":"0.22.1", "announcement_tag":"v0.107.6",
        "announcement_tag_is_implicit":false,"announcement_is_prerelease":false,
        "ci":{"github":{"artifacts_matrix":{"include":matrix}}},
        "releases":[{"app_name":"decapod","app_version":"0.107.6","artifacts":names}],"artifacts":artifacts})
}
fn verify_manifest(v: Value) -> Result<DistInventory> {
    verify_dist_plan(
        &get_profile(),
        &generated(),
        &serde_json::to_vec(&v).unwrap(),
    )
}

#[test]
fn actual_repo_config_drives_profile() {
    let p = get_profile();
    assert_eq!(p.targets.len(), 4);
    assert_eq!(p.binaries, BTreeSet::from(["decapod".to_owned()]));
    assert_eq!(p.rust_version, "1.96.1");
    assert_eq!(p.dist_version, "0.22.1");
    assert_eq!(p.runners["x86_64-apple-darwin"], "macos-14");
}
#[test]
fn unknown_or_malformed_production_config_fails_closed() {
    let b = source();
    let c = file_text(&b, "Cargo.toml").unwrap();
    let r = file_text(&b, ".github/release.toml").unwrap();
    let w = file_text(&b, "dist-workspace.toml").unwrap();
    let p = file_text(&b, "dist.toml").unwrap();
    for changed in [
        format!("{w}\nfuture-option = true\n"),
        w.replace("0.22.1", "0.23.0"),
        w.replace("installers = []", "installers = [\"shell\"]"),
        w.replace("members = [\"cargo:.\"]", "members = []"),
    ] {
        assert!(profile(c, r, &changed, p).is_err());
    }
    assert!(profile(c, &format!("{r}\nfuture-release-rule = true"), w, p).is_err());
    assert!(profile(c, r, w, "[package]\nbinaries = [\"decapod-build\"]").is_err());
}
#[test]
fn command_recipe_has_no_publish_or_skip_verification_escape() {
    let p = get_profile();
    let update = p.generation_command("2026-10-11").unwrap();
    assert_eq!(update.tool, Tool::ReleasePlz);
    assert_eq!(
        update.args,
        [
            "update",
            "--config",
            ".github/release.toml",
            "--manifest-path",
            "Cargo.toml",
            "--release-date",
            "2026-10-11"
        ]
    );
    let commands = p.verification_commands("0.107.6", &p.targets).unwrap();
    assert_eq!(commands.len(), 7);
    assert_eq!(
        commands[1].args,
        ["package", "--locked", "--package", "decapod"]
    );
    for command in commands {
        for arg in command.args {
            assert!(
                ![
                    "release",
                    "release-pr",
                    "publish",
                    "host",
                    "upload",
                    "--no-verify",
                    "--allow-dirty",
                    "--dry-run"
                ]
                .contains(&arg.as_str())
            );
        }
    }
    assert!(
        p.verification_commands("0.107.6", &BTreeSet::new())
            .is_err()
    );
}
#[test]
fn generation_date_rejects_injection_and_impossible_dates() {
    let p = get_profile();
    for date in [
        "2026-02-29",
        "2026-10-11;touch /tmp/pwn",
        "--help",
        "2026-1-11",
        "0000-01-01",
    ] {
        assert!(p.generation_command(date).is_err());
    }
    assert!(p.generation_command("2028-02-29").is_ok());
}
#[test]
fn unresolved_or_divergent_tools_are_failures() {
    let p = get_profile();
    for tool in [
        Tool::Rustc,
        Tool::Cargo,
        Tool::ReleasePlz,
        Tool::CargoDist,
        Tool::Git,
    ] {
        let mut t = tools();
        t.remove(&tool);
        assert!(verify_tools(&p, &t).is_err());
        let mut t = tools();
        t.get_mut(&tool).unwrap().production_versions.clear();
        assert!(verify_tools(&p, &t).is_err());
        let mut t = tools();
        t.get_mut(&tool)
            .unwrap()
            .production_versions
            .insert("other-runtime".into(), "unknown".into());
        assert!(verify_tools(&p, &t).is_err());
    }
}
#[test]
fn strict_generated_delta_uses_production_helpers() {
    let g = generated();
    assert_eq!(g.version, "0.107.6");
    assert_eq!(g.tag, "v0.107.6");
    verify_lock_unchanged(&g, &derived()["Cargo.lock"].bytes).unwrap();
    assert!(verify_lock_unchanged(&g, &source()["Cargo.lock"].bytes).is_err());
}
#[test]
fn no_op_generator_is_never_inferred_as_no_release() {
    let s = source();
    let mut b = binding(&s, &s);
    b.derived_git_tree = "f".repeat(40);
    assert!(
        verify_generation(&get_profile(), &s, &s, &b, &tools())
            .unwrap_err()
            .contains("no-release is not inferred")
    );
}
#[test]
fn dependency_or_non_release_file_mutation_is_rejected() {
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "src/main.rs",
        "dist.toml",
        ".github/release.toml",
    ] {
        let mut d = derived();
        change(&mut d, path, |s| format!("{s}\n# extra mutation\n"));
        assert!(
            verify_generation(
                &get_profile(),
                &source(),
                &d,
                &binding(&source(), &d),
                &tools()
            )
            .is_err(),
            "{path}"
        );
    }
}
#[test]
fn generated_symlinks_additions_deletions_and_executable_modes_fail() {
    for mode in [0o100755, 0o120000] {
        let mut d = derived();
        d.get_mut("CHANGELOG.md").unwrap().mode = mode;
        assert!(
            verify_generation(
                &get_profile(),
                &source(),
                &d,
                &binding(&source(), &d),
                &tools()
            )
            .is_err()
        );
    }
    let mut d = derived();
    d.remove("src/main.rs");
    assert!(
        verify_generation(
            &get_profile(),
            &source(),
            &d,
            &binding(&source(), &d),
            &tools()
        )
        .is_err()
    );
    let mut d = derived();
    d.insert(
        "extra.txt".into(),
        FileMaterial {
            mode: 0o100644,
            bytes: vec![],
        },
    );
    assert!(
        verify_generation(
            &get_profile(),
            &source(),
            &d,
            &binding(&source(), &d),
            &tools()
        )
        .is_err()
    );
}
#[test]
fn stale_or_unresolved_binding_fails() {
    let s = source();
    let d = derived();
    for field in [
        "source",
        "derived",
        "history",
        "base",
        "registry",
        "config",
        "tools",
        "date",
        "registry_version",
    ] {
        let mut b = binding(&s, &d);
        match field {
            "source" => b.source_material_sha256 = "f".repeat(64),
            "derived" => b.derived_material_sha256 = "e".repeat(64),
            "history" => b.history_sha256.clear(),
            "base" => b.accepted_base_commit.clear(),
            "registry" => b.registry_inputs_sha256.clear(),
            "config" => b.configuration_sha256.clear(),
            "tools" => b.tools_sha256.clear(),
            "date" => b.release_date = "2026-10-12".into(),
            "registry_version" => b.registry_version = "0.107.4".into(),
            _ => unreachable!(),
        }
        assert!(
            verify_generation(&get_profile(), &s, &d, &b, &tools()).is_err(),
            "{field}"
        );
    }
}
#[test]
fn changelog_history_date_and_section_count_are_bound() {
    for (old, new) in [
        (
            "restore Buildkite workflow compatibility",
            "tamper historical entry",
        ),
        (
            "- 2026-10-11\n\n### Fixed\n\n- fixture",
            "- 2026-10-12\n\n### Fixed\n\n- fixture",
        ),
        (
            "- fixture generated change",
            "## [0.107.7]\n\n- forged section",
        ),
    ] {
        let mut d = derived();
        change(&mut d, "CHANGELOG.md", |s| s.replace(old, new));
        assert!(
            verify_generation(
                &get_profile(),
                &source(),
                &d,
                &binding(&source(), &d),
                &tools()
            )
            .is_err()
        );
    }
}
#[test]
fn full_dist_inventory_matches_repository_targets() {
    let inventory = verify_manifest(manifest()).unwrap();
    assert_eq!(inventory.target_archives.len(), 4);
    assert_eq!(inventory.source_archive, "source.tar.gz");
    let built: BTreeMap<_, _> = inventory
        .artifact_names
        .iter()
        .map(|id| (id.clone(), "c".repeat(64)))
        .collect();
    verify_built_artifacts(&inventory, &built).unwrap();
    let mut partial = built.clone();
    partial.pop_first();
    assert!(verify_built_artifacts(&inventory, &partial).is_err());
    let mut missing_digest = built;
    *missing_digest.values_mut().next().unwrap() = "".into();
    assert!(verify_built_artifacts(&inventory, &missing_digest).is_err());
}
#[test]
fn dist_manifest_rejects_duplicate_keys_and_trailing_json() {
    for raw in [
        b"{} {}".as_slice(),
        b"[]",
        b"null",
        b"{\"artifacts\":{},\"artifacts\":{}}",
        b"{\"x\":{\"a\":1,\"a\":2}}",
    ] {
        assert!(verify_dist_plan(&get_profile(), &generated(), raw).is_err());
    }
}
#[test]
fn dist_manifest_rejects_stale_identity_and_unknown_kinds() {
    for (key, value) in [("dist_version", "0.23.0"), ("announcement_tag", "v0.107.5")] {
        let mut m = manifest();
        m[key] = json!(value);
        assert!(verify_manifest(m).is_err());
    }
    let mut m = manifest();
    m["releases"][0]["app_version"] = json!("0.107.5");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"]["source.tar.gz"]["kind"] = json!("installer");
    assert!(verify_manifest(m).is_err());
}
#[test]
fn dist_manifest_rejects_omitted_or_extra_inventory() {
    let mut m = manifest();
    m["artifacts"]
        .as_object_mut()
        .unwrap()
        .remove("source.tar.gz");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["releases"][0]["artifacts"]
        .as_array_mut()
        .unwrap()
        .push(json!("missing"));
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"app_name":"extra"}));
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    let key = "decapod-x86_64-apple-darwin.tar.xz";
    m["artifacts"][key]["target_triples"] = json!(["i686-pc-windows-msvc"]);
    assert!(verify_manifest(m).is_err());
}
#[test]
fn dist_manifest_rejects_maintenance_binaries_paths_and_bad_checksums() {
    let key = "decapod-x86_64-apple-darwin.tar.xz";
    let mut m = manifest();
    m["artifacts"][key]["assets"][0]["name"] = json!("decapod-build");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"][key]["assets"][0]["path"] = json!("../../bin/decapod");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"][key]["path"] = json!("/tmp/decapod.tar.xz");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"][key]["checksum"] = json!("source.tar.gz");
    assert!(verify_manifest(m).is_err());
}

#[test]
fn dist_matrix_tag_and_checksum_relationships_fail_closed() {
    let mut m = manifest();
    m["announcement_tag_is_implicit"] = json!(true);
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["ci"]["github"]["artifacts_matrix"]["include"][0]["runner"] = json!("macos-latest");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["ci"]["github"]["artifacts_matrix"]["include"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"]["source.tar.gz"]
        .as_object_mut()
        .unwrap()
        .remove("checksum");
    assert!(verify_manifest(m).is_err());
    let mut m = manifest();
    m["artifacts"]["source.tar.gz.sha256"]["target_triples"] = json!(["x86_64-apple-darwin"]);
    assert!(verify_manifest(m).is_err());
}

#[test]
fn required_catalog_is_complete_before_any_generation() {
    let p = get_profile();
    let checks = p.required_checks();
    assert_eq!(checks.len(), 11);
    let locals: BTreeSet<_> = checks
        .iter()
        .filter(|c| c.id == "dist-local")
        .map(|c| c.target.clone().unwrap())
        .collect();
    assert_eq!(locals, p.targets);
    assert!(checks.iter().any(|c| c.id == "cargo-package"));
    assert!(
        checks
            .iter()
            .any(|c| c.id == "dist-global" && c.runner.as_deref() == Some("ubuntu-24.04"))
    );
}
#[test]
fn actual_checksum_file_grammar_and_digest_are_required() {
    let plan = verify_manifest(manifest()).unwrap();
    let mut observed: BTreeMap<_, _> = plan
        .artifact_names
        .iter()
        .map(|id| (id.clone(), "d".repeat(64)))
        .collect();
    let contents: BTreeMap<_, _> = plan
        .checksums
        .iter()
        .map(|id| {
            let artifact = id.strip_suffix(".sha256").unwrap();
            (
                id.clone(),
                format!("{} *{artifact}\n", observed[artifact]).into_bytes(),
            )
        })
        .collect();
    for (name, bytes) in &contents {
        observed.insert(name.clone(), sha256(bytes));
    }
    verify_checksum_files(&plan, &observed, &contents).unwrap();
    let mut wrong = contents.clone();
    wrong.values_mut().next().unwrap().push(b'x');
    assert!(verify_checksum_files(&plan, &observed, &wrong).is_err());
    assert!(verify_checksum_files(&plan, &observed, &BTreeMap::new()).is_err());
}

// Synthetic custody fixtures only; these do not claim release-tool execution.
fn transport_fixture() -> (
    derived_release_driver::ReleaseTransport,
    BTreeMap<String, Vec<u8>>,
) {
    use derived_release_driver::{CommandRecord, ReleaseTransport};
    let mut files = BTreeMap::new();
    files.insert(
        "derived.bundle".into(),
        b"synthetic bundle fixture".to_vec(),
    );
    let mut args = get_profile().generation_command("2026-10-11").unwrap().args;
    args.extend([
        "--repo-url".into(),
        "https://github.com/DecapodLabs/decapod".into(),
    ]);
    let record = CommandRecord {
        program_sha256: "a".repeat(64),
        arguments: args,
        cwd: "/synthetic/owned/fixture".into(),
        environment_sha256: "e".repeat(64),
        stdout_sha256: sha256(b"fixture update"),
        stderr_sha256: sha256(b""),
        exit_code: 0,
        duration_ms: 1,
        output_policy: "raw".into(),
    };
    files.insert(
        format!("evidence/logs/{}.stdout", record.stdout_sha256),
        b"fixture update".to_vec(),
    );
    files.insert(
        format!("evidence/logs/{}.stderr", record.stderr_sha256),
        vec![],
    );
    for (name, file) in derived() {
        if ["Cargo.toml", "Cargo.lock", "CHANGELOG.md"].contains(&name.as_str()) {
            files.insert(format!("delta/{name}"), file.bytes);
        }
    }
    let mut records = vec![record];
    for (key, tool) in tools() {
        let label = match key {
            Tool::Cargo => "cargo",
            Tool::Rustc => "rustc",
            Tool::CargoDist => "cargo-dist",
            Tool::ReleasePlz => "release-plz",
            Tool::Git => "git version",
        };
        let output = format!("{label} {}\n", tool.actual_version).into_bytes();
        let digest = sha256(&output);
        files.insert(format!("evidence/logs/{digest}.stdout"), output);
        records.push(CommandRecord {
            program_sha256: tool.executable_sha256,
            cwd: "/synthetic/owned/fixture".into(),
            environment_sha256: "e".repeat(64),
            arguments: if key == Tool::CargoDist {
                vec!["dist".into(), "--version".into()]
            } else {
                vec!["--version".into()]
            },
            stdout_sha256: digest,
            stderr_sha256: sha256(b""),
            exit_code: 0,
            duration_ms: 1,
            output_policy: "raw".into(),
        });
    }
    let receipt = ReleaseTransport {
        schema: "derived-release-transport/1".into(),
        binding: binding(&source(), &derived()),
        generation_tools: tools(),
        bundle_sha256: sha256(&files["derived.bundle"]),
        preparation_records: records,
        tool_setup_sha256: "a".repeat(64),
        semver_checks_version: "0.51.0".into(),
        semver_checks_sha256: "c".repeat(64),
        installer_definition_sha256: "d".repeat(64),
    };
    files.insert(
        "transport.json".into(),
        serde_json::to_vec(&receipt).unwrap(),
    );
    (receipt, files)
}
fn verify_transport_fixture(
    files: &BTreeMap<String, Vec<u8>>,
) -> std::io::Result<(derived_release_driver::ReleaseTransport, GeneratedRelease)> {
    let binding = binding(&source(), &derived());
    derived_release_driver::verify_exported_transport(
        &get_profile(),
        &source(),
        &derived_release_driver::ExpectedTransport {
            source_commit: &binding.source_commit,
            source_git_tree: &binding.source_git_tree,
            accepted_base: &binding.accepted_base_commit,
            release_date: &binding.release_date,
            setup_contract_sha256: &"b".repeat(64),
        },
        |name| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| std::io::Error::other("missing fixture member"))
        },
    )
}
#[test]
fn custody_decoder_recomputes_full_generated_delta_and_binding() {
    let (_, files) = transport_fixture();
    let (_, generated) = verify_transport_fixture(&files).unwrap();
    assert_eq!(generated.version, "0.107.6");
}
#[test]
fn custody_decoder_rejects_missing_or_changed_bundle_delta_and_logs() {
    let (_, files) = transport_fixture();
    for name in files
        .keys()
        .filter(|name| name.as_str() != "transport.json")
    {
        let mut missing = files.clone();
        missing.remove(name);
        assert!(verify_transport_fixture(&missing).is_err(), "{name}");
        let mut changed = files.clone();
        changed.get_mut(name).unwrap().push(b'!');
        assert!(verify_transport_fixture(&changed).is_err(), "{name}");
    }
}
#[test]
fn custody_decoder_rejects_other_source_base_date_setup_and_generator() {
    let (receipt, files) = transport_fixture();
    for mutate in [0, 1, 2, 3, 4] {
        let mut receipt = receipt.clone();
        match mutate {
            0 => receipt.binding.source_commit = "9".repeat(40),
            1 => receipt.binding.accepted_base_commit = "9".repeat(40),
            2 => receipt.binding.release_date = "2026-10-12".into(),
            3 => {
                receipt
                    .generation_tools
                    .get_mut(&Tool::Cargo)
                    .unwrap()
                    .production_resolution_sha256 = "e".repeat(64)
            }
            _ => receipt.preparation_records[0].arguments[0] = "release-pr".into(),
        }
        let mut files = files.clone();
        files.insert(
            "transport.json".into(),
            serde_json::to_vec(&receipt).unwrap(),
        );
        assert!(verify_transport_fixture(&files).is_err());
    }
}
#[test]
fn custody_decoder_rejects_duplicate_json_keys() {
    let (_, mut files) = transport_fixture();
    let old = String::from_utf8(files["transport.json"].clone()).unwrap();
    files.insert(
        "transport.json".into(),
        old.replacen('{', "{\"schema\":\"derived-release-transport/1\",", 1)
            .into_bytes(),
    );
    assert!(verify_transport_fixture(&files).is_err());
}
#[test]
fn candidate_recipe_is_nonrecursive_and_checks_active_session_without_token_export() {
    let candidate = include_str!("derived_release_candidate.rs");
    let driver = include_str!("derived_release_driver.rs");
    assert!(candidate.contains("capabilities"));
    assert!(!candidate.contains("[\"--version\""));
    assert!(candidate.contains("project-check"));
    assert!(candidate.contains("Session active"));
    assert!(!candidate.contains("\"acquire\""));
    assert!(!candidate.contains("\"validate\""));
    assert!(driver.contains("session-status-first-line"));
    assert!(driver.contains("result.output.stdout.fill(0)"));
}

fn stage_plan_fixture() -> (
    derived_release_driver::ReleaseTransport,
    Vec<u8>,
    BTreeMap<String, Vec<u8>>,
) {
    use derived_release_driver::{CommandRecord, Stage, StageReport, WorkerSetup};
    let (receipt, mut files) = transport_fixture();
    let mut worker_tools = tools();
    let mut records = vec![];
    for (key, tool) in &mut worker_tools {
        let label = match key {
            Tool::Cargo => "cargo",
            Tool::Rustc => "rustc",
            Tool::CargoDist => "cargo-dist",
            Tool::ReleasePlz => "release-plz",
            Tool::Git => "git version",
        };
        tool.executable_sha256 = sha256(label.as_bytes());
        let output = format!("{label} {}\n", tool.actual_version).into_bytes();
        let digest = sha256(&output);
        files.insert(format!("evidence/logs/{digest}.stdout"), output);
        records.push(CommandRecord {
            program_sha256: tool.executable_sha256.clone(),
            arguments: if *key == Tool::CargoDist {
                vec!["dist".into(), "--version".into()]
            } else {
                vec!["--version".into()]
            },
            cwd: "/synthetic/owned/fixture".into(),
            environment_sha256: "e".repeat(64),
            stdout_sha256: digest,
            stderr_sha256: sha256(b""),
            exit_code: 0,
            duration_ms: 1,
            output_policy: "raw".into(),
        });
    }
    let semver = b"cargo-semver-checks 0.51.0\n";
    files.insert(
        format!("evidence/logs/{}.stdout", sha256(semver)),
        semver.to_vec(),
    );
    records.push(CommandRecord {
        program_sha256: "c".repeat(64),
        arguments: vec!["--version".into()],
        cwd: "/synthetic/owned/fixture".into(),
        environment_sha256: "e".repeat(64),
        stdout_sha256: sha256(semver),
        stderr_sha256: sha256(b""),
        exit_code: 0,
        duration_ms: 1,
        output_policy: "raw".into(),
    });
    let setup = WorkerSetup {
        tools: worker_tools.clone(),
        semver_checks_version: "0.51.0".into(),
        semver_checks_sha256: "c".repeat(64),
        installer_definition_sha256: "d".repeat(64),
        build_environment: BTreeMap::new(),
        curl_sha256: "a".repeat(64),
        xz_sha256: "a".repeat(64),
    };
    let setup_sha = sha256(&serde_json::to_vec(&setup).unwrap());
    let plan = serde_json::to_vec(&manifest()).unwrap();
    files.insert("evidence/plan-dist-manifest.json".into(), plan.clone());
    files.insert(
        format!("evidence/logs/{}.stdout", sha256(&plan)),
        plan.clone(),
    );
    let command = CommandRecord {
        program_sha256: worker_tools[&Tool::Cargo].executable_sha256.clone(),
        arguments: vec![
            "dist".into(),
            "plan".into(),
            "--tag".into(),
            "v0.107.6".into(),
            "--output-format=json".into(),
        ],
        cwd: "/synthetic/owned/fixture".into(),
        environment_sha256: "e".repeat(64),
        stdout_sha256: sha256(&plan),
        stderr_sha256: sha256(b""),
        exit_code: 0,
        duration_ms: 1,
        output_policy: "raw".into(),
    };
    let report = StageReport {
        stage: Stage::Plan,
        source_commit: receipt.binding.source_commit.clone(),
        derived_commit: receipt.binding.derived_commit.clone(),
        binding_sha256: generated().binding_sha256,
        command_records: vec![command],
        artifacts: BTreeMap::new(),
        tool_setup_sha256: setup_sha.clone(),
        worker_setup: setup,
    };
    let proof=serde_json::to_vec(&json!({"schema":"derived-release-execution/2","generation_binding":receipt.binding,"generated":generated(),"tool_setup_sha256":setup_sha,"preparation_records":records,"stage":report})).unwrap();
    (receipt, proof, files)
}
#[test]
fn stage_custody_requires_measured_worker_tools_and_actual_plan_output() {
    let (receipt, proof, files) = stage_plan_fixture();
    let report = derived_release_driver::verify_exported_stage(
        &get_profile(),
        &receipt,
        &generated(),
        &proof,
        |name| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| std::io::Error::other("missing fixture member"))
        },
    )
    .unwrap();
    assert_eq!(report.stage, derived_release_driver::Stage::Plan);
    for name in [
        "evidence/plan-dist-manifest.json",
        &format!("evidence/logs/{}.stdout", sha256(b"cargo 1.96.1\n")),
    ] {
        let mut changed = files.clone();
        changed.get_mut(name).unwrap().push(b' ');
        assert!(
            derived_release_driver::verify_exported_stage(
                &get_profile(),
                &receipt,
                &generated(),
                &proof,
                |name| changed
                    .get(name)
                    .cloned()
                    .ok_or_else(|| std::io::Error::other("missing fixture member"))
            )
            .is_err()
        );
    }
}
#[test]
fn stage_custody_rejects_wrong_binding_setup_and_missing_actual_command() {
    let (receipt, proof, files) = stage_plan_fixture();
    for change in 0..4 {
        let mut value: Value = serde_json::from_slice(&proof).unwrap();
        match change {
            0 => value["stage"]["binding_sha256"] = json!("9".repeat(64)),
            1 => {
                value["stage"]["worker_setup"]["tools"]["cargo"]["executable_sha256"] =
                    json!("9".repeat(64))
            }
            2 => value["stage"]["command_records"][0]["arguments"] = json!(["--help"]),
            _ => value["stage"]["source_commit"] = json!("9".repeat(40)),
        }
        let raw = serde_json::to_vec(&value).unwrap();
        assert!(
            derived_release_driver::verify_exported_stage(
                &get_profile(),
                &receipt,
                &generated(),
                &raw,
                |name| files
                    .get(name)
                    .cloned()
                    .ok_or_else(|| std::io::Error::other("missing fixture member"))
            )
            .is_err()
        );
    }
}
