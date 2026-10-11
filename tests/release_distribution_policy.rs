//! cargo-dist must package the runtime without enabling contributor-only tools.

use toml::Value;

fn manifest(source: &str) -> Value {
    toml::from_str(source).expect("parse release configuration")
}

#[test]
fn distribution_overlays_only_the_runtime_binary_selection() {
    let cargo = manifest(include_str!("../Cargo.toml"));
    let workspace = manifest(include_str!("../dist-workspace.toml"));
    let package = manifest(include_str!("../dist.toml"));

    assert_eq!(
        workspace["workspace"]["members"],
        Value::from(vec!["cargo:."])
    );
    assert_eq!(package["package"]["binaries"], Value::from(vec!["decapod"]));
    assert_eq!(
        package["package"].as_table().unwrap().len(),
        1,
        "inherit identity, version and the build command from Cargo; release-plz only bumps Cargo's version"
    );
    assert!(
        cargo["workspace"]
            .get("metadata")
            .and_then(|metadata| metadata.get("dist"))
            .is_none(),
        "keep dist settings in the outer workspace so package overlays are applied"
    );
}

#[test]
fn distribution_preserves_the_pinned_tool_and_four_release_targets() {
    let workspace = manifest(include_str!("../dist-workspace.toml"));
    let dist = &workspace["dist"];
    assert_eq!(dist["cargo-dist-version"].as_str(), Some("0.22.1"));
    assert_eq!(dist["ci"].as_str(), Some("github"));
    assert_eq!(dist["installers"].as_array().unwrap().len(), 0);
    assert_eq!(dist["allow-dirty"], Value::from(vec!["ci"]));
    assert_eq!(
        dist["targets"],
        Value::from(vec![
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ])
    );
    let runners = &dist["github-custom-runners"];
    assert_eq!(runners["global"].as_str(), Some("ubuntu-24.04"));
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        assert_eq!(runners[target].as_str(), Some("ubuntu-24.04"));
    }
    for target in ["x86_64-apple-darwin", "aarch64-apple-darwin"] {
        assert_eq!(runners[target].as_str(), Some("macos-14"));
    }
}

#[test]
fn release_selection_does_not_enable_maintenance_tools() {
    let cargo = manifest(include_str!("../Cargo.toml"));
    let workspace = manifest(include_str!("../dist-workspace.toml"));
    let dist = &workspace["dist"];
    assert_ne!(
        dist.get("all-features").and_then(Value::as_bool),
        Some(true)
    );
    assert!(dist.get("features").is_none());
    assert!(
        !cargo["features"]["default"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature.as_str() == Some("build-tools"))
    );

    let binaries = cargo["bin"].as_array().unwrap();
    let runtime = binaries
        .iter()
        .find(|bin| bin["name"].as_str() == Some("decapod"))
        .expect("runtime binary");
    assert_eq!(runtime["path"].as_str(), Some("src/main.rs"));
    assert!(runtime.get("required-features").is_none());
    for name in [
        "decapod-build",
        "decapod-release-metadata",
        "decapod-release-projection-drift",
    ] {
        let tool = binaries
            .iter()
            .find(|bin| bin["name"].as_str() == Some(name))
            .expect("maintenance binary");
        assert_eq!(tool["required-features"], Value::from(vec!["build-tools"]));
        assert!(tool["path"].as_str().unwrap().starts_with("assets/tools/"));
    }
}
