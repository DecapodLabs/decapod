#!/usr/bin/env python3
"""Author the reviewed billing residual in native custody and run its native publisher."""
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

REPO = "DecapodLabs/decapod"
BASE = "c5de35a89b27e788169612ae71681e45c13bf71a"
SOURCE = "26a499d9f0789a5ed00605d23cfc57f5392ae8fc"
OPS = "ops/stale-billing-publication-20261111"
ACTOR = "chatgpt-stale-billing"
RECEIVER = "chatgpt-stale-billing-next"
CONTROLLER = "22f26c69e7f6de5f813c11ee21d8fc2a5776b6a1ce477e13d835a9e210c69217"
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
    parser.add_argument("--patch", type=Path, required=True)
    parser.add_argument("--custody-recipient", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--publish-timeout", type=int, default=3600)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    paths = set(manifest["paths"])
    allowed_sources = set(manifest["allowed_source_changes"])
    reviews = manifest["spec_reviews"]
    expected_tree = manifest.get("expected_material_tree", "")
    reviewed_files = {entry["path"]: entry for entry in manifest.get("files", [])}
    if (manifest.get("repository") != REPO or manifest.get("base_sha") != BASE
            or manifest.get("source_sha") != SOURCE or manifest.get("slug") != "billing"
            or not paths or not allowed_sources <= paths
            or not re.fullmatch(r"sha256:[0-9a-f]{64}", manifest.get("patch_sha256", ""))
            or not re.fullmatch(r"[0-9a-f]{40}", expected_tree) or set(reviewed_files) != paths
            or not 60 <= args.publish_timeout <= 5400):
        parser.error("exact repository, source/base, scoped patch digest and bounded timeout required")
    for path in paths:
        if (path.startswith("/") or ".." in Path(path).parts or "\0" in path
                or path in GENERATED or path == ".decapod/config.toml"
                or (path.startswith(".decapod/") and not (path.startswith(SPEC_PREFIX) and path.endswith(".md")))
                or path.startswith((".github/", ".buildkite/"))):
            parser.error("patch cannot contain control state, protected CI carriers, or unsafe paths")
    if (set(reviews) != {"ARCHITECTURE", "INTERFACES", "SECURITY"}
            or any(v.get("disposition") not in {"updated", "unchanged-with-reason"}
                   or not v.get("reason", "").strip() for v in reviews.values())
            or SPEC_PREFIX + "VALIDATION.md" not in paths):
        parser.error("material authored VALIDATION and three current spec reviews are required")
    for key in ("title", "intent", "commit_message", "body"):
        if not isinstance(manifest.get(key), str) or not manifest[key].strip():
            parser.error("manifest requires nonempty " + key)
    root, out = Path.cwd().resolve(), args.output.resolve()
    if out.is_relative_to(root):
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
    stage, work, task, branch, control = "preconditions", None, None, None, None
    claimed = cert_verified = False
    controller = None
    spec_files = set()
    reviewed_inventory_digest = None

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
        # The expected tree is independently reviewed BASE + exact patch, not
        # a tree derived from the candidate under inspection. Compare all paths
        # so native projection refresh cannot silently alter accepted source.
        native = GENERATED | spec_files | {SPEC_PREFIX + ".manifest.json", ".decapod/config.toml"}
        delta = set(run(label + " complete tracked tree", "git", "diff", "--no-renames", "--name-only", "-z", expected_tree, cwd=work).split("\0")) - {""}
        if not delta <= native:
            raise Blocked("candidate differs from independently reviewed complete material tree: " + ", ".join(sorted(delta - native)))
        if run(label + " untracked material", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work):
            raise Blocked("untracked material remains outside complete reviewed tree")
        for path in sorted(spec_files):
            expected = run(label + " reviewed spec " + Path(path).stem, "git", "show", expected_tree + ":" + path, cwd=work)
            source = work / path
            if source.is_symlink() or not source.is_file():
                raise Blocked("authored specification is not a regular file")
            if BLOCKS.sub("", source.read_text()).strip() != BLOCKS.sub("", expected).strip():
                raise Blocked("authored spec semantics differ from review: " + path)
            if run(label + " spec mode " + Path(path).stem, "git", "ls-files", "--stage", "--", path, cwd=work).split()[0] != "100644":
                raise Blocked("authored spec file mode differs from review")
        expected_config = run(label + " accepted policy", "git", "show", expected_tree + ":.decapod/config.toml", cwd=work)
        if reviewed_inventory_digest is not None:
            expected_config, count = re.subn(r'(?m)^reviewed_sources_digest = "sha256:[0-9a-f]{64}"$',
                                             'reviewed_sources_digest = "' + reviewed_inventory_digest + '"', expected_config)
            if count != 1:
                raise Blocked("accepted policy lacks exact bounded inventory declaration")
        if (work / ".decapod/config.toml").read_text() != expected_config:
            raise Blocked("policy differs from accepted policy and explicitly measured inventory review")
        result.setdefault("material_verification", []).append(dict(stage=label, expected_material_tree=expected_tree,
            complete_non_native_tree_equal=True, authored_spec_semantics_equal=True, policy_changes_bounded=True))

    try:
        if (env.get("GITHUB_ACTIONS") != "true" or env.get("BUILDKITE") == "true"
                or env.get("GITHUB_REPOSITORY") != REPO or env.get("GITHUB_REF") != "refs/heads/" + OPS):
            raise Blocked("requires the exact authorized hosted operations context")
        located = shutil.which("decapod", path=env.get("PATH"))
        if not located:
            raise Blocked("official controller unavailable")
        controller = Path(located).resolve()
        if hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
            raise Blocked("controller differs from verified official v0.138.1")
        result["controller"] = dict(release="v0.138.1", binary_sha256=CONTROLLER)
        run("ingest native instructions", "decapod", "docs", "ingest")
        evaluation = doc("evaluate user request", "eval", "--stdin", "--format", "json",
                         data=REQUEST + "\n\nReviewed residual intent:\n" + manifest["intent"])
        result["prompt_evaluation"] = evaluation
        if evaluation.get("status") != "allow":
            raise Blocked("native evaluation did not admit this request")
        fingerprint = run("verify operator custody certificate", "openssl", "x509", "-in", str(args.custody_recipient.resolve()), "-noout", "-fingerprint", "-sha256").strip().split("=", 1)[-1].replace(":", "").lower()
        if fingerprint != CERT:
            raise Blocked("operator custody certificate identity mismatch")
        cert_verified = True
        result["custody_recipient_fingerprint"] = "sha256:" + fingerprint
        patch_bytes = args.patch.read_bytes()
        if not 0 < len(patch_bytes) <= 8 * 1024 * 1024 or digest(patch_bytes) != manifest["patch_sha256"]:
            raise Blocked("reviewed patch digest or size differs")
        patch = patch_bytes.decode("utf-8")
        result["patch_sha256"] = digest(patch_bytes)
        if run("verify accepted checkout", "git", "rev-parse", "HEAD").strip() != BASE or run("verify clean control checkout", "git", "status", "--porcelain").strip():
            raise Blocked("source checkout must start clean at exact accepted master")
        if run("verify repository origin", "git", "remote", "get-url", "origin").strip() not in ("https://github.com/" + REPO, "https://github.com/" + REPO + ".git"):
            raise Blocked("unexpected repository origin")
        fresh_target()
        if run("refuse duplicate billing publication", "git", "ls-remote", "--heads", "origin", "refs/heads/agent/" + ACTOR + "/*").strip():
            raise Blocked("a billing publication branch already exists; inspect and resume its original exported custody instead of creating another task")
        run("verify historical source exists", "git", "cat-file", "-e", SOURCE + "^{commit}")
        observed_paths = {entry.split("\t", 2)[2] for entry in run("inspect reviewed patch paths", "git", "apply", "--numstat", "-z", data=patch).split("\0") if entry}
        if observed_paths != paths:
            raise Blocked("patch paths differ from independently reviewed scope")
        control = "billing-publication-control-" + secrets.token_hex(6)
        run("name control checkout", "git", "switch", "-c", control, BASE)
        run("acquire native session", "decapod", "session", "acquire")
        baseline = reconciliation(doc("initialize native control plane", "rpc", "--op", "agent.init"), "baseline_reconciliation", True)
        run("discover native capabilities", "decapod", "capabilities", "--format", "json")
        run("inspect native workspaces", "decapod", "workspace", "status")
        task = doc("create native billing task", "todo", "add", manifest["title"], "--description", manifest["intent"],
                   "--tags", "code", "--priority", "high", "--ref", "https://github.com/" + REPO + "/commit/" + SOURCE, "--format", "json")["id"]
        result["task"] = task
        if not re.fullmatch(r"[a-z]+_[a-z0-9]+", task):
            raise Blocked("native task ID has unexpected shape")
        claim = doc("claim native billing task", "todo", "claim", "--id", task, "--lease-seconds", str(args.publish_timeout + 1800), "--format", "json")
        if claim.get("status") != "ok":
            raise Blocked("native claim was not confirmed")
        claimed = True
        branch = "agent/" + ACTOR + "/" + task + "-cloud-approval"
        result["branch"] = branch
        admitted = doc("admit isolated billing workspace", "workspace", "ensure", "--branch", branch)
        if admitted.get("can_work") is not True or admitted.get("branch") != branch:
            raise Blocked("native workspace admission did not authorize the new branch")
        work = Path(admitted["worktree_path"]).resolve()
        if run("verify admitted base", "git", "rev-parse", "HEAD", cwd=work).strip() != BASE:
            raise Blocked("native workspace does not start at reviewed accepted base")
        run("orient native billing task", "decapod", "infer", "orientation", "--task-id", task, cwd=work)
        run("read core doctrine", "decapod", "constitution", "get", "core/DECAPOD", cwd=work)
        doc("resolve current task context", "rpc", "--op", "context.resolve", cwd=work)
        code, stdout, stderr, timeout = execute("inspect historical ancestry", ["git", "merge-base", "--is-ancestor", SOURCE, BASE], cwd=work)
        if timeout or code not in (0, 1):
            raise Blocked("historical source ancestry could not be inspected")
        if code == 0:
            result["source_history"] = "accepted_source"
        else:
            try:
                doc("record native history recovery", "govern", "artifacts", "record-history-recovery", "--source-ref", SOURCE,
                    "--reason", "Preserve historical branch provenance and observed checkpoint gaps. Reauthor only the independently reviewed residual under this fresh task; no prior ownership, task continuity or proof is inherited.", cwd=work)
                result["source_history"] = "observed_gap_recorded"
            except Blocked:
                if result.get("blocker", {}).get("codes") != ["HISTORY_RECOVERY_NOT_APPLICABLE"]:
                    raise
                old_base = run("resolve historical merge base", "git", "merge-base", BASE, SOURCE, cwd=work).strip()
                proof = doc("verify historical checkpoint lineage", "govern", "artifacts", "verify-checkpoints", "--base-branch", old_base, "--head-ref", SOURCE, "--target-base", "master", "--format", "json", cwd=work)
                if not (proof.get("history_proven") is True and proof.get("inspection_complete") is True and proof.get("findings") == []):
                    raise Blocked("historical source is neither recoverable nor independently proven")
                result.pop("blocker", None)
                result["source_history"] = "already_proven"
        doc("record reviewed billing plan", "govern", "plan", "update", "--title", manifest["title"], "--intent", manifest["intent"],
            "--todo-id", task, "--proof-hook", "decapod workspace publish", "--proof-hook",
            "decapod govern artifacts verify-checkpoints --base-branch " + BASE + " --head-ref HEAD --target-base master --format json",
            "--file-touch-budget", str(len(paths) + 20), cwd=work)
        doc("accept user-authorized billing plan", "govern", "plan", "approve", cwd=work)
        doc("record falsifiable billing claim", "govern", "artifacts", "claim", "--id", "claim:billing-cloud-approval",
            "--statement", manifest["intent"], "--falsifier", manifest.get("falsifier", "Unreviewed source changes, unapproved cloud action, dropped local/cloud boundary, missing current tests, stale target or any missing native readiness evidence."),
            "--status", "open", "--proof-ref", SPEC_PREFIX + "VALIDATION.md", cwd=work)
        # Materialize the independently reviewed proposal in an isolated Git
        # index. It does not move any branch, edit source, or carry proof.
        review_env = dict(env, GIT_INDEX_FILE=str(private / "review-material.index"))
        for label, argv, data in [
            ("read immutable material baseline", ["git", "read-tree", BASE], None),
            ("apply exact proposal to isolated index", ["git", "apply", "--cached", "--whitespace=error", "-"], patch),
            ("compute reviewed complete tree", ["git", "write-tree"], None),
        ]:
            observed = subprocess.run(argv, cwd=work, env=review_env, input=data, text=True, capture_output=True, timeout=120)
            if observed.returncode:
                raise Blocked(label + " failed")
        if observed.stdout.strip() != expected_tree:
            raise Blocked("exact patch does not produce independently reviewed complete material tree")
        for path, reviewed in reviewed_files.items():
            entry = run("verify reviewed material blob " + path, "git", "ls-tree", expected_tree, "--", path, cwd=work).split()
            if len(entry) != 4 or entry[:3] != [reviewed["mode"], "blob", reviewed["after_blob"]]:
                raise Blocked("proposal tree differs from source review blob/mode binding")
        spec_files = {p for p in run("enumerate reviewed canonical specs", "git", "ls-tree", "-r", "--name-only", expected_tree, "--", SPEC_PREFIX, cwd=work).splitlines() if p.endswith(".md")}
        result["expected_material_tree"] = expected_tree
        native_blocks = {path: [m.group(0) for m in BLOCKS.finditer((work / path).read_text())]
                         for path in paths if path.startswith(SPEC_PREFIX)}
        run("check reviewed residual patch", "git", "apply", "--check", "-", cwd=work, data=patch)
        run("apply reviewed residual patch", "git", "apply", "-", cwd=work, data=patch)
        # Authored Markdown is allowed; no proposal may rewrite generated blocks.
        for path in sorted(paths):
            if path.startswith(SPEC_PREFIX):
                after = (work / path).read_text()
                if native_blocks[path] != [m.group(0) for m in BLOCKS.finditer(after)]:
                    raise Blocked("reviewed authored spec patch changes generated blocks")
        candidate = reconciliation(doc("inspect candidate source inventory", "rpc", "--op", "agent.init", cwd=work), "candidate_reconciliation", True)
        differences = []
        for path in sorted(set(candidate["discovered_sources"]) | set(baseline["discovered_sources"])):
            # Preserve exact file bytes when comparing the native SHA-256
            # inventory; text-mode newline conversion must not alter a digest.
            original = subprocess.run(["git", "show", BASE + ":" + path], cwd=work, env=env,
                                      capture_output=True, timeout=240)
            if original.returncode != 0 and path in baseline["discovered_sources"]:
                raise Blocked("cannot read an accepted inventoried source")
            old_digest = digest(original.stdout) if original.returncode == 0 else None
            if candidate["discovered_sources"].get(path) != old_digest:
                differences.append(path)
        if not set(differences) <= allowed_sources:
            raise Blocked("candidate source-inventory changes exceed reviewed allowed paths")
        result["inventory_review"] = dict(differences=differences, allowed_source_changes=sorted(allowed_sources),
                                           inventory_digest=candidate["inventory_digest"], carrier_approval=False, execution_proof=False)
        reviewed_config = set()
        if candidate.get("status") != "proof_pending":
            if not differences:
                raise Blocked("stale source declaration has no measured reviewed source change; no repin allowed")
            config = work / ".decapod/config.toml"
            updated, count = re.subn(r'(?m)^reviewed_sources_digest = "sha256:[0-9a-f]{64}"$',
                                     'reviewed_sources_digest = "' + candidate["inventory_digest"] + '"', config.read_text())
            if count != 1:
                raise Blocked("reviewed declaration is not uniquely identifiable")
            config.write_text(updated)
            reviewed_config.add(".decapod/config.toml")
            reviewed_inventory_digest = candidate["inventory_digest"]
        reconciliation(doc("verify reviewed source inventory", "rpc", "--op", "agent.init", cwd=work), "final_reconciliation")
        doc("refresh native release projections", "rpc", "--op", "specs.refresh", cwd=work)
        for name, review in reviews.items():
            doc("record spec review " + name, "govern", "plan", "review-spec", "--path", SPEC_PREFIX + name + ".md",
                "--disposition", review["disposition"], "--reason", review["reason"], cwd=work)
        run("verify whitespace", "git", "diff", "--check", cwd=work)
        run_id = doc("inspect native trajectory", "govern", "artifacts", "status", cwd=work)["sections"]["trajectory"]["run_id"]
        doc("record actual preparation evidence", "govern", "trajectory", "record", "--run-id", run_id, "--task-id", task,
            "--phase", "validation", "--scope", "billing", "--boundary", "Reviewed billing residual under fresh native custody; no historical proof or completed readiness claimed",
            "--command", "git apply independently reviewed patch " + manifest["patch_sha256"], "--command", "decapod rpc --op specs.refresh",
            "--check", "git-diff-whitespace=passed", "--check", "full-workbranch-validation=unavailable",
            "--evidence", "Native session, task, claim and workspace admitted. Historical source " + SOURCE + " recorded as " + result["source_history"] + ". Complete native publication remains required on the checkpointed candidate.", cwd=work)
        changed = set(run("inspect prepared tracked paths", "git", "diff", "HEAD", "--name-only", "-z", cwd=work).split("\0")) - {""}
        changed |= set(run("inspect prepared untracked paths", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
        if any(p not in paths | GENERATED | reviewed_config and not p.startswith(SPEC_PREFIX) for p in changed):
            raise Blocked("prepared source exceeds reviewed authored/generated scope")
        run("stage reviewed native candidate", "git", "add", "--", *sorted(changed), cwd=work)
        run("verify staged whitespace", "git", "diff", "--cached", "--check", cwd=work)
        verify_material("after native projection refresh")
        doc("checkpoint prepared material", "govern", "artifacts", "checkpoint", "--id", "billing-cloud-approval",
            "--summary", "Preserve reviewed historical billing residual, corrected for current cloud/local authorization boundary; newly authored task and native projections, with full readiness still pending.",
            "--proof-ref", SPEC_PREFIX + "VALIDATION.md", cwd=work)
        run("stage native checkpoint", "git", "add", ".decapod/governance.json", cwd=work)
        verify_material("after native checkpoint")
        run("commit checkpointed candidate", "git", "commit", "--signoff", "-m", manifest["commit_message"], cwd=work)
        head = run("bind exact candidate", "git", "rev-parse", "HEAD", cwd=work).strip()
        result.update(head_sha=head, tree=run("bind exact candidate tree", "git", "rev-parse", "HEAD^{tree}", cwd=work).strip())
        proof = doc("verify exact checkpoint lineage", "govern", "artifacts", "verify-checkpoints", "--base-branch", BASE, "--head-ref", head, "--target-base", "master", "--format", "json", cwd=work)
        if not (proof.get("history_proven") is True and proof.get("inspection_complete") is True and proof.get("findings") == [] and proof.get("base_commit") == BASE and proof.get("head_commit") == head):
            raise Blocked("exact candidate checkpoint lineage is not proven")
        result.update(checkpoints=True, checkpoint_verification=proof)
        clean("verify checkpointed candidate cleanliness")
        verify_material("exact committed candidate")
        archive = private / "candidate.bundle"
        run("preserve checkpointed source", "git", "bundle", "create", str(archive), "HEAD", "^" + BASE, cwd=work)
        run("verify source preservation bundle", "git", "bundle", "verify", str(archive), cwd=work)
        result["candidate_bundle"] = encrypt(archive, "billing-candidate.der")
        fresh_target()
        # Source validation freezes the release history too. Read all existing
        # remote tags without forcing, pruning, or creating any remote tag.
        run("refresh complete remote tag history", "git", "-c", "fetch.prune=false", "-c", "fetch.pruneTags=false", "fetch", "--no-force", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--no-prune", "--no-prune-tags", "--refmap=", "origin", "refs/tags/*:refs/tags/*", cwd=work)
        remote_tags = sorted(run("observe remote tag history", "git", "ls-remote", "--tags", "origin", cwd=work).splitlines())
        local_tags = sorted(run("observe local tag history", "git", "show-ref", "--tags", "--dereference", cwd=work).splitlines())
        # show-ref uses a space; ls-remote uses a tab.
        if sorted(tuple(line.split()) for line in remote_tags) != sorted(tuple(line.split()) for line in local_tags):
            raise Blocked("local tag history differs from current remote history")
        if sorted(run("recheck stable remote tag history", "git", "ls-remote", "--tags", "origin", cwd=work).splitlines()) != remote_tags:
            raise Blocked("remote release history changed before native validation")
        result["tag_history_verified"] = True
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
                and re.fullmatch(r"https://github.com/DecapodLabs/decapod/pull/[0-9]+", published.get("pr_url", ""))):
            result["publisher"]["diagnostic_excerpt"] = scrub(stderr[-8000:])
            raise Blocked("native publisher did not confirm complete readiness and exact PR")
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
