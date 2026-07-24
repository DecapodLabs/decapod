// Moved from src/decapod/core/gatekeeper.rs
use super::*;
use tempfile::tempdir;

#[test]
fn test_glob_match() {
    assert!(glob_match("*", "foo"));
    assert!(glob_match("*.rs", "main.rs"));
    assert!(glob_match("**/.credentials", "foo/bar/.credentials"));
    assert!(glob_match("src/**", "src/lib.rs"));
    assert!(glob_match(".env*", ".env.local"));
}

#[test]
fn test_secret_patterns() {
    let patterns = secret_patterns();

    // AWS key
    let line = "AWS_KEY=AKIAIOSFODNN7EXAMPLE";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // GitHub token
    let line = "token=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // Private key
    let line = "-----BEGIN PRIVATE KEY-----";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));
}

#[test]
fn test_dangerous_patterns() {
    let patterns = dangerous_patterns();

    // eval with variable
    let line = "eval $CMD";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));

    // shell=True
    let line = "subprocess.run(cmd, shell=True)";
    assert!(patterns.iter().any(|p| p.is_match(line).unwrap_or(false)));
}

#[test]
fn test_gatekeeper_default_config() {
    let config = GatekeeperConfig::default();
    assert!(config.scan_secrets);
    assert!(config.scan_dangerous_patterns);
    assert!(!config.block_paths.is_empty());
}

#[test]
fn protected_paths_produce_typed_findings() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("README.md"), "safe\n").expect("write fixture");
    let config = GatekeeperConfig {
        protected_paths: vec!["README.md".to_string()],
        ..GatekeeperConfig::default()
    };

    let result = run_gatekeeper(tmp.path(), &[PathBuf::from("README.md")], 0, &config)
        .expect("run gatekeeper");
    assert!(!result.passed);
    assert!(
        result
            .violations
            .iter()
            .any(|violation| violation.kind == ViolationKind::ProtectedPath)
    );
}
