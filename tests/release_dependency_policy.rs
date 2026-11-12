//! Keep the registry release path and storage feature contract publishable.

fn manifest() -> toml::Value {
    toml::from_str(include_str!("../Cargo.toml")).expect("valid Cargo manifest")
}

#[test]
fn dactyl_release_uses_registry_version_and_matching_lockfile() {
    let manifest = manifest();
    let dependency = &manifest["dependencies"]["dactyl-db"];
    assert_eq!(dependency["version"].as_str(), Some("0.13.0"));
    for source in ["git", "rev", "branch", "tag", "path"] {
        assert!(
            dependency.get(source).is_none(),
            "unexpected {source} override"
        );
    }

    let lock: toml::Value =
        toml::from_str(include_str!("../Cargo.lock")).expect("valid Cargo lockfile");
    let packages = lock["package"].as_array().expect("locked packages");
    let dactyl: Vec<_> = packages
        .iter()
        .filter(|package| package["name"].as_str() == Some("dactyl-db"))
        .collect();
    assert_eq!(dactyl.len(), 1, "one canonical Dactyl dependency");
    assert_eq!(dactyl[0]["version"].as_str(), Some("0.13.0"));
    assert_eq!(
        dactyl[0]["source"].as_str(),
        Some("registry+https://github.com/rust-lang/crates.io-index")
    );
    assert_eq!(
        dactyl[0]["checksum"].as_str(),
        Some("ba334815ca49f203b6a933d559b84b5aca5559686237be9791f48d71b3cb6292")
    );
}

#[test]
fn dactyl_release_preserves_sqlite_neon_and_default_supabase_features() {
    let manifest = manifest();
    let features = manifest["dependencies"]["dactyl-db"]["features"]
        .as_array()
        .expect("Dactyl features");
    for feature in ["sqlite", "neon"] {
        assert!(features.iter().any(|value| value.as_str() == Some(feature)));
    }
    assert!(
        manifest["features"]["default"]
            .as_array()
            .expect("default features")
            .iter()
            .any(|value| value.as_str() == Some("supabase-cloud"))
    );
    assert!(
        manifest["features"]["supabase-cloud"]
            .as_array()
            .expect("Supabase feature forwarding")
            .iter()
            .any(|value| value.as_str() == Some("dactyl-db/supabase"))
    );
}
