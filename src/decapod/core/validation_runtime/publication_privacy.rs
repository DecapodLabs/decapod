use super::{CONFIG, Options, RuntimeBudget, git, invalid};
use crate::core::{fs_permissions, readiness_evidence::MaterialIdentity, store};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::Path;

const POLICY_FILE: &str = ".decapod/data/publication-deny.txt";
const MAX_POLICY_BYTES: usize = 16 * 1024;
const MAX_RULES: usize = 128;
const MAX_RULE_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleSource {
    LocalPolicy,
    Config,
}

#[derive(Debug, Clone)]
struct Rule {
    source: RuleSource,
    line: usize,
    literal: String,
}

#[derive(Deserialize, Default)]
struct PartialPrivacyConfig {
    #[serde(default)]
    governance: PartialGovernancePrivacy,
}

#[derive(Deserialize, Default)]
struct PartialGovernancePrivacy {
    #[serde(default)]
    publication_deny_identifiers: Vec<String>,
}

pub(super) fn check(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    identity: &MaterialIdentity,
) -> io::Result<()> {
    let policy_path = store::find_governance_root(&options.root).join(POLICY_FILE);
    let mut rules = read_local_rules(&policy_path)?;
    rules.extend(read_config_rules(options, budget, identity)?);
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
    check_match(&rules, &history, false)?;

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

fn read_local_rules(path: &Path) -> io::Result<Vec<Rule>> {
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
            source: RuleSource::LocalPolicy,
            line: line_number + 1,
            literal: literal.to_owned(),
        });
    }
    Ok(rules)
}

fn parse_config_rules(bytes: &[u8]) -> io::Result<Vec<String>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid("PUBLICATION_PRIVACY_POLICY_INVALID: config.toml is not UTF-8"))?;
    let parsed: PartialPrivacyConfig = toml::from_str(text).map_err(|_| {
        invalid("PUBLICATION_PRIVACY_POLICY_INVALID: could not parse governance.publication_deny_identifiers")
    })?;
    Ok(parsed.governance.publication_deny_identifiers)
}

fn read_config_rules(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    identity: &MaterialIdentity,
) -> io::Result<Vec<Rule>> {
    let mut combined = BTreeSet::new();
    // Always enforce accepted-target config rules so a candidate cannot weaken them.
    let accepted_entries = git(
        options,
        budget,
        &[
            "ls-tree",
            "-z",
            &identity.accepted_base_revision,
            "--",
            CONFIG,
        ],
    )?;
    if !accepted_entries.is_empty() {
        let accepted_bytes = git(
            options,
            budget,
            &[
                "show",
                &format!("{}:{CONFIG}", identity.accepted_base_revision),
            ],
        )?;
        for item in parse_config_rules(&accepted_bytes)? {
            combined.insert(item);
        }
    }
    // Also honor newly added candidate config rules.
    let candidate_path = options.root.join(CONFIG);
    if let Ok(candidate_bytes) = std::fs::read(&candidate_path) {
        for item in parse_config_rules(&candidate_bytes)? {
            combined.insert(item);
        }
    }
    let mut rules = Vec::new();
    for (index, literal) in combined.into_iter().enumerate() {
        if literal.is_empty() {
            continue;
        }
        if rules.len() == MAX_RULES || literal.len() > MAX_RULE_BYTES || literal.contains('\0') {
            return Err(invalid(
                "PUBLICATION_PRIVACY_POLICY_INVALID: governance.publication_deny_identifiers exceeds its rule limits",
            ));
        }
        rules.push(Rule {
            source: RuleSource::Config,
            line: index + 1,
            literal,
        });
    }
    Ok(rules)
}

fn check_patch(rules: &[Rule], patch: &[u8]) -> io::Result<()> {
    let mut lines = patch.split(|byte| *byte == b'\n').peekable();
    let mut in_config_file = false;
    while let Some(line) = lines.next() {
        if line.starts_with(b"diff --git ") {
            in_config_file = line == b"diff --git a/.decapod/config.toml b/.decapod/config.toml";
        }
        let path_is_retained = line.starts_with(b"diff --git ")
            && !lines
                .peek()
                .is_some_and(|next| next.starts_with(b"deleted file mode "));
        if line.starts_with(b"+") || path_is_retained {
            check_match(rules, line, in_config_file)?;
        }
    }
    Ok(())
}

fn check_match(rules: &[Rule], bytes: &[u8], in_config_file: bool) -> io::Result<()> {
    let text = String::from_utf8_lossy(bytes);
    if let Some(rule) = rules.iter().find(|rule| {
        !(in_config_file && rule.source == RuleSource::Config) && text.contains(&rule.literal)
    }) {
        let origin = match rule.source {
            RuleSource::LocalPolicy => format!("local deny rule on line {}", rule.line),
            RuleSource::Config => format!(
                "governance.publication_deny_identifiers rule #{}",
                rule.line
            ),
        };
        return Err(invalid(format!(
            "PUBLICATION_PRIVATE_IDENTIFIER: authored Git material matched {origin}. Inspect the privacy policy and remove the match from the authored history before publication."
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/publication_privacy_tests.rs"]
mod tests;
