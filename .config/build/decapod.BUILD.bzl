load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")
load(
    "@rules_rust//cargo:defs.bzl",
    "cargo_build_script",
    "cargo_toml_env_vars",
)
load("@crates//:defs.bzl", "all_crate_deps")


def decapod_targets():
    cargo_toml_env_vars(
        name = "cargo_pkg_env",
        src = "Cargo.toml",
    )

    cargo_build_script(
        name = "build_script",
        srcs = ["assets/build/compress_constitution.rs"],
        edition = "2024",
        data = [
            "assets/constitution.json",
            "docs/agent/README.md",
            "docs/agent/api-index.md",
            "docs/agent/command-contracts.md",
            "docs/agent/config-schema.md",
            "docs/agent/error-recovery.md",
            "docs/agent/llms.txt",
            "docs/agent/mcp.md",
            "docs/agent/payload-examples.md",
            "docs/agent/state-model.md",
        ],
        deps = all_crate_deps(build = True),
    )

    rust_library(
        name = "decapod_lib",
        srcs = glob(["src/**/*.rs"], exclude = ["src/main.rs", "src/bin/*.rs"]),
        compile_data = glob(["src/**/*.sql"]) + glob(["assets/schemas/*.schema.json"]),
        edition = "2024",
        crate_name = "decapod",
        # Keep Bazel's release identity aligned with Cargo.toml instead of
        # duplicating the package version in this BUILD file.
        rustc_env_files = [":cargo_pkg_env"],
        deps = [
            ":build_script",
        ] + all_crate_deps(normal = True),
        proc_macro_deps = all_crate_deps(proc_macro = True),
    )

    rust_binary(
        name = "decapod",
        srcs = ["src/main.rs"],
        edition = "2024",
        deps = [
            ":decapod_lib",
        ] + all_crate_deps(normal = True),
        proc_macro_deps = all_crate_deps(proc_macro = True),
    )

    rust_test(
        name = "core_tests",
        srcs = ["tests/core/core.rs"],
        edition = "2024",
        rustc_env_files = [":cargo_pkg_env"],
        rustc_env = {
            "CARGO_BIN_EXE_decapod": "./$(rootpath //:decapod)",
        },
        data = [
            "//:decapod",
        ],
        deps = [
            ":decapod_lib",
        ] + all_crate_deps(normal = True, normal_dev = True),
        proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
    )

    # Automatically generate rust_test targets for each integration test in tests/*.rs
    [
        rust_test(
            name = path[6:-3],  # strip "tests/" and ".rs"
            srcs = [path],
            edition = "2024",
            rustc_env_files = [":cargo_pkg_env"],
            rustc_env = {
                "CARGO_BIN_EXE_decapod": "./$(rootpath //:decapod)",
            },
            data = [
                "//:decapod",
                ".decapod/config.toml",
                ".decapod/contracts/README_CONTRACTS.json",
            ] + glob([
                ".decapod/generated/**/*",
                ".decapod/governance/**/*",
            ]),
            deps = [
                ":decapod_lib",
            ] + all_crate_deps(normal = True, normal_dev = True),
            proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
        )
        for path in glob(["tests/*.rs"])
    ]

    # Automatically generate rust_test targets for plugins integration tests
    [
        rust_test(
            name = "plugins_" + path[14:-3] + "_tests",  # e.g., plugins_todo_tests
            srcs = [path],
            edition = "2024",
            rustc_env_files = [":cargo_pkg_env"],
            rustc_env = {
                "CARGO_BIN_EXE_decapod": "./$(rootpath //:decapod)",
            },
            data = [
                "//:decapod",
            ],
            deps = [
                ":decapod_lib",
            ] + all_crate_deps(normal = True, normal_dev = True),
            proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
        )
        for path in glob(["tests/plugins/*.rs"], exclude = ["tests/plugins/mod.rs"])
    ]
