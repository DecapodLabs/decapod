//! Deterministic mechanical clarity & controlled-English validator for
//! agent-generated governance artifacts (#1097, #1365).
//!
//! IMPORTANT: Decapod does NOT claim formal ASD-STE100 compliance. This module
//! implements a deterministic, zero-LLM rule-based profile inspired by
//! ASD-STE100 to surface objective mechanical clarity defects and canonical
//! terminology drift in durable governance artifacts, living specs, procedures,
//! CLI help, and error messages. Human prompts remain unconstrained.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const CLARITY_REPORT_SCHEMA: &str = "decapod.artifact-clarity.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClaritySeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClarityProfileKind {
    SteLite,
    SteStrict,
    Custom,
}

impl ClarityProfileKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ste-lite" | "lite" => Ok(Self::SteLite),
            "ste-strict" | "strict" => Ok(Self::SteStrict),
            "custom" => Ok(Self::Custom),
            other => Err(format!(
                "INVALID_CLARITY_PROFILE: unknown profile '{other}' (expected ste-lite, ste-strict, or custom)"
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SteLite => "ste-lite",
            Self::SteStrict => "ste-strict",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalTermEntry {
    pub canonical_term: String,
    pub meaning: String,
    pub disallowed_synonyms: Vec<String>,
}

/// Canonical Decapod governance terminology table (#1365).
/// One canonical term per core concept; one meaning per canonical term.
pub fn canonical_decapod_terminology() -> Vec<CanonicalTermEntry> {
    vec![
        CanonicalTermEntry {
            canonical_term: "agent".to_string(),
            meaning: "An automated actor invoking Decapod commands or proposing repository edits."
                .to_string(),
            disallowed_synonyms: vec![
                "ai assistant".to_string(),
                "coding bot".to_string(),
                "autonomous worker".to_string(),
                "task executor".to_string(),
            ],
        },
        CanonicalTermEntry {
            canonical_term: "todo".to_string(),
            meaning: "A tracked unit of governed work in Decapod's task store.".to_string(),
            disallowed_synonyms: vec![
                "work-ticket".to_string(),
                "jira item".to_string(),
                "backlog card".to_string(),
            ],
        },
        CanonicalTermEntry {
            canonical_term: "worktree".to_string(),
            meaning: "An isolated git working tree bound to a claimed Decapod todo.".to_string(),
            disallowed_synonyms: vec![
                "sandbox checkout".to_string(),
                "scratch clone".to_string(),
                "side branch folder".to_string(),
            ],
        },
        CanonicalTermEntry {
            canonical_term: "obligation".to_string(),
            meaning: "A derived, proof-gated requirement tracked until independently satisfied."
                .to_string(),
            disallowed_synonyms: vec![
                "deliverable promise".to_string(),
                "completion pledge".to_string(),
            ],
        },
        CanonicalTermEntry {
            canonical_term: "proof".to_string(),
            meaning: "Independently executed and observed verification evidence.".to_string(),
            disallowed_synonyms: vec![
                "self-attestation".to_string(),
                "confidence claim".to_string(),
            ],
        },
        CanonicalTermEntry {
            canonical_term: "checkpoint".to_string(),
            meaning: "An immutable digest-bound governance record anchored to staged material."
                .to_string(),
            disallowed_synonyms: vec!["savepoint tag".to_string(), "progress bookmark".to_string()],
        },
        CanonicalTermEntry {
            canonical_term: "constitution".to_string(),
            meaning: "Decapod's embedded institutional doctrine and pre-inference router."
                .to_string(),
            disallowed_synonyms: vec!["global rulebook".to_string(), "policy bible".to_string()],
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarityProfileConfig {
    pub kind: ClarityProfileKind,
    pub max_sentence_words: usize,
    pub default_severity: ClaritySeverity,
    pub check_sentence_length: bool,
    pub check_passive_voice: bool,
    pub check_nominalizations: bool,
    pub check_hedge_language: bool,
    pub check_marketing_language: bool,
    pub check_phrasal_verbs: bool,
    pub check_terminology_drift: bool,
    pub check_multi_action_sentences: bool,
    pub check_duplicate_concepts: bool,
    #[serde(default)]
    pub terminology: Vec<CanonicalTermEntry>,
}

impl ClarityProfileConfig {
    pub fn ste_lite() -> Self {
        Self {
            kind: ClarityProfileKind::SteLite,
            max_sentence_words: 28,
            default_severity: ClaritySeverity::Warning,
            check_sentence_length: true,
            check_passive_voice: true,
            check_nominalizations: true,
            check_hedge_language: true,
            check_marketing_language: true,
            check_phrasal_verbs: true,
            check_terminology_drift: true,
            check_multi_action_sentences: false,
            check_duplicate_concepts: true,
            terminology: canonical_decapod_terminology(),
        }
    }

    pub fn ste_strict() -> Self {
        Self {
            kind: ClarityProfileKind::SteStrict,
            max_sentence_words: 20,
            default_severity: ClaritySeverity::Error,
            check_sentence_length: true,
            check_passive_voice: true,
            check_nominalizations: true,
            check_hedge_language: true,
            check_marketing_language: true,
            check_phrasal_verbs: true,
            check_terminology_drift: true,
            check_multi_action_sentences: true,
            check_duplicate_concepts: true,
            terminology: canonical_decapod_terminology(),
        }
    }

    pub fn from_kind(kind: ClarityProfileKind) -> Self {
        match kind {
            ClarityProfileKind::SteLite => Self::ste_lite(),
            ClarityProfileKind::SteStrict => Self::ste_strict(),
            ClarityProfileKind::Custom => {
                let mut cfg = Self::ste_lite();
                cfg.kind = ClarityProfileKind::Custom;
                cfg
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarityFinding {
    pub finding_id: String,
    pub rule_id: String,
    pub severity: ClaritySeverity,
    pub source_path: String,
    pub line: usize,
    pub column: usize,
    pub offending_text: String,
    pub explanation: String,
    pub suggested_remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClarityReport {
    pub schema: String,
    pub profile: String,
    /// Explicitly records that Decapod does not claim formal ASD-STE100 certification (#1365).
    pub formal_asd_ste100_compliance: bool,
    pub deterministic: bool,
    pub passed: bool,
    pub files_checked: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub findings: Vec<ClarityFinding>,
}

const NOMINALIZATIONS: &[(&str, &str)] = &[
    ("perform an analysis", "analyze"),
    ("make a decision", "decide"),
    ("conduct a review", "review"),
    ("provide a verification", "verify"),
    ("carry out an evaluation", "evaluate"),
    ("give consideration to", "consider"),
    ("take into consideration", "consider"),
    ("perform a check", "check"),
    ("make a modification", "modify"),
    ("provide an explanation", "explain"),
    ("achieve completion", "complete"),
    ("perform validation", "validate"),
];

const HEDGE_PHRASES: &[(&str, &str)] = &[
    ("may or may not", "state the exact condition explicitly"),
    ("seems to", "state observed behavior directly"),
    ("appears to", "state observed behavior directly"),
    ("kind of", "replace with a precise technical term"),
    ("sort of", "replace with a precise technical term"),
    ("arguably", "state the falsifiable condition or omit"),
    ("possibly", "specify the exact condition or state unknown"),
    ("probably", "specify the measured probability or condition"),
    ("perhaps", "state the concrete condition"),
    ("potentially", "specify the triggering condition"),
    (
        "might",
        "use 'may' only for explicit permission or specify the exact condition",
    ),
];

const MARKETING_WORDS: &[(&str, &str)] = &[
    ("revolutionary", "describe the concrete mechanism"),
    ("seamless", "specify exact integration behavior"),
    ("blazingly", "cite measured latency or throughput"),
    ("world-class", "cite verifiable benchmark criteria"),
    ("cutting-edge", "describe the concrete technical approach"),
    ("next-generation", "name the exact version or capability"),
    ("best-in-class", "cite objective comparison criteria"),
    ("effortless", "specify the exact user steps required"),
    ("supercharged", "state the measurable change"),
    (
        "game-changing",
        "describe the specific architectural impact",
    ),
    (
        "enterprise-grade",
        "name the enforced security or reliability guarantees",
    ),
];

const PHRASAL_VERBS: &[(&str, &str)] = &[
    ("look into", "investigate"),
    ("figure out", "determine"),
    ("set up", "configure"),
    ("clean up", "remove"),
    ("carry out", "execute"),
    ("find out", "discover"),
    ("bring up", "raise"),
    ("point out", "identify"),
    ("come up with", "create"),
    ("turn on", "enable"),
    ("turn off", "disable"),
];

const PASSIVE_AUXILIARIES: &[&str] = &["is", "are", "was", "were", "be", "been", "being"];
const IRREGULAR_PARTICIPLES: &[&str] = &[
    "built",
    "chosen",
    "done",
    "driven",
    "forgotten",
    "given",
    "held",
    "kept",
    "known",
    "made",
    "proven",
    "read",
    "run",
    "seen",
    "sent",
    "set",
    "shown",
    "taken",
    "written",
];

const MULTI_ACTION_CONJUNCTIONS: &[&str] = &[
    " and then ",
    " as well as ",
    ", and also ",
    "; also ",
    " and additionally ",
];

const SYNONYM_ROTATION_GROUPS: &[(&str, &[&str])] = &[
    ("agent", &["agent", "worker", "executor"]),
    ("todo", &["todo", "ticket", "work-item"]),
    ("proof", &["proof", "attestation", "guarantee"]),
];

fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn find_word_phrase(haystack_lower: &str, needle: &str) -> Option<usize> {
    let mut search_start = 0;
    while let Some(rel) = haystack_lower[search_start..].find(needle) {
        let idx = search_start + rel;
        let before_ok = idx == 0
            || !haystack_lower[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let end = idx + needle.len();
        let after_ok = end == haystack_lower.len()
            || !haystack_lower[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if before_ok && after_ok {
            return Some(idx);
        }
        search_start = idx + 1;
    }
    None
}

fn looks_like_past_participle(word: &str) -> bool {
    let clean = word.trim_matches(|c: char| !c.is_ascii_alphabetic());
    if clean.len() < 4 {
        return false;
    }
    if IRREGULAR_PARTICIPLES.contains(&clean) {
        return true;
    }
    // Common non-participle adjectives ending in -ed excluded to reduce false positives.
    if matches!(
        clean,
        "required"
            | "expected"
            | "allowed"
            | "enabled"
            | "disabled"
            | "bounded"
            | "scoped"
            | "pinned"
            | "named"
            | "typed"
            | "fixed"
            | "based"
            | "owned"
            | "shared"
            | "isolated"
            | "red"
            | "bed"
            | "need"
            | "seed"
            | "speed"
            | "feed"
            | "used"
    ) {
        return false;
    }
    clean.ends_with("ed")
}

fn split_sentences(line: &str) -> Vec<(usize, &str)> {
    let mut sentences = Vec::new();
    let mut start = 0usize;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if (b == b'.' || b == b'!' || b == b'?')
            && (i + 1 == bytes.len() || bytes[i + 1].is_ascii_whitespace())
        {
            let slice = line[start..=i].trim();
            if !slice.is_empty() {
                sentences.push((start, slice));
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < line.len() {
        let slice = line[start..].trim();
        if !slice.is_empty() {
            sentences.push((start, slice));
        }
    }
    sentences
}

/// Deterministically analyze one text artifact under the given clarity profile.
pub fn validate_text_clarity(
    source_path: &str,
    content: &str,
    profile: &ClarityProfileConfig,
) -> Vec<ClarityFinding> {
    let mut findings = Vec::new();
    let mut in_fenced_code = false;
    let mut concept_variants_seen: BTreeMap<&str, BTreeSet<(&str, usize)>> = BTreeMap::new();
    let mut counter = 0usize;

    let mut push_finding = |rule_id: &str,
                            severity: ClaritySeverity,
                            line: usize,
                            column: usize,
                            offending_text: String,
                            explanation: String,
                            suggested_remediation: String| {
        counter += 1;
        findings.push(ClarityFinding {
            finding_id: format!("CL-{}-{:04}", rule_id, counter),
            rule_id: rule_id.to_string(),
            severity,
            source_path: source_path.to_string(),
            line,
            column,
            offending_text,
            explanation,
            suggested_remediation,
        });
    };

    for (zero_line, raw_line) in content.lines().enumerate() {
        let line_no = zero_line + 1;
        let trimmed = raw_line.trim();
        if trimmed.starts_with("```") {
            in_fenced_code = !in_fenced_code;
            continue;
        }
        if in_fenced_code || trimmed.is_empty() || trimmed.starts_with("<!--") {
            continue;
        }
        // Skip markdown headings, tables, and URL-only lines from sentence prose checks.
        let clean_line = strip_inline_code(raw_line);
        let lower_line = clean_line.to_ascii_lowercase();

        if profile.check_sentence_length && !trimmed.starts_with('#') && !trimmed.starts_with('|') {
            for (offset, sentence) in split_sentences(&clean_line) {
                let words: Vec<&str> = sentence
                    .split_whitespace()
                    .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
                    .collect();
                if words.len() > profile.max_sentence_words {
                    push_finding(
                        "CLARITY_SENTENCE_LENGTH",
                        profile.default_severity,
                        line_no,
                        offset + 1,
                        sentence.chars().take(120).collect(),
                        format!(
                            "Sentence has {} words, exceeding {} profile limit of {} words.",
                            words.len(),
                            profile.kind.as_str(),
                            profile.max_sentence_words
                        ),
                        format!(
                            "Split into short direct sentences of at most {} words.",
                            profile.max_sentence_words
                        ),
                    );
                }
            }
        }

        if profile.check_nominalizations {
            for (phrase, replacement) in NOMINALIZATIONS {
                if let Some(col) = find_word_phrase(&lower_line, phrase) {
                    push_finding(
                        "CLARITY_NOMINALIZATION",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        phrase.to_string(),
                        format!("Nominalization '{phrase}' obscures the direct technical action."),
                        format!("Replace '{phrase}' with the direct verb '{replacement}'."),
                    );
                }
            }
        }

        if profile.check_hedge_language {
            for (phrase, hint) in HEDGE_PHRASES {
                if let Some(col) = find_word_phrase(&lower_line, phrase) {
                    push_finding(
                        "CLARITY_HEDGE_LANGUAGE",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        phrase.to_string(),
                        format!(
                            "Hedging phrase '{phrase}' introduces ambiguity into technical governance prose."
                        ),
                        hint.to_string(),
                    );
                }
            }
        }

        if profile.check_marketing_language {
            for (word, hint) in MARKETING_WORDS {
                if let Some(col) = find_word_phrase(&lower_line, word) {
                    push_finding(
                        "CLARITY_MARKETING_LANGUAGE",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        word.to_string(),
                        format!(
                            "Non-verifiable marketing adjective '{word}' does not convey objective engineering state."
                        ),
                        hint.to_string(),
                    );
                }
            }
        }

        if profile.check_phrasal_verbs {
            for (phrase, replacement) in PHRASAL_VERBS {
                if let Some(col) = find_word_phrase(&lower_line, phrase) {
                    push_finding(
                        "CLARITY_PHRASAL_VERB",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        phrase.to_string(),
                        format!("Soft phrasal verb '{phrase}' reduces mechanical precision."),
                        format!("Replace '{phrase}' with '{replacement}'."),
                    );
                }
            }
        }

        if profile.check_passive_voice {
            let tokens: Vec<&str> = lower_line.split_whitespace().collect();
            for pair in tokens.windows(2) {
                let aux = pair[0].trim_matches(|c: char| !c.is_ascii_alphabetic());
                let next = pair[1].trim_matches(|c: char| !c.is_ascii_alphabetic());
                if PASSIVE_AUXILIARIES.contains(&aux) && looks_like_past_participle(next) {
                    let phrase = format!("{aux} {next}");
                    let col = find_word_phrase(&lower_line, &phrase).unwrap_or(0);
                    push_finding(
                        "CLARITY_PASSIVE_VOICE",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        phrase.clone(),
                        format!(
                            "Passive construction '{phrase}' omits or obscures the responsible actor."
                        ),
                        "Rewrite in active voice naming the explicit actor (e.g., 'Decapod validates ...' or 'The agent runs ...').".to_string(),
                    );
                }
            }
        }

        if profile.check_terminology_drift {
            for entry in &profile.terminology {
                for synonym in &entry.disallowed_synonyms {
                    let syn_lower = synonym.to_ascii_lowercase();
                    if let Some(col) = find_word_phrase(&lower_line, &syn_lower) {
                        push_finding(
                            "CLARITY_TERMINOLOGY_DRIFT",
                            profile.default_severity,
                            line_no,
                            col + 1,
                            synonym.clone(),
                            format!(
                                "Non-canonical synonym '{synonym}' drifts from canonical Decapod term '{}'.",
                                entry.canonical_term
                            ),
                            format!(
                                "Use canonical term '{}' ({}).",
                                entry.canonical_term, entry.meaning
                            ),
                        );
                    }
                }
            }
        }

        if profile.check_multi_action_sentences {
            for conj in MULTI_ACTION_CONJUNCTIONS {
                if let Some(col) = lower_line.find(conj) {
                    push_finding(
                        "CLARITY_MULTI_ACTION_SENTENCE",
                        profile.default_severity,
                        line_no,
                        col + 1,
                        conj.trim().to_string(),
                        "Sentence chains multiple actions with a compound connector.".to_string(),
                        "Use one instruction or action per sentence.".to_string(),
                    );
                }
            }
        }

        if profile.check_duplicate_concepts {
            for (canonical, group) in SYNONYM_ROTATION_GROUPS {
                for term in *group {
                    if find_word_phrase(&lower_line, term).is_some() {
                        concept_variants_seen
                            .entry(*canonical)
                            .or_default()
                            .insert((*term, line_no));
                    }
                }
            }
        }
    }

    if profile.check_duplicate_concepts {
        for (canonical, seen) in concept_variants_seen {
            let distinct_terms: BTreeSet<&str> = seen.iter().map(|(term, _)| *term).collect();
            if distinct_terms.len() > 1 {
                let first_line = seen.iter().map(|(_, line)| *line).min().unwrap_or(1);
                let joined: Vec<&str> = distinct_terms.into_iter().collect();
                push_finding(
                    "CLARITY_DUPLICATE_CONCEPT",
                    profile.default_severity,
                    first_line,
                    1,
                    joined.join(", "),
                    format!(
                        "Artifact rotates multiple terms ({}) for the same core concept '{canonical}'.",
                        joined.join("/")
                    ),
                    format!("Standardize on the single canonical term '{canonical}'."),
                );
            }
        }
    }

    findings.sort_by(|a, b| {
        (
            &a.source_path,
            a.line,
            a.column,
            &a.rule_id,
            &a.offending_text,
        )
            .cmp(&(
                &b.source_path,
                b.line,
                b.column,
                &b.rule_id,
                &b.offending_text,
            ))
    });
    findings
}

/// Extract governance-authored text fields from a structured governance JSON
/// document while intentionally leaving free-form human prompts unconstrained (#1365).
pub fn extract_governance_json_prose(value: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    fn walk(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(map) => {
                for (k, child) in map {
                    // Human prompts remain unconstrained by controlled-English rules (#1365).
                    if matches!(
                        k.as_str(),
                        "original_prompt"
                            | "user_prompt"
                            | "raw_prompt"
                            | "human_prompt"
                            | "signature"
                            | "hash"
                            | "sha256"
                            | "digest"
                            | "updated_at"
                            | "created_at"
                    ) {
                        continue;
                    }
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&path, child, out);
                }
            }
            Value::Array(items) => {
                for (idx, child) in items.iter().enumerate() {
                    walk(&format!("{prefix}[{idx}]"), child, out);
                }
            }
            Value::String(s)
                if s.split_whitespace().count() >= 4
                    && !s.starts_with("http://")
                    && !s.starts_with("https://")
                    && !s.starts_with("sha256:") =>
            {
                // Only inspect natural-language prose fields (multi-word sentences), not IDs/URLs/paths.
                out.push((prefix.to_string(), s.clone()));
            }
            _ => {}
        }
    }
    walk("", value, &mut out);
    out
}

/// Validate a governance JSON artifact (e.g. `.decapod/governance.json`) under a clarity profile.
pub fn validate_governance_json_clarity(
    source_path: &str,
    json_value: &Value,
    profile: &ClarityProfileConfig,
) -> Vec<ClarityFinding> {
    let mut findings = Vec::new();
    for (field_path, prose) in extract_governance_json_prose(json_value) {
        let labeled_path = format!("{source_path}#/{field_path}");
        findings.extend(validate_text_clarity(&labeled_path, &prose, profile));
    }
    findings
}

/// Run clarity validation over a list of `(source_path, content)` artifacts and
/// produce a deterministic `ClarityReport`.
pub fn evaluate_artifacts(
    artifacts: &[(&str, &str)],
    profile: &ClarityProfileConfig,
) -> ClarityReport {
    let mut findings = Vec::new();
    for (path, content) in artifacts {
        if path.ends_with(".json")
            && let Ok(val) = serde_json::from_str::<Value>(content)
        {
            findings.extend(validate_governance_json_clarity(path, &val, profile));
            continue;
        }
        findings.extend(validate_text_clarity(path, content, profile));
    }
    build_report(artifacts.len(), profile, findings)
}

fn build_report(
    files_checked: usize,
    profile: &ClarityProfileConfig,
    findings: Vec<ClarityFinding>,
) -> ClarityReport {
    let error_count = findings
        .iter()
        .filter(|f| f.severity == ClaritySeverity::Error)
        .count();
    let warning_count = findings
        .iter()
        .filter(|f| f.severity == ClaritySeverity::Warning)
        .count();
    let info_count = findings
        .iter()
        .filter(|f| f.severity == ClaritySeverity::Info)
        .count();

    ClarityReport {
        schema: CLARITY_REPORT_SCHEMA.to_string(),
        profile: profile.kind.as_str().to_string(),
        formal_asd_ste100_compliance: false,
        deterministic: true,
        passed: error_count == 0,
        files_checked,
        error_count,
        warning_count,
        info_count,
        findings,
    }
}

/// Run clarity validation over a repository's durable governance surfaces
/// (`.decapod/governance.json` and `.decapod/managed/specs/*.md`).
pub fn validate_workspace_clarity(
    workspace_root: &Path,
    profile_name: Option<&str>,
) -> Result<ClarityReport, String> {
    let kind = match profile_name {
        Some(name) => ClarityProfileKind::parse(name)?,
        None => ClarityProfileKind::SteLite,
    };
    let profile = ClarityProfileConfig::from_kind(kind);

    let mut collected: Vec<(String, String)> = Vec::new();
    let gov_path = workspace_root.join(".decapod/governance.json");
    if gov_path.is_file()
        && let Ok(raw) = fs::read_to_string(&gov_path)
    {
        collected.push((".decapod/governance.json".to_string(), raw));
    }
    let specs_dir = workspace_root.join(".decapod/managed/specs");
    if specs_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&specs_dir)
    {
        let mut spec_paths: Vec<_> = entries
            .filter_map(|e| e.ok().map(|ent| ent.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        spec_paths.sort();
        for path in spec_paths {
            if let Ok(raw) = fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                collected.push((rel, raw));
            }
        }
    }

    let borrowed: Vec<(&str, &str)> = collected
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    Ok(evaluate_artifacts(&borrowed, &profile))
}
