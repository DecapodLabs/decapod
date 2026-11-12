#!/usr/bin/env python3
"""Restore the original billing task's exact clean native checkpoint, then publish natively."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
from container_billing import verify_runtime, verify_github_cli, OCI_CONTROLLER_IDENTITY, HOST_CONTROLLER_IDENTITY, GITHUB_CLI_IDENTITY

REPO = "DecapodLabs/decapod"
BASE = "c5de35a89b27e788169612ae71681e45c13bf71a"
SOURCE = "26a499d9f0789a5ed00605d23cfc57f5392ae8fc"
OPS = "ops/stale-billing-publication-20261111"
ACTOR = "chatgpt-stale-billing-next"
RECEIVER = "chatgpt-stale-billing"
CONTROLLER = "dd30b7516c331ac0a6c3128f7a9c433833935bc9f380d311b188502037f857e0"
MATERIAL_BASE = "c7609e42bbbcff9c72af6be7a86333d4b0408922"
REMOTE_HEAD = "b984a386fec57ff302f09013a143e63740639ced"
CURRENT = "b984a386fec57ff302f09013a143e63740639ced"
CURRENT_TREE = "cbb32bf666025718e35ba733ab9dbc02272b55d6"
EXPECTED_PR = {
    "number": 1631, "url": "https://github.com/DecapodLabs/decapod/pull/1631",
    "state": "open", "draft": True, "repository": REPO, "repository_id": 1155300084,
    "head_ref": "agent/chatgpt-stale-billing/code_01m76aq15fea61e8-cloud-approval",
    "head_sha": CURRENT, "base_ref": "master", "base_sha": BASE,
}
PREPARATION_HEAD = "04ddbedf90cd107ab9e7e3a46b4cb00e9c69bf6b"
PREPARATION_MANIFEST = "sha256:9f5ed85eb003abb6eae5b271b2b0105fc3de9308c8c8f33c7439cdcd7f8c836b"
PREPARATION_CANONICAL = "sha256:642a6ff37e04a841bbe86f512262b37884313c48daabcd734fd5f430707f42f4"
OLD_BRANCH = "agent/codex-init-billing/code_01m7513af6jtzhp4-admin-checkout"
ARCHIVE_TAG = "archive/stale-20261111-billing-26a499d9"
RELEASE_GENERATED = {"AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md", ".decapod/managed/Dockerfile.decapod", ".decapod/managed/specs/.manifest.json"}
REPAIR_SCOPE = {"src/decapod/lib.rs", "docs/agent/command-contracts.md", "docs/agent/config-schema.md", "docs/cloud-repository-billing.md"}
NATIVE_DOCS = {"docs/agent/command-contracts.md", "docs/agent/config-schema.md"}
SPEC_ORDER = ["README", "INTENT", "ARCHITECTURE", "INTERFACES", "VALIDATION", "SEMANTICS", "OPERATIONS", "SECURITY"]
ATTESTATION_SPECS = {".decapod/managed/specs/" + name + ".md" for name in SPEC_ORDER}
CERT = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
SPEC_PREFIX = ".decapod/managed/specs/"
GENERATED = {"AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md",
             ".decapod/governance.json", ".decapod/managed/Dockerfile.decapod"}
BLOCKS = re.compile(r"<!-- decapod:([A-Za-z0-9:_-]+):start -->.*?<!-- decapod:\1:end -->", re.S)
CODES = r"\b(?:AUTHORING_|SESSION_|WORKSPACE_|SCOPE_|GOVERNANCE_|HISTORY_|TASK_TRANSFER_|READINESS_|PUBLICATION_|CONTROLLER_|PR_|DECISION_GATE|POLICY_DENIED)[A-Z0-9_]*\b"
REQUEST = "we still seem to have 9 stale branches, and 2 of which have open PRs. Please address this by closing the stale unneeded branches, and making PRs for the remaining ones."


class Blocked(RuntimeError):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--repair-patch", type=Path, required=True)
    parser.add_argument("--release-drift", type=Path, required=True)
    parser.add_argument("--custody-recipient", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--publish-timeout", type=int, default=3600)
    parser.add_argument("--resume-private-dir", type=Path, required=True)
    parser.add_argument("--container-context", type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    resume, repair = manifest["resume"], manifest["repair"]
    prior = resume["prior_result"]
    prepared = resume["preparation_result"]
    task_id = "code_01m76aq15fea61e8"
    preserved_branch = "agent/chatgpt-stale-billing/" + task_id + "-cloud-approval"
    native_digest = "sha256:c849917c6bb779362d3682af11a3c82de5752479b5bf6b607c0f79f3b0fc5b94"
    approved_inventory = "sha256:83c629aa22fba433bbd69e480aa0ecbd0abfdda74dc7dfec2a88758932920577"
    if (manifest.get("repository") != REPO or manifest.get("base_sha") != BASE
            or manifest.get("source_sha") != SOURCE or manifest.get("slug") != "billing"
            or resume.get("task_id") != task_id or resume.get("branch") != preserved_branch
            or resume.get("receiver") != ACTOR or resume.get("prior_run_id") != "43130813275"
            or resume.get("return_receiver") != RECEIVER or resume.get("remote_head_sha") != REMOTE_HEAD
            or resume.get("prior_operations_sha") != "fa1d2fd0ad0e6aee7596d1eb30d95f6a5e942447"
            or resume.get("prior_report_sha256") != "sha256:87f47dbfe67b94368ffed6c8d1737cd7a6fa56079ab7407a911933ea3caa2cba"
            or digest((json.dumps(prior, indent=2) + "\n").encode()) != resume.get("prior_report_sha256")
            or resume.get("preparation_run_id") != "43123122460"
            or resume.get("preparation_report_sha256") != "sha256:412a64caf4324bca7fa03c812f7a58c565748dfd8c7829fb0c588becdc720873"
            or digest((json.dumps(prepared, indent=2) + "\n").encode()) != resume.get("preparation_report_sha256")
            or resume.get("candidate_sha") != CURRENT or resume.get("candidate_tree") != CURRENT_TREE
            or resume.get("expected_pr") != EXPECTED_PR
            or resume.get("native_transfer", {}).get("plaintext_sha256") != native_digest
            or resume.get("native_transfer", {}).get("transfer_id") != "01M76M6DHXHYPTYFZ3X1JJ1Y3H"
            or prior.get("task") != task_id or prior.get("branch") != preserved_branch
            or prior.get("status") != "blocked" or prior.get("head_sha") != CURRENT
            or prior.get("pushed") is not True or prior.get("checkpoints") is not True
            or prior.get("observed_remote_head_sha") != REMOTE_HEAD
            or prior.get("checkpoint_verification", {}).get("history_proven") is not True
            or prior.get("checkpoint_verification", {}).get("head_commit") != CURRENT
            or prior.get("custody_exported") is not True or prior.get("full_boundary_passed") is not False
            or prior.get("final_head_sha") != CURRENT or prior.get("source_dirty_after_attempt") is not False
            or set(resume.get("private_files", {})) != {"native-transfer.json", "candidate.bundle"}
            or resume.get("pending_paths") != []
            or resume.get("approved_baseline_inventory_digest") != approved_inventory
            or manifest.get("allowed_source_changes") != []
            or set(manifest.get("release_generated_paths", [])) != RELEASE_GENERATED
            or set(repair.get("reviewed_paths", [])) != REPAIR_SCOPE
            or not set(repair.get("changed_paths", [])) <= REPAIR_SCOPE
            or set(repair.get("changed_paths", [])) - NATIVE_DOCS != {"src/decapod/lib.rs", "docs/cloud-repository-billing.md"}
            or repair.get("base_sha") != MATERIAL_BASE
            or not re.fullmatch(r"[0-9a-f]{40}", repair.get("expected_material_tree", ""))
            or not re.fullmatch(r"[0-9a-f]{40}", manifest.get("projection_reference_tree", ""))
            or not 60 <= args.publish_timeout <= 5400):
        parser.error("only the exact original task, preserved checkpoint and authentic preparation receipt may continue")
    incoming = args.resume_private_dir.resolve()
    if incoming.is_symlink():
        parser.error("private continuation directory cannot be a symlink")
    paths = set(manifest["original_authored_paths"]) | REPAIR_SCOPE
    expected_tree = manifest["projection_reference_tree"]
    reviewed_files = {entry["path"]: entry for entry in repair["files"]}
    if set(reviewed_files) != REPAIR_SCOPE:
        parser.error("all reviewed authored/generated documentation paths require exact blob identities")
    for key in ("title", "intent", "commit_message", "body"):
        if not isinstance(manifest.get(key), str) or not manifest[key].strip():
            parser.error("manifest requires nonempty " + key)
    root, out = Path.cwd().resolve(), args.output.resolve()
    if out.is_relative_to(root) or incoming.is_relative_to(out) or out.is_relative_to(incoming):
        parser.error("uploadable result directory must be outside the source checkout")
    os.umask(0o077)
    out.mkdir(parents=True, exist_ok=False, mode=0o700)
    private = Path(tempfile.mkdtemp(prefix="decapod-billing-private-", dir=out.parent))
    diagnostics = private / "diagnostics"
    diagnostics.mkdir(mode=0o700)
    env = os.environ.copy()
    env["DECAPOD_AGENT_ID"] = ACTOR
    env["DECAPOD_CLAIM_AUTORUN"] = "false"
    env["DECAPOD_SESSION_PASSWORD"] = secrets.token_hex(32)
    sensitive = [v for k, v in env.items() if re.search("token|password|secret|credential|authorization", k, re.I) and len(v) >= 8]
    result = dict(schema="decapod.branch-publication.v1", slug="billing", repository=REPO,
                  base_sha=BASE, source_sha=SOURCE, operations_sha=env.get("GITHUB_SHA"),
                  status="blocked", branch=None, task=None, head_sha=None, tree=None,
                  checkpoints=False, pushed=False, publication_ready=False,
                  full_boundary_passed=False, custody_exported=False, pr_url=None)
    result.update(resumed=True, continuation_from=CURRENT, prior_run_id=resume["prior_run_id"], imported=False,
                  incoming_transfer_id=resume["native_transfer"]["transfer_id"])
    stage, work, task, branch, control = "preconditions", None, None, None, None
    claimed = cert_verified = False
    controller = None
    spec_files = set()
    reviewed_inventory_digest = None
    preparation = prepared["project_prerequisites"]
    project_report = preparation.get("native_json", {})
    validation_epoch = project_report.get("report", {}).get("validation_epoch", {})
    if (preparation.get("head_sha") != PREPARATION_HEAD or preparation.get("exit_code") != 0 or preparation.get("timeout") is not False
            or project_report.get("status") != "ok" or project_report.get("scope") != "project_prerequisites"
            or project_report.get("publication_ready") is not False or project_report.get("readiness") is not None
            or project_report.get("report", {}).get("status") != "ok" or project_report.get("report", {}).get("fail_count") != 0
            or project_report.get("report", {}).get("failures") != [] or project_report.get("report", {}).get("pass_count") != 206
            or validation_epoch.get("generated_specs_manifest_hash") != PREPARATION_CANONICAL):
        parser.error("continuation requires the authentic successful preparation at the exact preserved checkpoint")

    def scrub(value):
        if isinstance(value, dict):
            return {k: scrub(v) for k, v in value.items() if not re.search("password|secret|credential|authorization|session_token|access_token", k, re.I)}
        if isinstance(value, list):
            return [scrub(v) for v in value]
        if isinstance(value, str):
            for secret in sensitive:
                value = value.replace(secret, "[REDACTED]")
            return re.sub(r"(?:github_pat_|gh[pousr]_)[A-Za-z0-9_]{20,}|(?i:bearer)\s+\S+|https?://[^/\s:@]+:[^/@\s]+@", "[REDACTED]", value)
        return value

    def execute(label, argv, cwd=None, data=None, timeout=240):
        nonlocal stage
        stage = label
        if argv[0] == "decapod":
            if controller is None or hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
                raise Blocked("official controller identity unavailable or changed")
            argv = [str(controller), *argv[1:]]
        process = subprocess.Popen(argv, cwd=cwd or root, env=env,
                                   stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   text=True, start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(data, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                stdout, stderr = process.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate()
        stem = re.sub(r"[^a-z0-9-]", "-", label.lower())
        (diagnostics / (stem + ".stdout.txt")).write_text(stdout)
        (diagnostics / (stem + ".stderr.txt")).write_text(stderr)
        return process.returncode, stdout, stderr, timed_out

    def run(label, *argv, cwd=None, data=None, timeout=240):
        code, stdout, stderr, timed_out = execute(label, list(argv), cwd, data, timeout)
        if code or timed_out:
            result["blocker"] = dict(stage=label, exit_code=code, timeout=timed_out,
                                     codes=sorted(set(re.findall(CODES, stdout + stderr))),
                                     detail=scrub(stderr[-2000:]), full_output="encrypted diagnostics")
            raise Blocked("native or Git command failed; preserve its exact diagnosis")
        return stdout

    def doc(label, *argv, cwd=None, data=None):
        value = json.loads(run(label, "decapod", *argv, cwd=cwd, data=data))
        if isinstance(value, dict) and value.get("error"):
            result["blocker"] = dict(stage=label, detail=scrub(value["error"]), codes=sorted(set(re.findall(CODES, json.dumps(value)))))
            raise Blocked("native operation returned an error")
        return value

    def encrypt(source, name):
        target = out / name
        if not cert_verified or target.exists():
            raise Blocked("encryption requires verified recipient and unused output")
        run("encrypt " + name, "openssl", "cms", "-encrypt", "-binary", "-aes-256-cbc",
            "-in", str(source), "-out", str(target), "-outform", "DER", str(args.custody_recipient.resolve()))
        if not target.is_file() or target.stat().st_size == 0:
            raise Blocked("encrypted artifact missing")
        return dict(artifact=target.name, plaintext_sha256=digest(source.read_bytes()), ciphertext_sha256=digest(target.read_bytes()))

    def fresh_target():
        if run("verify current accepted target", "git", "ls-remote", "origin", "refs/heads/master").split() != [BASE, "refs/heads/master"]:
            raise Blocked("accepted target changed; new source review and native validation required")

    def reconciliation(value, label, permit_stale=False):
        report = value.get("result", {}).get("reconciliation")
        if not isinstance(report, dict):
            raise Blocked("native initialization omitted reconciliation")
        repairs = [r for r in report.get("repair_tasks", []) if r.get("code") != "REQUIRED_PROOF_NOT_RUN"]
        result[label] = dict(status=report.get("status"), inventory_digest=report.get("inventory_digest"), repair_codes=[r.get("code") for r in repairs])
        if any(not (permit_stale and r.get("code") == "STALE_SOURCE_INVENTORY") for r in repairs) or (not permit_stale and report.get("status") != "proof_pending"):
            raise Blocked("source reconciliation contains an unsatisfied obligation")
        return report

    def clean(label):
        if run(label, "git", "status", "--porcelain", cwd=work).strip():
            raise Blocked("candidate contains uncommitted or untracked material")

    def verify_material(label):
        # This reference tree fixes all authored changes and release projections.
        # The successful prior native preparation owns the exact manifest time,
        # attestation and hashes. Compare them without refreshing or normalizing.
        manifest_path = SPEC_PREFIX + ".manifest.json"
        native_fields = {".decapod/governance.json", manifest_path} | ATTESTATION_SPECS
        delta = set(run(label + " complete tracked tree", "git", "diff", "--no-renames", "--name-only", "-z", expected_tree, cwd=work).split("\0")) - {""}
        if not delta <= native_fields:
            raise Blocked("candidate differs from independently reviewed authored/release material: " + ", ".join(sorted(delta - native_fields)))
        if run(label + " untracked material", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work):
            raise Blocked("untracked material remains outside complete reviewed tree")
        manifest_bytes = (work / manifest_path).read_bytes()
        if digest(manifest_bytes) != PREPARATION_MANIFEST:
            raise Blocked("native preparation manifest bytes differ from the authenticated committed snapshot")
        actual_manifest = json.loads(manifest_bytes)
        canonical_manifest = dict(actual_manifest)
        canonical_manifest.pop("generated_at")
        if digest(json.dumps(canonical_manifest, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()) != PREPARATION_CANONICAL:
            raise Blocked("native manifest canonical identity differs from the authentic preparation epoch")
        signal = validation_epoch.get("generated_specs_fingerprint", "")
        generated_at = actual_manifest.get("generated_at")
        if (signal != "f52d1dd54bc5da5637472029b9070177ea359485bd2f706b57e34686ceb10d7f"
                or generated_at != "1794441332Z"):
            raise Blocked("native preparation fingerprint or observed timestamp differs")
        expected_manifest = json.loads(run(label + " observed release manifest reference", "git", "show", expected_tree + ":" + manifest_path, cwd=work))
        original_signal = expected_manifest["repo_signal_fingerprint"]
        if [entry["path"] for entry in expected_manifest["files"]] != [SPEC_PREFIX + name + ".md" for name in SPEC_ORDER]:
            raise Blocked("canonical spec manifest order differs from reviewed native contract")
        input_hash = hashlib.sha256()
        spec_hashes = {}
        for name in SPEC_ORDER:
            path = SPEC_PREFIX + name + ".md"
            original = run(label + " original authored spec " + name, "git", "show", MATERIAL_BASE + ":" + path, cwd=work)
            start, end = "<!-- decapod:codebase-attestation:start -->", "<!-- decapod:codebase-attestation:end -->"
            if original.count(start) != 1 or original.count(end) != 1:
                raise Blocked("original spec does not have one canonical native attestation block")
            before, rest = original.split(start, 1)
            block, after = rest.split(end, 1)
            old_line = "- Repository signal fingerprint: `" + original_signal + "`"
            if block.count(old_line) != 1:
                raise Blocked("original spec fingerprint differs from its authentic manifest")
            expected = before + start + block.replace(old_line, "- Repository signal fingerprint: `" + signal + "`", 1) + end + after
            actual = (work / path).read_bytes()
            if actual != expected.encode():
                raise Blocked("spec changed beyond exact native fingerprint substitution: " + path)
            content_hash = hashlib.sha256(actual).hexdigest()
            if validation_epoch.get("material_hashes", {}).get("generated_spec:" + path) != "sha256:" + content_hash:
                raise Blocked("native preparation epoch does not bind the exact preserved spec: " + path)
            spec_hashes[path] = content_hash
            input_hash.update(path.encode() + b"\0" + content_hash.encode() + b"\n")
        expected_manifest["generated_at"] = generated_at
        expected_manifest["repo_signal_fingerprint"] = signal
        expected_manifest["spec_input_hash"] = input_hash.hexdigest()
        for entry in expected_manifest["files"]:
            entry["content_hash"] = spec_hashes[entry["path"]]
        if actual_manifest != expected_manifest:
            raise Blocked("native manifest differs beyond current attestation/time and independently rehashed spec content")
        for path in RELEASE_GENERATED | ATTESTATION_SPECS | {".decapod/governance.json"}:
            if (work / path).is_symlink() or not (work / path).is_file():
                raise Blocked("native generated material is not a regular file")
            if run(label + " native file mode " + Path(path).name, "git", "ls-files", "--stage", "--", path, cwd=work).split()[0] != "100644":
                raise Blocked("native generated material mode differs from review")
        result.setdefault("material_verification", []).append(dict(stage=label, projection_reference_tree=expected_tree,
            complete_authored_and_release_material_equal=True, authored_specs_preserved_byte_exact=True,
            native_attestation_signal=signal, independently_rehashed_spec_input=input_hash.hexdigest(),
            manifest_raw_sha256=PREPARATION_MANIFEST, manifest_canonical_sha256=PREPARATION_CANONICAL,
            authentic_native_generated_at=generated_at, manifest_other_fields_exact=True,
            policy_unchanged_from_published_candidate=True))

    def require_no_open_pr(branch_name, label):
        raw = run(label, "gh", "api", "--method", "GET", "repos/" + REPO + "/pulls",
                  "-f", "state=open", "-f", "head=DecapodLabs:" + branch_name, "-f", "per_page=100")
        if json.loads(raw) != []:
            raise Blocked("a branch requiring no open PR now has one; retain its current state")

    def require_expected_pr(label):
        pr = json.loads(run(label, "gh", "api", "--method", "GET", "repos/" + REPO + "/pulls/1631"))
        head, base = pr.get("head", {}), pr.get("base", {})
        head_repo, base_repo = head.get("repo") or {}, base.get("repo") or {}
        observed = dict(number=pr.get("number"), url=pr.get("html_url"), state=pr.get("state"),
            draft=pr.get("draft"), repository=head_repo.get("full_name"), repository_id=head_repo.get("id"),
            head_ref=head.get("ref"), head_sha=head.get("sha"), base_ref=base.get("ref"), base_sha=base.get("sha"))
        if (observed != EXPECTED_PR or base_repo.get("full_name") != REPO
                or base_repo.get("id") != EXPECTED_PR["repository_id"]):
            raise Blocked("the authorized existing draft PR identity or source/base changed; preserve its current state")
        result.setdefault("expected_pr_observations", []).append(dict(stage=label, **observed))

    def current_remote():
        if run("verify published continuation source lease", "git", "ls-remote", "origin", "refs/heads/" + preserved_branch).split() != [REMOTE_HEAD, "refs/heads/" + preserved_branch]:
            raise Blocked("original published candidate changed; no source overwrite or replacement is permitted")

    def archive_original_before_freeze():
        old_ref, tag_ref = "refs/heads/" + OLD_BRANCH, "refs/tags/" + ARCHIVE_TAG
        if run("verify old billing archive source lease", "git", "ls-remote", "origin", old_ref).split() != [SOURCE, old_ref]:
            raise Blocked("old billing archive source changed; preserve all existing refs")
        require_no_open_pr(OLD_BRANCH, "verify old billing source has no open PR")
        observed = run("inspect exact old billing archive tag", "git", "ls-remote", "origin", tag_ref).split()
        if observed == []:
            # This lease permits only absent -> exact source. It cannot replace
            # an existing tag. Failure is terminal; no API or hook bypass retry.
            run("archive original billing source before readiness freeze", "git", "push",
                "--no-follow-tags", "--force-with-lease=" + tag_ref + ":", "origin", SOURCE + ":" + tag_ref, cwd=work)
            created = True
        elif observed == [SOURCE, tag_ref]:
            created = False
        else:
            raise Blocked("old billing archive tag already names another object")
        if run("verify retained old billing archive", "git", "ls-remote", "origin", tag_ref).split() != [SOURCE, tag_ref]:
            raise Blocked("old billing archive was not confirmed exact")
        if run("verify old source branch preserved", "git", "ls-remote", "origin", old_ref).split() != [SOURCE, old_ref]:
            raise Blocked("old billing source branch changed during archive creation")
        result["old_source_archive"] = dict(tag=ARCHIVE_TAG, source_sha=SOURCE, source_branch=OLD_BRANCH,
            exact_verified=True, created=created, original_branch_deleted=False, before_readiness_freeze=True)

    try:
        if (env.get("GITHUB_ACTIONS") != "true" or env.get("BUILDKITE") == "true"
                or env.get("GITHUB_REPOSITORY") != REPO or env.get("GITHUB_REF") != "refs/heads/" + OPS):
            raise Blocked("requires the exact authorized hosted operations context")
        runtime_state = json.loads(args.container_context.read_text())
        if Path(runtime_state["root"]).resolve() != root:
            raise Blocked("original billing custody requires the exact native container source context")
        try:
            result["container_execution"] = verify_runtime(runtime_state)
            if manifest.get("github_cli") != GITHUB_CLI_IDENTITY:
                raise RuntimeError("GitHub CLI manifest differs from the exact approved official release")
            result["github_cli"] = verify_github_cli(runtime_state, env)
        except RuntimeError as error:
            raise Blocked(str(error)) from None
        if runtime_state["engine"] == "podman" and env.get("CONTAINER_ID") != result["container_execution"]["container_id"]:
            raise Blocked("native runtime identity is not the exact observed rootless container ID")
        if (runtime_state.get("child_controller") != OCI_CONTROLLER_IDENTITY or runtime_state.get("host_controller") != HOST_CONTROLLER_IDENTITY
                or manifest.get("controller") != OCI_CONTROLLER_IDENTITY or manifest.get("host_controller") != HOST_CONTROLLER_IDENTITY):
            raise Blocked("official OCI child and GNU host controller provenance differs from the reviewed manifest")
        located = shutil.which("decapod", path=env.get("PATH"))
        if not located:
            raise Blocked("official controller unavailable")
        controller = Path(located).resolve()
        if hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
            raise Blocked("controller differs from verified official v0.138.2")
        result["controller"] = OCI_CONTROLLER_IDENTITY
        result["host_controller"] = HOST_CONTROLLER_IDENTITY
        run("ingest native instructions", "decapod", "docs", "ingest")
        evaluation = doc("evaluate original user request and bounded continuation", "eval", "--stdin", "--format", "json",
                         data=REQUEST + "\n\nReviewed residual intent:\n" + manifest["intent"] + "\n" + repair["intent"])
        result["prompt_evaluation"] = evaluation
        if evaluation.get("status") != "allow":
            raise Blocked("native evaluation did not admit this request")
        fingerprint = run("verify operator custody certificate", "openssl", "x509", "-in", str(args.custody_recipient.resolve()), "-noout", "-fingerprint", "-sha256").strip().split("=", 1)[-1].replace(":", "").lower()
        if fingerprint != CERT:
            raise Blocked("operator custody certificate identity mismatch")
        cert_verified = True
        result["custody_recipient_fingerprint"] = "sha256:" + fingerprint
        repair_bytes, release_bytes = args.repair_patch.read_bytes(), args.release_drift.read_bytes()
        if (not 0 < len(repair_bytes) <= 1024 * 1024 or digest(repair_bytes) != repair["patch_sha256"]
                or not 0 < len(release_bytes) <= 1024 * 1024 or digest(release_bytes) != manifest["release_drift_sha256"]):
            raise Blocked("reviewed gate repair or exact observed release drift differs")
        patch, release_patch = repair_bytes.decode("utf-8"), release_bytes.decode("utf-8")
        result.update(repair_patch_sha256=digest(repair_bytes), release_drift_sha256=digest(release_bytes))
        if run("verify accepted control checkout", "git", "rev-parse", "HEAD").strip() != BASE or run("verify clean control checkout", "git", "status", "--porcelain").strip():
            raise Blocked("control checkout must start clean at exact accepted master")
        if run("verify repository origin", "git", "remote", "get-url", "origin").strip() not in ("https://github.com/" + REPO, "https://github.com/" + REPO + ".git"):
            raise Blocked("unexpected repository origin")
        fresh_target()
        current_remote()
        require_expected_pr("verify authorized existing draft PR before original task import")
        run("fetch exact original published candidate", "git", "fetch", "--no-tags", "--no-write-fetch-head", "origin", "refs/heads/" + preserved_branch + ":refs/remotes/origin/" + preserved_branch)
        if run("verify fetched published candidate", "git", "rev-parse", "refs/remotes/origin/" + preserved_branch).strip() != REMOTE_HEAD:
            raise Blocked("fetched candidate identity differs from authenticated prior run")
        bundle = incoming / "candidate.bundle"
        bundle_meta = resume["private_files"]["candidate.bundle"]
        if (bundle.is_symlink() or not bundle.is_file() or bundle.stat().st_size != bundle_meta["bytes"]
                or digest(bundle.read_bytes()) != bundle_meta["sha256"]
                or bundle_meta["sha256"] != "sha256:28fc48547140ddd787cc417767031234db560608629b54db02ca97c88fa37ecb"):
            raise Blocked("authentic checkpoint bundle differs from the captured native source")
        run("verify authentic checkpoint source bundle", "git", "bundle", "verify", str(bundle))
        if run("inspect exact checkpoint bundle refs", "git", "bundle", "list-heads", str(bundle)).split() != [CURRENT, "HEAD"]:
            raise Blocked("checkpoint bundle contains unexpected source refs")
        run("import exact native checkpoint Git objects", "git", "fetch", "--no-tags", "--no-write-fetch-head", str(bundle), "HEAD")
        if (run("verify preserved native checkpoint tree", "git", "rev-parse", CURRENT + "^{tree}").strip() != CURRENT_TREE
                or run("verify preserved native checkpoint parent", "git", "rev-parse", CURRENT + "^").strip() != PREPARATION_HEAD
                or run("verify preparation checkpoint parent", "git", "rev-parse", PREPARATION_HEAD + "^").strip() != MATERIAL_BASE):
            raise Blocked("captured source checkpoint or direct parent differs")
        run("verify historical archive source is available", "git", "cat-file", "-e", SOURCE + "^{commit}")
        native_file = incoming / "native-transfer.json"
        metadata = resume["private_files"]["native-transfer.json"]
        if native_file.is_symlink() or not native_file.is_file() or native_file.stat().st_size != metadata["bytes"] or digest(native_file.read_bytes()) != native_digest or metadata["sha256"] != native_digest:
            raise Blocked("private native task transfer differs")
        native = json.loads(native_file.read_bytes())
        if (native["receiver"] != ACTOR or native["transfer_id"] != resume["native_transfer"]["transfer_id"]
                or native["projection"]["task"]["id"] != task_id or native["source_commit"] != BASE
                or native["source_branch"] != "billing-publication-control-d7af1b96e31b"):
            raise Blocked("private native transfer does not match authenticated original task")
        observed_paths = {entry.split("\t", 2)[2] for entry in run("inspect approved repair patch paths", "git", "apply", "--numstat", "-z", data=patch).split("\0") if entry}
        release_paths = {entry.split("\t", 2)[2] for entry in run("inspect observed release drift paths", "git", "apply", "--numstat", "-z", data=release_patch).split("\0") if entry}
        if observed_paths != set(repair["changed_paths"]) or release_paths != RELEASE_GENERATED:
            raise Blocked("repair or release-generated paths differ from independent review")
        control = "billing-publication-control-" + secrets.token_hex(6)
        run("name control checkout", "git", "switch", "-c", control, BASE)
        run("acquire native session", "decapod", "session", "acquire")
        baseline = reconciliation(doc("initialize native control plane", "rpc", "--op", "agent.init"), "baseline_reconciliation", True)
        run("discover native capabilities", "decapod", "capabilities", "--format", "json")
        native_container = doc("inspect native container before original task import", "workspace", "status")
        if (native_container.get("workspace", {}).get("container", {}).get("in_container") is not True
                or native_container["workspace"]["container"].get("container_id") != result["container_execution"]["hostname"]):
            raise Blocked("native workspace inspection did not observe a real container before custody import")
        result["container_execution"]["native_before_import"] = native_container["workspace"]["container"]
        imported = doc("import original native billing custody", "todo", "transfer", "import",
                       "--input", str(native_file), "--expected-digest", native_digest,
                       "--accept-source-transfer", "--format", "json")
        if imported.get("status") != "ok" or imported.get("task_id") != task_id:
            raise Blocked("native custody import did not confirm the original task")
        result["imported"] = True
        task = task_id
        result["task"] = task
        claim = doc("claim original billing task", "todo", "claim", "--id", task, "--lease-seconds", str(args.publish_timeout + 1800), "--format", "json")
        if claim.get("status") != "ok":
            raise Blocked("native claim was not confirmed")
        claimed = True
        branch = preserved_branch
        result["branch"] = branch
        admitted = doc("admit original task workspace", "workspace", "ensure", "--branch", branch)
        if admitted.get("can_work") is not True or admitted.get("branch") != branch or admitted.get("in_container") is not True:
            raise Blocked("native workspace admission did not authorize the original branch")
        work = Path(admitted["worktree_path"]).resolve()
        if run("verify native workspace initial base", "git", "rev-parse", "HEAD", cwd=work).strip() != BASE:
            raise Blocked("new native workspace must start at reviewed accepted base")
        seed_paths = set(run("inspect native admission seed", "git", "diff", "HEAD", "--name-only", "-z", cwd=work).split("\0")) - {""}
        seed_paths |= set(run("inspect untracked native seed", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
        if any(path not in GENERATED and not path.startswith(SPEC_PREFIX) for path in seed_paths):
            raise Blocked("native admission introduced unexpected source material")
        if seed_paths:
            run("preserve receiver admission seed", "git", "stash", "push", "--include-untracked", "--message", "Preserve native receiver admission before exact original source continuation", "--", *sorted(seed_paths), cwd=work)
            seed_archive = private / "admission-seed.bundle"
            run("archive native admission seed", "git", "bundle", "create", str(seed_archive), "refs/stash", "^" + BASE, cwd=work)
            result["admission_seed"] = encrypt(seed_archive, "billing-admission-seed.der")
        clean("verify clean receiver transition")
        run("restore exact authenticated native checkpoint", "git", "merge", "--ff-only", CURRENT, cwd=work)
        if run("verify continued task source", "git", "rev-parse", "HEAD", cwd=work).strip() != CURRENT:
            raise Blocked("workspace did not continue the exact preserved native checkpoint")
        clean("verify exact source restoration cleanliness")
        journal = run("bind restored authentic committed journal", "git", "hash-object", ".decapod/governance.json", cwd=work).strip()
        readmitted = doc("readmit preserved native task journal", "workspace", "ensure", "--branch", branch, cwd=work)
        if readmitted.get("can_work") is not True or readmitted.get("branch") != branch:
            raise Blocked("native readmission did not authorize original task continuation")
        if run("verify preserved journal after readmission", "git", "hash-object", ".decapod/governance.json", cwd=work).strip() != journal:
            raise Blocked("native readmission altered the authentic committed journal")
        clean("verify unchanged clean checkpoint after readmission")
        result.update(head_sha=CURRENT, tree=CURRENT_TREE)
        result["original_committed_journal"] = dict(blob=journal, source_commit=CURRENT,
            source_bundle_sha256=bundle_meta["sha256"], prior_run_id=resume["prior_run_id"], exact_bytes_preserved=True)
        run("orient original billing task", "decapod", "infer", "orientation", "--task-id", task, cwd=work)
        run("read core doctrine", "decapod", "constitution", "get", "core/DECAPOD", cwd=work)
        doc("resolve current task context", "rpc", "--op", "context.resolve", cwd=work)
        proof = doc("verify preserved original checkpoint lineage", "govern", "artifacts", "verify-checkpoints", "--base-branch", BASE, "--head-ref", CURRENT, "--target-base", "master", "--format", "json", cwd=work)
        if not (proof.get("history_proven") is True and proof.get("inspection_complete") is True and proof.get("findings") == [] and proof.get("head_commit") == CURRENT and proof.get("base_commit") == BASE):
            raise Blocked("preserved original checkpoint lineage no longer verifies")
        result["prior_checkpoint_verification"] = proof
        # Materialize both independently reviewed deltas only in a private index.
        # These are comparison references only. The authenticated checkpoint
        # already contains the authored/docs/release changes; do not rerender.
        review_env = dict(env, GIT_INDEX_FILE=str(private / "review-material.index"))
        for label, argv, data in [
            ("read immutable reviewed material baseline", ["git", "read-tree", MATERIAL_BASE], None),
            ("apply reviewed gate material to isolated index", ["git", "apply", "--cached", "--whitespace=error", "-"], patch),
            ("compute reviewed repaired source tree", ["git", "write-tree"], None),
        ]:
            observed = subprocess.run(argv, cwd=work, env=review_env, input=data, text=True, capture_output=True, timeout=120)
            if observed.returncode:
                raise Blocked(label + " failed")
        if observed.stdout.strip() != repair["expected_material_tree"]:
            raise Blocked("reviewed source gate patch does not produce its bound complete material tree")
        for path, reviewed in reviewed_files.items():
            before = run("verify original material blob " + path, "git", "rev-parse", MATERIAL_BASE + ":" + path, cwd=work).strip()
            after = run("verify repaired material blob " + path, "git", "rev-parse", repair["expected_material_tree"] + ":" + path, cwd=work).strip()
            if before != reviewed["before_blob"] or after != reviewed["after_blob"]:
                raise Blocked("independent repair material blob identity differs")
        for label, argv, data in [
            ("apply observed six-file drift to isolated index", ["git", "apply", "--cached", "--whitespace=error", "-"], release_patch),
            ("compute reviewed complete native result tree", ["git", "write-tree"], None),
        ]:
            observed = subprocess.run(argv, cwd=work, env=review_env, input=data, text=True, capture_output=True, timeout=120)
            if observed.returncode:
                raise Blocked(label + " failed")
        if observed.stdout.strip() != expected_tree:
            raise Blocked("reviewed release projection tree identity differs")
        result["projection_reference_tree"] = expected_tree
        approved_map = resume["baseline_inventory"]["discovered_sources"]
        recomputed = digest(json.dumps({"files": dict(sorted(approved_map.items()))}, separators=(",", ":")).encode())
        if (len(approved_map) != 105 or recomputed != approved_inventory
                or set(approved_map) & REPAIR_SCOPE or baseline["discovered_sources"] != approved_map):
            raise Blocked("continued candidate is not exactly the reviewed accepted-baseline inventory")
        accepted_modes = {}
        for record in run("inspect accepted inventory Git modes", "git", "ls-tree", "-r", "-z", BASE, "--", *sorted(approved_map), cwd=work).split("\0"):
            if record:
                header, path = record.split("\t", 1)
                mode, kind, oid = header.split()
                if kind != "blob" or mode not in {"100644", "120000"}:
                    raise Blocked("accepted inventory contains an unsupported Git object mode")
                accepted_modes[path] = (mode, oid)
        if set(accepted_modes) != set(approved_map):
            raise Blocked("accepted inventory Git mode catalog differs")
        for path, expected_digest in approved_map.items():
            source = subprocess.run(["git", "show", BASE + ":" + path], cwd=work, env=env, capture_output=True, timeout=120)
            actual_source = work / path
            mode, oid = accepted_modes[path]
            if mode == "120000":
                if (path != "rust-toolchain.toml" or oid != "7f77ea3839511a3c538d1c8000b480157a8cd73b"
                        or not actual_source.is_symlink()):
                    raise Blocked("accepted inventory symlink identity or filesystem mode differs")
                actual_bytes = os.fsencode(os.readlink(actual_source))
                if actual_bytes != b".config/build/rust-toolchain.toml":
                    raise Blocked("accepted inventory symlink target differs")
            else:
                if actual_source.is_symlink() or not actual_source.is_file() or actual_source.stat().st_mode & 0o111:
                    raise Blocked("accepted inventory regular-file mode differs")
                actual_bytes = actual_source.read_bytes()
            if source.returncode or digest(source.stdout) != expected_digest or digest(actual_bytes) != expected_digest:
                raise Blocked("accepted-base source digest differs: " + path)
        result["inventory_review"] = dict(inventory_digest=approved_inventory, independently_hashed_sources=105,
            candidate_inventory_equal_to_accepted_base=True, new_policy_changes=False, carrier_approval=False, execution_proof=False)
        for path, reviewed in reviewed_files.items():
            if run("verify restored approved material " + path, "git", "hash-object", path, cwd=work).strip() != reviewed["after_blob"]:
                raise Blocked("restored pending authored/doc material differs from independently approved gate repair")
        # The source and receipt are already committed by the original native
        # task. Restore their exact bytes and validate their existing lineage;
        # do not create a replacement checkpoint or rerun preparation.
        verify_material("exact restored committed native material")
        clean("verify existing checkpoint remains clean")
        if run("bind unchanged native checkpoint", "git", "rev-parse", "HEAD", cwd=work).strip() != CURRENT:
            raise Blocked("restored native checkpoint changed before full publication")
        head = CURRENT
        result.update(head_sha=head, tree=CURRENT_TREE, checkpoints=True, checkpoint_verification=proof)
        result["project_prerequisites"] = dict(preparation,
            preserved_from_run=resume["preparation_run_id"], native_reexecution=False,
            preparation_only=True, publication_ready=False, receipt_commit_sha=CURRENT)
        result["committed_receipt_continuation"] = dict(
            head_sha=CURRENT, source_bundle_sha256=bundle_meta["sha256"],
            preparation_head_sha=PREPARATION_HEAD, preparation_run_id=resume["preparation_run_id"],
            manifest_raw_sha256=PREPARATION_MANIFEST, manifest_canonical_sha256=PREPARATION_CANONICAL,
            source_regenerated=False, prerequisite_rerun=False, new_checkpoint=False)
        archive = private / "candidate.bundle"
        run("preserve final checkpointed source", "git", "bundle", "create", str(archive), "HEAD", "^" + BASE, cwd=work)
        run("verify preserved final source bundle", "git", "bundle", "verify", str(archive), cwd=work)
        result["candidate_bundle"] = encrypt(archive, "billing-candidate.der")
        fresh_target()
        current_remote()
        archive_original_before_freeze()
        run("refresh complete remote tag history", "git", "-c", "fetch.prune=false", "-c", "fetch.pruneTags=false", "fetch", "--no-force", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--no-prune", "--no-prune-tags", "--refmap=", "origin", "refs/tags/*:refs/tags/*", cwd=work)
        remote_tags = sorted(run("observe remote tag history", "git", "ls-remote", "--tags", "origin", cwd=work).splitlines())
        local_tags = sorted(run("observe local tag history", "git", "show-ref", "--tags", "--dereference", cwd=work).splitlines())
        if sorted(tuple(line.split()) for line in remote_tags) != sorted(tuple(line.split()) for line in local_tags):
            raise Blocked("local tag history differs from current remote history")
        if sorted(run("recheck stable remote tag history", "git", "ls-remote", "--tags", "origin", cwd=work).splitlines()) != remote_tags:
            raise Blocked("remote release history changed before native validation")
        result["tag_history_verified"] = True
        try:
            if verify_github_cli(runtime_state, env) != result["github_cli"]:
                raise RuntimeError("Verified GitHub CLI changed before native publication")
        except RuntimeError as error:
            raise Blocked(str(error)) from None
        body = manifest["body"].replace("{{HEAD_SHA}}", head).replace("{{BASE_SHA}}", BASE)
        code, stdout, stderr, timeout = execute("native full workspace publisher", ["decapod", "workspace", "publish", "--title", manifest["title"], "--description", body], cwd=work, timeout=args.publish_timeout)
        result["publisher"] = dict(exit_code=code, timeout=timeout, codes=sorted(set(re.findall(CODES, stdout + stderr))), ci_identity_preserved=True)
        try:
            published = json.loads(stdout)
        except ValueError:
            published = None
        if not (code == 0 and not timeout and isinstance(published, dict) and published.get("status") == "ok"
                and published.get("pr_verified") is True and published.get("remote_verified") is True
                and published.get("branch") == branch and published.get("commit_hash") == head
                and published.get("pr_url") == EXPECTED_PR["url"]):
            result["publisher"]["diagnostic_excerpt"] = scrub(stderr[-8000:])
            raise Blocked("native publisher did not confirm complete readiness and exact PR")
        require_expected_pr("verify exact draft PR after native publication readback")
        result.update(status="published_after_native_full_boundary", pushed=True, publication_ready=True,
                      full_boundary_passed=True, pr_url=published["pr_url"], publication=published)
        fresh_target()
        if run("verify exact published branch", "git", "ls-remote", "origin", "refs/heads/" + branch, cwd=work).split() != [head, "refs/heads/" + branch]:
            raise Blocked("published branch no longer matches exact proven candidate")
        clean("verify source after native publisher")
        verify_material("after native full publisher")
    except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        result.setdefault("blocker", dict(stage=stage, kind=type(exc).__name__))
        if isinstance(exc, Blocked):
            result["blocker"]["reason"] = str(exc)
    finally:
        first_blocker = result.get("blocker")
        if result["imported"] and not claimed:
            result["custody_handoff_incomplete"] = "Native import succeeded but claim did not. No task ownership or export is fabricated; retain authenticated source handoff and encrypted import diagnostics for explicit native recovery."
        if claimed:
            try:
                # Export from the named, unchanged control checkout; worktree
                # material may be incomplete after any failed native operation.
                if run("verify export control branch", "git", "symbolic-ref", "--quiet", "HEAD").strip() != "refs/heads/" + control or run("verify export control base", "git", "rev-parse", "HEAD").strip() != BASE:
                    raise Blocked("native export control context changed")
                run("yield native task", "decapod", "todo", "yield", "--id", task, "--reason", "Explicit custody transfer to " + RECEIVER + " before ephemeral runner teardown")
                transfer = private / "next-task-transfer.json"
                code, stdout, stderr, timeout = execute("export native task custody", ["decapod", "todo", "transfer", "export", "--id", task, "--to", RECEIVER, "--output", str(transfer), "--format", "json"])
                if code or timeout:
                    raw = stdout + "\n" + stderr
                    sealed = re.search(r"source sealed as ([0-9A-HJKMNP-TV-Z]{26}); artifact publication unconfirmed:", raw)
                    if timeout or sealed is None or ("Use transfer recover --id " + task + " --transfer-id " + sealed[1] + " --output <new-private-path>") not in raw or re.search(r"AUTHORING_|SESSION_|SCOPE_VIOLATION|DECISION_GATE|POLICY_DENIED", raw):
                        raise Blocked("native export failed without exact documented sealed-artifact recovery")
                    transfer = private / "recovered-task-transfer.json"
                    exported = doc("recover exact sealed transfer", "todo", "transfer", "recover", "--id", task, "--transfer-id", sealed[1], "--output", str(transfer), "--format", "json")
                    if exported.get("transfer_id") != sealed[1]:
                        raise Blocked("recovered transfer identity differs")
                    result["custody_export_recovery"] = dict(transfer_id=sealed[1], reexported=False)
                else:
                    exported = json.loads(stdout)
                data = transfer.read_bytes()
                native = json.loads(data)
                if not (exported.get("status") == "ok" and exported.get("source_sealed") is True
                        and exported.get("artifact_sha256") == digest(data) and native["receiver"] == RECEIVER
                        and native["projection"]["task"]["id"] == task and native["transfer_id"] == exported["transfer_id"]
                        and native["source_commit"] == BASE and native["source_branch"] == control):
                    raise Blocked("native export task/receiver/source identity differs")
                if any(secret.encode() in data for secret in sensitive):
                    raise Blocked("private task export unexpectedly contains a session credential")
                (diagnostics / "native-custody-transfer.json").write_bytes(data)
                encrypted = encrypt(transfer, "billing-custody.der")
                result["transfer"] = dict(task_id=task, transfer_id=exported["transfer_id"], receiver=RECEIVER,
                                           native_source_commit=BASE, native_source_branch=control,
                                           source_sealed=True, candidate_sha=result.get("head_sha"), candidate_branch=branch, **encrypted)
                transfer.unlink()
                result["custody_exported"] = True
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["transfer_error"] = dict(stage=stage, kind=type(exc).__name__, reason=str(exc) if isinstance(exc, Blocked) else "native handoff incomplete")
        if cert_verified:
            try:
                if work is not None:
                    code, stdout, stderr, timeout = execute("observe final workspace", ["git", "status", "--porcelain"], cwd=work)
                    result["source_dirty_after_attempt"] = code != 0 or timeout or bool(stdout.strip())
                    result["final_head_sha"] = run("observe final head", "git", "rev-parse", "HEAD", cwd=work).strip()
                    if result["final_head_sha"] != BASE and "candidate_bundle" not in result:
                        recovery = private / "recovery-candidate.bundle"
                        run("preserve otherwise unpublished source", "git", "bundle", "create", str(recovery), "HEAD", "^" + BASE, cwd=work)
                        run("verify recovery source bundle", "git", "bundle", "verify", str(recovery), cwd=work)
                        result["recovery_candidate_bundle"] = encrypt(recovery, "billing-candidate-recovery.der")
                    execute("preserve pending tracked material", ["git", "diff", "--binary", "HEAD"], cwd=work)
                    execute("preserve pending index material", ["git", "diff", "--cached", "--binary", "HEAD"], cwd=work)
                    # Preserve only reviewed/generated source paths, never runtime state.
                    pending = set(run("inspect pending untracked source", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
                    for path in sorted(pending & (paths | GENERATED)):
                        src = work / path
                        if src.is_file() and not src.is_symlink() and src.stat().st_size <= 8 * 1024 * 1024:
                            dst = diagnostics / "untracked-source" / path
                            dst.parent.mkdir(parents=True, exist_ok=True)
                            dst.write_bytes(src.read_bytes())
                    if branch:
                        code, stdout, stderr, timeout = execute("observe actual remote candidate", ["git", "ls-remote", "origin", "refs/heads/" + branch], cwd=work)
                        fields = stdout.split()
                        if code == 0 and not timeout and fields:
                            result["observed_remote_head_sha"] = fields[0]
                            result["pushed"] = fields == [result.get("head_sha"), "refs/heads/" + branch]
                archive = private / "diagnostics.tar"
                run("archive private diagnostics", "tar", "-cf", str(archive), "-C", str(private), "diagnostics")
                result["diagnostics"] = encrypt(archive, "billing-diagnostics.der")
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["preservation_error"] = dict(stage=stage, kind=type(exc).__name__)
        if first_blocker is not None:
            result["blocker"] = first_blocker
        if result["publication_ready"] and not result["custody_exported"]:
            result["status"] = "published_but_custody_export_failed"
        elif result["publication_ready"] and (result.get("blocker") or result.get("source_dirty_after_attempt") or result.get("final_head_sha") != result["head_sha"]):
            result["status"] = "published_with_postpublication_observation_blocker"
        with (out / "billing.json").open("x") as handle:
            json.dump(scrub(result), handle, indent=2)
            handle.write("\n")
        print(json.dumps({k: result[k] for k in ("status", "branch", "task", "head_sha", "pr_url", "full_boundary_passed", "custody_exported")}))
    return 0 if result["status"] == "published_after_native_full_boundary" and result["custody_exported"] else 1


if __name__ == "__main__":
    sys.exit(main())
