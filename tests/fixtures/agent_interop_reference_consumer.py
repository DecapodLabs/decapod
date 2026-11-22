#!/usr/bin/env python3
"""Independent reference consumer for Decapod Governance Envelope v1 (#870/#1733).

Uses ONLY the Python standard library. No Decapod imports, database, CLI, or
Rust types. Evaluates raw JSON governance envelopes against a local trust
policy and replay state and outputs normalized verdict + decisive reason class.

Conformance never grants local mutation, completion, or publication authority.
"""

import hashlib
import json
import sys

PROFILE_ID = "decapod.governance-envelope"
PROFILE_VERSION = "v1"
SUPPORTED_MAJOR = 1
SUPPORTED_EXTENSIONS = {
    "receipts",
    "interlocks",
    "trajectory-lineage",
    "delegation-narrowing",
    "decision-disposition",
}


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def parse_major(version: str):
    parts = version.split(".")
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        return None
    return int(parts[0])


def has_ambiguous_target(s: str) -> bool:
    lower = s.lower()
    if ".." in s or "\\" in s or "%2f" in lower or "%5c" in lower or "\x00" in s:
        return True
    if s.startswith("http://") or s.startswith("https://"):
        rest = s.split("://", 1)[1]
        authority = rest.split("/", 1)[0]
        if "@" in authority:
            return True
        host = authority.split(":", 1)[0]
        octets = host.split(".")
        if (
            len(octets) == 4
            and all(o.isdigit() for o in octets)
            and any(len(o) > 1 and o.startswith("0") for o in octets)
        ):
            return True
    return False


def compute_envelope_sha256(env: dict) -> str:
    clone = json.loads(json.dumps(env))
    clone["envelope_sha256"] = ""
    # Sorted-key compact JSON matches Rust serde_json::to_value (BTreeMap) across languages.
    raw = json.dumps(
        clone, separators=(",", ":"), sort_keys=True, ensure_ascii=False
    ).encode("utf-8")
    return f"sha256:{sha256_hex(raw)}"


def evaluate(payload: dict) -> dict:
    env = payload["envelope"]
    policy = payload["trust_policy"]
    now_epoch = int(payload.get("now_epoch", 0))
    retired_runs = set(
        tuple(pair) for pair in payload.get("retired_runs", [])
    )
    prior_ops = {
        tuple(item["key"]): item["digest"] for item in payload.get("prior_ops", [])
    }

    opt_exts = env.get("authority", {}).get("optional_extensions", [])
    omitted_optional = sorted(set(e for e in opt_exts if e not in SUPPORTED_EXTENSIONS))

    def result(verdict: str, reason: str) -> dict:
        return {
            "verdict": verdict,
            "reason_class": reason,
            "imported_authority_granted": False,
            "omitted_optional_extensions": omitted_optional,
        }

    # 1. Profile & plane versions
    if env.get("profile_id") != PROFILE_ID or env.get("profile_version") != PROFILE_VERSION:
        return result("rejected", "incompatible_plane_version")
    pv = env.get("plane_versions", {})
    for plane in ("intent", "authority", "evidence", "outcome"):
        if parse_major(str(pv.get(plane, ""))) != SUPPORTED_MAJOR:
            return result("rejected", "incompatible_plane_version")

    # 2. Mandatory extensions
    for ext in env.get("authority", {}).get("mandatory_extensions", []):
        if ext not in SUPPORTED_EXTENSIONS:
            return result("rejected", "unsupported_mandatory_extension")

    # 3. Envelope tamper check
    if env.get("envelope_sha256") != compute_envelope_sha256(env):
        return result("rejected", "envelope_tampered")

    # 4. Input ambiguity check
    outcome = env["outcome"]
    target = outcome.get("target_uri_or_path")
    if target is not None and has_ambiguous_target(target):
        return result("rejected", "input_ambiguity_rejected")

    # 5. Cross-repo / tenant / recipient
    intent = env["intent"]
    if (
        intent["repository_id"] != policy["repository_id"]
        or intent["tenant_id"] != policy["tenant_id"]
        or intent["recipient_id"] != policy["expected_recipient_id"]
    ):
        return result("rejected", "cross_tenant_or_repo_denied")

    # 6. Replay check
    if (intent["replay_domain"], intent["run_id"]) in retired_runs:
        return result("rejected", "replay_detected")
    op_key = (
        intent["replay_domain"],
        intent["repository_id"],
        intent["tenant_id"],
        intent["run_id"],
        intent["op_id"],
    )
    if op_key in prior_ops:
        return result("rejected", "replay_detected")

    # 7. Summary-fiction & raw receipt byte check
    ev = env["evidence"]
    if ev.get("self_generated_checksum_only", False):
        return result("not_proven", "summary_fiction_missing_raw_receipts")
    claimed = ev.get("claimed_receipt_digests", [])
    raws = ev.get("raw_receipts", [])
    if not claimed and not raws:
        return result("not_proven", "summary_fiction_missing_raw_receipts")
    verified_digests = set()
    for raw in raws:
        actual = sha256_hex(raw["raw_bytes_utf8"].encode("utf-8"))
        if raw["claimed_sha256"] != actual:
            return result("rejected", "raw_receipt_digest_mismatch")
        verified_digests.add(actual)
    for digest in claimed:
        if digest not in verified_digests:
            return result("not_proven", "summary_fiction_missing_raw_receipts")

    # 8. Producer trust
    accepted_producers = set(policy.get("accepted_producers", []))
    if (
        ev.get("producer_trust_class") not in ("independent_executor", "trusted_ci")
        or ev.get("producer_id") not in accepted_producers
    ):
        return result("rejected", "untrusted_producer")

    # 9. Freshness
    auth = env["authority"]
    if (
        auth["expires_at_epoch"] <= auth["issued_at_epoch"]
        or now_epoch > auth["expires_at_epoch"]
        or intent["lease_generation"] < policy["min_lease_generation"]
        or ev["head_commit_sha"] != policy["expected_head_commit_sha"]
        or ev["base_commit_sha"] != policy["expected_base_commit_sha"]
        or ev["source_tree_sha"] != policy["expected_source_tree_sha"]
        or ev["proof_plan_epoch"] != policy["current_proof_plan_epoch"]
        or ev.get("newer_suite_failed", False)
    ):
        return result("rejected", "stale_evidence")

    # 10. Delegation narrowing
    deleg = auth.get("delegation")
    if deleg is not None:
        action_ok = deleg["child_action"] in deleg["parent_allowed_actions"]
        recipient_ok = deleg["child_recipient_id"] == deleg["parent_recipient_id"]
        expiry_ok = deleg["child_expires_at_epoch"] <= deleg["parent_expires_at_epoch"]
        budget_ok = deleg["child_budget"] <= deleg["parent_max_budget"]
        paths_ok = all(
            any(cp == pp or cp.startswith(pp + "/") for pp in deleg["parent_allowed_paths"])
            for cp in deleg["child_paths"]
        )
        if not (action_ok and recipient_ok and expiry_ok and budget_ok and paths_ok):
            return result("rejected", "authority_widening_denied")

    # 11. Exact human approval binding
    req_human = set(policy.get("require_human_approval_for_ops", []))
    if (
        auth["requested_operation"] in req_human
        and not auth.get("authenticated_human_approval", False)
    ):
        return result("rejected", "missing_human_approval")
    approved_op = auth.get("claimed_approved_operation")
    if approved_op is not None:
        if (
            approved_op != auth["requested_operation"]
            or auth.get("claimed_approved_parameters_canonical")
            != auth["actual_parameters_canonical"]
        ):
            return result("rejected", "approval_parameter_mismatch")

    # 12. Tampered execution environment & history completeness
    if ev.get("environment_tamper_signals"):
        return result("rejected", "tampered_execution_environment")
    history = ev.get("history_events", [])
    if history:
        for idx, item in enumerate(history):
            if item["seq"] != idx + 1:
                return result("rejected", "history_incomplete_or_reordered")
        term = ev.get("expected_terminal_seq")
        if term is not None and history[-1]["seq"] != term:
            return result("rejected", "history_incomplete_or_reordered")

    # 13. Process vs outcome integrity & semantic mismatch
    if not outcome.get("process_path_authorized", False):
        return result("rejected", "prohibited_execution_path")
    if (
        outcome.get("claim_kind") == "precommit_containment"
        and outcome.get("declared_status", "").lower() == "goal_completed"
    ):
        return result("not_proven", "precommit_containment_not_goal_completion")

    declared_ok = outcome.get("declared_status", "").lower() in (
        "passed",
        "verified",
        "completed",
        "success",
    )
    observed_ok = outcome.get("observed_status", "").lower() in (
        "passed",
        "verified",
        "completed",
        "success",
    )
    zero_assertions = outcome.get("observed_assertions") == 0
    http_err = (
        outcome.get("observed_http_status") is not None
        and outcome["observed_http_status"] >= 400
    )
    if declared_ok and (
        not observed_ok
        or zero_assertions
        or http_err
        or not outcome.get("behavioral_passed", False)
    ):
        return result("rejected", "semantic_outcome_mismatch")
    if not ev.get("independent_effect_observed", False):
        return result("not_proven", "semantic_outcome_mismatch")

    return result("accepted_external_evidence", "valid_external_evidence")


def main() -> int:
    raw = sys.stdin.read()
    # Reject duplicate keys at every nesting level.
    def reject_duplicates(pairs):
        out = {}
        for k, v in pairs:
            if k in out:
                raise ValueError(f"duplicate JSON key: {k}")
            out[k] = v
        return out

    try:
        payload = json.loads(raw, object_pairs_hook=reject_duplicates)
    except Exception as exc:
        print(json.dumps({"verdict": "rejected", "reason_class": "input_ambiguity_rejected", "error": str(exc)}))
        return 0

    if isinstance(payload, dict) and "vectors" in payload:
        results = []
        for vec in payload["vectors"]:
            res = evaluate(vec)
            res["id"] = vec["id"]
            results.append(res)
        print(json.dumps({"results": results}, sort_keys=True))
    else:
        print(json.dumps(evaluate(payload), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
