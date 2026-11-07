#!/usr/bin/env python3
"""Gate the pre-build validate report (#1047).

The `pre-build-validate` job in `.github/workflows/decapod-validate.yml` runs
`govern project-check --format json` with the latest RELEASED Decapod binary
against the PR checkout, before any branch build starts. This script turns that
JSON report into a pass/fail gate:

- `report.status == "pass"` with no failures passes.
- Until a release ships the manifest relocation (#1079), the released binary's
  generated-artifact whitelist still only knows the legacy
  `.decapod/managed/specs/.manifest.json` path, so a branch that carries the
  new `.decapod/managed/.manifest.json` produces exactly one known
  forward-compatibility failure. Failures of that exact shape, with every
  offending path under `.decapod/managed/`, are allowed and reported.
- Any other failure fails the job, kicking structural/completeness problems
  (missing claims, stale projections, ...) back to the agent before the heavy
  build pipeline runs.
"""

import json
import re
import sys

WHITELIST_FAILURE = re.compile(
    r"^Tracked non-whitelisted generated artifacts found: \[(?P<offenders>.*)\]\."
)
OFFENDER = re.compile(r'"([^"]+)"')
MANAGED_PREFIX = ".decapod/managed/"


def is_allowlisted_forward_compat(failure: str) -> bool:
    match = WHITELIST_FAILURE.match(failure)
    if not match:
        return False
    offenders = OFFENDER.findall(match.group("offenders"))
    return bool(offenders) and all(o.startswith(MANAGED_PREFIX) for o in offenders)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: pre_build_validate_gate.py <project-check.json>", file=sys.stderr)
        return 2
    raw = open(sys.argv[1], encoding="utf-8").read()
    start = raw.find("{")
    if start < 0:
        print("FAIL: pre-build validate report contains no JSON object")
        return 1
    try:
        doc = json.loads(raw[start:])
    except json.JSONDecodeError as error:
        print(f"FAIL: pre-build validate report is not valid JSON: {error}")
        return 1

    report = doc.get("report", doc)
    failures = report.get("failures", []) or []
    status = report.get("status", doc.get("status", ""))

    if status == "pass" and not failures:
        print("OK: pre-build project-check passed with the latest released Decapod")
        return 0

    allowed = [f for f in failures if is_allowlisted_forward_compat(f)]
    rejected = [f for f in failures if not is_allowlisted_forward_compat(f)]

    if not rejected and (allowed or status == "pass"):
        for failure in allowed:
            print(f"allowed forward-compat finding (resolved once a release ships #1079): {failure}")
        print("OK: pre-build project-check has no unexpected failures")
        return 0

    print("FAIL: pre-build validate (latest released Decapod) found problems:")
    for failure in rejected or failures:
        print(f"  - {failure}")
    print("Fix the structural/completeness issues above and push again (#1047).")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
