#!/usr/bin/env python3
"""Prepare a native coordination container; import original billing custody only inside it."""
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

BASE = "c5de35a89b27e788169612ae71681e45c13bf71a"
REPO = "DecapodLabs/decapod"
OPS = "ops/stale-billing-publication-20261111"
CONTROLLER = "31bd8e3983dd328a1796da8af827647be691982c91d7ec7a7b11c7fdc2b4d723"
OCI_BINARY = "dd30b7516c331ac0a6c3128f7a9c433833935bc9f380d311b188502037f857e0"
OCI_INDEX = "sha256:38ff07bbbaf33643f7dcb1b98ba7e4352a67e9f6e40e0a21f1988dfec804ade2"
OCI_MANIFEST = "sha256:aaca38cd94450d512f85415e529e0176c0dab2168cc135602ff601d6514aefe4"
OCI_CONFIG = "sha256:89b8305bb9de658d420c10b75b281b30f5f02f598116be58fe506f9a6daeb416"
OCI_REPOSITORY = "ghcr.io/decapodlabs/decapod"
OCI_IMAGE = OCI_REPOSITORY + "@" + OCI_INDEX
OCI_TAG = OCI_REPOSITORY + ":v0.138.2-debian"
OCI_LAYERS = ["sha256:1d7b9dd8778ebc4d9bf2f64436dd358640718cb349f5640b33bebaea4ffd62c2",
              "sha256:70a8643c19a81cf1fb923662b68df5b476057f3612dec0b32eb5c48347fec3d1",
              "sha256:eb2a4a74344cf17b72044dffabfff586c77132371f3b2f02eda721ab1e9c5a98",
              "sha256:a5a62c72b3529e15955cfc56c1d755a03a82daf8eb4ef07d6822aec850f61398",
              "sha256:5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef"]
HOST_CONTROLLER_IDENTITY = dict(release="v0.138.2", distribution="github_release", binary_sha256=CONTROLLER,
    archive_sha256="718fa969ca3eb7372a7b31e72e8572e3517ac61f969adad03790928a9d03dc06",
    release_id=442229037, asset_id=737052855, release_tag_commit=BASE)
OCI_CONTROLLER_IDENTITY = dict(release="v0.138.2", distribution="oci", binary_sha256=OCI_BINARY, binary_bytes=29185240,
    image_repository=OCI_REPOSITORY, image_index_digest=OCI_INDEX, platform_manifest_digest=OCI_MANIFEST,
    image_config_digest=OCI_CONFIG, platform=dict(os="linux", architecture="amd64"), release_tag_commit=BASE,
    source_workflow_run_id=43106957801, source_workflow_job_id=130812709187)
CERT = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
COORDINATOR = "chatgpt-billing-container-preparation"
ASSETS = ("container_billing.py", "container_entry.sh", "continue_billing.py", "billing-continuation.json", "gate-repair.patch", "release-generated.patch", "custody-recipient.pem")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def quote(value):
    return "'" + str(value).replace("'", "'\\''") + "'"


def safe_path(value):
    path = Path(value).resolve()
    require(path.is_absolute() and not any(ord(c) < 32 or c in ':,\"' for c in str(path)), "Unsupported native mount path")
    return path


def provider(env):
    require(env.get("GITHUB_ACTIONS") == "true" and env.get("BUILDKITE") != "true", "Actual GitHub execution identity is required")
    require(env.get("GITHUB_REPOSITORY") == REPO and env.get("GITHUB_REF") == "refs/heads/" + OPS, "Wrong authorized operations context")


def mapped_id(number, entries):
    matches = [item["host_id"] + number - item["container_id"] for item in entries
               if item["container_id"] <= number < item["container_id"] + item["size"]]
    require(len(matches) == 1, "Runtime namespace mapping is missing or ambiguous")
    return matches[0]


def kernel_map(name):
    entries = []
    for line in Path("/proc/self/" + name).read_text().splitlines():
        fields = line.split()
        require(len(fields) == 3 and all(value.isdecimal() for value in fields), "Invalid kernel namespace map")
        entries.append(dict(zip(("container_id", "host_id", "size"), map(int, fields))))
    return entries


def verify_runtime(state):
    """Observe the actual engine/kernel identity; never create an isolation marker."""
    root = safe_path(state["root"])
    identity = dict(engine=state["engine"], hostname=os.uname().nodename,
                    effective_uid=os.geteuid(), effective_gid=os.getegid(),
                    repository_owner_uid=root.stat().st_uid, repository_owner_gid=root.stat().st_gid,
                    host_uid=state["uid"], host_gid=state["gid"], actual_dockerenv=Path("/.dockerenv").is_file())
    require(identity["effective_uid"] == identity["repository_owner_uid"] and identity["effective_gid"] == identity["repository_owner_gid"], "Container process does not own the repository in its actual namespace")
    if state["engine"] == "docker":
        require(identity["actual_dockerenv"] and os.geteuid() == state["uid"] != 0 and os.getegid() == state["gid"], "Docker owner mapping is not the actual non-root runner")
        return identity
    require(state["engine"] == "podman" and state["rootless_host"] is True, "Unexpected or rootful native runtime")
    require(os.geteuid() == os.getegid() == 0 and state["uid"] != 0 and state["gid"] != 0, "Podman must retain its default rootless owner mapping")
    uid_map, gid_map = kernel_map("uid_map"), kernel_map("gid_map")
    require(mapped_id(0, uid_map) == state["uid"] and mapped_id(0, gid_map) == state["gid"], "Kernel namespace owner does not map to the actual host runner")
    require(mapped_id(0, state["host_id_mappings"]["uidmap"]) == state["uid"] and mapped_id(0, state["host_id_mappings"]["gidmap"]) == state["gid"], "Observed rootless engine mapping differs from the host runner")
    marker = Path("/run/.containerenv")
    require(marker.is_file() and not marker.is_symlink() and marker.stat().st_size <= 65536, "Real Podman container marker is unavailable")
    cid = Path(state["cidfile"]).read_text().strip()
    require(re.fullmatch(r"[0-9a-f]{64}", cid) and identity["hostname"] == cid[:12], "Engine-issued container ID does not match the actual private-UTS hostname")
    marker_text = marker.read_text()
    marker_ids = re.findall(r'^id=(.*)$', marker_text, re.M)
    require(len(marker_ids) <= 1 and (not marker_ids or marker_ids[0] in (cid, '"' + cid + '"')), "Optional Podman marker ID disagrees with its engine")
    identity.update(actual_containerenv=True, container_id=cid, marker_id=cid if marker_ids else None,
                    marker_metadata_present=bool(marker_text.strip()), rootless_host=True,
                    uid_map=uid_map, gid_map=gid_map, host_id_mappings=state["host_id_mappings"])
    return identity


def inside(context):
    state = json.loads(context.read_text())
    root, assets, runtime = (safe_path(state[k]) for k in ("root", "assets", "runtime"))
    observed_runtime = verify_runtime(state)
    for name, expected in state["asset_digests"].items():
        require(name in ASSETS and sha((assets / name).read_bytes()) == expected, "Read-only reviewed runtime asset changed")
    controller = safe_path(state["controller"])
    require(controller == Path("/usr/local/bin/decapod") and controller.is_file() and os.access(controller, os.X_OK)
            and controller.stat().st_size == OCI_CONTROLLER_IDENTITY["binary_bytes"]
            and hashlib.sha256(controller.read_bytes()).hexdigest() == OCI_BINARY, "Official immutable OCI child controller identity differs")
    require(state["child_controller"] == OCI_CONTROLLER_IDENTITY and state["host_controller"] == HOST_CONTROLLER_IDENTITY, "Host and child official distributions are not honestly bound")
    env = os.environ.copy()
    env.update(state["ci_environment"])
    if state["engine"] == "podman":
        # This is the observed engine ID, never an invented detector flag.
        env["CONTAINER_ID"] = observed_runtime["container_id"]
    provider(env)
    require(sha(Path(env["GITHUB_EVENT_PATH"]).read_bytes()) == state["event_sha256"], "Actual workflow event metadata changed")
    # Match the native container API's private home contract; no host home is mounted.
    child_home = Path("/tmp/decapod-home")
    child_home.mkdir(mode=0o700, exist_ok=False)
    env["HOME"] = str(child_home)
    env["GIT_CONFIG_GLOBAL"] = str(child_home / ".gitconfig")
    env["PATH"] = str(controller.parent) + ":" + env.get("PATH", "/usr/local/bin:/usr/bin:/bin")
    os.chdir(root)
    command = [sys.executable, str(assets / "continue_billing.py"),
               "--manifest", str(assets / "billing-continuation.json"),
               "--repair-patch", str(assets / "gate-repair.patch"),
               "--release-drift", str(assets / "release-generated.patch"),
               "--resume-private-dir", state["incoming"],
               "--custody-recipient", str(assets / "custody-recipient.pem"),
               "--container-context", str(context),
               "--output", str(runtime / "result"), "--publish-timeout", str(state["publish_timeout"])]
    # All original-task mutation, failure preservation, and native export occurs here.
    return subprocess.run(command, env=env, check=False).returncode


def host(args):
    os.umask(0o077)
    root, assets, incoming, runtime = (safe_path(value) for value in (Path.cwd(), args.assets, args.incoming, args.runtime))
    env = os.environ.copy()
    provider(env)
    require(os.geteuid() != 0 and os.geteuid() == root.stat().st_uid, "Runner must own the accepted source checkout")
    require(not runtime.is_relative_to(root) and not incoming.is_relative_to(runtime), "Runtime and private ingress must be separate from source")
    require(60 <= args.publish_timeout <= 5400, "Publication deadline is outside reviewed bounds")
    runtime.mkdir(mode=0o700, exist_ok=False)
    private, public = runtime / "private", runtime / "bootstrap-public"
    private.mkdir(mode=0o700); public.mkdir(mode=0o700)
    report = dict(schema="decapod.billing-native-container.v1", status="blocked", original_task_imported_on_host=False,
                  original_task_exported_on_host=False, operations_sha=env["GITHUB_SHA"], uid=os.geteuid(), gid=os.getegid())
    label, cert_verified = "preconditions", False
    env["DECAPOD_AGENT_ID"] = COORDINATOR
    env["DECAPOD_SESSION_PASSWORD"] = secrets.token_hex(32)
    env["DECAPOD_CLAIM_AUTORUN"] = "false"

    def run(name, argv, cwd=None, data=None, timeout=240, allow_failure=False):
        nonlocal label
        label = name
        process = subprocess.Popen(argv, cwd=cwd or root, env=env, stdin=subprocess.PIPE if data is not None else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(data, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate()
        stem = re.sub(r"[^a-z0-9-]", "-", name.lower())
        (private / (stem + ".stdout.txt")).write_text(stdout)
        (private / (stem + ".stderr.txt")).write_text(stderr)
        if process.returncode or timed_out:
            report["blocker"] = dict(stage=name, exit_code=process.returncode, timeout=timed_out, full_output="encrypted bootstrap diagnostics")
            require(allow_failure, "Native container operation did not complete successfully")
        return 124 if timed_out else process.returncode, stdout

    def text(name, *argv, **kw):
        return run(name, list(argv), **kw)[1]

    def native(name, *argv, **kw):
        require(hashlib.sha256(controller.read_bytes()).hexdigest() == CONTROLLER, "Official controller changed")
        return text(name, str(controller), *argv, **kw)

    try:
        located = shutil.which("decapod", path=env.get("PATH"))
        require(located is not None, "Verified official controller missing")
        controller = Path(located).resolve()
        require(hashlib.sha256(controller.read_bytes()).hexdigest() == CONTROLLER, "Controller differs from accepted official release")
        report["host_controller"] = HOST_CONTROLLER_IDENTITY
        certificate = assets / "custody-recipient.pem"
        observed = text("verify bootstrap encryption recipient", "openssl", "x509", "-in", str(certificate), "-noout", "-fingerprint", "-sha256").split("=", 1)[-1].strip().replace(":", "").lower()
        require(observed == CERT, "Operator custody certificate differs")
        cert_verified = True
        require(text("verify accepted source", "git", "rev-parse", "HEAD").strip() == BASE, "Accepted source changed")
        require(not text("verify clean source before preparation", "git", "status", "--porcelain").strip(), "Source checkout is not clean")
        native("ingest native coordination instructions", "docs", "ingest")
        prompt = "Prepare only a native workspace coordination container so the original transferred billing task code_01m76aq15fea61e8 can be imported and continued inside it. Use the official released controller, actual GitHub identity, private native database snapshot and repository owner UID. Do not import or claim the original task on the host. The user requested closing unneeded stale branches and making PRs for remaining useful work; full native publication and custody preservation remain mandatory."
        evaluation = json.loads(native("evaluate exact container coordination", "eval", "--stdin", "--format", "json", data=prompt))
        require(evaluation.get("status") == "allow", "Native evaluation did not allow coordination")
        native("acquire separate coordination session", "session", "acquire")
        initialized = json.loads(native("initialize native coordination", "rpc", "--op", "agent.init"))
        require(not initialized.get("error"), "Native coordination initialization failed")
        native("discover native coordination capabilities", "capabilities", "--format", "json")
        expected_engine = "podman" if shutil.which("podman", path=env.get("PATH")) else "docker"
        require(shutil.which(expected_engine, path=env.get("PATH")) is not None, "Native container engine is absent")
        index = json.loads(text("inspect immutable official OCI index", expected_engine, "manifest", "inspect", OCI_IMAGE))
        platform = [entry for entry in index.get("manifests", []) if entry.get("platform") == {"architecture":"amd64", "os":"linux"}]
        require(len(platform) == 1 and platform[0].get("digest") == OCI_MANIFEST, "Official index does not bind the accepted linux-amd64 platform")
        text("pull immutable official OCI release", expected_engine, "pull", OCI_IMAGE, timeout=600)
        pinned = json.loads(text("inspect immutable official OCI platform", expected_engine, "image", "inspect", OCI_IMAGE))
        require(isinstance(pinned, list) and len(pinned) == 1, "Immutable official image inspection is ambiguous")
        pinned = pinned[0]
        image_id = pinned.get("Id", "")
        if re.fullmatch(r"[0-9a-f]{64}", image_id):
            image_id = "sha256:" + image_id
        labels = pinned.get("Config", {}).get("Labels", {})
        require(image_id == OCI_CONFIG and pinned.get("Architecture") == "amd64" and pinned.get("Os") == "linux"
                and pinned.get("RootFS", {}).get("Layers") == OCI_LAYERS
                and labels.get("org.opencontainers.image.revision") == BASE
                and labels.get("org.opencontainers.image.version") == "v0.138.2-debian"
                and labels.get("org.opencontainers.image.source") == "https://github.com/" + REPO,
                "Pinned official image platform, source, release or rootfs identity differs")
        checksum = text("verify official OCI controller bytes without source execution", expected_engine, "run", "--rm", "--read-only", "--network", "none", "--entrypoint", "/usr/bin/sha256sum", OCI_IMAGE, "/usr/local/bin/decapod").split()
        require(checksum == [OCI_BINARY, "/usr/local/bin/decapod"], "Immutable official image controller differs from independently extracted bytes")
        # Freeze the native version-tag input locally to this accepted release.
        # The subsequent derived-image ancestry guard rejects a different pull.
        text("bind native release image tag to accepted immutable image", expected_engine, "tag", OCI_IMAGE, OCI_TAG)
        prepared = json.loads(native("prepare native container workspace", "workspace", "ensure", "--container", timeout=900))
        work = safe_path(prepared["worktree_path"])
        require(work.parent == root / ".decapod/workspaces" and prepared.get("branch", "").startswith("agent/" + COORDINATOR + "/"), "Native coordination destination differs")
        require(text("verify accepted source after coordination", "git", "rev-parse", "HEAD").strip() == BASE and not text("verify source preserved after coordination", "git", "status", "--porcelain").strip(), "Native coordination changed accepted source")
        hints = [b["resolve_hint"] for b in prepared.get("blockers", []) if b.get("message") == "Container environment prepared; its launch command will seed a private local data-store snapshot."]
        require(len(hints) == 1 and "Enter containerized workspace" in prepared.get("required_actions", []), "Native container launch contract is missing or ambiguous")
        hint = hints[0]
        engine_match = re.search(r" && '(docker|podman)' run --rm -it ", hint)
        invocation = re.search(r" --label 'org\.decapod\.invocation=([0-9A-HJKMNP-TV-Z]{26})' ", hint)
        image = re.search(r" -w " + re.escape(quote(work)) + r" '([a-z0-9][a-z0-9./:_-]*)' sh -lc ", hint)
        require(engine_match is not None and invocation is not None and image is not None, "Native engine/invocation/image binding is unavailable")
        engine = engine_match[1]
        require(engine == expected_engine, "Native engine changed after immutable image binding")
        store = root / ".decapod/data"
        cleanup = 'result_code=$?; trap - EXIT; rm -f -- "$snapshot_dir/decapod.db"; rmdir -- "$snapshot_dir"; exit "$result_code"'
        seed = "cp -- /tmp/decapod-store.db " + quote(store) + "/decapod.db && exec bash"
        expected = ("( cd " + quote(work) + " && mkdir -p target || exit; snapshot_dir=$(mktemp -d " + quote(work / "target/decapod-container-store.XXXXXX") + ") || exit; trap " + quote(cleanup)
                    + " EXIT; trap 'exit 129' HUP; trap 'exit 130' INT; trap 'exit 143' TERM; decapod data database backup --destination \"$snapshot_dir/decapod.db\" && " + quote(engine) + " run --rm -it --label org.decapod.managed=workspace --label "
                    + quote("org.decapod.workspace.path=" + str(work)) + " --label " + quote("org.decapod.invocation=" + invocation[1]) + " -e DECAPOD_CONTAINER=1 -v " + quote(str(root) + ":" + str(root))
                    + ' --mount "type=bind,src=$snapshot_dir/decapod.db,dst=/tmp/decapod-store.db,readonly" --tmpfs ' + quote(str(store) + ":rw,nosuid,nodev,mode=0700") + " -w " + quote(work) + " " + quote(image[1]) + " sh -lc " + quote(seed) + " )")
        require(hint == expected, "Native launch contract changed; preserve it for review")
        derived = json.loads(text("verify native workspace image ancestry", engine, "image", "inspect", image[1]))
        require(isinstance(derived, list) and len(derived) == 1, "Native workspace image inspection is ambiguous")
        derived = derived[0]
        derived_layers = derived.get("RootFS", {}).get("Layers", [])
        require(derived.get("Architecture") == "amd64" and derived.get("Os") == "linux"
                and derived_layers[:len(OCI_LAYERS)] == OCI_LAYERS and len(derived_layers) > len(OCI_LAYERS),
                "Native workspace image does not extend the exact accepted official image layers")
        report["official_oci_controller"] = OCI_CONTROLLER_IDENTITY
        report["native_image_ancestry"] = dict(official_image=OCI_IMAGE, official_config_digest=OCI_CONFIG,
            official_rootfs_layers=OCI_LAYERS, workspace_image_id=derived.get("Id"), workspace_rootfs_layers=derived_layers, exact_base_prefix_verified=True)
        rootless, id_mappings = False, None
        if engine == "podman":
            host_info = json.loads(text("observe actual native rootless runtime", "podman", "info", "--format", "json"))["host"]
            rootless, id_mappings = host_info["security"]["rootless"], host_info["idMappings"]
            require(rootless is True and mapped_id(0, id_mappings["uidmap"]) == os.geteuid() and mapped_id(0, id_mappings["gidmap"]) == os.getegid(), "Native Podman must be rootless and mapped to the actual runner")
        staged = work / ".decapod/managed/decapod"
        require(hashlib.sha256(staged.read_bytes()).hexdigest() == CONTROLLER, "Native staged binary differs from accepted release")
        event = safe_path(env["GITHUB_EVENT_PATH"])
        require(event.is_file() and event.stat().st_size <= 10 * 1024 * 1024, "Actual workflow event file missing")
        cidfile = private / "native-container.cid"
        handoff = dict(root=str(root), assets=str(assets), incoming=str(incoming), runtime=str(runtime), controller="/usr/local/bin/decapod",
                       child_controller=OCI_CONTROLLER_IDENTITY, host_controller=HOST_CONTROLLER_IDENTITY,
                       engine=engine, rootless_host=rootless, host_id_mappings=id_mappings, cidfile=str(cidfile),
                       uid=os.geteuid(), gid=os.getegid(), publish_timeout=args.publish_timeout,
                       asset_digests={name: sha((assets / name).read_bytes()) for name in ASSETS}, event_sha256=sha(event.read_bytes()),
                       ci_environment={k:v for k,v in os.environ.items() if k.startswith(("GITHUB_", "RUNNER_")) or k in {"CI", "BUILDKITE", "GH_TOKEN"}})
        context = private / "container-context.json"
        context.write_text(json.dumps(handoff))
        mounts = "".join(" --mount " + quote("type=bind,src=" + str(path) + ",dst=" + str(path) + (",readonly" if readonly else "")) for path, readonly in ((assets, True), (incoming, True), (runtime, False), (event, True)))
        entry = "bash " + quote(assets / "container_entry.sh") + " " + " ".join(quote(value) for value in (context, os.geteuid(), os.getegid(), root, assets, engine))
        cid_option = " --cidfile " + quote(cidfile) if engine == "podman" else ""
        automated = hint.replace(quote(engine) + " run --rm -it ", quote(engine) + " run --rm -i" + cid_option + mounts + " ", 1)
        automated = automated.replace("sh -lc " + quote(seed) + " )", "sh -lc " + quote(seed.removesuffix("exec bash") + "exec " + entry) + " )", 1)
        (private / "native-launch-contract.txt").write_text(hint + "\n")
        (private / "automated-native-launch.txt").write_text(automated + "\n")
        report.update(native_coordination_branch=prepared["branch"], native_coordination_path=str(work), invocation=invocation[1], image=image[1],
                      engine=engine, rootless_host=rootless, host_id_mappings=id_mappings,
                      native_contract_sha256=sha(hint.encode()), automated_contract_sha256=sha(automated.encode()),
                      native_backup_and_mount_contract_preserved=True, original_task_mutations_owner="container child only")
        # No original-task command executes on the host, before or after this handoff.
        code, _ = run("execute native container continuation", ["bash", "-c", automated], timeout=args.publish_timeout + 1800, allow_failure=True)
        terminal = runtime / "result/billing.json"
        if terminal.is_file():
            value = json.loads(terminal.read_text())
            report["child_result"] = {k:value.get(k) for k in ("status", "task", "head_sha", "full_boundary_passed", "custody_exported")}
        report["status"] = "container_child_completed" if code == 0 else "container_child_blocked"
        return code
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report.setdefault("blocker", dict(stage=label, kind=type(error).__name__, detail=str(error) if isinstance(error, RuntimeError) else "See encrypted bootstrap diagnostics"))
        return 1
    finally:
        original_blocker = report.get("blocker")
        if cert_verified:
            try:
                archive = runtime / "bootstrap-diagnostics.tar"
                text("archive private bootstrap diagnostics", "tar", "-cf", str(archive), "-C", str(runtime), "private")
                cipher = public / "billing-container-diagnostics.der"
                text("encrypt bootstrap diagnostics", "openssl", "cms", "-encrypt", "-binary", "-aes-256-cbc", "-in", str(archive), "-out", str(cipher), "-outform", "DER", str(assets / "custody-recipient.pem"))
                report["diagnostics"] = dict(artifact=cipher.name, plaintext_sha256=sha(archive.read_bytes()), ciphertext_sha256=sha(cipher.read_bytes()))
            except (RuntimeError, OSError, subprocess.SubprocessError):
                report["diagnostics_preservation_failed"] = True
        if original_blocker is not None:
            report["blocker"] = original_blocker
        (public / "billing-container.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({k:report[k] for k in ("status", "original_task_imported_on_host", "original_task_exported_on_host")}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    outer = commands.add_parser("host")
    outer.add_argument("--assets", type=Path, required=True)
    outer.add_argument("--incoming", type=Path, required=True)
    outer.add_argument("--runtime", type=Path, required=True)
    outer.add_argument("--publish-timeout", type=int, default=3600)
    child = commands.add_parser("inside")
    child.add_argument("--context", type=Path, required=True)
    args = parser.parse_args()
    return inside(args.context) if args.action == "inside" else host(args)


if __name__ == "__main__":
    sys.exit(main())
