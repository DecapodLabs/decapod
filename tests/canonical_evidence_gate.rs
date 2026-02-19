use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn split_md_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

#[derive(Debug, Deserialize)]
struct KcrTrendRow {
    enforced_claims: u64,
    enforced_with_gate: u64,
    kcr: f64,
}

#[test]
fn enforced_claims_must_have_gate_mapping_and_kcr_trend_must_match() {
    let root = repo_root();
    let claims_path = root.join("constitution/interfaces/CLAIMS.md");
    let claims = fs::read_to_string(&claims_path).expect("read CLAIMS.md");

    let mut in_table = false;
    let mut enforced_total = 0u64;
    let mut enforced_with_gate = 0u64;

    for line in claims.lines() {
        if line.contains(
            "| Claim ID | Claim (normative) | Owner Doc | Enforcement | Proof Surface | Notes |",
        ) {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if !line.trim_start().starts_with('|') {
            break;
        }
        if line.contains("|---|") {
            continue;
        }
        let cols = split_md_row(line);
        if cols.len() < 6 || !cols[0].starts_with("claim.") {
            continue;
        }
        let enforcement = cols[3].to_lowercase();
        let proof = cols[4].trim();
        if enforcement == "enforced" {
            enforced_total += 1;
            let proof_lc = proof.to_lowercase();
            let has_mapping = !proof.is_empty()
                && proof_lc != "n/a"
                && !proof_lc.contains("planned:")
                && !proof_lc.contains("planned ");
            assert!(
                has_mapping,
                "ENFORCED claim lacks gate/test mapping in {}: {}",
                claims_path.display(),
                line
            );
            enforced_with_gate += 1;
        }
    }

    assert!(enforced_total > 0, "No enforced claims found in CLAIMS.md");
    let kcr = enforced_with_gate as f64 / enforced_total as f64;

    let trend_path = root.join("docs/metrics/KCR_TREND.jsonl");
    let trend = fs::read_to_string(&trend_path).expect("read KCR trend");
    let last = trend
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .expect("KCR trend must contain at least one row");
    let row: KcrTrendRow = serde_json::from_str(last).expect("parse KCR trend row JSON");

    assert_eq!(
        row.enforced_claims, enforced_total,
        "KCR trend enforced_claims mismatch"
    );
    assert_eq!(
        row.enforced_with_gate, enforced_with_gate,
        "KCR trend enforced_with_gate mismatch"
    );
    assert!(
        (row.kcr - kcr).abs() < 1e-9,
        "KCR trend row does not match computed KCR"
    );
}

#[test]
fn verified_closed_drift_rows_must_include_proof_links_and_citations() {
    let root = repo_root();
    let doc_path = root.join("docs/ARCH_REVIEW_CANONICAL.md");
    let doc = fs::read_to_string(&doc_path).expect("read ARCH_REVIEW_CANONICAL.md");
    let citation_re =
        Regex::new(r"[A-Za-z0-9_./-]+\.(rs|md):[0-9]+-[0-9]+").expect("valid citation regex");

    let mut checked = 0usize;
    for line in doc.lines() {
        if !line.trim_start().starts_with("| D-") {
            continue;
        }
        let cols = split_md_row(line);
        if cols.len() < 8 {
            continue;
        }
        let status = cols[7].to_uppercase();
        if !status.contains("VERIFIED CLOSED") {
            continue;
        }
        checked += 1;
        let claim_col = &cols[1];
        let runtime_col = &cols[2];
        let proof_col = &cols[5];

        assert!(
            citation_re.is_match(claim_col),
            "VERIFIED CLOSED row missing claim citation(s): {}",
            line
        );
        assert!(
            citation_re.is_match(runtime_col),
            "VERIFIED CLOSED row missing runtime citation(s): {}",
            line
        );
        assert!(
            proof_col.contains("->"),
            "VERIFIED CLOSED row must include failing->passing proof link: {}",
            line
        );
        assert!(
            proof_col.contains("tests/") || proof_col.contains("src/"),
            "VERIFIED CLOSED row must include proof reference path: {}",
            line
        );
    }

    assert!(
        checked > 0,
        "Expected at least one VERIFIED CLOSED drift row in canonical doc"
    );
}
