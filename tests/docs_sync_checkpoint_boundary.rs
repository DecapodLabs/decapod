//! Both PR and pre-PR adapters must use this exact, read-only docs proof.
#[allow(dead_code)]
#[path = "../assets/tools/docs_sync.rs"]
mod docs_sync;

const WORKFLOW: &str = include_str!("../.github/workflows/docs_sync.yml");
const ACTION: &str = include_str!("../.github/actions/docs-drift/action.yml");
const RECIPE: &str = include_str!("../assets/tools/source_ci/docs-drift.sh");
const TOOL: &str = include_str!("../assets/tools/docs_sync.rs");

#[test]
fn pr_documentation_sync_uses_least_privilege_immutable_shared_proof() {
    for forbidden in [
        "contents: write",
        "create-github-app-token",
        "secrets.",
        "APP_TOKEN",
        "github.head_ref",
        "git fetch",
        "git commit",
        "git push",
        "git-auto-commit",
        "create-pull-request",
        "xargs",
        "ref:",
    ] {
        assert!(
            !WORKFLOW.contains(forbidden),
            "PR docs workflow contains {forbidden}"
        );
        assert!(
            !ACTION.contains(forbidden),
            "Shared docs action contains {forbidden}"
        );
    }
    assert!(WORKFLOW.contains("contents: read"));
    assert!(WORKFLOW.contains("uses: actions/checkout@v4"));
    assert!(WORKFLOW.contains("fetch-depth: 0"));
    assert!(WORKFLOW.contains("persist-credentials: false"));
    assert!(WORKFLOW.contains("uses: ./.github/actions/docs-drift"));
    assert!(WORKFLOW.contains("base-sha: ${{ steps.context.outputs.base }}"));
    assert!(WORKFLOW.contains("head-sha: ${{ steps.context.outputs.head }}"));
    assert!(
        !WORKFLOW.contains("cargo build"),
        "adapter must not fork shared implementation"
    );
    assert!(
        !WORKFLOW.contains("docs build"),
        "adapter must not fork shared implementation"
    );
    assert!(ACTION.contains("BASE_SHA: ${{ inputs.base-sha }}"));
    assert!(ACTION.contains("HEAD_SHA: ${{ inputs.head-sha }}"));
    assert!(ACTION.contains("run-ci --check docs-drift"));
    assert!(RECIPE.contains("cargo build --all-features"));
    assert!(RECIPE.contains("select --repo . --base \"$BASE_SHA\" --head \"$HEAD_SHA\""));
    assert!(RECIPE.contains(
        "verify --repo . --base \"$BASE_SHA\" --head \"$HEAD_SHA\" --decapod target/debug/decapod"
    ));
    assert!(
        RECIPE.find("select --repo").unwrap() < RECIPE.find("cargo build --all-features").unwrap()
    );
    assert!(
        RECIPE.find("cargo build --all-features").unwrap() < RECIPE.find("verify --repo").unwrap()
    );
    assert!(TOOL.contains("\"--no-renames\""));
    assert!(TOOL.contains("\"-z\""));
    assert!(TOOL.contains("command.arg(\"--touched\").args(paths)"));
    assert!(!TOOL.contains("Command::new(\"bash\")"));
    assert!(!TOOL.contains("Command::new(\"sh\")"));
    assert!(TOOL.contains("--cached"));
    assert!(TOOL.contains("decapod docs build"));
    assert!(TOOL.contains("decapod validate"));
    assert!(TOOL.contains("decapod govern artifacts checkpoint"));
    assert!(TOOL.contains("stage .decapod/governance.json"));
    assert!(!TOOL.contains("DECAPOD_VALIDATE_SKIP"));
}
