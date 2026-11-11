"""Continue the exact sealed client task; preserve the actual dependency blocker."""
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
import tomllib
import urllib.request

REPO = "DecapodLabs/decapod"
BASE = "c5de35a89b27e788169612ae71681e45c13bf71a"
SOURCE = BASE
TASK = "code_01m76c1kqhjgr3yr"
BRANCH = "agent/chatgpt-issue1628-decapod/" + TASK + "-cloud-connector"
ACTOR = "chatgpt-issue1628-decapod-next"
RECEIVER = "chatgpt-issue1628-decapod"
OPS = "ops/issue1628-native-20261111"
TRANSFER = "01M76C2BDC2SWT2V852BA93XWY"
INCOMING = "sha256:c1734174448bdd416beffba9f08ec5827aec991197493b2f49d20f2fd5804f95"
CONTROLLER = "31bd8e3983dd328a1796da8af827647be691982c91d7ec7a7b11c7fdc2b4d723"
CERT = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
SPEC = ".decapod/managed/specs/"
GENERATED = {"AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md", ".decapod/governance.json", ".decapod/managed/Dockerfile.decapod"}
BLOCKS = re.compile(r"<!-- decapod:([A-Za-z0-9:_-]+):start -->.*?<!-- decapod:\1:end -->", re.S)
REQUEST = "This must be solved. Do so now: https://github.com/DecapodLabs/decapod/issues/1628"
CODES = r"\b(?:AUTHORING_|SESSION_|WORKSPACE_|SCOPE_|GOVERNANCE_|HISTORY_|TASK_TRANSFER_|READINESS_|PUBLICATION_|CONTROLLER_|REPOSITORY_|PR_|DECISION_GATE|POLICY_DENIED)[A-Z0-9_]*\b"


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
            or manifest.get("branch") != BRANCH or out.is_relative_to(root)):
        parser.error("requires exact task, source/base and separate result directory")
    files = {x["path"]: x for x in manifest["files"]}
    if (not files or len(files) != len(manifest["files"])
            or any(p.startswith("/") or ".." in Path(p).parts or p in GENERATED
                   or p.startswith(".decapod/") for p in files)):
        parser.error("reviewed material cannot contain runtime or generated governance")
    out.mkdir(parents=True, mode=0o700, exist_ok=False)
    private = Path(tempfile.mkdtemp(prefix="issue1628-decapod-private-", dir="/root"))
    diagnostics = private / "diagnostics"
    diagnostics.mkdir(mode=0o700)
    env = dict(os.environ, DECAPOD_AGENT_ID=ACTOR, DECAPOD_CLAIM_AUTORUN="false",
               DECAPOD_SESSION_PASSWORD=secrets.token_hex(32), DECAPOD_VALIDATE_TIMEOUT_SECS="600")
    sensitive = [v for k, v in env.items() if re.search("token|password|secret|credential|authorization", k, re.I) and len(v) >= 8]
    result = dict(schema="decapod.issue1628-client-continuation.v1", repository=REPO,
                  base=BASE, source=SOURCE, task=TASK, branch=BRANCH,
                  operations_sha=env.get("GITHUB_SHA"), status="blocked", imported=False,
                  admitted=False, container_observed=False, checkpoints=False,
                  publication_ready=False, full_boundary_passed=False, custody_exported=False,
                  head_sha=None, pr_url=None, pushed=False)
    stage, work, control, controller = "preconditions", None, None, None
    claimed = certificate_verified = False
    prepared_hashes, authored_specs = {}, {}

    def scrub(value):
        if isinstance(value, str):
            for secret in sensitive:
                value = value.replace(secret, "[REDACTED]")
            return re.sub(r"(?:github_pat_|gh[pousr]_)[A-Za-z0-9_]{20,}|(?i:bearer)\s+\S+|https?://[^/\s:@]+:[^/@\s]+@", "[REDACTED]", value)
        if isinstance(value, dict):
            return {k: scrub(v) for k, v in value.items() if not re.search("password|secret|credential|authorization|session_token|access_token", k, re.I)}
        if isinstance(value, list):
            return [scrub(x) for x in value]
        return value

    def execute(label, argv, cwd=None, data=None, timeout=240):
        nonlocal stage
        stage = label
        if argv[0] == "decapod":
            if controller is None or hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
                raise Blocked("official controller identity changed")
            argv = [str(controller), *argv[1:]]
        print(json.dumps({"stage": label}), flush=True)
        proc = subprocess.Popen(argv, cwd=cwd or root, env=env, text=True, start_new_session=True,
                                stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        timed_out = False
        try:
            stdout, stderr = proc.communicate(data, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                stdout, stderr = proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                stdout, stderr = proc.communicate()
        stem = re.sub(r"[^a-z0-9-]", "-", label.lower())
        (diagnostics / (stem + ".stdout.txt")).write_text(scrub(stdout))
        (diagnostics / (stem + ".stderr.txt")).write_text(scrub(stderr))
        return proc.returncode, stdout, stderr, timed_out

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

    def fresh_target():
        if run("verify current accepted target", "git", "ls-remote", "origin", "refs/heads/master").split() != [BASE, "refs/heads/master"]:
            raise Blocked("accepted target changed; source review and full validation required")

    def reconciliation(value):
        report = value.get("result", {}).get("reconciliation")
        if not isinstance(report, dict):
            raise Blocked("native initialization omitted required reconciliation")
        return report

    def verify_candidate(label):
        for p, expected in prepared_hashes.items():
            f = work / p
            if f.is_symlink() or not f.is_file() or digest(f.read_bytes()) != expected:
                raise Blocked(label + ": prepared material changed: " + p)
        for p, expected in authored_specs.items():
            if BLOCKS.sub("", (work / p).read_text()).strip() != expected:
                raise Blocked(label + ": authored specification changed: " + p)
        changed = set(run(label + " tracked scope", "git", "diff", "--name-only", "-z", BASE, cwd=work).split("\0")) - {""}
        changed |= set(run(label + " untracked scope", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
        if any(p not in set(files) | GENERATED | {".decapod/config.toml"} and not p.startswith(SPEC) for p in changed):
            raise Blocked(label + ": source exceeds reviewed product/proof/generated scope")

    try:
        if (env.get("GITHUB_ACTIONS") != "true" or env.get("BUILDKITE") == "true"
                or env.get("GITHUB_REPOSITORY") != REPO or env.get("GITHUB_REF") != "refs/heads/" + OPS
                or not Path("/.dockerenv").is_file()):
            raise Blocked("requires exact authorized repository and actual hosted container")
        result["container_observed"] = True
        fingerprint = run("verify operator custody certificate", "openssl", "x509", "-in", str(args.certificate.resolve()),
                          "-noout", "-fingerprint", "-sha256").strip().split("=", 1)[1].replace(":", "").lower()
        if fingerprint != CERT:
            raise Blocked("operator custody certificate differs")
        certificate_verified = True
        # Reuse the reviewed metadata/digest-checked installer, never a candidate build.
        installer = material.parent / "install_controller.py"
        run("install exact official released controller", "/usr/bin/python3.12", str(installer), str(private))
        controller = private / "decapod"
        if hashlib.sha256(controller.read_bytes()).hexdigest() != CONTROLLER:
            raise Blocked("released controller bytes differ")
        env["PATH"] = str(private) + os.pathsep + env["PATH"]
        result["controller"] = dict(release="v0.138.2", binary_sha256=CONTROLLER)
        run("ingest native doctrine", "decapod", "docs", "ingest")
        evaluation = doc("evaluate exact requested work", "eval", "--stdin", "--format", "json", data=REQUEST + "\n\n" + manifest["intent"])
        if evaluation.get("status") != "allow":
            raise Blocked("native request evaluation did not allow this work")
        result["evaluation"] = evaluation
        if run("verify accepted checkout", "git", "rev-parse", "HEAD").strip() != BASE or run("verify clean checkout", "git", "status", "--porcelain").strip():
            raise Blocked("initial source is not exact clean accepted master")
        if run("verify repository origin", "git", "remote", "get-url", "origin").strip() != "https://github.com/" + REPO + ".git":
            raise Blocked("unexpected source origin")
        fresh_target()
        if run("verify task branch remains unpublished", "git", "ls-remote", "origin", "refs/heads/" + BRANCH).strip():
            raise Blocked("task branch exists remotely; recover its exact candidate instead of initial bootstrap")
        run("verify historical source", "git", "cat-file", "-e", SOURCE + "^{commit}")
        native_bytes = args.native_transfer.read_bytes()
        native = json.loads(native_bytes)
        if (digest(native_bytes) != INCOMING or native["transfer_id"] != TRANSFER
                or native["receiver"] != ACTOR or native["projection"]["task"]["id"] != TASK
                or native["source_commit"] != BASE or native["source_branch"] != manifest["incoming_source_branch"]):
            raise Blocked("sealed incoming task identity differs")
        for p, entry in files.items():
            f = material / p
            if f.is_symlink() or not f.is_file() or f.stat().st_size != entry["bytes"] or digest(f.read_bytes()) != entry["sha256"]:
                raise Blocked("reviewed proposal digest differs: " + p)
        control = "issue1628-decapod-continuation-" + secrets.token_hex(6)
        run("name private control checkout", "git", "switch", "-c", control, BASE)
        run("acquire native session", "decapod", "session", "acquire")
        baseline = reconciliation(doc("initialize accepted control plane", "rpc", "--op", "agent.init"))
        if baseline["discovered_sources"] != manifest["accepted_sources"]:
            raise Blocked("accepted source inventory differs from independent review")
        if any(r.get("owner") != "agent" or r.get("code") not in {"STALE_SOURCE_INVENTORY", "REQUIRED_PROOF_NOT_RUN"} for r in baseline["repair_tasks"]):
            raise Blocked("native initial adoption contains an unreviewed decision or access gate")
        result["baseline_reconciliation"] = baseline
        imported = doc("import exact sealed task custody", "todo", "transfer", "import", "--input", str(args.native_transfer.resolve()),
                       "--expected-digest", INCOMING, "--accept-source-transfer", "--format", "json")
        if imported.get("status") != "ok" or imported.get("task_id") != TASK:
            raise Blocked("native task import was not confirmed")
        result["imported"] = True
        claim = doc("claim original native task", "todo", "claim", "--id", TASK, "--lease-seconds", "10800", "--format", "json")
        if claim.get("status") != "ok":
            raise Blocked("native task claim was not confirmed")
        claimed = True
        admitted = doc("readmit original governed workspace", "workspace", "ensure", "--branch", BRANCH)
        if admitted.get("can_work") is not True or admitted.get("branch") != BRANCH:
            raise Blocked("native workspace admission failed")
        work = Path(admitted["worktree_path"]).resolve()
        result["admitted"] = True
        if run("verify workspace source base", "git", "rev-parse", "HEAD", cwd=work).strip() != BASE:
            raise Blocked("readmitted workspace has unexpected source")
        run("orient original task", "decapod", "infer", "orientation", "--task-id", TASK, cwd=work)
        run("read native core doctrine", "decapod", "constitution", "get", "core/DECAPOD", cwd=work)
        doc("resolve task context", "rpc", "--op", "context.resolve", cwd=work)
        result["source_history"] = "new_work_on_accepted_base"
        doc("record reviewed connector plan", "govern", "plan", "update", "--title", manifest["title"], "--intent", manifest["intent"],
            "--todo-id", TASK, "--proof-hook", "decapod workspace publish", "--file-touch-budget", str(len(files) + 24), cwd=work)
        doc("approve already authorized plan", "govern", "plan", "approve", cwd=work)
        doc("record falsifiable connector claim", "govern", "artifacts", "claim", "--id", "claim:decapod-cloud-client",
            "--statement", manifest["intent"], "--falsifier", manifest["falsifier"], "--status", "open", "--proof-ref", SPEC + "VALIDATION.md", cwd=work)
        for p in sorted(files):
            destination = work / p
            if destination.is_symlink():
                raise Blocked("reviewed proposal cannot overwrite a symlink")
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((material / p).read_bytes())
            destination.chmod(0o644)
        for p, prose in manifest["spec_additions"].items():
            if not p.startswith(SPEC) or not p.endswith(".md") or "decapod:" in prose:
                raise Blocked("spec proposal may contain authored prose only")
            f = work / p
            previous = f.read_text()
            title = re.search(r"(?m)^# [^\n]*\n", previous)
            if not title:
                raise Blocked("spec lacks reviewed heading insertion point")
            rewritten = previous[:title.end()] + "\n" + prose + "\n" + previous[title.end():]
            if [m.group(0) for m in BLOCKS.finditer(previous)] != [m.group(0) for m in BLOCKS.finditer(rewritten)]:
                raise Blocked("authored spec proposal changes native blocks")
            f.write_text(rewritten)
        # Formatting is preparation only. Full validation owns all builds/tests.
        run("format reviewed client source", "/usr/local/rustup/toolchains/1.96.1-x86_64-unknown-linux-gnu/bin/cargo", "fmt", "--all", cwd=work)
        candidate = reconciliation(doc("inspect exact reviewed source inventory", "rpc", "--op", "agent.init", cwd=work))
        if candidate["discovered_sources"] != manifest["candidate_sources"]:
            raise Blocked("candidate source/CI inventory differs from independent accepted-source review")
        # This accepted master already has a stale declaration. Rehash every
        # inventoried immutable blob; no workflow/provider/carrier changes occur.
        for path, expected in manifest["accepted_sources"].items():
            observed = subprocess.run(["git", "show", BASE + ":" + path], cwd=work, env=env,
                                      capture_output=True, timeout=120, check=True).stdout
            if digest(observed) != expected:
                raise Blocked("independent accepted source digest differs: " + path)
        source_digest = candidate["inventory_digest"]
        if source_digest != manifest["reviewed_source_inventory_digest"]:
            raise Blocked("reviewed complete source inventory digest differs")
        config = work / ".decapod/config.toml"
        accepted_config = run("read accepted publication policy", "git", "show", BASE + ":.decapod/config.toml", cwd=work)
        if config.read_text() != accepted_config:
            raise Blocked("unreviewed publication policy change")
        updated, count = re.subn(r'(?m)^reviewed_sources_digest = "sha256:2aea2eef7fb20d5343821996f637449a9afe7aea0020b56355702218838e5e80"$',
                                 'reviewed_sources_digest = "' + source_digest + '"', accepted_config)
        if count != 1:
            raise Blocked("accepted stale inventory declaration is not uniquely identifiable")
        config.write_text(updated)
        final = reconciliation(doc("verify unchanged required source-CI profile", "rpc", "--op", "agent.init", cwd=work))
        if final.get("status") != "proof_pending" or any(r["code"] != "REQUIRED_PROOF_NOT_RUN" for r in final["repair_tasks"]):
            result["final_reconciliation"] = final
            raise Blocked("required source-CI profile still has a remediation")
        result["profile_review"] = dict(inventory_digest=source_digest,
            unchanged_profile="decapod.pr-readiness.v1", provider="Buildkite", source_inventory_verified=True,
            execution_proof=False, policy_change="exact existing stale inventory declaration only")
        result["source_reviewed_dependency_expectation"] = dict(
            evidence_basis="Reviewed client source and accepted registry pin; candidate compilation has not run",
            **manifest["dependency_blocker"])
        result["preparation_validation"] = "not_run"
        doc("refresh native source projections", "rpc", "--op", "specs.refresh", cwd=work)
        for name in ["ARCHITECTURE", "INTERFACES", "SECURITY"]:
            review = manifest["spec_reviews"][name]
            doc("record current spec review " + name.lower(), "govern", "plan", "review-spec", "--path", SPEC + name + ".md",
                "--disposition", review["disposition"], "--reason", review["reason"], cwd=work)
        run("verify prepared source whitespace", "git", "diff", "--check", cwd=work)
        trajectory = doc("inspect native trajectory", "govern", "artifacts", "status", cwd=work)["sections"]["trajectory"]["run_id"]
        doc("record actual preparation evidence", "govern", "trajectory", "record", "--run-id", trajectory, "--task-id", TASK,
            "--phase", "validation", "--scope", "decapod-cloud-client", "--boundary", "Reviewed managed client and private custody source; configured Buildkite proof remains required and Dactyl registry API availability requires native confirmation",
            "--command", "cargo fmt --all", "--command", "decapod rpc --op specs.refresh",
            "--check", "source-review=passed", "--check", "full-workbranch-proof-result=unavailable",
            "--evidence", "Exact source " + SOURCE + "; original sealed task imported and readmitted. Full native validation has not run; unavailable describes the absent proof result, not an observed provider failure. The accepted source-CI contract and release dependency policy remain intact; no unpublished source overlay is substituted.", cwd=work)
        prepared_hashes = {p: digest((work / p).read_bytes()) for p in files}
        prepared_hashes[".decapod/config.toml"] = digest((work / ".decapod/config.toml").read_bytes())
        authored_specs = {str(p.relative_to(work)): BLOCKS.sub("", p.read_text()).strip() for p in (work / SPEC).glob("*.md")}
        verify_candidate("prepared candidate")
        run("stage reviewed candidate", "git", "add", "-A", cwd=work)
        doc("checkpoint exact governed client source", "govern", "artifacts", "checkpoint", "--id", "decapod-cloud-client-1628",
            "--summary", "Implement Dactyl-routed managed cloud coordination and private caller custody with durable uncertainty, session reuse and current authorized read readiness. Full source-CI readiness remains pending the published Dactyl dependency; no release-policy exception is introduced.",
            "--proof-ref", SPEC + "VALIDATION.md", cwd=work)
        run("stage native checkpoint", "git", "add", ".decapod/governance.json", cwd=work)
        run("commit exact checkpointed candidate", "git", "commit", "--signoff", "-m", manifest["commit_message"], cwd=work)
        head = run("bind checkpointed candidate", "git", "rev-parse", "HEAD", cwd=work).strip()
        result["head_sha"] = head
        proof = doc("verify exact checkpoint history", "govern", "artifacts", "verify-checkpoints", "--base-branch", BASE,
                    "--head-ref", head, "--target-base", "master", "--format", "json", cwd=work)
        if not (proof.get("history_proven") and proof.get("inspection_complete") and proof.get("findings") == []
                and proof.get("base_commit") == BASE and proof.get("head_commit") == head):
            raise Blocked("candidate checkpoint lineage is not proven")
        result.update(checkpoints=True, checkpoint_verification=proof)
        verify_candidate("committed candidate")
        if run("verify clean checkpointed worktree", "git", "status", "--porcelain", cwd=work).strip():
            raise Blocked("checkpointed candidate is dirty")
        fresh_target()
        run("refresh exact remote tag history", "git", "-c", "fetch.prune=false", "-c", "fetch.pruneTags=false", "fetch", "--no-force", "--no-tags", "--no-write-fetch-head", "--no-recurse-submodules", "--no-prune", "--no-prune-tags", "--refmap=", "origin", "refs/tags/*:refs/tags/*", cwd=work)
        remote_tags = sorted(tuple(x.split()) for x in run("observe current tags", "git", "ls-remote", "--tags", "origin", cwd=work).splitlines())
        local_tags = sorted(tuple(x.split()) for x in run("observe local tags", "git", "show-ref", "--tags", "--dereference", cwd=work).splitlines())
        if remote_tags != local_tags:
            raise Blocked("local and remote release history differ")
        body = manifest["body"].replace("{{HEAD_SHA}}", head).replace("{{BASE_SHA}}", BASE)
        code, stdout, stderr, timeout = execute("native full workspace publisher", ["decapod", "workspace", "publish", "--title", manifest["title"], "--description", body], cwd=work, timeout=7200)
        result["publisher"] = dict(exit_code=code, timeout=timeout, ci_identity_preserved=True, codes=sorted(set(re.findall(CODES, stdout + stderr))))
        try:
            published = json.loads(stdout)
        except ValueError:
            published = None
        if not (code == 0 and not timeout and isinstance(published, dict) and published.get("status") == "ok"
                and published.get("remote_verified") is True and published.get("pr_verified") is True
                and published.get("branch") == BRANCH and published.get("commit_hash") == head
                and re.fullmatch(r"https://github.com/DecapodLabs/decapod/pull/[0-9]+", published.get("pr_url", ""))):
            result["blocker"] = dict(stage=stage, diagnostic=scrub((stderr or stdout)[-24000:]))
            raise Blocked("full native publisher did not confirm exact readiness and PR")
        result.update(status="published_after_native_full_boundary", publication_ready=True, full_boundary_passed=True,
                      pr_url=published["pr_url"], pushed=True, publication=published)
        fresh_target()
        if run("verify exact remote candidate", "git", "ls-remote", "origin", "refs/heads/" + BRANCH, cwd=work).split() != [head, "refs/heads/" + BRANCH]:
            raise Blocked("remote candidate no longer matches verified publication")
        verify_candidate("after full native publisher")
    except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        result.setdefault("blocker", dict(stage=stage, kind=type(exc).__name__))
        result["blocker"]["reason"] = str(exc)
    finally:
        first_blocker = result.get("blocker")
        # The configured Buildkite route may publish the candidate transport
        # before its complete proof succeeds. Observe that independent fact;
        # it neither grants readiness nor permits PR creation.
        if work is not None and result.get("head_sha"):
            try:
                code, stdout, stderr, timeout = execute(
                    "observe final remote candidate", ["git", "ls-remote", "origin", "refs/heads/" + BRANCH],
                    cwd=work, timeout=120)
                if code or timeout:
                    result["remote_observation_error"] = dict(exit_code=code, timeout=timeout)
                else:
                    fields = stdout.split()
                    if not fields:
                        result["observed_remote_head_sha"] = None
                        result["pushed"] = False
                    elif (len(fields) == 2 and re.fullmatch(r"[0-9a-f]{40}", fields[0])
                            and fields[1] == "refs/heads/" + BRANCH):
                        result["observed_remote_head_sha"] = fields[0]
                        result["pushed"] = fields[0] == result["head_sha"]
                    else:
                        result["remote_observation_error"] = dict(reason="unexpected remote ref response")
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["remote_observation_error"] = dict(kind=type(exc).__name__, reason=str(exc))
        if claimed:
            try:
                if (run("verify custody control branch", "git", "symbolic-ref", "--quiet", "HEAD").strip() != "refs/heads/" + control
                        or run("verify custody source base", "git", "rev-parse", "HEAD").strip() != BASE):
                    raise Blocked("private export source changed")
                run("yield exact native task", "decapod", "todo", "yield", "--id", TASK, "--reason", "Explicit transfer to " + RECEIVER + " before ephemeral runner teardown")
                transfer = private / "native-transfer.json"
                code, stdout, stderr, timeout = execute("export sealed native custody", ["decapod", "todo", "transfer", "export", "--id", TASK, "--to", RECEIVER, "--output", str(transfer), "--format", "json"])
                if code or timeout:
                    sealed = re.search(r"source sealed as ([0-9A-HJKMNP-TV-Z]{26}); artifact publication unconfirmed:", stdout + stderr)
                    if timeout or not sealed:
                        raise Blocked("native export failed without supported exact recovery")
                    transfer = private / "native-transfer-recovered.json"
                    exported = doc("recover exact sealed custody", "todo", "transfer", "recover", "--id", TASK, "--transfer-id", sealed[1], "--output", str(transfer), "--format", "json")
                else:
                    exported = json.loads(stdout)
                b = transfer.read_bytes()
                n = json.loads(b)
                if not (exported.get("status") == "ok" and exported.get("source_sealed") is True
                        and exported.get("artifact_sha256") == digest(b) and n["receiver"] == RECEIVER
                        and n["projection"]["task"]["id"] == TASK and n["source_commit"] == BASE
                        and n["source_branch"] == control and n["transfer_id"] == exported["transfer_id"]
                        and not any(secret.encode() in b for secret in sensitive)):
                    raise Blocked("exported native custody binding differs")
                result["transfer"] = dict(task_id=TASK, transfer_id=n["transfer_id"], receiver=RECEIVER,
                    source_sealed=True, native_source_commit=BASE, native_source_branch=control,
                    candidate_branch=BRANCH, candidate_sha=result.get("head_sha"), **encrypt(transfer, "native-custody.der"))
                result["custody_exported"] = True
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["transfer_error"] = dict(stage=stage, kind=type(exc).__name__, reason=str(exc))
        if certificate_verified:
            try:
                if work is not None:
                    final_head = run("observe final source", "git", "rev-parse", "HEAD", cwd=work).strip()
                    result["final_head_sha"] = final_head
                    result["source_dirty"] = bool(run("observe final worktree", "git", "status", "--porcelain", cwd=work).strip())
                    if final_head != BASE:
                        bundle = private / "candidate.bundle"
                        run("preserve exact candidate history", "git", "bundle", "create", str(bundle), "HEAD", "^" + BASE, cwd=work)
                        run("verify candidate preservation", "git", "bundle", "verify", str(bundle), cwd=work)
                        result["candidate_bundle"] = encrypt(bundle, "candidate.der")
                    execute("preserve pending tracked material", ["git", "diff", "--binary", "HEAD"], cwd=work)
                    execute("preserve pending index", ["git", "diff", "--cached", "--binary", "HEAD"], cwd=work)
                    pending = set(run("inspect pending source paths", "git", "ls-files", "--others", "--exclude-standard", "-z", cwd=work).split("\0")) - {""}
                    for p in sorted(pending):
                        if p in set(files) | GENERATED or p.startswith(SPEC):
                            f = work / p
                            if f.is_file() and not f.is_symlink() and f.stat().st_size <= 8 * 1024 * 1024:
                                destination = diagnostics / "untracked-source" / p
                                destination.parent.mkdir(parents=True, exist_ok=True)
                                destination.write_bytes(f.read_bytes())
                    (diagnostics / "prepared-hashes.json").write_text(json.dumps(prepared_hashes, indent=2))
                archive = private / "diagnostics.tar"
                with tarfile.open(archive, "w") as tar:
                    tar.add(diagnostics, arcname="diagnostics")
                result["diagnostics"] = encrypt(archive, "native-diagnostics.der")
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["preservation_error"] = dict(stage=stage, kind=type(exc).__name__, reason=str(exc))
        if first_blocker is not None:
            result["blocker"] = first_blocker
        if result["publication_ready"] and (not result["custody_exported"] or result.get("blocker")):
            result["status"] = "published_with_observation_or_custody_blocker"
        (out / "result.json").write_text(json.dumps(scrub(result), indent=2) + "\n")
        print(json.dumps(scrub(result), indent=2), flush=True)
    return 0 if result["status"] == "published_after_native_full_boundary" and result["custody_exported"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
