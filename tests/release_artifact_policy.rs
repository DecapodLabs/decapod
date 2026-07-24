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

#[test]
fn release_source_tree_contains_no_test_code() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let violations = rust_sources(&source_root)
        .into_iter()
        .filter_map(|path| {
            let source = fs::read_to_string(&path).expect("read Rust source");
            let has_test_case = source.lines().any(|line| line.trim() == "#[test]");
            let has_inline_test_module = source.lines().any(|line| {
                let line = line.trim();
                line == "mod tests {" || line.starts_with("mod tests {")
            });
            let lines = source.lines().map(str::trim).collect::<Vec<_>>();
            let has_invalid_test_module_declaration = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.starts_with("#[cfg(") && line.contains("test"))
                .any(|(index, _)| {
                    let path_attr = lines.get(index + 1).copied().unwrap_or_default();
                    let module = lines.get(index + 2).copied().unwrap_or_default();
                    !path_attr.starts_with("#[path = \"tests/unit/")
                        && !path_attr.starts_with("#[path = \"../../tests/unit/")
                        && !path_attr.starts_with("#[path = \"../../../tests/unit/")
                        || !module.starts_with("mod ")
                        || !module.ends_with(';')
                });
            (has_test_case || has_inline_test_module || has_invalid_test_module_declaration)
                .then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "test implementations must live under tests/unit/, found: {violations:?}"
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
