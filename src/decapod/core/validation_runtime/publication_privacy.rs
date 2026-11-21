use super::{Options, RuntimeBudget, git, invalid};
use crate::core::{fs_permissions, readiness_evidence::MaterialIdentity, store};
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::Path;

const POLICY_FILE: &str = ".decapod/data/publication-deny.txt";
const MAX_POLICY_BYTES: usize = 16 * 1024;
const MAX_RULES: usize = 128;
const MAX_RULE_BYTES: usize = 256;

struct Rule {
    line: usize,
    literal: String,
}

pub(super) fn check(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    identity: &MaterialIdentity,
) -> io::Result<()> {
    let policy_path = store::find_governance_root(&options.root).join(POLICY_FILE);
    let rules = read_rules(&policy_path)?;
    if rules.is_empty() {
        return Ok(());
    }
    let range = format!(
        "{}..{}",
        identity.accepted_base_revision, identity.head_revision
    );
    let history = git(
        options,
        budget,
        &[
            "log",
            "--no-notes",
            "--no-show-signature",
            "--format=%B",
            &range,
            "--",
        ],
    )?;
    check_match(&rules, &history)?;

    let patch_flags = [
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--ignore-submodules=none",
        "--text",
        "--unified=0",
    ];
    let mut arguments = vec![
        "-c",
        "core.quotePath=false",
        "log",
        "--no-notes",
        "--no-show-signature",
        "--format=",
        "--no-merges",
        "--patch",
    ];
    arguments.extend(patch_flags);
    arguments.extend([&range, "--"]);
    check_patch(&rules, &git(options, budget, &arguments)?)?;

    // The cumulative patch includes merge conflict resolutions that are absent
    // from the non-merge history scan.
    let cumulative = format!(
        "{}...{}",
        identity.accepted_base_revision, identity.head_revision
    );
    let mut arguments = vec!["-c", "core.quotePath=false", "diff"];
    arguments.extend(patch_flags);
    arguments.extend([&cumulative, "--"]);
    check_patch(&rules, &git(options, budget, &arguments)?)
}

fn read_rules(path: &Path) -> io::Result<Vec<Rule>> {
    if let Err(error) = path.symlink_metadata() {
        if error.kind() == io::ErrorKind::NotFound {
            return Ok(Vec::new());
        }
        return Err(error);
    }
    let input = fs_permissions::open_private_file(path, OpenOptions::new().read(true))?;
    let mut text = String::new();
    input
        .take(MAX_POLICY_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    if text.len() > MAX_POLICY_BYTES {
        return Err(invalid(
            "PUBLICATION_PRIVACY_POLICY_INVALID: local deny list exceeds its size limit",
        ));
    }

    let mut rules = Vec::new();
    for (line_number, literal) in text.lines().enumerate() {
        if literal.is_empty() {
            continue;
        }
        if rules.len() == MAX_RULES || literal.len() > MAX_RULE_BYTES || literal.contains('\0') {
            return Err(invalid(
                "PUBLICATION_PRIVACY_POLICY_INVALID: local deny list exceeds its rule limits",
            ));
        }
        rules.push(Rule {
            line: line_number + 1,
            literal: literal.to_owned(),
        });
    }
    Ok(rules)
}

fn check_patch(rules: &[Rule], patch: &[u8]) -> io::Result<()> {
    let mut lines = patch.split(|byte| *byte == b'\n').peekable();
    while let Some(line) = lines.next() {
        let path_is_retained = line.starts_with(b"diff --git ")
            && !lines
                .peek()
                .is_some_and(|next| next.starts_with(b"deleted file mode "));
        if line.starts_with(b"+") || path_is_retained {
            check_match(rules, line)?;
        }
    }
    Ok(())
}

fn check_match(rules: &[Rule], bytes: &[u8]) -> io::Result<()> {
    let text = String::from_utf8_lossy(bytes);
    if let Some(rule) = rules.iter().find(|rule| text.contains(&rule.literal)) {
        return Err(invalid(format!(
            "PUBLICATION_PRIVATE_IDENTIFIER: authored Git material matched local deny rule on line {}. Inspect the private policy and remove the match from the authored history before publication.",
            rule.line
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/publication_privacy_tests.rs"]
mod tests;
