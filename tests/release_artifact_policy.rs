//! Release policy: production `src/` may *reference* unit tests, never define them.
//!
//! Allowed in `src/**/*.rs`:
//! ```ignore
//! #[cfg(test)]
//! #[path = ".../tests/unit/..._tests.rs"]
//! mod tests;
//! ```
//!
//! Forbidden in `src/**/*.rs`:
//! - `#[test]` / other test-case attributes
//! - inline `mod … { … }` test bodies
//! - `#[cfg(test)]` modules without a `tests/unit/` path attribute

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).expect("read source directory") {
            let entry = entry.expect("read source entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn is_allowed_unit_test_path_attr(path_attr: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "#[path = \"tests/unit/",
        "#[path = \"../tests/unit/",
        "#[path = \"../../tests/unit/",
        "#[path = \"../../../tests/unit/",
        "#[path = \"../../../../tests/unit/",
    ];
    PREFIXES.iter().any(|prefix| path_attr.starts_with(prefix))
}

fn path_attr_target(source_file: &Path, path_attr: &str) -> Option<PathBuf> {
    let start = path_attr.find("#[path = \"")?;
    let rest = &path_attr[start + "#[path = \"".len()..];
    let end = rest.find('"')?;
    let rel = &rest[..end];
    Some(source_file.parent()?.join(rel))
}

/// Production sources may only *reference* unit tests under `tests/unit/`.
#[test]
fn release_source_tree_contains_no_test_code() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for path in rust_sources(&source_root) {
        let source = fs::read_to_string(&path).expect("read Rust source");
        let lines: Vec<&str> = source.lines().map(str::trim).collect();
        let mut reasons = Vec::new();

        for (index, line) in lines.iter().enumerate() {
            // Forbidden: test-case attributes in production sources.
            if *line == "#[test]"
                || line.starts_with("#[test(")
                || line.starts_with("#[tokio::test")
                || line.starts_with("#[rstest")
            {
                reasons.push(format!("line {}: test-case attribute `{line}`", index + 1));
            }

            // Forbidden: inline module bodies that look like test modules.
            if *line == "mod tests {" || line.starts_with("mod tests {") {
                reasons.push(format!(
                    "line {}: inline `mod tests {{ ... }}` body (tests must live under tests/unit/)",
                    index + 1
                ));
            }

            // cfg(test) modules must be path stubs into tests/unit/.
            if line.starts_with("#[cfg(") && line.contains("test") {
                // Skip attributes after cfg until path / mod.
                let mut cursor = index + 1;
                while cursor < lines.len()
                    && (lines[cursor].is_empty()
                        || (lines[cursor].starts_with("#[")
                            && !lines[cursor].starts_with("#[path")))
                {
                    cursor += 1;
                }
                let path_attr = lines.get(cursor).copied().unwrap_or_default();
                let module = lines.get(cursor + 1).copied().unwrap_or_default();

                let path_ok = is_allowed_unit_test_path_attr(path_attr);
                let module_ok = module.starts_with("mod ") && module.ends_with(';');
                if !path_ok || !module_ok {
                    reasons.push(format!(
                        "line {}: cfg(test) must be followed by `#[path = \".../tests/unit/...\"]` and `mod name;`, got path=`{path_attr}` module=`{module}`",
                        index + 1
                    ));
                    continue;
                }

                if let Some(target) = path_attr_target(&path, path_attr) {
                    if !target.exists() {
                        reasons.push(format!(
                            "line {}: unit test path does not exist: {}",
                            index + 1,
                            target.display()
                        ));
                    } else {
                        let canon = target.canonicalize().unwrap_or(target.clone());
                        let tests_unit = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unit");
                        let tests_unit = tests_unit.canonicalize().unwrap_or(tests_unit);
                        if !canon.starts_with(&tests_unit) {
                            reasons.push(format!(
                                "line {}: unit test path must resolve under tests/unit/, got {}",
                                index + 1,
                                canon.display()
                            ));
                        }
                    }
                }
            }
        }

        // Forbidden: test function definitions in production sources.
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("fn test_") || trimmed.starts_with("async fn test_") {
                reasons.push(format!(
                    "line {}: test function definition `{trimmed}` (must live under tests/unit/)",
                    index + 1
                ));
            }
        }

        if !reasons.is_empty() {
            violations.push(format!("{}: {}", path.display(), reasons.join("; ")));
        }
    }

    assert!(
        violations.is_empty(),
        "src/ may only reference unit tests under tests/unit/, never define them. Violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn release_targets_are_production_sources_plus_runtime_assets() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read Cargo manifest");
    assert!(manifest.contains("path = \"src/decapod/lib.rs\""));
    assert!(manifest.contains("path = \"src/main.rs\""));
    assert!(manifest.contains("build = \"assets/build/compress_constitution.rs\""));

    let bazel = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".config/build/decapod.BUILD.bzl"),
    )
    .expect("read canonical Bazel build graph");
    let library_graph = bazel
        .split("rust_binary(")
        .next()
        .expect("rust library graph");
    assert!(library_graph.contains("srcs = native.glob([\"src/**/*.rs\"]"));
    assert!(library_graph.contains("assets/schemas/*.schema.json"));
    assert!(
        !library_graph.contains("tests/"),
        "test sources must not be dependencies of the release library"
    );
}
