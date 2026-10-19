//! Source audit only. A maintainer review is an external judgment proxy, not
//! independently adjudicated ground truth about every fact in a bounded prompt.
//! No fitting, candidate inference or final scientific acceptance occurs here.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SCHEMA: &str = "decapod.vega.public-review-source-records.v1";
pub const SOURCE_SHA256: &str =
    "sha256:50ce9421f775db9145c8bef8540a32e0b2f8581222627dd426de460d25cf33c2";

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing source field {key}").into())
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("missing source list {key}").into())
}
fn utc(value: &str) -> Result<&str> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7 | 10 | 13 | 16 | 19) || b.is_ascii_digit())
    {
        return Err("source timestamp must use canonical second-resolution UTC".into());
    }
    let year: u32 = value[..4].parse()?;
    let month: usize = value[5..7].parse()?;
    let day: u32 = value[8..10].parse()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days[month - 1]
        || value[11..13].parse::<u32>()? > 23
        || value[14..16].parse::<u32>()? > 59
        || value[17..19].parse::<u32>()? > 59
    {
        return Err("invalid source calendar timestamp".into());
    }
    Ok(value)
}
fn complete(connection: &Value) -> bool {
    connection["pageInfo"]["hasNextPage"] == false
        && connection["nodes"].is_array()
        && connection["totalCount"].as_u64()
            == connection["nodes"].as_array().map(|v| v.len() as u64)
}
fn trusted(review: &Value, author: &str) -> bool {
    review["author"]["__typename"] == "User"
        && review["author"]["login"]
            .as_str()
            .is_some_and(|login| login != author)
        && matches!(
            review["authorAssociation"].as_str(),
            Some("OWNER" | "MEMBER" | "COLLABORATOR")
        )
}
fn old_corpus_owner(owner: &str) -> bool {
    matches!(
        owner,
        "astral-sh" | "nushell" | "sharkdp" | "burntsushi" | "tokio-rs" | "uutils"
    )
}
fn audit(row: &Value) -> Result<Value> {
    let repo = text(row, "repo")?;
    let (owner, name) = repo.split_once('/').ok_or("invalid source repository")?;
    if [owner, name].iter().any(|part| {
        part.is_empty()
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    }) {
        return Err("invalid source repository identity".into());
    }
    let commit = text(row, "commit")?;
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid source commit".into());
    }
    let record = &row["record"];
    let pr = &record["pullRequest"];
    if record["nameWithOwner"] != repo
        || pr["number"] != row["number"]
        || record["object"]["oid"] != commit
        || row["review_id"].as_u64().is_none()
    {
        return Err("source repository/PR/commit/review binding mismatch".into());
    }
    let expected_url = format!(
        "https://github.com/{repo}/pull/{}#pullrequestreview-{}",
        row["number"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or("invalid source PR number")?,
        row["review_id"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or("invalid source review ID")?
    );
    if row["review_url"] != expected_url {
        return Err("source review URL identity mismatch".into());
    }
    let label = match text(row, "source_label")? {
        "approved" => "APPROVED",
        "changes_requested" => "CHANGES_REQUESTED",
        _ => return Err("invalid source review label".into()),
    };
    let cutoff = utc(text(row, "review_at")?)?;
    let reviews = &pr["reviews"];
    let selected = array(reviews, "nodes")?
        .iter()
        .find(|v| v["databaseId"] == row["review_id"])
        .ok_or("selected review absent from captured history")?;
    if selected["commit"]["oid"] != commit
        || selected["state"] != label
        || selected["submittedAt"] != cutoff
        || selected["url"] != row["review_url"]
    {
        return Err("selected source review identity/outcome mismatch".into());
    }
    let mut reasons = BTreeSet::new();
    if old_corpus_owner(&owner.to_ascii_lowercase()) {
        reasons.insert("owner_used_by_previous_corpus");
    }
    if !trusted(selected, text(&pr["author"], "login")?) {
        reasons.insert("review_not_external_source_maintainer");
    }
    if !complete(reviews) {
        reasons.insert("incomplete_review_history");
    }
    if utc(text(pr, "createdAt")?)? > cutoff {
        return Err("review predates PR creation".into());
    }
    if !pr["lastEditedAt"].is_null() && utc(text(pr, "lastEditedAt")?)? > cutoff {
        reasons.insert("requirements_body_edited_after_decision");
    }
    // GitHub's filtered timeline totalCount can count unrelated event types.
    // Pagination completeness and returned rename events are checked separately.
    let titles = &pr["timelineItems"];
    if titles["pageInfo"]["hasNextPage"] != false {
        reasons.insert("incomplete_title_history");
    }
    for edit in array(titles, "nodes")? {
        if utc(text(edit, "createdAt")?)? > cutoff {
            reasons.insert("title_edited_after_decision");
        }
    }
    if text(pr, "title")?.trim().is_empty() || text(pr, "body")?.trim().is_empty() {
        reasons.insert("missing_explicit_requirement_description");
    }
    let mut opinions = BTreeMap::new();
    for review in array(reviews, "nodes")? {
        if !trusted(review, text(&pr["author"], "login")?)
            || review["commit"]["oid"] != commit
            || !matches!(
                review["state"].as_str(),
                Some("APPROVED" | "CHANGES_REQUESTED")
            )
        {
            continue;
        }
        let at = utc(text(review, "submittedAt")?)?;
        if at <= cutoff {
            let login = text(&review["author"], "login")?;
            let previous = opinions
                .entry(login)
                .or_insert((at, text(review, "state")?));
            if at > previous.0 {
                *previous = (at, text(review, "state")?);
            }
        }
    }
    if opinions.values().any(|(_, state)| *state != label) {
        reasons.insert("conflicting_same_commit_maintainer_verdicts");
    }
    let contexts = &record["object"]["statusCheckRollup"]["contexts"];
    if !complete(contexts) {
        reasons.insert("incomplete_check_history");
    }
    let mut observed = Vec::new();
    let mut future = 0;
    let mut required = 0;
    for check in array(contexts, "nodes")? {
        let (name, conclusion, timestamp) = match check["__typename"].as_str() {
            Some("CheckRun") => (
                text(check, "name")?,
                text(check, "conclusion").unwrap_or("PENDING"),
                &check["completedAt"],
            ),
            Some("StatusContext") => (
                text(check, "context")?,
                text(check, "state")?,
                &check["createdAt"],
            ),
            _ => return Err("unknown public check observation type".into()),
        };
        let current_required = check["isRequired"]
            .as_bool()
            .ok_or("missing current requiredness")?;
        let before = timestamp
            .as_str()
            .map(utc)
            .transpose()?
            .is_some_and(|at| at <= cutoff);
        if !before {
            future += 1;
            if current_required {
                reasons.insert("required_check_not_complete_before_decision");
            }
            continue;
        }
        required += usize::from(current_required);
        if conclusion != "SUCCESS" && !(conclusion == "SKIPPED" && !current_required) {
            reasons.insert("adverse_check_before_decision");
        }
        observed.push(
            json!({"name":name,"conclusion":conclusion,"completed_at":timestamp,
            "required_at_capture":current_required}),
        );
    }
    if required == 0 {
        reasons.insert("no_observed_required_checks_before_decision");
    }
    let owner_group = owner.to_ascii_lowercase();
    // Source family, not variants or individual PRs, determines the prospective
    // split. Hash assignment and exclusions predate all inference on these data.
    let split_hash = decapod::core::vega_readiness::hash(
        format!("vega-public-review-split-v1:{owner_group}").as_bytes(),
    );
    let split = if u8::from_str_radix(&split_hash[7..9], 16)? < 128 {
        "development"
    } else {
        "holdout"
    };
    let input = json!({
        "intent":format!("{}\n{}",text(pr,"title")?,text(pr,"body")?),
        "validation":{"provider":"public GitHub source snapshot, not a Decapod receipt",
            "checks_observed_before_decision":observed},
        "plan_obligations":{"coverage":"No Decapod plan is published by this source; additional obligations are unknown, not declared empty"},
        "coverage":"Public requirements and captured checks only. Current requiredness does not authenticate historical branch policy. Source changes, raw logs and unpublished requirements are not supplied."
    });
    if serde_json::to_vec(&input)?.len() > 32 * 1024 {
        reasons.insert("input_exceeds_production_byte_bound");
    }
    Ok(json!({"repo":repo,"number":row["number"],"commit":commit,
        "source_review_id":row["review_id"],"source_review_url":row["review_url"],
        "source_review_at":cutoff,"source_verdict":label,"owner_group":owner_group,
        "prospective_split":split,"source_binding_eligible":reasons.is_empty(),"exclusions":reasons,
        "future_or_unfinished_checks_not_used":future,"input":input,
        "label_status":"external source-review proxy; semantic scope and contradictory evidence still require case audit",
        "independent_human_adjudication":false}))
}

pub fn audit_bundle(bytes: &[u8]) -> Result<Value> {
    if decapod::core::vega_readiness::hash(bytes) != SOURCE_SHA256 {
        return Err("frozen public source-record digest mismatch".into());
    }
    let bundle: Value = serde_json::from_slice(bytes)?;
    if bundle["schema"] != SCHEMA {
        return Err("unknown public review-source schema".into());
    }
    let mut seen = BTreeSet::new();
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for row in array(&bundle, "records")? {
        let identity = (
            row["repo"].clone().to_string(),
            row["review_id"].clone().to_string(),
        );
        if !seen.insert(identity) {
            return Err("duplicate public source review".into());
        }
        match audit(row) {
            Ok(case) => cases.push(case),
            Err(error) => failures.push(
                json!({"repo":row["repo"],"review_id":row["review_id"],"error":error.to_string()}),
            ),
        }
    }
    let eligible = cases
        .iter()
        .filter(|case| case["source_binding_eligible"] == true)
        .count();
    Ok(
        json!({"schema":"decapod.vega.public-review-source-audit.v1",
        "source_sha256":decapod::core::vega_readiness::hash(bytes),"source_records":seen.len(),
        "source_binding_eligible":eligible,"cases":cases,"malformed":failures,
        "publication_ready":false,"model_inference":"not_executed","calibration_fit":"not_performed",
        "production_calibration":"unsatisfied","holdout_frozen_for_inference":false,
        "limitations":["Current API history is not authenticated historical branch policy",
            "Source-member User identity does not establish unaided human authorship",
            "Review-to-input semantic labels require separate audit; approval is not a universal readiness oracle",
            "Repository owners are grouping units, not established statistically independent samples"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        let sha = "a".repeat(40);
        json!({"repo":"example/project","number":1,"commit":sha,"review_id":10,
            "source_label":"approved","review_at":"2026-10-18T12:00:00Z","review_url":"https://github.com/example/project/pull/1#pullrequestreview-10",
            "record":{"nameWithOwner":"example/project","object":{"oid":sha,"statusCheckRollup":{"contexts":{
                "totalCount":1,"pageInfo":{"hasNextPage":false},"nodes":[{"__typename":"CheckRun","name":"tests","conclusion":"SUCCESS","completedAt":"2026-10-18T11:00:00Z","isRequired":true}]}}},
            "pullRequest":{"number":1,"title":"verify the public behavior","body":"The documented behavior must have regression coverage.",
                "createdAt":"2026-10-18T10:00:00Z","lastEditedAt":null,"author":{"login":"contributor"},
                "timelineItems":{"pageInfo":{"hasNextPage":false},"nodes":[]},
                "reviews":{"totalCount":1,"pageInfo":{"hasNextPage":false},"nodes":[{"databaseId":10,"state":"APPROVED","submittedAt":"2026-10-18T12:00:00Z","url":"https://github.com/example/project/pull/1#pullrequestreview-10",
                    "authorAssociation":"MEMBER","author":{"__typename":"User","login":"maintainer"},"commit":{"oid":sha},"body":"SECRET_LABEL_RATIONALE"}]}}}})
    }
    #[test]
    fn source_audit_does_not_leak_review_verdict_or_rationale_to_input() {
        let result = audit(&fixture()).unwrap();
        assert_eq!(result["source_binding_eligible"], true);
        let input = result["input"].to_string();
        for secret in [
            "APPROVED",
            "SECRET_LABEL_RATIONALE",
            "maintainer",
            "review_id",
            "prospective_split",
        ] {
            assert!(!input.contains(secret));
        }
        assert_eq!(result["independent_human_adjudication"], false);
    }
    #[test]
    fn future_success_does_not_become_predecision_validation() {
        let mut row = fixture();
        row["record"]["object"]["statusCheckRollup"]["contexts"]["nodes"][0]["completedAt"] =
            "2026-10-18T12:01:00Z".into();
        let result = audit(&row).unwrap();
        assert_eq!(result["source_binding_eligible"], false);
        assert_eq!(
            result["input"]["validation"]["checks_observed_before_decision"],
            json!([])
        );
        assert_eq!(result["future_or_unfinished_checks_not_used"], 1);
    }
    #[test]
    fn edits_wrong_commit_and_incomplete_history_cannot_supply_labels() {
        let mut row = fixture();
        row["record"]["pullRequest"]["lastEditedAt"] = "2026-10-18T13:00:00Z".into();
        assert_eq!(audit(&row).unwrap()["source_binding_eligible"], false);
        row["record"]["pullRequest"]["reviews"]["pageInfo"]["hasNextPage"] = true.into();
        assert!(
            audit(&row).unwrap()["exclusions"]
                .as_array()
                .unwrap()
                .contains(&json!("incomplete_review_history"))
        );
        row["record"]["pullRequest"]["reviews"]["nodes"][0]["commit"]["oid"] =
            "b".repeat(40).into();
        assert!(audit(&row).is_err());
    }
    #[test]
    fn related_repositories_share_split_and_prior_corpus_owners_are_excluded() {
        let row = fixture();
        let mut related = row.clone();
        related["repo"] = "example/related".into();
        related["record"]["nameWithOwner"] = related["repo"].clone();
        assert_eq!(
            audit(&row).unwrap()["prospective_split"],
            audit(&related).unwrap()["prospective_split"]
        );
        assert!(old_corpus_owner("tokio-rs"));
        assert!(old_corpus_owner("astral-sh"));
    }
    #[test]
    fn timestamp_and_source_identity_are_strict() {
        for invalid in [
            "2026-02-29T00:00:00Z",
            "2026-10-18T24:00:00Z",
            "2026-10-18T00:00:00+00:00",
            "garbage",
        ] {
            assert!(utc(invalid).is_err());
        }
        assert!(utc("2024-02-29T23:59:59Z").is_ok());
        let mut row = fixture();
        row["repo"] = "../outside/project".into();
        assert!(audit(&row).is_err());
    }
    #[test]
    fn public_capture_is_pinned_and_audit_is_non_authorizing() {
        let bytes = include_bytes!(
            "../../../tests/fixtures/vega-public-review-sources/source-records.json"
        );
        let result = audit_bundle(bytes).unwrap();
        assert_eq!(result["source_records"], 356);
        assert_eq!(result["publication_ready"], false);
        assert_eq!(result["model_inference"], "not_executed");
        assert_eq!(result["production_calibration"], "unsatisfied");
        assert_eq!(result["holdout_frozen_for_inference"], false);
        assert_eq!(
            result["cases"].as_array().unwrap().len()
                + result["malformed"].as_array().unwrap().len(),
            356
        );
        assert!(audit_bundle(b"{}").is_err());
        eprintln!(
            "public review-source audit: {} source records; {} pass structural/timing filters; {} malformed; no inference or calibration acceptance",
            result["source_records"],
            result["source_binding_eligible"],
            result["malformed"].as_array().unwrap().len()
        );
    }
}
