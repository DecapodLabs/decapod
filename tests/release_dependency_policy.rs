//! Keep the registry release path and storage feature contract publishable.

fn manifest() -> toml::Value {
    toml::from_str(include_str!("../Cargo.toml")).expect("valid Cargo manifest")
}

#[test]
fn dactyl_release_uses_registry_version_and_matching_lockfile() {
    let manifest = manifest();
    let dependency = &manifest["dependencies"]["dactyl-db"];
    assert_eq!(dependency["version"].as_str(), Some("=0.16.1"));
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
    assert_eq!(dactyl[0]["version"].as_str(), Some("0.16.1"));
    assert_eq!(
        dactyl[0]["source"].as_str(),
        Some("registry+https://github.com/rust-lang/crates.io-index")
    );
    assert_eq!(
        dactyl[0]["checksum"].as_str(),
        Some("1b41dde2e8be3fc986f7f5a7eb4e5bfb9af49d4c39dabe98fd74a0ccb4ad7ac6")
    );
}

#[test]
fn locked_quinn_proto_preserves_the_patched_floor_including_inactive_entries() {
    let lock: toml::Value =
        toml::from_str(include_str!("../Cargo.lock")).expect("valid Cargo lockfile");
    let packages = lock["package"].as_array().expect("locked packages");
    let versions: Vec<_> = packages
        .iter()
        .filter(|package| package["name"].as_str() == Some("quinn-proto"))
        .map(|package| {
            let version = package["version"].as_str().expect("Quinn protocol version");
            let components: Vec<u64> = version
                .split('.')
                .map(|part| part.parse().expect("stable numeric Quinn protocol version"))
                .collect();
            assert_eq!(components.len(), 3, "complete protocol version: {version}");
            (components[0], components[1], components[2])
        })
        .collect();
    assert!(
        !versions.is_empty(),
        "Quinn protocol remains in the lock audit"
    );
    for version in versions {
        assert!(version >= (0, 11, 19), "unpatched quinn-proto: {version:?}");
    }
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
