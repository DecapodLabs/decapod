"""Deliver the authenticated original billing task/current source to one exact runner."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request

REPOSITORY = "DecapodLabs/decapod"
BRANCH = "ops/stale-billing-publication-20261111"
OPERATIONS_PATH = "assets/tools/maintenance/stale-billing-publication-20261111"
TASK = "code_01m76aq15fea61e8"


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def run(*args):
    result = subprocess.run(args, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError("Custody cryptographic operation failed")
    return result.stdout


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["prepare", "receive"])
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("BUILDKITE") != "true", "Requires the actual hosted receiver")
    require(os.environ.get("GITHUB_REPOSITORY") == REPOSITORY and os.environ.get("GITHUB_REF") == "refs/heads/" + BRANCH, "Wrong authorized operations context")
    manifest_bytes = args.manifest.read_bytes()
    manifest = json.loads(manifest_bytes)
    resume = manifest["resume"]
    require(resume["task_id"] == TASK and resume["receiver"] == "chatgpt-stale-billing-next", "Wrong native task receiver")
    root = args.directory.resolve()
    public = root / "public"
    if args.action == "prepare":
        root.mkdir(mode=0o700)
        public.mkdir(mode=0o700)
        run("openssl", "req", "-x509", "-newkey", "rsa:3072", "-nodes",
            "-keyout", str(root / "recipient-private.pem"), "-out", str(public / "recipient.pem"),
            "-days", "1", "-subj", "/CN=Decapod exact billing custody receiver")
        certificate = run("openssl", "x509", "-in", str(public / "recipient.pem"), "-outform", "DER")
        identity = {
            "schema": "decapod.billing-custody-relay.v1", "repository": REPOSITORY,
            "run_id": os.environ["GITHUB_RUN_ID"], "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
            "operations_sha": os.environ["GITHUB_SHA"], "manifest_sha256": digest(manifest_bytes),
            "task_id": TASK, "branch": resume["branch"], "base_sha": manifest["base_sha"], "candidate_sha": resume["candidate_sha"],
            "receiver": resume["receiver"], "prior_run_id": resume["prior_run_id"],
            "native_transfer_sha256": resume["native_transfer"]["plaintext_sha256"],
            "package_sha256": resume["package_sha256"],
            "recipient_certificate_sha256": digest(certificate), "challenge": secrets.token_hex(32),
        }
        (public / "relay.json").write_text(json.dumps(identity, indent=2) + "\n")
        print(json.dumps({"status": "awaiting_authenticated_original_custody", **identity}))
        return

    identity = json.loads((public / "relay.json").read_text())
    require(identity["operations_sha"] == os.environ["GITHUB_SHA"] and identity["manifest_sha256"] == digest(manifest_bytes), "Receiver source/manifest changed")
    name = identity["run_id"] + "-" + identity["run_attempt"] + "-billing.json"
    path = OPERATIONS_PATH + "/replies/" + name
    url = "https://api.github.com/repos/" + REPOSITORY + "/contents/" + path + "?ref=" + urllib.parse.quote(BRANCH, safe="")
    deadline = time.monotonic() + 1200
    reply = None
    while time.monotonic() < deadline:
        request = urllib.request.Request(url, headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
            "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                body = json.loads(response.read(1500000))
            require(body["type"] == "file" and body["encoding"] == "base64" and body["size"] < 1000000, "Unexpected relay response shape")
            reply = json.loads(base64.b64decode(body["content"]))
            break
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise RuntimeError("Custody relay read failed: HTTP " + str(error.code)) from None
        print("Waiting for the authenticated original task/material package.", flush=True)
        time.sleep(15)
    require(reply is not None, "No authenticated custody relay arrived within bounded wait")
    require(reply["identity"] == identity, "Reply belongs to another runner, source, task or challenge")
    encrypted = base64.b64decode(reply["ciphertext_base64"], validate=True)
    require(digest(encrypted) == reply["ciphertext_sha256"], "Ciphertext digest mismatch")
    cipher_path = root / "received.der"
    cipher_path.write_bytes(encrypted)
    package_bytes = run("openssl", "cms", "-decrypt", "-binary", "-inform", "DER", "-in", str(cipher_path),
                        "-inkey", str(root / "recipient-private.pem"), "-recip", str(public / "recipient.pem"))
    require(digest(package_bytes) == identity["package_sha256"], "Private package differs from authenticated source")
    package = json.loads(package_bytes)
    require(package["schema"] == "decapod.billing-private-recovery.v1" and package["repository"] == REPOSITORY
            and package["task_id"] == TASK and package["branch"] == resume["branch"]
            and package["base_sha"] == manifest["base_sha"] and package["candidate_sha"] == resume["candidate_sha"]
            and package["receiver"] == resume["receiver"]
            and package["prior_run_id"] == resume["prior_run_id"], "Private package context differs")
    require(set(package["files"]) == set(resume["private_files"]), "Private file catalog differs")
    destination = root / "recovery"
    destination.mkdir(mode=0o700)
    for name, item in package["files"].items():
        require(not name.startswith("/") and ".." not in Path(name).parts, "Unsafe private recovery path")
        data = base64.b64decode(item["base64"], validate=True)
        approved = resume["private_files"][name]
        require(digest(data) == item["sha256"] == approved["sha256"] and len(data) == approved["bytes"], "Private recovery file digest differs")
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        target.write_bytes(data)
    native = (destination / "native-transfer.json").read_bytes()
    require(digest(native) == identity["native_transfer_sha256"], "Native task transfer differs from original authenticated handoff")
    print(json.dumps({"status": "authenticated_original_custody_received", "task_id": TASK,
                      "native_transfer_digest_verified": True, "pending_material_digest_verified": True}))


if __name__ == "__main__":
    main()
