#!/usr/bin/env python3
"""Read-only GitHub publication transport diagnosis; no native task or source use."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import urllib.request

REPO = "DecapodLabs/decapod"
OPS = "ops/stale-billing-publication-20261111"
BRANCH = "agent/chatgpt-stale-billing/code_01m76aq15fea61e8-cloud-approval"
CERT = "c54fbd6b328ebc89c57a0b158cbdddc434e37f48b8283b0e9af532506600f6dc"
ARCHIVE_SHA = "4c806978ffce1cec70853302a56d70ac6da4f19bae7adca4e1a8ab00633dd91d"
BINARY_SHA = "b768184570b12536be4ac1528893f6340d1c6a7b40b56ad45bca9cd0e87ad34d"
ARCHIVE_BYTES, BINARY_BYTES = 15031972, 41070752
PROBES = [
    ("native_paginated_pr_lookup", ["api", "--method", "GET", "repos/" + REPO + "/pulls", "-f", "state=open", "-f", "head=DecapodLabs:" + BRANCH, "-f", "per_page=100", "--paginate", "--slurp"]),
    ("repository_metadata", ["api", "--method", "GET", "repos/" + REPO]),
    ("source_head_metadata", ["api", "--method", "GET", "repos/" + REPO + "/git/ref/heads/" + BRANCH]),
    ("accepted_base_metadata", ["api", "--method", "GET", "repos/" + REPO + "/git/ref/heads/master"]),
    ("graphql_viewer_permission", ["api", "graphql", "-f", 'query=query { viewer { login } repository(owner: "DecapodLabs", name: "decapod") { nameWithOwner viewerPermission isArchived } }']),
    ("actions_workflow_permissions", ["api", "--method", "GET", "repos/" + REPO + "/actions/permissions/workflow"]),
]


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--recipient", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    env = os.environ.copy()
    require(env.get("GITHUB_ACTIONS") == "true" and env.get("BUILDKITE") != "true"
            and env.get("GITHUB_REPOSITORY") == REPO and env.get("GITHUB_REF") == "refs/heads/" + OPS
            and bool(env.get("GH_TOKEN")), "Wrong authorized read-only diagnostic context")
    root = args.directory.resolve()
    root.mkdir(mode=0o700, exist_ok=False)
    private, public = root / "private", root / "public"
    private.mkdir(mode=0o700); public.mkdir(mode=0o700)
    report = dict(schema="decapod.billing-github-read-diagnostics.v1", operations_sha=env["GITHUB_SHA"],
                  run_id=env["GITHUB_RUN_ID"], run_attempt=env["GITHUB_RUN_ATTEMPT"], status="blocked",
                  remote_mutations=False, original_task_accessed=False, probes=[])
    certified = False
    try:
        cert = subprocess.run(["openssl", "x509", "-in", str(args.recipient.resolve()), "-noout", "-fingerprint", "-sha256"], capture_output=True, text=True, timeout=30)
        require(cert.returncode == 0 and cert.stdout.split("=", 1)[-1].strip().replace(":", "").lower() == CERT, "Operator recipient differs")
        certified = True
        url = "https://github.com/cli/cli/releases/download/v2.103.3/gh_2.103.3_linux_amd64.tar.gz"
        with urllib.request.urlopen(url, timeout=60) as response:
            archive_bytes = response.read(ARCHIVE_BYTES + 1)
        require(len(archive_bytes) == ARCHIVE_BYTES and hashlib.sha256(archive_bytes).hexdigest() == ARCHIVE_SHA, "Official archive differs")
        archive_path = root / "gh.tar.gz"
        archive_path.write_bytes(archive_bytes)
        with tarfile.open(archive_path, "r:gz") as archive:
            members = [member for member in archive.getmembers() if member.name == "gh_2.103.3_linux_amd64/bin/gh"]
            require(len(members) == 1 and members[0].isfile() and members[0].size == BINARY_BYTES, "Official binary member differs")
            binary_bytes = archive.extractfile(members[0]).read(BINARY_BYTES + 1)
        require(len(binary_bytes) == BINARY_BYTES and hashlib.sha256(binary_bytes).hexdigest() == BINARY_SHA, "Official binary differs")
        gh = root / "gh"
        gh.write_bytes(binary_bytes); gh.chmod(0o755)
        version = subprocess.run([str(gh), "--version"], capture_output=True, timeout=30, env=env)
        help_result = subprocess.run([str(gh), "api", "--help"], capture_output=True, timeout=30, env=env)
        (private / "gh-version.stdout.txt").write_bytes(version.stdout)
        (private / "gh-api-help.stdout.txt").write_bytes(help_result.stdout)
        require(version.returncode == 0 and version.stdout.startswith(b"gh version 2.103.3 ")
                and help_result.returncode == 0 and b"--slurp" in help_result.stdout, "Required official CLI support is unavailable")
        report["official_cli_verified"] = True
        for label, argv in PROBES:
            timed_out = False
            try:
                result = subprocess.run([str(gh), *argv], capture_output=True, timeout=60, env=env, cwd=root)
                code, stdout, stderr = result.returncode, result.stdout, result.stderr
            except subprocess.TimeoutExpired as error:
                code, timed_out, stdout, stderr = 124, True, error.stdout or b"", error.stderr or b""
            (private / (label + ".stdout.txt")).write_bytes(stdout)
            (private / (label + ".stderr.txt")).write_bytes(stderr)
            text = (stdout + b"\n" + stderr).decode(errors="replace")
            statuses = sorted({int(value) for value in re.findall(r"HTTP[ /](?:[0-9.]+ )?([1-5][0-9]{2})\b", text)})
            codes = []
            for marker, value in [("Resource not accessible by integration", "RESOURCE_NOT_ACCESSIBLE"),
                                  ("Bad credentials", "BAD_CREDENTIALS"), ("FORBIDDEN", "FORBIDDEN"),
                                  ("NOT_FOUND", "NOT_FOUND"), ("rate limit", "RATE_LIMITED"),
                                  ("unknown flag", "UNKNOWN_FLAG")]:
                if marker.lower() in text.lower():
                    codes.append(value)
            report["probes"].append(dict(label=label, exit_code=code, timeout=timed_out,
                status="ok" if code == 0 else "failed", http_statuses=statuses, error_codes=codes))
        report["status"] = "read_only_diagnostics_completed"
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError, tarfile.TarError) as error:
        (private / "local-error.txt").write_text(type(error).__name__ + ": " + str(error))
        report["error_kind"] = type(error).__name__
    finally:
        if certified:
            archive_path = root / "raw-diagnostics.tar"
            with tarfile.open(archive_path, "w") as archive:
                archive.add(private, arcname="private")
            cipher = public / "github-publication-diagnostics.der"
            encrypted = subprocess.run(["openssl", "cms", "-encrypt", "-binary", "-aes-256-cbc", "-in", str(archive_path),
                "-out", str(cipher), "-outform", "DER", str(args.recipient.resolve())], capture_output=True, timeout=60)
            if encrypted.returncode == 0 and cipher.is_file():
                report["encrypted_diagnostics"] = dict(artifact=cipher.name, plaintext_sha256=digest(archive_path.read_bytes()), ciphertext_sha256=digest(cipher.read_bytes()))
            else:
                report["status"] = "diagnostics_encryption_failed"
        (public / "github-publication-diagnostics.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report))
    return 0 if report["status"] == "read_only_diagnostics_completed" and "encrypted_diagnostics" in report else 1


if __name__ == "__main__":
    raise SystemExit(main())
