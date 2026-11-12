"""Continue exact native client custody, checkpoint reviewed corrections, and fast-forward its source branch."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import signal
import subprocess
import tarfile
import tempfile

REPO = "DecapodLabs/decapod"
BASE = "c5de35a89b27e788169612ae71681e45c13bf71a"
SOURCE = "1289a80a782d651af337125ee447db4b5723cbd7"
TASK = "code_01m76c1kqhjgr3yr"
BRANCH = "agent/chatgpt-issue1628-decapod/" + TASK + "-cloud-connector"
ACTOR = "chatgpt-issue1628-decapod"
RECEIVER = "chatgpt-issue1628-decapod-next"
OPS = "ops/issue1628-native-20261111"
TRANSFER = "01M76HEVW68M4ZFHWX9NXKXZDX"
INCOMING = "sha256:2d88601af5d679fe85ddecd200b7f0c50c10596cde1041182d5fe28a0313df09"
INCOMING_BRANCH = "issue1628-decapod-corrections-8f05354b04ba"
CONTROLLER = "31bd8e3983dd328a1796da8af827647be691982c91d7ec7a7b11c7fdc2b4d723"
CERT = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
SPEC = ".decapod/managed/specs/"
GOVERNANCE = ".decapod/governance.json"
VALIDATION = SPEC + "VALIDATION.md"
CHECKPOINT = "decapod-cloud-client-1628-evidence-correction"
GENERATED = {"AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md", GOVERNANCE,
             ".decapod/managed/Dockerfile.decapod"}
PATCH_PATHS = {"src/decapod/lib.rs", "docs/book/src/reference/supabase-validation.md",
               "docs/book/src/reference/propodus.md"}
BLOCKS = re.compile(r"<!-- decapod:([A-Za-z0-9:_-]+):start -->.*?<!-- decapod:\1:end -->", re.S)
CODES = r"\b(?:AUTHORING_|SESSION_|WORKSPACE_|SCOPE_|GOVERNANCE_|HISTORY_|TASK_TRANSFER_|READINESS_|PUBLICATION_|CONTROLLER_|REPOSITORY_|PR_|DECISION_GATE|POLICY_DENIED)[A-Z0-9_]*\b"
REQUEST = "This must be solved. Do so now: https://github.com/DecapodLabs/decapod/issues/1628"


class Blocked(RuntimeError):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--material", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--native-transfer", type=Path, required=True)
    parser.add_argument("--certificate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    root, material, out = Path.cwd().resolve(), args.material.resolve(), args.output.resolve()
    manifest = json.loads(args.manifest.read_text())
    if (manifest.get("repository") != REPO or manifest.get("base") != BASE
            or manifest.get("source") != SOURCE or manifest.get("task") != TASK
            or manifest.get("branch") != BRANCH or out.is_relative_to(root)
            or args.native_transfer.resolve().is_relative_to(out)):
        parser.error("requires exact original task/source/base and private custody outside source/output")
    if ([item["name"] for item in manifest["patches"]] != ["unused-imports.patch", "evidence-boundary.patch"]
            or set(manifest["changed_files"]) != PATCH_PATHS
            or set(manifest["spec_reviews"]) != {"ARCHITECTURE", "INTERFACES", "SECURITY"}
            or any(review.get("disposition") != "unchanged-with-reason" or not review.get("reason", "").strip()
                   for review in manifest["spec_reviews"].values())):
        parser.error("only the exact reviewed corrections and three unchanged spec reviews are allowed")
    addition = manifest["validation_addition"]
    if not isinstance(addition, str) or not addition.startswith("## ") or "decapod:" in addition:
        parser.error("VALIDATION addition must be authored prose, without generated block markers")
    out.mkdir(parents=True, mode=0o700, exist_ok=False)
    private = Path(tempfile.mkdtemp(prefix="issue1628-client-corrections-", dir="/root"))
    diagnostics = private / "diagnostics"
    diagnostics.mkdir(mode=0o700)
    env = dict(os.environ, DECAPOD_AGENT_ID=ACTOR, DECAPOD_CLAIM_AUTORUN="false",
               DECAPOD_SESSION_PASSWORD=secrets.token_hex(32), DECAPOD_VALIDATE_TIMEOUT_SECS="600")
    sensitive = [value for key, value in env.items()
                 if re.search("token|password|secret|credential|authorization", key, re.I) and len(value) >= 8]
    result = dict(schema="decapod.issue1628-client-corrections.v1", repository=REPO,
                  base=BASE, source=SOURCE, task=TASK, branch=BRANCH,
                  operations_sha=env.get("GITHUB_SHA"), status="blocked", imported=False,
                  admitted=False, container_observed=False, original_journal_preserved=False,
                  checkpoints=False, publication_ready=False, full_boundary_passed=False,
                  custody_exported=False, head_sha=SOURCE, pr_url=None, pushed=False,
                  build_tests="not_run", full_publisher="not_run",
                  publication_boundary="Checkpointed source push only; complete Buildkite readiness and registry release handoff remain required")
    stage, work, control, controller = "preconditions", None, None, None
    claimed = certificate_verified = False
    original_journal, authored_specs, prepared_hashes = None, {}, {}

    def scrub(value):
        if isinstance(value, str):
            for secret in sensitive:
                value = value.replace(secret, "[REDACTED]")
            return re.sub(r"(?:github_pat_|gh[pousr]_)[A-Za-z0-9_]{20,}|(?i:bearer)\s+\S+|https?://[^/\s:@]+:[^/@\s]+@", "[REDACTED]", value)
        if isinstance(value, dict):
            return {key: scrub(item) for key, item in value.items()
                    if not re.search("password|secret|credential|authorization|session_token|access_token", key, re.I)}
        if isinstance(value, list):
            return [scrub(item) for item in value]
        return value

    def execute(label, argv, cwd=None, data=None, timeout=240):
        nonlocal stage
        stage = label
        if argv[0] == "decapod":
            if controller is None or hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
                raise Blocked("official released controller identity changed")
            argv = [str(controller), *argv[1:]]
        print(json.dumps({"stage": label}), flush=True)
        process = subprocess.Popen(argv, cwd=cwd or root, env=env, text=True, start_new_session=True,
                                   stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
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
        (diagnostics / (stem + ".stdout.txt")).write_text(scrub(stdout))
        (diagnostics / (stem + ".stderr.txt")).write_text(scrub(stderr))
        return process.returncode, stdout, stderr, timed_out

    def run(label, *argv, cwd=None, data=None, timeout=240):
        code, stdout, stderr, timed_out = execute(label, list(argv), cwd, data, timeout)
        if code or timed_out:
            result["blocker"] = dict(stage=label, exit_code=code, timeout=timed_out,
                                     codes=sorted(set(re.findall(CODES, stdout + stderr))),
                                     diagnostic=scrub((stderr or stdout)[-12000:]))
            raise Blocked("native or source preparation command failed")
        return stdout

    def doc(label, *argv, cwd=None, data=None):
        value = json.loads(run(label, "decapod", *argv, cwd=cwd, data=data))
        if isinstance(value, dict) and value.get("error"):
            result["blocker"] = dict(stage=label, diagnostic=scrub(value["error"]))
            raise Blocked("native operation returned an error")
        return value

    def encrypt(source, name):
        if not certificate_verified:
            raise Blocked("unverified custody recipient")
        target = out / name
        run("encrypt " + name, "openssl", "cms", "-encrypt", "-binary", "-aes-256-cbc",
            "-in", str(source), "-out", str(target), "-outform", "DER", str(args.certificate.resolve()))
        return dict(artifact=name, plaintext_sha256=digest(source.read_bytes()), ciphertext_sha256=digest(target.read_bytes()))

    def input_bytes(name, expected):
        source = material / name
        if source.is_symlink() or not source.is_file():
            raise Blocked("reviewed material is missing or unsafe: " + name)
        value = source.read_bytes()
        if digest(value) != expected:
            raise Blocked("reviewed material digest differs: " + name)
        return value

    def fresh_target():
        if run("verify current accepted target", "git", "ls-remote", "origin", "refs/heads/master").split() != [BASE, "refs/heads/master"]:
            raise Blocked("accepted target changed; preserve custody for renewed source review")

    def remote_binding(expected=SOURCE):
        if run("verify exact remote task branch", "git", "ls-remote", "origin", "refs/heads/" + BRANCH).split() != [expected, "refs/heads/" + BRANCH]:
            raise Blocked("remote task branch changed; no source push is authorized against another tip")

    def clean(label):
        if run(label, "git", "status", "--porcelain", cwd=work).strip():
            raise Blocked(label + ": governed source is dirty")

    def reconciliation(value):
        report = value.get("result", {}).get("reconciliation")
        if not isinstance(report, dict):
            raise Blocked("native initialization omitted reconciliation")
        return report

    def verify_history(head):
        proof = doc("verify exact checkpoint history", "govern", "artifacts", "verify-checkpoints",
                    "--base-branch", BASE, "--head-ref", head, "--target-base", "master", "--format", "json", cwd=work)
        expected = [SOURCE] if head == SOURCE else [SOURCE, head]
        if not (proof.get("history_proven") is True and proof.get("inspection_complete") is True
                and proof.get("findings") == [] and proof.get("base_commit") == BASE
                and proof.get("head_commit") == head and sorted(proof.get("verified_commits", [])) == sorted(expected)):
            raise Blocked("exact original and new checkpoint lineage is not proven")
        if run("verify unrewritten candidate ancestry", "git", "rev-list", "--reverse", BASE + ".." + head, cwd=work).split() != expected:
            raise Blocked("candidate history was rewritten or gained an unexpected commit")
        return proof

    def verify_material(label):
        for path, expected in prepared_hashes.items():
            source = work / path
            if source.is_symlink() or not source.is_file() or digest(source.read_bytes()) != expected:
                raise Blocked(label + ": exact reviewed source changed: " + path)
        for path, expected in authored_specs.items():
            source = work / path
            if source.is_symlink() or not source.is_file() or digest(BLOCKS.sub("", source.read_text()).strip().encode()) != expected:
                raise Blocked(label + ": authored spec semantics changed: " + path)
        changed = set(run(label + " tracked scope", "git", "diff", "--name-only", "-z", SOURCE, cwd=work).split("\0")) - {""}
        changed |= set(run(label + " untracked scope", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
        if not (PATCH_PATHS | {VALIDATION}) <= changed:
            raise Blocked(label + ": a required correction or material VALIDATION update is missing")
        if any(path not in PATCH_PATHS | GENERATED and not path.startswith(SPEC) for path in changed):
            raise Blocked(label + ": source exceeds the exact correction and native projection scope")

    try:
        if (env.get("GITHUB_ACTIONS") != "true" or env.get("BUILDKITE") == "true"
                or env.get("GITHUB_REPOSITORY") != REPO or env.get("GITHUB_REF") != "refs/heads/" + OPS
                or not Path("/.dockerenv").is_file()):
            raise Blocked("requires the exact authorized repository and actual hosted container")
        result["container_observed"] = True
        fingerprint = run("verify operator custody certificate", "openssl", "x509", "-in", str(args.certificate.resolve()),
                          "-noout", "-fingerprint", "-sha256").strip().split("=", 1)[1].replace(":", "").lower()
        if fingerprint != CERT:
            raise Blocked("operator custody certificate differs")
        certificate_verified = True
        run("install exact official released controller", "/usr/bin/python3.12", str(material.parent / "install_controller.py"), str(private))
        controller = private / "decapod"
        if hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
            raise Blocked("released controller bytes differ")
        env["PATH"] = str(private) + os.pathsep + env["PATH"]
        result["controller"] = dict(release="v0.138.2", binary_sha256=CONTROLLER)
        run("ingest native doctrine", "decapod", "docs", "ingest")
        evaluation = doc("evaluate exact requested continuation", "eval", "--stdin", "--format", "json", data=REQUEST + "\n\n" + manifest["intent"])
        if evaluation.get("status") != "allow":
            raise Blocked("native request evaluation did not allow continuation")
        result["evaluation"] = evaluation
        source_prior = json.loads(input_bytes("source-result.json", manifest["source_report_sha256"]))
        prior_proof = source_prior["checkpoint_verification"]
        if not (source_prior.get("repository") == REPO and source_prior.get("task") == TASK
                and source_prior.get("branch") == BRANCH and source_prior.get("base") == BASE
                and source_prior.get("head_sha") == SOURCE and source_prior.get("final_head_sha") == SOURCE
                and source_prior.get("source_dirty") is False and source_prior.get("custody_exported") is True
                and source_prior.get("publication_ready") is False and prior_proof.get("history_proven") is True
                and prior_proof.get("inspection_complete") is True and prior_proof.get("findings") == []
                and prior_proof.get("base_commit") == BASE and prior_proof.get("head_commit") == SOURCE):
            raise Blocked("original source report does not bind the exact clean checkpoint")
        prior = json.loads(input_bytes("formatting-failure-result.json", manifest["formatting_failure_report_sha256"]))
        recovery = manifest["formatting_recovery"]
        prior_transfer = prior["transfer"]
        if not (prior.get("repository") == REPO and prior.get("task") == TASK and prior.get("branch") == BRANCH
                and prior.get("base") == BASE and prior.get("head_sha") == SOURCE and prior.get("final_head_sha") == SOURCE
                and prior.get("source") == SOURCE and prior.get("source_dirty") is True
                and prior.get("custody_exported") is True and prior.get("publication_ready") is False
                and prior.get("full_boundary_passed") is False and prior.get("checkpoints") is False
                and prior.get("original_journal_preserved") is True
                and prior.get("original_journal_blob") == recovery["original_journal_blob"]
                and prior.get("original_checkpoint_verification") == prior_proof
                and prior.get("source_push") is None and prior.get("pushed") is False
                and prior.get("observed_remote_head_sha") == SOURCE and prior.get("status") == "blocked"
                and prior.get("build_tests") == "not_run" and prior.get("full_publisher") == "not_run"
                and recovery.get("run_id") == 43123429502 and prior.get("blocker") == recovery["blocker"]
                and prior.get("diagnostics", {}).get("plaintext_sha256") == recovery["diagnostics_archive_sha256"]
                and prior_transfer.get("transfer_id") == "01M76GWVMZWGY8XFXFZ8W5JR6V"
                and prior_transfer.get("receiver") == "chatgpt-issue1628-decapod-next"
                and prior_transfer.get("plaintext_sha256") == "sha256:b3cbf82ce49aa91fcf740580d36ab8f2e57e6766207aa8dcddbc3948774756dc"
                and prior_transfer.get("source_sealed") is True
                and prior_transfer.get("candidate_sha") == SOURCE and prior_transfer.get("candidate_branch") == BRANCH
                and prior_transfer.get("native_source_commit") == BASE
                and prior_transfer.get("native_source_branch") == "issue1628-decapod-corrections-757cabb0b9e1"):
            raise Blocked("historical formatting report does not bind the exact preserved failure")
        input_bytes("formatting-failure-pending.patch", recovery["pending_patch_sha256"])
        input_bytes("formatting-failure-prepared-hashes.json", recovery["prepared_hashes_sha256"])
        input_bytes("unused-imports-preformat.patch", recovery["original_patch_sha256"])
        result["formatting_recovery"] = recovery
        prior = json.loads(input_bytes("previous-result.json", manifest["previous_report_sha256"]))
        refresh_recovery = manifest["spec_refresh_recovery"]
        prior_transfer = prior["transfer"]
        if not (prior.get("repository") == REPO and prior.get("task") == TASK and prior.get("branch") == BRANCH
                and prior.get("base") == BASE and prior.get("source") == SOURCE
                and prior.get("head_sha") == SOURCE and prior.get("final_head_sha") == SOURCE
                and prior.get("source_dirty") is True and prior.get("custody_exported") is True
                and prior.get("publication_ready") is False and prior.get("full_boundary_passed") is False
                and prior.get("checkpoints") is False and prior.get("original_journal_preserved") is True
                and prior.get("original_journal_blob") == refresh_recovery["original_journal_blob"]
                and prior.get("original_checkpoint_verification") == prior_proof
                and prior.get("source_push") is None and prior.get("pushed") is False
                and prior.get("observed_remote_head_sha") == SOURCE and prior.get("status") == "blocked"
                and prior.get("build_tests") == "not_run" and prior.get("full_publisher") == "not_run"
                and refresh_recovery.get("run_id") == 43124963685
                and prior.get("blocker") == refresh_recovery["blocker"]
                and prior.get("formatting_recovery") == recovery
                and prior.get("diagnostics", {}).get("plaintext_sha256") == refresh_recovery["diagnostics_archive_sha256"]
                and prior_transfer.get("transfer_id") == TRANSFER and prior_transfer.get("receiver") == ACTOR
                and prior_transfer.get("plaintext_sha256") == INCOMING and prior_transfer.get("source_sealed") is True
                and prior_transfer.get("candidate_sha") == SOURCE and prior_transfer.get("candidate_branch") == BRANCH
                and prior_transfer.get("native_source_commit") == BASE and prior_transfer.get("native_source_branch") == INCOMING_BRANCH):
            raise Blocked("spec refresh recovery does not bind the exact preserved failure and sealed successor")
        input_bytes("spec-refresh-observed-validation.patch", refresh_recovery["validation_diff_sha256"])
        # The previous uncommitted native journal remains historical evidence
        # in its verified encrypted archive. Its event identity and step digest
        # are recorded through the CLI below, never restored by raw file writes.
        result["spec_refresh_recovery"] = refresh_recovery
        observed = json.loads(input_bytes("buildkite-7937-observed-errors.json", manifest["buildkite_evidence_sha256"]))
        if (observed.get("candidate") != SOURCE or observed.get("accepted_base") != BASE
                or observed.get("build") != 7937 or observed.get("compiler_result", {}).get("errors") != 11
                or observed.get("observed_dependency_compilation", {}).get("version") != "0.11.1"):
            raise Blocked("historical compiler evidence does not bind the reviewed candidate and registry dependency")
        result["historical_buildkite_evidence"] = dict(artifact_sha256=manifest["buildkite_evidence_sha256"],
            source=SOURCE, build=7937, url="https://buildkite.com/decapodlabs/decapod/builds/7937",
            scope="Observed prior candidate failure only; not a corrected-candidate test result")
        for patch in manifest["patches"]:
            input_bytes(patch["name"], patch["sha256"])
        native_bytes = args.native_transfer.read_bytes()
        native = json.loads(native_bytes)
        if (digest(native_bytes) != INCOMING or native["transfer_id"] != TRANSFER
                or native["receiver"] != ACTOR or native["projection"]["task"]["id"] != TASK
                or native["source_commit"] != BASE or native["source_branch"] != INCOMING_BRANCH):
            raise Blocked("sealed incoming task identity differs")
        if run("verify accepted checkout", "git", "rev-parse", "HEAD").strip() != BASE or run("verify clean control checkout", "git", "status", "--porcelain").strip():
            raise Blocked("initial source is not exact clean accepted master")
        if run("verify repository origin", "git", "remote", "get-url", "origin").strip() != "https://github.com/" + REPO + ".git":
            raise Blocked("unexpected repository origin")
        fresh_target()
        remote_binding()
        run("fetch exact preserved remote candidate", "git", "-c", "fetch.prune=false", "-c", "fetch.pruneTags=false",
            "fetch", "--no-force", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--no-prune",
            "--no-prune-tags", "--refmap=", "origin", "refs/heads/" + BRANCH)
        remote_binding()
        run("verify preserved candidate object", "git", "cat-file", "-e", SOURCE + "^{commit}")
        if run("verify original candidate parent", "git", "rev-list", "--parents", "-n", "1", SOURCE).split() != [SOURCE, BASE]:
            raise Blocked("preserved candidate no longer has its exact original parent")
        original_journal = json.loads(run("bind original committed journal", "git", "show", SOURCE + ":" + GOVERNANCE))
        if ([item.get("id") for item in original_journal.get("checkpoints", [])] != ["decapod-cloud-client-1628"]
                or original_journal.get("change") != manifest["original_change"]
                or original_journal.get("sections", {}).get("trajectory", {}).get("run_id") != manifest["original_run_id"]):
            raise Blocked("original native journal identity differs from review")
        code, _, _, timed_out = execute("verify unused local task branch", ["git", "show-ref", "--verify", "--quiet", "refs/heads/" + BRANCH])
        if code != 1 or timed_out:
            raise Blocked("task branch already exists locally or its absence could not be verified")
        control = "issue1628-decapod-corrections-" + secrets.token_hex(6)
        run("name private control checkout", "git", "switch", "-c", control, BASE)
        run("acquire receiver session", "decapod", "session", "acquire")
        baseline = reconciliation(doc("initialize accepted control plane", "rpc", "--op", "agent.init"))
        if baseline.get("inventory_digest") != manifest["source_inventory_digest"] or any(
                repair.get("owner") != "agent" or repair.get("code") not in {"STALE_SOURCE_INVENTORY", "REQUIRED_PROOF_NOT_RUN"}
                for repair in baseline["repair_tasks"]):
            raise Blocked("accepted control-plane reconciliation differs from the reviewed source contract")
        imported = doc("import exact sealed successor", "todo", "transfer", "import", "--input", str(args.native_transfer.resolve()),
                       "--expected-digest", INCOMING, "--accept-source-transfer", "--format", "json")
        if imported.get("status") != "ok" or imported.get("task_id") != TASK:
            raise Blocked("native import did not confirm the original task")
        result["imported"] = True
        claim = doc("claim original native task", "todo", "claim", "--id", TASK, "--lease-seconds", "10800", "--format", "json")
        if claim.get("status") != "ok":
            raise Blocked("native task claim was not confirmed")
        claimed = True
        admitted = doc("admit original governed branch", "workspace", "ensure", "--branch", BRANCH)
        if admitted.get("can_work") is not True or admitted.get("branch") != BRANCH:
            raise Blocked("native workspace admission failed")
        work = Path(admitted["worktree_path"]).resolve()
        result["admitted"] = True
        if run("verify admission seed base", "git", "rev-parse", "HEAD", cwd=work).strip() != BASE:
            raise Blocked("native admission did not begin at the expected accepted base")
        changed = set(run("inspect native admission seed", "git", "diff", "HEAD", "--name-only", "-z", cwd=work).split("\0")) - {""}
        untracked = set(run("inspect untracked native seed", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
        if any(path not in GENERATED and not path.startswith(SPEC) for path in changed | untracked):
            raise Blocked("native admission seed contains unexpected material")
        if changed or untracked:
            run("preserve native admission seed", "git", "stash", "push", "--include-untracked", "--message",
                "Preserve native admission seed before exact issue1628 task continuation", "--", *sorted(changed | untracked), cwd=work)
            seed = run("bind preserved admission seed", "git", "rev-parse", "refs/stash", cwd=work).strip()
            if run("verify native seed ancestry", "git", "rev-parse", seed + "^1", cwd=work).strip() != BASE:
                raise Blocked("native seed archive has the wrong accepted parent")
            archive = private / "admission-seed.bundle"
            run("archive native admission seed", "git", "bundle", "create", str(archive), "refs/stash", "^" + BASE, cwd=work)
            run("verify native seed archive", "git", "bundle", "verify", str(archive), cwd=work)
            result["admission_seed"] = dict(oid=seed, paths=sorted(changed | untracked), **encrypt(archive, "admission-seed.der"))
        clean("verify clean admission transition")
        if run("verify exact admitted branch", "git", "symbolic-ref", "--quiet", "HEAD", cwd=work).strip() != "refs/heads/" + BRANCH:
            raise Blocked("admitted local branch differs")
        if run("verify accepted remote-tracking target", "git", "rev-parse", "refs/remotes/origin/master", cwd=work).strip() != BASE:
            raise Blocked("accepted target ref differs before source restoration")
        remote_binding()
        run("restore exact checkpointed local source", "git", "reset", "--keep", SOURCE, cwd=work)
        if (run("verify restored source", "git", "rev-parse", "HEAD", cwd=work).strip() != SOURCE
                or run("verify accepted target remained intact", "git", "rev-parse", "refs/remotes/origin/master", cwd=work).strip() != BASE):
            raise Blocked("source restoration changed the wrong reference")
        journal_blob = run("bind original journal blob", "git", "rev-parse", SOURCE + ":" + GOVERNANCE, cwd=work).strip()
        readmitted = doc("readmit preserved task journal", "workspace", "ensure", "--branch", BRANCH, cwd=work)
        if (readmitted.get("can_work") is not True or readmitted.get("branch") != BRANCH
                or Path(readmitted["worktree_path"]).resolve() != work):
            raise Blocked("native readmission did not authorize the same task workspace")
        if run("verify unchanged original journal", "git", "hash-object", GOVERNANCE, cwd=work).strip() != journal_blob:
            raise Blocked("native readmission changed the original journal")
        clean("verify exact restored candidate")
        result.update(original_journal_preserved=True, original_journal_blob=journal_blob)
        run("orient original task", "decapod", "infer", "orientation", "--task-id", TASK, cwd=work)
        run("read native core doctrine", "decapod", "constitution", "get", "core/DECAPOD", cwd=work)
        doc("resolve original task context", "rpc", "--op", "context.resolve", cwd=work)
        clean("verify source after orientation")
        result["original_checkpoint_verification"] = verify_history(SOURCE)
        doc("check original approved execution plan", "govern", "plan", "check-execute", "--todo-id", TASK, cwd=work)
        for path, binding in manifest["changed_files"].items():
            source = work / path
            if source.is_symlink() or not source.is_file() or digest(source.read_bytes()) != binding["before_sha256"]:
                raise Blocked("patch input differs from exact reviewed source: " + path)
        for path, expected in manifest["preserved_files"].items():
            source = work / path
            if source.is_symlink() or not source.is_file() or digest(source.read_bytes()) != expected:
                raise Blocked("accepted dependency or publication contract differs: " + path)
            prepared_hashes[path] = expected
        authored_specs = {str(source.relative_to(work)): digest(BLOCKS.sub("", source.read_text()).strip().encode())
                          for source in (work / SPEC).glob("*.md")}
        if digest((work / VALIDATION).read_bytes()) != manifest["validation_before_sha256"]:
            raise Blocked("authored VALIDATION source differs")
        for patch in manifest["patches"]:
            path = str(material / patch["name"])
            run("check exact correction " + patch["name"], "git", "apply", "--check", "--whitespace=error-all", path, cwd=work)
            run("apply exact correction " + patch["name"], "git", "apply", "--whitespace=error-all", path, cwd=work)
        for path, binding in manifest["changed_files"].items():
            prepared_hashes[path] = binding["after_sha256"]
        validation = work / VALIDATION
        previous = validation.read_text()
        title = re.search(r"(?m)^# [^\n]*\n", previous)
        if not title or addition in previous:
            raise Blocked("VALIDATION insertion point is ambiguous or correction was already applied")
        rewritten = previous[:title.end()] + "\n" + addition + "\n" + previous[title.end():]
        if [match.group(0) for match in BLOCKS.finditer(previous)] != [match.group(0) for match in BLOCKS.finditer(rewritten)]:
            raise Blocked("authored VALIDATION addition changes a generated block")
        validation.write_text(rewritten)
        authored_specs[VALIDATION] = manifest["validation_after_authored_sha256"]
        # Preparation only: no compilation, tests, validator or publisher is invoked.
        run("format reviewed client correction", "/usr/local/rustup/toolchains/1.96.1-x86_64-unknown-linux-gnu/bin/cargo", "fmt", "--all", cwd=work)
        verify_material("formatted correction")
        candidate = reconciliation(doc("verify unchanged complete source contract", "rpc", "--op", "agent.init", cwd=work))
        if (candidate.get("inventory_digest") != manifest["source_inventory_digest"]
                or candidate.get("status") != "proof_pending"
                or any(repair.get("code") != "REQUIRED_PROOF_NOT_RUN" for repair in candidate["repair_tasks"])):
            raise Blocked("candidate source contract requires unreviewed reconciliation")
        doc("refresh native source projections", "rpc", "--op", "specs.refresh", cwd=work)
        for name, review in manifest["spec_reviews"].items():
            doc("record unchanged " + name.lower() + " review", "govern", "plan", "review-spec", "--path", SPEC + name + ".md",
                "--disposition", review["disposition"], "--reason", review["reason"], cwd=work)
        trajectory = doc("inspect preserved trajectory", "govern", "artifacts", "status", cwd=work)["sections"]["trajectory"]["run_id"]
        if trajectory != manifest["original_run_id"]:
            raise Blocked("the original run journal was replaced")
        doc("record observed failure and correction boundary", "govern", "trajectory", "record", "--run-id", trajectory,
            "--task-id", TASK, "--phase", "validation", "--scope", "decapod-cloud-client",
            "--boundary", "Same-task reviewed corrections and checkpointed source push only; complete Buildkite readiness and separate registry handoff remain unsatisfied",
            "--command", "cargo fmt --all", "--command", "decapod rpc --op specs.refresh",
            "--check", "prior-buildkite7937-compilation=failed", "--check", "correction-source-review=passed",
            "--check", "corrected-candidate-build-tests=unavailable", "--evidence", manifest["trajectory_evidence"], cwd=work)
        verify_material("prepared correction")
        run("verify unstaged correction whitespace", "git", "diff", "--check", cwd=work)
        run("stage reviewed correction", "git", "add", "-A", cwd=work)
        run("verify staged correction whitespace", "git", "diff", "--cached", "--check", cwd=work)
        doc("checkpoint exact client corrections", "govern", "artifacts", "checkpoint", "--id", CHECKPOINT,
            "--summary", manifest["checkpoint_summary"], "--proof-ref", VALIDATION, cwd=work)
        run("stage native correction checkpoint", "git", "add", GOVERNANCE, cwd=work)
        run("verify complete staged checkpoint whitespace", "git", "diff", "--cached", "--check", cwd=work)
        staged_journal = json.loads(run("inspect staged native journal continuity", "git", "show", ":" + GOVERNANCE, cwd=work))
        old_checkpoints = original_journal["checkpoints"]
        if (staged_journal.get("change") != original_journal.get("change")
                or staged_journal.get("baseline") != original_journal.get("baseline")
                or staged_journal.get("checkpoints", [])[:-1] != old_checkpoints
                or staged_journal.get("checkpoints", [{}])[-1].get("id") != CHECKPOINT
                or staged_journal.get("sections", {}).get("trajectory", {}).get("run_id") != manifest["original_run_id"]):
            raise Blocked("native checkpoint did not append to the exact original journal")
        for key in ["intent", "title", "proof_hooks", "todo_ids", "constraints"]:
            if staged_journal["sections"]["plan"].get(key) != original_journal["sections"]["plan"].get(key):
                raise Blocked("correction changed the original approved plan contract: " + key)
        run("commit exact checkpointed correction", "git", "commit", "--signoff", "-m", manifest["commit_message"], cwd=work)
        head = run("bind corrected candidate", "git", "rev-parse", "HEAD", cwd=work).strip()
        result["head_sha"] = head
        if run("verify correction fast-forward parent", "git", "rev-list", "--parents", "-n", "1", head, cwd=work).split() != [head, SOURCE]:
            raise Blocked("correction is not an ordinary child of the preserved candidate")
        result["checkpoint_verification"] = verify_history(head)
        result["checkpoints"] = True
        verify_material("checkpointed correction")
        clean("verify clean checkpointed correction")
        fresh_target()
        remote_binding()
        result["source_push"] = dict(attempted=True, expected_remote_head=SOURCE, candidate=head,
                                     mode="normal_fast_forward", creates_pr=False, full_readiness=False)
        run("push exact corrected source branch", "git", "-c", "push.followTags=false", "-c", "push.recurseSubmodules=no",
            "push", "--porcelain", "origin", head + ":refs/heads/" + BRANCH, cwd=work)
        remote_binding(head)
        verify_material("pushed correction")
        clean("verify unchanged source after branch push")
        result.update(status="checkpointed_corrections_pushed_readiness_blocked", pushed=True,
                      observed_remote_head_sha=head, original_checkpoint_records=old_checkpoints,
                      next_action="Preserve both candidates and complete Dactyl native proof/release handoff; rerun full Decapod readiness only after its actual registry dependency supports the connector API")
    except (Blocked, OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError) as exc:
        result.setdefault("blocker", dict(stage=stage, kind=type(exc).__name__))
        result["blocker"]["reason"] = str(exc)
    finally:
        first_blocker = result.get("blocker")
        if work is not None:
            try:
                code, stdout, stderr, timeout = execute("observe final remote source branch",
                    ["git", "ls-remote", "origin", "refs/heads/" + BRANCH], cwd=work, timeout=120)
                fields = stdout.split()
                if not code and not timeout and len(fields) == 2 and fields[1] == "refs/heads/" + BRANCH and re.fullmatch(r"[0-9a-f]{40}", fields[0]):
                    result["observed_remote_head_sha"] = fields[0]
                    result["candidate_matches_observed_remote"] = fields[0] == result["head_sha"]
                    result["pushed"] = result.get("source_push", {}).get("attempted", False) and fields[0] == result["head_sha"]
                else:
                    result["remote_observation_error"] = dict(exit_code=code, timeout=timeout, reason="exact branch head unavailable")
            except (Blocked, OSError, ValueError, subprocess.SubprocessError) as exc:
                result["remote_observation_error"] = dict(kind=type(exc).__name__, reason=str(exc))
        if claimed:
            try:
                if (run("verify custody control branch", "git", "symbolic-ref", "--quiet", "HEAD").strip() != "refs/heads/" + control
                        or run("verify custody control base", "git", "rev-parse", "HEAD").strip() != BASE):
                    raise Blocked("private custody export context changed")
                run("yield exact native task", "decapod", "todo", "yield", "--id", TASK, "--reason", "Explicit transfer to " + RECEIVER + " before ephemeral runner teardown")
                transfer = private / "native-transfer.json"
                code, stdout, stderr, timeout = execute("export sealed native successor", ["decapod", "todo", "transfer", "export",
                    "--id", TASK, "--to", RECEIVER, "--output", str(transfer), "--format", "json"])
                if code or timeout:
                    raw = stdout + stderr
                    sealed = re.search(r"source sealed as ([0-9A-HJKMNP-TV-Z]{26}); artifact publication unconfirmed:", raw)
                    if (timeout or not sealed or re.search(r"AUTHORING_|SESSION_|SCOPE_VIOLATION|DECISION_GATE|POLICY_DENIED", raw)):
                        raise Blocked("native export failed without supported exact sealed-artifact recovery")
                    transfer = private / "native-transfer-recovered.json"
                    exported = doc("recover exact sealed successor", "todo", "transfer", "recover", "--id", TASK,
                        "--transfer-id", sealed[1], "--output", str(transfer), "--format", "json")
                else:
                    exported = json.loads(stdout)
                value = transfer.read_bytes()
                native = json.loads(value)
                if not (exported.get("status") == "ok" and exported.get("source_sealed") is True
                        and exported.get("artifact_sha256") == digest(value) and native["receiver"] == RECEIVER
                        and native["projection"]["task"]["id"] == TASK and native["source_commit"] == BASE
                        and native["source_branch"] == control and native["transfer_id"] == exported["transfer_id"]
                        and not any(secret.encode() in value for secret in sensitive)):
                    raise Blocked("exported successor custody binding differs")
                result["transfer"] = dict(task_id=TASK, transfer_id=native["transfer_id"], receiver=RECEIVER,
                    source_sealed=True, native_source_commit=BASE, native_source_branch=control,
                    candidate_branch=BRANCH, candidate_sha=result.get("head_sha"), **encrypt(transfer, "native-custody.der"))
                result["custody_exported"] = True
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["transfer_error"] = dict(stage=stage, kind=type(exc).__name__, reason=str(exc))
        if certificate_verified:
            try:
                if work is not None:
                    head = run("observe final source commit", "git", "rev-parse", "HEAD", cwd=work).strip()
                    result["final_head_sha"] = head
                    result["source_dirty"] = bool(run("observe final worktree status", "git", "status", "--porcelain", cwd=work).strip())
                    if head != BASE:
                        bundle = private / "candidate.bundle"
                        run("preserve complete candidate history", "git", "bundle", "create", str(bundle), "HEAD", "^" + BASE, cwd=work)
                        run("verify preserved candidate bundle", "git", "bundle", "verify", str(bundle), cwd=work)
                        result["candidate_bundle"] = encrypt(bundle, "candidate.der")
                    for label, argv, name in [
                        ("preserve pending tracked material", ["git", "diff", "--binary", "HEAD"], "pending-tracked.patch"),
                        ("preserve pending index", ["git", "diff", "--cached", "--binary", "HEAD"], "pending-index.patch"),
                    ]:
                        code, stdout, stderr, timeout = execute(label, argv, cwd=work)
                        if code or timeout:
                            raise Blocked("exact pending source preservation failed")
                        (diagnostics / name).write_text(stdout)
                    pending = set(run("inspect pending source files", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
                    for path in sorted(pending):
                        if path in PATCH_PATHS | GENERATED or path.startswith(SPEC):
                            source = work / path
                            if source.is_file() and not source.is_symlink() and source.stat().st_size <= 8 * 1024 * 1024:
                                destination = diagnostics / "untracked-source" / path
                                destination.parent.mkdir(parents=True, exist_ok=True)
                                destination.write_bytes(source.read_bytes())
                    (diagnostics / "prepared-hashes.json").write_text(json.dumps(prepared_hashes, indent=2))
                archive = private / "diagnostics.tar"
                with tarfile.open(archive, "w") as tar:
                    tar.add(diagnostics, arcname="diagnostics")
                result["diagnostics"] = encrypt(archive, "native-diagnostics.der")
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["preservation_error"] = dict(stage=stage, kind=type(exc).__name__, reason=str(exc))
        if first_blocker is not None:
            result["blocker"] = first_blocker
        (out / "result.json").write_text(json.dumps(scrub(result), indent=2) + "\n")
        print(json.dumps(scrub(result), indent=2), flush=True)
    return 0 if (result["status"] == "checkpointed_corrections_pushed_readiness_blocked"
                 and result["custody_exported"] and not result.get("preservation_error")) else 1


if __name__ == "__main__":
    raise SystemExit(main())
