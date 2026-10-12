load("@rules_rust//rust:defs.bzl", "rust_binary", "rust_library", "rust_test")
load(
    "@rules_rust//cargo:defs.bzl",
    "cargo_build_script",
    "cargo_toml_env_vars",
)
load("@crates//:defs.bzl", "all_crate_deps")


def decapod_targets():
    # Native-proof integration fixture only; never a shipped Cargo binary.
    rust_binary(
        name = "native_proof_fixture",
        srcs = ["tests/fixtures/native_proof_fixture.rs"],
        edition = "2024",
        testonly = True,
    )

    # Compile the real verifier for its shell-boundary test without ambient rustc.
    rust_binary(
        name = "integration_custody_fixture",
        srcs = ["assets/tools/integration_custody.rs"],
        edition = "2024",
        testonly = True,
    )

    # Docs-boundary tests run in relocated shards without an ambient Rust install.
    rust_binary(
        name = "docs_sync_fixture",
        srcs = ["tests/fixtures/docs_sync_fixture.rs"],
        edition = "2024",
        testonly = True,
    )

    # This supervisor owns only test scratch, never Bazel's output/cache trees.
    rust_binary(
        name = "build_lifecycle",
        srcs = ["assets/tools/build_lifecycle.rs"],
        edition = "2024",
    )

    rust_binary(
        name = "integration_shards",
        srcs = ["assets/tools/integration_shards.rs"],
        edition = "2024",
    )

    rust_test(
        name = "build_lifecycle_tests",
        srcs = [
            "assets/tools/build_lifecycle.rs",
            "assets/tools/build_lifecycle_tests.rs",
        ],
        crate_root = "assets/tools/build_lifecycle.rs",
        edition = "2024",
        compile_data = [
            "//:.cargo/config.toml",
            "//.config/build:BUILD.bazel",
            "//.config/build:bazelrc",
            "//.config/build:decapod.BUILD.bzl",
        ],
    )

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
        srcs = native.glob(["src/**/*.rs"], exclude = ["src/main.rs", "src/bin/*.rs"]),
        compile_data = native.glob(["src/**/*.sql"])
        + [
            # Synthetic release inputs are materialized only inside test fixtures.
            "tests/fixtures/derived-release/Cargo.toml.fixture",
            "tests/fixtures/derived-release/Cargo.lock",
            "tests/fixtures/derived-release/CHANGELOG.md",
            "tests/fixtures/derived-release/.github_release.toml",
            "tests/fixtures/derived-release/dist-workspace.toml",
            "tests/fixtures/derived-release/dist.toml",
        ]
        + native.glob(["src/decapod/core/snapshot_provider/source_execution/*.json"])
        + native.glob(["assets/tools/source_ci/*.sh"])
        + native.glob(["assets/schemas/*.schema.json"])
        + [
            "assets/tools/source_ci_profile.rs",
                "assets/tools/source_ci_custody.rs",
                "assets/tools/source_ci_toolchain.rs",
                "assets/tools/source_ci_workflow_owned.rs",
                "assets/tools/release_metadata.rs",
                "assets/tools/docs_sync.rs",
                "assets/tools/integration_custody.rs",
                "assets/tools/ci_event_context.rs",
            "assets/tools/integration_shards.rs",
            "assets/templates/claims.json",
            "assets/templates/governance.json",
            "src/decapod/hooks/commit-msg.sh",
            "src/decapod/plugins/container/Dockerfile.template",
            "src/decapod/plugins/container/startup.sh",
            "src/decapod/plugins/container/host-control-plane.sh",
        ],
        edition = "2024",
        crate_features = ["supabase-cloud"],
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
        crate_features = ["supabase-cloud"],
        deps = [
            ":decapod_lib",
        ] + all_crate_deps(normal = True),
        proc_macro_deps = all_crate_deps(proc_macro = True),
    )

    rust_binary(
        name = "release_projection_drift",
        srcs = [
            "assets/tools/release_projection_drift.rs",
            "src/decapod/core/release_projection_drift.rs",
        ],
        crate_root = "assets/tools/release_projection_drift.rs",
        edition = "2024",
        deps = [":decapod_lib"] + all_crate_deps(normal = True),
        proc_macro_deps = all_crate_deps(proc_macro = True),
    )

    rust_test(
        name = "release_projection_drift_policy",
        srcs = [
            "assets/tools/release_projection_drift.rs",
            "src/decapod/core/release_projection_drift.rs",
            "assets/tools/release_projection_drift_tests.rs",
        ],
        crate_root = "assets/tools/release_projection_drift.rs",
        edition = "2024",
        deps = [":decapod_lib"] + all_crate_deps(normal = True, normal_dev = True),
        proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
    )

    rust_test(
        name = "core_tests",
        srcs = ["tests/core/core.rs"],
        edition = "2024",
        crate_features = ["supabase-cloud"],
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
            # The broker suite includes 20 separately authenticated clients,
            # crash recovery and launch probes. Individual client waits remain
            # bounded; retain a finite aggregate budget for their stage evidence.
            timeout = "long" if path == "tests/group_broker.rs" else "moderate",
            # This suite asserts uncontended process wall-clock budgets. Rust's
            # test-threads flag alone does not isolate concurrent Bazel targets.
            tags = ["exclusive"] if path == "tests/validate_termination.rs" else [],
            srcs = [path] + [
                "assets/tools/release_metadata.rs",
                "src/decapod/core/release_metadata.rs",
                "assets/tools/source_ci_profile.rs",
                "assets/tools/source_ci_custody.rs",
                "assets/tools/source_ci_toolchain.rs",
                "assets/tools/source_ci_workflow_owned.rs",
                "assets/tools/docs_sync.rs",
                "assets/tools/integration_custody.rs",
                "assets/tools/ci_event_context.rs",
                "assets/tools/integration_shards.rs",
            ] if path == "tests/release_metadata_policy.rs" else [path] + [
                "assets/tools/integration_shards.rs",
            ] if path == "tests/integration_sharding_policy.rs" else [path] + [
                "assets/tools/ci_event_context.rs",
                "src/decapod/core/release_metadata.rs",
            ] if path == "tests/ci_event_context_policy.rs" else [path] + [
                "assets/tools/integration_custody.rs",
                "assets/tools/ci_event_context.rs",
            ] if path == "tests/integration_custody_policy.rs" else [path] + [
                "assets/tools/docs_sync.rs",
                "tests/support/docs_sync_cases.rs",
            ] if path == "tests/docs_sync_checkpoint_boundary.rs" else [path] + [
                "assets/tools/source_ci_profile.rs",
                "assets/tools/source_ci_custody.rs",
                "assets/tools/source_ci_toolchain.rs",
                "assets/tools/source_ci_workflow_owned.rs",
                "assets/tools/release_metadata.rs",
                "assets/tools/docs_sync.rs",
                "assets/tools/integration_custody.rs",
                "assets/tools/ci_event_context.rs",
                "assets/tools/integration_shards.rs",
                "src/decapod/core/release_metadata.rs",
            ] if path in ["tests/source_ci_profile_policy.rs", "tests/ci_checkout_policy.rs"] else [path],
            crate_root = path,
            compile_data = native.glob(["assets/tools/source_ci/*.sh"]) + ([".github/actions/ci-context/action.yml"] if path == "tests/ci_event_context_policy.rs" else [".github/workflows/ci.yml", ".github/workflows/decapod-validate.yml", ".github/workflows/docs.yml", ".github/workflows/docs_sync.yml"] if path == "tests/integration_custody_policy.rs" else ["tests/fixtures/native_proof_fixture.rs"] if path == "tests/native_proof_execution.rs" else ["Cargo.toml", "Cargo.lock"] if path == "tests/release_dependency_policy.rs" else [
                ".github/workflows/ci.yml",
            ] if path == "tests/release_metadata_policy.rs" else [
                ".github/workflows/ci.yml",
                "//.config/build:decapod.BUILD.bzl",
            ] if path == "tests/integration_sharding_policy.rs" else [
                ".bazelversion",
                "//.config/build:bazelrc",
                ".github/actions/validation-checkout/action.yml",
                ".github/actions/ci-context/action.yml",
                ".github/workflows/ci.yml",
                ".github/workflows/decapod-validate.yml",
            ] if path == "tests/ci_checkout_policy.rs" else [
                ".github/actions/ci-context/action.yml",
                ".github/actions/source-ci-profile/action.yml",
                "assets/tools/source_ci_profile.rs",
                "assets/tools/source_ci_custody.rs",
                "assets/tools/source_ci_workflow_owned.rs",
            ] if path == "tests/release_workflow_policy.rs" else [
                "Cargo.toml",
                "dist.toml",
                "dist-workspace.toml",
            ] if path == "tests/release_distribution_policy.rs" else [
                "assets/schemas/trajectory.schema.json",
                "docs/agent/command-contracts.md",
                "docs/agent/payload-examples.md",
            ] if path == "tests/trajectory_docs.rs" else [
                "src/decapod/core/assets.rs",
                "src/decapod/core/docs_cli.rs",
                "src/decapod/core/validate.rs",
                "src/decapod/cli.rs",
                "src/decapod/lib.rs",
                "docs/agent/command-contracts.md",
                "docs/agent/config-schema.md",
            ] if path == "tests/specs_refresh_docs.rs" else [
                ".github/workflows/docs_sync.yml",
                ".github/actions/docs-drift/action.yml",
                "assets/tools/docs_sync.rs",
                "tests/fixtures/docs_sync_fixture.rs",
            ] if path == "tests/docs_sync_checkpoint_boundary.rs" else [
                ".github/actions/source-ci-profile/action.yml",
                "tests/fixtures/derived-release/Cargo.toml.fixture",
                "src/decapod/core/derived_release_profile_tests.rs",
            ] + native.glob([".github/workflows/*.yml"]) if path == "tests/source_ci_profile_policy.rs" else []),
            edition = "2024",
            crate_features = ["supabase-cloud"],
            rustc_env_files = [":cargo_pkg_env"],
            rustc_env = {
                "CARGO_BIN_EXE_decapod": "./$(rootpath //:decapod)",
                "DECAPOD_CUSTODY_TEST_FIXTURE": "./$(rootpath //:integration_custody_fixture)" if path == "tests/integration_custody_policy.rs" else "",
                "DECAPOD_DOCS_SYNC_TEST_FIXTURE": "./$(rootpath //:docs_sync_fixture)" if path == "tests/docs_sync_checkpoint_boundary.rs" else "",
                "DECAPOD_NATIVE_PROOF_TEST_FIXTURE": "./$(rootpath //:native_proof_fixture)" if path == "tests/native_proof_execution.rs" else "",
            },
            data = [
                "//:decapod",
                ".github/workflows/ci.yml",
                ".decapod/config.toml",
                ".decapod/contracts/README_CONTRACTS.json",
                "AGENTS.md",
                "CLAUDE.md",
                "CODEX.md",
                "GEMINI.md",
                "README.md",
                "SECURITY.md",
                "assets/constitution.json",
                "src/decapod/cli.rs",
                "src/decapod/lib.rs",
            ] + ([
                # This policy reads repository fixtures and executes only the
                # checked-in plan helper against mocked cargo binaries.
                "Cargo.toml",
                "Cargo.lock",
                ".github/scripts/release-plan.sh",
                ".github/workflows/release.yml",
                ".github/workflows/decapod-validate.yml",
                ".github/workflows/docs_sync.yml",
                "assets/Dockerfile.workspace",
            ] if path == "tests/release_workflow_policy.rs" else native.glob([
                ".github/workflows/*.yml",
            ], exclude = [".github/workflows/ci.yml"]) if path == "tests/source_ci_profile_policy.rs" else []) + native.glob([
                # Runtime state is intentionally absent from a clean checkout;
                # tests create the managed surfaces they need at runtime.
                ".decapod/managed/**/*",
                ".decapod/governance.json",
            ], allow_empty = True) + ([":native_proof_fixture"] if path == "tests/native_proof_execution.rs" else [":integration_custody_fixture"] if path == "tests/integration_custody_policy.rs" else [":docs_sync_fixture"] if path == "tests/docs_sync_checkpoint_boundary.rs" else []),
            deps = [
                ":decapod_lib",
            ] + all_crate_deps(normal = True, normal_dev = True),
            proc_macro_deps = all_crate_deps(proc_macro = True, proc_macro_dev = True),
        )
        for path in native.glob(["tests/*.rs"])
    ]

    # Automatically generate rust_test targets for plugins integration tests
    [
        rust_test(
            name = "plugins_" + path[14:-3] + "_tests",  # e.g., plugins_todo_tests
            srcs = [path],
            edition = "2024",
            crate_features = ["supabase-cloud"],
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
        for path in native.glob(["tests/plugins/*.rs"], exclude = ["tests/plugins/mod.rs"])
    ]
