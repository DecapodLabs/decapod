"""Relay native task custody only to this authenticated, bounded hosted receiver."""
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

OPS = "ops/issue1628-native-20261111"
PREFIX = "assets/tools/maintenance/issue1628-native-20261111"
BINDINGS = {
    "decapod": {
        "repository": "DecapodLabs/decapod",
        "task": "code_01m76c1kqhjgr3yr",
        "base": "c5de35a89b27e788169612ae71681e45c13bf71a",
        "transfer_id": "01M76C2BDC2SWT2V852BA93XWY",
        "digest": "sha256:c1734174448bdd416beffba9f08ec5827aec991197493b2f49d20f2fd5804f95",
    },
    "dactyl": {
        "repository": "DecapodLabs/dactyl",
        "task": "feat_01m76c2002nnt3za",
        "base": "1fa52a232e1d34a27be28d158298805ffc175ffc",
        "transfer_id": "01M76C2MMAC26KJ982SYF1BAYJ",
        "digest": "sha256:55b7ab6f429f307790556ffa23e52af7f8f57b00bdc94c0b19ed0460613a2b78",
    },
}


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def run(*args):
    result = subprocess.run(args, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError("Custody cryptographic operation failed")
    return result.stdout


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "receive"])
    parser.add_argument("--slug", choices=BINDINGS, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--runtime-inventory", type=Path)
    args = parser.parse_args()
    binding = BINDINGS[args.slug]
    report = json.loads(args.report.read_text())
    assert os.environ.get("GITHUB_ACTIONS") == "true"
    assert os.environ.get("BUILDKITE") != "true"
    assert os.environ["GITHUB_REPOSITORY"] == binding["repository"]
    assert os.environ["GITHUB_REF"] == "refs/heads/" + OPS
    assert report["repository"] == binding["repository"]
    assert report["task"] == binding["task"] and report["base"] == binding["base"]
    assert report["admitted"] and report["container_observed"] and report["custody_exported"]
    assert report["transfer"]["transfer_id"] == binding["transfer_id"]
    assert report["transfer"]["plaintext_sha256"] == binding["digest"]
    assert report["transfer"]["source_sealed"]
    receiver = "chatgpt-issue1628-" + args.slug + "-next"
    assert report["transfer"]["receiver"] == receiver
    root = args.directory.resolve()
    assert root.parent == Path("/root"), "Native private custody requires owned /root ancestry"
    public = root / "public"
    if args.action == "prepare":
        root.mkdir(mode=0o700)
        public.mkdir(mode=0o700)
        run("openssl", "req", "-x509", "-newkey", "rsa:3072", "-nodes",
            "-keyout", str(root / "recipient-private.pem"),
            "-out", str(public / "recipient.pem"), "-days", "1", "-subj",
            "/CN=Issue1628 native custody receiver " + args.slug)
        der = run("openssl", "x509", "-in", str(public / "recipient.pem"), "-outform", "DER")
        identity = {
            "schema": "decapod.issue1628-custody-relay.v1",
            "repository": binding["repository"],
            "run_id": os.environ["GITHUB_RUN_ID"],
            "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
            "operations_sha": os.environ["GITHUB_SHA"],
            "slug": args.slug, "task_id": binding["task"],
            "source_base": binding["base"], "transfer_id": binding["transfer_id"],
            "receiver": receiver, "plaintext_sha256": binding["digest"],
            "recipient_certificate_sha256": digest(der),
            "challenge": secrets.token_hex(32),
        }
        if args.runtime_inventory:
            runtime = args.runtime_inventory.read_bytes()
            inventory = json.loads(runtime)
            assert isinstance(inventory.get("toolchain_files"), dict) and inventory["toolchain_files"]
            (public / "runtime-inventory.json").write_bytes(runtime)
            identity["runtime_inventory_sha256"] = digest(runtime)
        (public / "relay.json").write_text(json.dumps(identity, indent=2) + "\n")
        print(json.dumps({"status": "awaiting_private_custody", **identity}))
        return
    identity = json.loads((public / "relay.json").read_text())
    assert identity["task_id"] == binding["task"] and identity["plaintext_sha256"] == binding["digest"]
    name = "-".join([identity["run_id"], identity["run_attempt"], args.slug]) + ".json"
    path = PREFIX + "/replies/" + name
    url = "https://api.github.com/repos/" + binding["repository"] + "/contents/" + path
    url += "?ref=" + urllib.parse.quote(OPS, safe="")
    deadline = time.monotonic() + 1200
    reply = None
    while time.monotonic() < deadline:
        request = urllib.request.Request(url, headers={
            "Authorization": "Bearer " + os.environ["GH_TOKEN"],
            "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        })
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                raw = response.read(1500000)
            body = json.loads(raw)
            assert body["type"] == "file" and body["encoding"] == "base64" and body["size"] < 1000000
            reply = json.loads(base64.b64decode(body["content"]))
            break
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise RuntimeError("Custody relay read failed: HTTP " + str(error.code)) from None
        print("Waiting for bound private task custody.", flush=True)
        time.sleep(15)
    assert reply is not None, "No custody reply arrived within bounded deadline"
    assert reply["identity"] == identity, "Custody reply belongs to another runner or source"
    if "runtime_inventory_sha256" in identity:
        assert reply.get("runtime_review") == {
            "runtime_inventory_sha256": identity["runtime_inventory_sha256"],
            "reviewed": True,
            "scope": "installed canonical Rust1.96.1 and Rust1.75.0 x86 runtime for required local proof",
        }, "Installed tool closure needs independent review before native import"
        (root / "runtime-review.json").write_text(json.dumps(reply["runtime_review"], indent=2) + "\n")
    ciphertext = base64.b64decode(reply["ciphertext_base64"], validate=True)
    assert digest(ciphertext) == reply["ciphertext_sha256"]
    encrypted = root / "received.der"
    encrypted.write_bytes(ciphertext)
    plaintext = run("openssl", "cms", "-decrypt", "-binary", "-inform", "DER",
                    "-in", str(encrypted), "-inkey", str(root / "recipient-private.pem"),
                    "-recip", str(public / "recipient.pem"))
    assert digest(plaintext) == binding["digest"]
    native = json.loads(plaintext)
    assert native["receiver"] == receiver and native["projection"]["task"]["id"] == binding["task"]
    assert native["transfer_id"] == binding["transfer_id"] and native["source_commit"] == binding["base"]
    (root / "native-transfer.json").write_bytes(plaintext)
    print(json.dumps({"status": "private_custody_received", "slug": args.slug,
                      "task_id": binding["task"], "native_digest_verified": True}))


if __name__ == "__main__":
    main()
