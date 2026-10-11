//! A generated-doc check must not append uncheckpointed history to an agent PR.
const DOCS_SYNC: &str = include_str!("../.github/workflows/docs_sync.yml");

#[test]
fn pr_documentation_sync_is_read_only_and_reports_governed_remediation() {
    for forbidden in [
        "git commit",
        "git push",
        "git-auto-commit",
        "create-pull-request",
    ] {
        assert!(
            !DOCS_SYNC.contains(forbidden),
            "PR documentation check must not create history through {forbidden}"
        );
    }
    let check = DOCS_SYNC
        .split("- name: Verify generated documentation has no drift")
        .nth(1)
        .expect("generated documentation drift gate");
    assert!(check.contains("set -euo pipefail"));
    assert!(check.contains(
        "git diff --exit-code HEAD -- docs/agent/command-contracts.md docs/agent/config-schema.md"
    ));
    assert!(check.contains("::error::Generated documentation differs"));
    assert!(check.contains("exit 1"));
    assert!(check.contains("decapod docs build"));
    assert!(check.contains("decapod validate"));
    assert!(check.contains("decapod govern artifacts checkpoint"));
    assert!(check.contains("stage .decapod/governance.json"));
    assert!(!check.contains("DECAPOD_VALIDATE_SKIP"));
}
