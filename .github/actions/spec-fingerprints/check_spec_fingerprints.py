#!/usr/bin/env python3
"""Verify that a PR commits the regenerated Decapod spec fingerprint files (#1134).

Fails early (before the heavy build/test pipeline) when the specs manifest or
any fingerprinted projection is missing or stale. Checks, against the checkout
root:

- `.decapod/managed/.manifest.json` exists and the legacy
  `.decapod/managed/specs/.manifest.json` location is gone.
- every `files[]` entry exists and its sha256 equals `content_hash`;
  every spec file still carries the manifest's repo signal fingerprint line.
- every `entrypoints[]` entry exists, starts with the release/fingerprint
  marker lines recorded in the manifest, hashes to `content_hash`, and
  recomputes to the manifest `fingerprint`.
"""

import hashlib
import json
import sys
from pathlib import Path

MANIFEST = Path(".decapod/managed/.manifest.json")
LEGACY_MANIFEST = Path(".decapod/managed/specs/.manifest.json")
FINGERPRINT_PLACEHOLDER = "<fingerprint>"


def sha256_hex(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(".")
    failures: list[str] = []

    manifest_path = root / MANIFEST
    if not manifest_path.is_file():
        print(f"FAIL: missing specs manifest at {MANIFEST}")
        print(
            "Run `decapod rpc --op specs.refresh` and commit "
            f"{MANIFEST} (#1134)."
        )
        return 1
    if (root / LEGACY_MANIFEST).exists():
        failures.append(
            f"legacy manifest {LEGACY_MANIFEST} is still committed; refresh removes it (#1079)"
        )

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    release = manifest.get("decapod_release", "")
    repo_fp = manifest.get("repo_signal_fingerprint", "")

    for entry in manifest.get("files", []):
        rel = entry.get("path", "")
        path = root / rel
        if not path.is_file():
            failures.append(f"files[]: {rel} is missing")
            continue
        content = path.read_text(encoding="utf-8")
        actual = sha256_hex(content)
        if actual != entry.get("content_hash"):
            failures.append(
                f"files[]: {rel} content_hash mismatch (manifest={entry.get('content_hash')} actual={actual}); "
                "regenerate with `decapod rpc --op specs.refresh` (#1134)"
            )
        if rel.startswith(".decapod/managed/specs/") and rel.endswith(".md"):
            expected_line = f"- Repository signal fingerprint: `{repo_fp}`"
            if repo_fp and expected_line not in content:
                failures.append(
                    f"files[]: {rel} does not carry the current repo signal fingerprint {repo_fp}; "
                    "regenerate with `decapod rpc --op specs.refresh` (#1134)"
                )

    for entry in manifest.get("entrypoints", []):
        rel = entry.get("path", "")
        path = root / rel
        if not path.is_file():
            failures.append(f"entrypoints[]: {rel} is missing")
            continue
        raw = path.read_text(encoding="utf-8")
        actual_hash = sha256_hex(raw)
        if actual_hash != entry.get("content_hash"):
            failures.append(
                f"entrypoints[]: {rel} content_hash mismatch (manifest={entry.get('content_hash')} actual={actual_hash}); "
                "regenerate the entrypoint with the current decapod release (#1134)"
            )
        lines = raw.split("\n", 2)
        if len(lines) < 3:
            failures.append(f"entrypoints[]: {rel} is missing its marker lines")
            continue
        expected_release = f"<!-- decapod-release: {release} -->"
        expected_fp = f"<!-- decapod-fingerprint: {entry.get('fingerprint', '')} -->"
        if lines[0] != expected_release:
            failures.append(
                f"entrypoints[]: {rel} release marker mismatch (expected `{expected_release}`, got `{lines[0]}`)"
            )
        if lines[1] != expected_fp:
            failures.append(
                f"entrypoints[]: {rel} fingerprint marker mismatch (expected `{expected_fp}`, got `{lines[1]}`)"
            )
        payload = lines[2]
        fp_input = (
            f"decapod-entrypoint:{rel}\n"
            f"<!-- decapod-release: {release} -->\n"
            f"<!-- decapod-fingerprint: {FINGERPRINT_PLACEHOLDER} -->\n"
            f"{payload}"
        )
        if sha256_hex(fp_input) != entry.get("fingerprint"):
            failures.append(
                f"entrypoints[]: {rel} fingerprint does not recompute from file bytes; "
                "regenerate the entrypoint with the current decapod release (#1134)"
            )

    if failures:
        print("FAIL: spec fingerprint files are missing or stale (#1134):")
        for failure in failures:
            print(f"  - {failure}")
        print("Run `decapod rpc --op specs.refresh`, commit the regenerated files, and push.")
        return 1

    print(
        f"OK: {len(manifest.get('files', []))} spec files and "
        f"{len(manifest.get('entrypoints', []))} entrypoints match {MANIFEST}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
