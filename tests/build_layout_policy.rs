use std::fs;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn build_and_toolchain_configuration_has_one_canonical_home() {
    let root = root();
    let canonical = root.join(".config/build");

    assert!(canonical.join("BUILD.bazel").is_file());
    assert!(canonical.join("decapod.BUILD.bzl").is_file());
    assert!(canonical.join("decapod.MODULE.bazel").is_file());
    assert!(canonical.join("bazelrc").is_file());
    assert!(canonical.join("rust-toolchain.toml").is_file());

    assert_eq!(
        fs::read_to_string(root.join("BUILD.bazel")).expect("read Bazel root shim"),
        "load(\"//.config/build:decapod.BUILD.bzl\", \"decapod_targets\")\n\npackage(default_visibility = [\"//visibility:public\"])\n\ndecapod_targets()\n",
    );
    assert_eq!(
        fs::read_to_string(root.join("MODULE.bazel")).expect("read module root shim"),
        "module(\n    name = \"decapod\",\n)\n\ninclude(\"//.config/build:decapod.MODULE.bazel\")\n",
    );
    assert_eq!(
        fs::read_to_string(root.join(".bazelrc")).expect("read Bazel rc root shim"),
        "try-import %workspace%/.config/build/bazelrc\n",
    );

    let toolchain = root.join("rust-toolchain.toml");
    assert!(
        fs::symlink_metadata(&toolchain)
            .expect("inspect rustup compatibility shim")
            .file_type()
            .is_symlink(),
        "root rust-toolchain.toml must remain a symlink to the canonical .config/build file",
    );
    assert_eq!(
        fs::read_to_string(&toolchain).expect("read rustup compatibility shim"),
        fs::read_to_string(canonical.join("rust-toolchain.toml"))
            .expect("read canonical Rust toolchain"),
    );
}

#[test]
fn generated_bazel_lockfile_remains_at_the_bazel_root() {
    assert!(
        root().join("MODULE.bazel.lock").is_file(),
        "Bazel's generated module lockfile remains root-discovered",
    );
    assert!(
        !root().join(".config/build/MODULE.bazel.lock").exists(),
        "do not create a second competing generated lockfile",
    );
}
