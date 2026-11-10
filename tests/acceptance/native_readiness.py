#!/usr/bin/env python3
"""Opt-in acceptance of a released CLI against real, already-governed branches.

This is not a provider, prerequisite stub, or publication authority. See
 docs/native-readiness-acceptance.md. All subprocesses use literal arguments.
"""
import argparse
import hashlib
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import tomllib


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def digest(value, *, sorted_keys=False):
    return sha(json.dumps(value, sort_keys=sorted_keys, ensure_ascii=False,
                          separators=(",", ":"), allow_nan=False).encode())


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], timeout=30)


def identity(root):
    require(not git(root, "status", "--porcelain", "--untracked-files=all"),
            f"Acceptance needs a clean committed candidate: {root}")
    branch = git(root, "branch", "--show-current").decode().strip()
    require(branch and branch not in {"main", "master"}, "Use a governed work branch")
    config = tomllib.loads((root / ".decapod/config.toml").read_text())
    require(config.get("proof", {}).get("validation_buildkite_preflights") is False,
            "Acceptance requires explicit native execution, not a provider override")
    governance = (root / ".decapod/governance.json").read_bytes()
    document = json.loads(governance)
    require("validation" not in document.get("sections", {}),
            "Fresh acceptance must not contain a legacy validation receipt")
    return {"head": git(root, "rev-parse", "HEAD").decode().strip(),
            "tree": git(root, "rev-parse", "HEAD^{tree}").decode().strip(),
            "branch": branch, "governance": sha(governance),
            "configuration": sha((root / ".decapod/config.toml").read_bytes()),
            "remote": git(root, "remote", "get-url", "origin").decode().strip(),
            "trajectory": digest(document["sections"]["trajectory"], sorted_keys=True)}


def invoke(binary, root, output, label, timeout):
    # Preserve the real host's supported routing. Do not forge CI flags or inject
    # replacement tools/providers. A timeout is a failed acceptance, never a skip.
    process = subprocess.Popen([str(binary), "validate", "--format", "json"],
                               cwd=root, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except BaseException:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        (output / f"{label}.stdout.json").write_bytes(stdout)
        (output / f"{label}.stderr.txt").write_bytes(stderr)
        raise
    (output / f"{label}.stdout.json").write_bytes(stdout)
    (output / f"{label}.stderr.txt").write_bytes(stderr)
    report = json.loads(stdout)
    require(report.get("scope") != "ci_local", "CI-local proof cannot qualify")
    return process.returncode, report


def evidence(report, root):
    readiness = report.get("readiness") or {}
    path = readiness.get("evidence_path")
    if path is None:
        return None
    directory = Path(path).resolve(strict=True)
    require(not directory.is_relative_to(root), "Evidence must be outside the candidate")
    return directory


def passed(report, returncode, root, before):
    require(returncode == 0 and report.get("publication_ready") is True,
            "Released CLI did not establish full publication readiness")
    require(report.get("report", {}).get("fail_count") == 0,
            "Actual project prerequisite gates did not all pass")
    directory = evidence(report, root)
    require(directory is not None, "Successful run has no external evidence")
    receipt = json.loads((directory / "completion/native-readiness-receipt.json").read_bytes())
    verdict = json.loads((directory / "completion/verdict.json").read_bytes())
    observations = json.loads((directory / "observations.json").read_bytes())
    require(verdict == report["readiness"] and verdict.get("ready") is True,
            "Paired verdict differs from the actual CLI result")
    require(receipt.get("kind") == "native_readiness_receipt" and receipt.get("status") == "ok",
            "Missing successful native receipt")
    require(receipt["scope"] == verdict["scope"], "Receipt/verdict scope mismatch")
    material = receipt["material"]
    require(material["head_revision"] == before["head"] and
            material["work_branch"] == before["branch"], "Receipt names a different candidate")
    require(receipt["governance_digest"] == before["governance"] and
            receipt["trajectory_digest"] == before["trajectory"], "Governance binding mismatch")
    require(receipt["evidence_digest"] == digest(observations), "Evidence digest mismatch")
    require(receipt["laya_readiness_digest"] == digest(verdict["laya_readiness"]),
            "Decision digest mismatch")
    receipt_hash = receipt.pop("receipt_hash")
    require(receipt_hash == digest(receipt, sorted_keys=True), "Receipt integrity mismatch")
    require(observations["before"] == observations["after"] == material,
            "Execution did not bind unchanged material and accepted target")
    require(receipt["proof_plan_digest"] == observations["expected_plan_digest"],
            "Receipt and executed proof plan differ")
    require(receipt["invocation_id"] == directory.name, "Receipt belongs to another invocation")
    results = observations.get("results", [])
    require(results and all(row["executor"] == "native" for row in results),
            "Acceptance requires nonempty real native observations")
    require(all(row["identity"] == material for row in results), "Per-check candidate mismatch")
    require(all(row["observation"]["kind"] in {"executed", "not_applicable"}
                for row in results), "Native execution must not reuse or import provider evidence")
    require(identity(root) == before, "Validation changed the accepted candidate")
    return str(directory)


def rejected(report, returncode, root):
    require(returncode != 0 and report.get("publication_ready") is False,
            "Failed/stale candidate published success")
    directory = evidence(report, root)
    if directory:
        require(not (directory / "completion").exists(),
                "Failed/stale invocation published a completion bundle")
        verdict = directory / "verdict.json"
        if verdict.exists():
            require(json.loads(verdict.read_bytes()).get("ready") is False,
                    "Failed/stale invocation persisted a successful verdict")
    return directory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--released-binary", type=Path, required=True)
    parser.add_argument("--passing-project", type=Path, required=True)
    parser.add_argument("--failing-project", type=Path, required=True)
    parser.add_argument("--stale-file", required=True,
                        help="Tracked ordinary source file in passing project; restored after probe")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=7200)
    args = parser.parse_args()
    require(not any(os.environ.get(key, "").lower() == "true"
                    for key in ("CI", "GITHUB_ACTIONS", "BUILDKITE")),
            "Run acceptance outside CI; do not unset flags to impersonate a host")
    binary = args.released_binary.resolve(strict=True)
    passing = args.passing_project.resolve(strict=True)
    failing = args.failing_project.resolve(strict=True)
    require(passing != failing, "Failure control must be a separate governed candidate")
    output = args.output.resolve()
    require(not any(output.is_relative_to(root) or binary.is_relative_to(root)
                    for root in (passing, failing)), "Binary/output must be external")
    before, negative = identity(passing), identity(failing)
    require(before["configuration"] == negative["configuration"] and
            before["remote"] == negative["remote"],
            "Controls must retain the same repository and required proof configuration")
    stale = passing / args.stale_file
    require(not Path(args.stale_file).is_absolute() and not stale.is_symlink() and
            stale.resolve(strict=True).is_relative_to(passing) and
            stale.resolve().relative_to(passing).parts[0] not in {".git", ".decapod"} and
            ".." not in Path(args.stale_file).parts and stale.stat().st_nlink == 1,
            "Stale probe must name ordinary source, never control-plane state")
    git(passing, "ls-files", "--error-unmatch", "--", args.stale_file)
    original = stale.read_bytes()
    probe = original + b"\nNative acceptance stale candidate probe\n"
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    (output / "controller.json").write_text(json.dumps({"path": str(binary),
                                                        "sha256": sha(binary.read_bytes())}))
    code, report = invoke(binary, passing, output, "passing", args.timeout)
    positive_evidence = passed(report, code, passing, before)
    code, report = invoke(binary, failing, output, "failing", args.timeout)
    failed_evidence = rejected(report, code, failing)
    require(identity(failing) == negative, "Failure run changed its committed candidate")
    # A governance/precondition failure alone does not prove failed native work.
    require(failed_evidence is not None, "Failure control never reached native proof")
    observations = json.loads((failed_evidence / "observations.json").read_bytes())
    require(any(row.get("executor") == "native" and
                row.get("observation", {}).get("conclusion") in
                {"failure", "timed_out", "cancelled"}
                for row in observations.get("results", [])),
            "Failure control needs actual failed native execution, not a missing prerequisite")
    try:
        stale.write_bytes(probe)
        code, report = invoke(binary, passing, output, "stale", args.timeout)
        rejected(report, code, passing)
    finally:
        require(stale.read_bytes() == probe,
                "Source changed concurrently; retained bytes for agent recovery instead of overwriting")
        stale.write_bytes(original)
    require(identity(passing) == before, "Stale probe did not restore the candidate")
    (output / "acceptance.json").write_text(json.dumps({"status": "passed",
        "candidate": before, "passing_evidence": positive_evidence,
        "failing_evidence": str(failed_evidence), "controls": ["failed_native", "dirty_candidate"]}, indent=2))
    print("Released CLI acceptance passed; no PR publication was performed.")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Native acceptance FAILED: {error}", file=sys.stderr)
        sys.exit(1)
