"""Observe native task/workspace admission; retain encrypted custody, never publish."""
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
import urllib.request

RELEASE = "v0.138.2"
RELEASE_ID = 442229037
ASSET_ID = 737052855
ASSET_NAME = "decapod-x86_64-unknown-linux-gnu.tar.xz"
ASSET_SIZE = 8241216
ASSET_DIGEST = "718fa969ca3eb7372a7b31e72e8572e3517ac61f969adad03790928a9d03dc06"
CERT_DIGEST = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
BASES = {
    "decapod": ("DecapodLabs/decapod", "master", "c5de35a89b27e788169612ae71681e45c13bf71a", 1628),
    "dactyl": ("DecapodLabs/dactyl", "main", "1fa52a232e1d34a27be28d158298805ffc175ffc", 100),
}
REQUEST = "This must be solved. Do so now:\n\nhttps://github.com/DecapodLabs/decapod/issues/1628"
BOUNDARY = "Bootstrap the required native task and dedicated worktree for the client/Dactyl transport work. Do not modify product source, change policy, push product branches, publish PRs, merge, or claim validation. Preserve actual blockers and export only encrypted task custody before teardown."
TASK_INTENTS = {
    "decapod": "Implement issue #1628: logical local/cloud project selection through Dactyl; machine-private origin/repository-bound sessions and durable pending flows; serialized refresh and interruption recovery; server-confirmed account readiness; no cloud failure fallback to SQLite; offline client regression coverage and accurate proof boundaries. Published Dactyl package handoff remains an explicit dependency, not a waived release gate.",
    "dactyl": "Implement Dactyl issue #100 as the dependency for decapod #1628: an additive instance-scoped logical local/cloud connector, public discovery, supported managed authentication/onboarding/account transport and current-bearer native data connections. Preserve caller-owned private custody, strict origin isolation, bounded redacted transport, server-owned entitlement/policy, and separation of fixture evidence from hosted acceptance.",
}
EARLIER_BOOTSTRAPS = {
    "decapod": {"run_id": 43112618001, "task": "code_01m76bwvngbby68c"},
    "dactyl": {"run_id": 43112637243, "task": "feat_01m76bwjb7637nj6"},
}


class Blocked(RuntimeError):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--slug", choices=BASES, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--certificate", type=Path, required=True)
    args = parser.parse_args()
    repo, target, base, issue = BASES[args.slug]
    actor = "chatgpt-issue1628-" + args.slug
    receiver = actor + "-next"
    ops = "ops/issue1628-native-20261111"
    root = Path.cwd().resolve()
    out = args.output.resolve()
    if out.is_relative_to(root):
        parser.error("results must be outside source")
    os.umask(0o077)
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    # GitHub's mounted RUNNER_TEMP has host ownership. Native custody must
    # remain under this actual container user's private ancestry instead.
    private = Path(tempfile.mkdtemp(prefix="issue1628-private-", dir="/root"))
    diagnostics = private / "diagnostics"
    diagnostics.mkdir(mode=0o700)
    env = os.environ.copy()
    env["DECAPOD_AGENT_ID"] = actor
    env["DECAPOD_CLAIM_AUTORUN"] = "false"
    env["DECAPOD_SESSION_PASSWORD"] = secrets.token_hex(32)
    sensitive = [v for k, v in env.items() if re.search("token|password|secret|credential|authorization", k, re.I) and len(v) >= 8]
    result = dict(schema="decapod.issue1628-bootstrap.v1", slug=args.slug, repository=repo,
                  target=target, base=base, operations_sha=env.get("GITHUB_SHA"),
                  status="blocked", task=None, branch=None, admitted=False,
                  container_observed=False, custody_exported=False, publication_ready=False)
    result["earlier_bootstrap"] = dict(**EARLIER_BOOTSTRAPS[args.slug],
        disposition="historical admission only; no product changes or remote branch; ephemeral runner ended after yield and failed custody export; this is a new task, not restored custody or inherited proof")
    controller = None
    controller_digest = None
    work = None
    task = None
    control = None
    claimed = False
    certificate_verified = False
    stage = "preconditions"

    def scrub(value):
        if isinstance(value, str):
            for secret in sensitive:
                value = value.replace(secret, "[REDACTED]")
            return re.sub(r"(?:github_pat_|gh[pousr]_)[A-Za-z0-9_]{20,}|(?i:bearer)\s+\S+", "[REDACTED]", value)
        if isinstance(value, dict):
            return {k: scrub(v) for k, v in value.items() if not re.search("password|secret|credential|authorization|session_token|access_token", k, re.I)}
        if isinstance(value, list):
            return [scrub(v) for v in value]
        return value

    def execute(label, argv, cwd=None, data=None, timeout=240):
        nonlocal stage
        stage = label
        if argv[0] == "decapod":
            if controller is None or digest(controller.read_bytes()) != controller_digest:
                raise Blocked("released controller identity changed")
            argv = [str(controller), *argv[1:]]
        proc = subprocess.Popen(argv, cwd=cwd or root, env=env,
                                stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = proc.communicate(data, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                stdout, stderr = proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                stdout, stderr = proc.communicate()
        name = re.sub(r"[^a-z0-9-]", "-", label.lower())
        (diagnostics / (name + ".stdout.txt")).write_text(scrub(stdout))
        (diagnostics / (name + ".stderr.txt")).write_text(scrub(stderr))
        return proc.returncode, stdout, stderr, timed_out

    def run(label, *argv, cwd=None, data=None, timeout=240):
        code, stdout, stderr, timed_out = execute(label, list(argv), cwd, data, timeout)
        if code or timed_out:
            result["blocker"] = dict(stage=label, exit_code=code, timeout=timed_out,
                                     diagnostic=scrub((stderr or stdout)[-6000:]))
            raise Blocked("native or Git command failed")
        return stdout

    def doc(label, *argv, cwd=None, data=None):
        value = json.loads(run(label, "decapod", *argv, cwd=cwd, data=data))
        if isinstance(value, dict) and value.get("error"):
            result["blocker"] = dict(stage=label, diagnostic=scrub(value["error"]))
            raise Blocked("native operation returned error")
        return value

    def encrypt(source, name):
        if not certificate_verified:
            raise Blocked("custody recipient is unverified")
        destination = out / name
        run("encrypt " + name, "openssl", "cms", "-encrypt", "-binary", "-aes-256-cbc",
            "-in", str(source), "-out", str(destination), "-outform", "DER", str(args.certificate.resolve()))
        return dict(artifact=name, plaintext_sha256=digest(source.read_bytes()),
                    ciphertext_sha256=digest(destination.read_bytes()))

    try:
        if (env.get("GITHUB_ACTIONS") != "true" or env.get("GITHUB_REPOSITORY") != repo
                or env.get("GITHUB_REF") != "refs/heads/" + ops):
            raise Blocked("requires exact hosted repository and operations branch")
        if not Path("/.dockerenv").is_file():
            raise Blocked("bootstrap requires the actual configured GitHub job container")
        result["container_observed"] = True
        fingerprint = run("verify custody recipient", "openssl", "x509", "-in", str(args.certificate.resolve()),
                          "-noout", "-fingerprint", "-sha256").strip().split("=", 1)[1].replace(":", "").lower()
        if fingerprint != CERT_DIGEST:
            raise Blocked("custody certificate mismatch")
        certificate_verified = True
        url = "https://api.github.com/repos/DecapodLabs/decapod/releases/tags/" + RELEASE
        headers = {"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}
        with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=30) as response:
            release = json.load(response)
        matches = [a for a in release["assets"] if a["name"] == ASSET_NAME]
        if release["id"] != RELEASE_ID or release["tag_name"] != RELEASE or release["draft"] or len(matches) != 1:
            raise Blocked("official release metadata mismatch")
        asset = matches[0]
        download = "https://github.com/DecapodLabs/decapod/releases/download/" + RELEASE + "/" + ASSET_NAME
        if (asset["id"] != ASSET_ID or asset["size"] != ASSET_SIZE or asset["digest"] != "sha256:" + ASSET_DIGEST
                or asset["browser_download_url"] != download or asset["state"] != "uploaded"):
            raise Blocked("official release asset mismatch")
        archive = private / ASSET_NAME
        with urllib.request.urlopen(download, timeout=60) as response, archive.open("wb") as handle:
            while block := response.read(1024 * 1024):
                handle.write(block)
                if handle.tell() > ASSET_SIZE:
                    raise Blocked("release asset exceeded declared bound")
        if archive.stat().st_size != ASSET_SIZE or digest(archive.read_bytes()) != "sha256:" + ASSET_DIGEST:
            raise Blocked("official release digest mismatch")
        with tarfile.open(archive) as tar:
            members = [m for m in tar.getmembers() if m.isfile() and Path(m.name).name == "decapod"]
            if len(members) != 1:
                raise Blocked("release has unexpected executable inventory")
            controller = private / "decapod"
            controller.write_bytes(tar.extractfile(members[0]).read())
            controller.chmod(0o700)
        controller_digest = digest(controller.read_bytes())
        env["PATH"] = str(private) + os.pathsep + env["PATH"]
        run("ingest released native orientation", "decapod", "docs", "ingest")
        evaluation = doc("evaluate task request", "eval", "--stdin", "--format", "json", data=REQUEST + "\n\n" + BOUNDARY)
        if evaluation.get("status") != "allow":
            raise Blocked("native prompt evaluation did not allow task")
        result["evaluation"] = evaluation
        result["controller"] = dict(release=RELEASE, release_id=RELEASE_ID, asset_id=ASSET_ID,
                                    archive_sha256="sha256:" + ASSET_DIGEST, binary_sha256=controller_digest)
        if run("verify source", "git", "rev-parse", "HEAD").strip() != base:
            raise Blocked("checkout is not exact accepted base")
        if run("verify clean source", "git", "status", "--porcelain").strip():
            raise Blocked("checkout is not clean")
        if run("verify current target", "git", "ls-remote", "origin", "refs/heads/" + target).split() != [base, "refs/heads/" + target]:
            raise Blocked("accepted target changed")
        control = "issue1628-control-" + args.slug + "-" + secrets.token_hex(6)
        run("name private control checkout", "git", "switch", "-c", control, base)
        run("acquire native session", "decapod", "session", "acquire")
        initialized = doc("initialize native control plane", "rpc", "--op", "agent.init")
        result["reconciliation"] = initialized.get("result", {}).get("reconciliation")
        run("read native capabilities", "decapod", "capabilities", "--format", "json")
        run("inspect native workspaces", "decapod", "workspace", "status")
        title = "Route cloud initialization and account readiness through Dactyl" if args.slug == "decapod" else "Add logical connector and cloud auth/account transport for decapod"
        history = "Earlier ephemeral bootstrap " + json.dumps(result["earlier_bootstrap"], sort_keys=True)
        task_doc = doc("create native task", "todo", "add", title, "--description", REQUEST + "\n\n" + TASK_INTENTS[args.slug] + "\n\n" + history,
                       "--tags", "code", "--priority", "high", "--ref", f"https://github.com/{repo}/issues/{issue}", "--format", "json")
        task = task_doc["id"]
        result["task"] = task
        claim = doc("claim native task", "todo", "claim", "--id", task, "--lease-seconds", "3600", "--format", "json")
        if claim.get("status") != "ok":
            raise Blocked("native claim not confirmed")
        claimed = True
        branch = "agent/" + actor + "/" + task + "-cloud-connector"
        result["branch"] = branch
        admission = doc("admit isolated worktree", "workspace", "ensure", "--branch", branch)
        result["admission"] = admission
        if admission.get("can_work") is not True or admission.get("branch") != branch:
            raise Blocked("native workspace admission did not allow task")
        work = Path(admission["worktree_path"]).resolve()
        if run("verify workspace base", "git", "rev-parse", "HEAD", cwd=work).strip() != base:
            raise Blocked("native workspace starts at wrong base")
        run("orient native task", "decapod", "infer", "orientation", "--task-id", task, cwd=work)
        run("read core doctrine", "decapod", "constitution", "get", "core/DECAPOD", cwd=work)
        doc("resolve task context", "rpc", "--op", "context.resolve", cwd=work)
        current = doc("inspect governed workstream", "govern", "artifacts", "status", cwd=work)
        run_id = current["sections"]["trajectory"]["run_id"]
        doc("record bootstrap custody gap", "govern", "trajectory", "record", "--run-id", run_id,
            "--task-id", task, "--evidence", history,
            "--check", "full-workbranch-validation=unavailable", cwd=work)
        result.update(status="admitted_no_implementation", admitted=True)
    except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        result.setdefault("blocker", dict(stage=stage, type=type(exc).__name__, reason=str(exc)))
    finally:
        first_blocker = result.get("blocker")
        if claimed:
            try:
                run("yield native custody", "decapod", "todo", "yield", "--id", task,
                    "--reason", "Transfer this task to " + receiver + " before ephemeral bootstrap teardown")
                transfer = private / "native-transfer.json"
                code, stdout, stderr, timed_out = execute("export native custody", ["decapod", "todo", "transfer", "export", "--id", task,
                                                         "--to", receiver, "--output", str(transfer), "--format", "json"])
                if code or timed_out:
                    combined = stdout + "\n" + stderr
                    sealed = re.search(r"source sealed as ([0-9A-HJKMNP-TV-Z]{26}); artifact publication unconfirmed:", combined)
                    if timed_out or sealed is None:
                        raise Blocked("native export failed without supported sealed-artifact recovery")
                    transfer = private / "native-transfer-recovered.json"
                    exported = doc("recover exact sealed custody", "todo", "transfer", "recover", "--id", task,
                                   "--transfer-id", sealed[1], "--output", str(transfer), "--format", "json")
                else:
                    exported = json.loads(stdout)
                body = transfer.read_bytes()
                native = json.loads(body)
                if (exported.get("status") != "ok" or exported.get("source_sealed") is not True
                        or exported.get("artifact_sha256") != digest(body)
                        or native["receiver"] != receiver or native["projection"]["task"]["id"] != task
                        or native["source_commit"] != base or native["source_branch"] != control
                        or any(secret.encode() in body for secret in sensitive)):
                    raise Blocked("native custody identity or secret exclusion check failed")
                result["transfer"] = dict(task_id=task, transfer_id=exported["transfer_id"], receiver=receiver,
                                          source_sealed=True, native_source_commit=base, native_source_branch=control,
                                          **encrypt(transfer, "native-custody.der"))
                result["custody_exported"] = True
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["transfer_error"] = dict(stage=stage, type=type(exc).__name__, reason=str(exc))
        if certificate_verified:
            try:
                if work is not None:
                    run("preserve generated tracked projections", "git", "diff", "--binary", "HEAD", cwd=work)
                    result["workspace_status"] = run("inspect final worktree", "git", "status", "--porcelain", cwd=work)
                archive = private / "diagnostics.tar"
                with tarfile.open(archive, "w") as tar:
                    tar.add(diagnostics, arcname="diagnostics")
                result["diagnostics"] = encrypt(archive, "native-diagnostics.der")
            except (Blocked, OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
                result["preservation_error"] = dict(stage=stage, type=type(exc).__name__, reason=str(exc))
        if first_blocker is not None:
            result["blocker"] = first_blocker
        (out / "result.json").write_text(json.dumps(scrub(result), indent=2) + "\n")
        print(json.dumps(scrub(result), indent=2))
    return 0 if result["admitted"] and result["custody_exported"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
