use std::path::{Path, PathBuf};
use std::process::Command;

fn read_first_existing(root: &Path, rel_paths: &[&str]) -> String {
    for rel in rel_paths {
        let path = root.join(rel);
        if path.exists() {
            return std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        }
    }
    panic!("none of the expected files exist: {}", rel_paths.join(", "));
}

#[test]
fn claude_workflow_example_contains_required_ops() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow = read_first_existing(
        &root,
        &[
            "project/examples/claude_code_workflow.md",
            "CLAUDE.md",
            "AGENTS.md",
        ],
    );
    assert!(workflow.contains("decapod validate"));
    assert!(workflow.contains("decapod docs ingest"));
    assert!(
        workflow.contains("decapod session acquire") || workflow.contains("decapod session init")
    );
    assert!(
        workflow.contains("decapod workspace ensure")
            || workflow.contains("decapod workspace publish")
    );
}

#[test]
fn release_check_surface_exists_and_runs() {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["release", "check"])
        .output()
        .expect("run release check");
    assert!(
        output.status.success(),
        "release check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"status\":\"ok\""),
        "release check should emit ok envelope"
    );
}

#[test]
fn release_inventory_surface_exists_and_writes_artifact() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(&root)
        .args(["release", "inventory"])
        .output()
        .expect("run release inventory");
    assert!(
        output.status.success(),
        "release inventory failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"cmd\":\"release.inventory\""),
        "release inventory should emit envelope"
    );
    assert!(
        root.join(".decapod/generated/artifacts/inventory/repo_inventory.json")
            .exists(),
        "release inventory should write deterministic artifact"
    );
    std::fs::remove_file(root.join(".decapod/generated/artifacts/inventory/repo_inventory.json"))
        .expect("cleanup generated inventory artifact");
}

#[test]
fn verification_guide_pins_jit_capsule_flow() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let guide = read_first_existing(
        &root,
        &[
            "docs/VERIFICATION.md",
            "AGENTS.md",
            ".decapod/generated/specs/VALIDATION.md",
        ],
    );
    let capsule_contract =
        read_first_existing(&root, &["constitution/interfaces/AGENT_CONTEXT_PACK.md"]);
    assert!(
        guide.contains("decapod govern capsule query"),
        "verification guide must include governed capsule query flow"
    );
    assert!(
        capsule_contract.contains(".decapod/generated/policy/context_capsule_policy.json"),
        "verification guide must pin capsule policy contract artifact path"
    );
    assert!(
        capsule_contract.contains("CAPSULE_SCOPE_DENIED"),
        "verification guide must include fail-closed policy denial marker"
    );
}

#[test]
fn release_lineage_sync_surface_exists_and_runs() {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["release", "lineage-sync"])
        .output()
        .expect("run release lineage sync");
    assert!(
        output.status.success(),
        "release lineage sync failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"cmd\":\"release.lineage_sync\""),
        "release lineage sync should emit ok envelope"
    );
}
