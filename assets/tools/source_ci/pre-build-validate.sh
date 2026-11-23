#!/usr/bin/env bash
# Fast pre-build structural, governance, and SHA-256 fingerprint gate (#1047, #1134).
# Fails closed in seconds before incurring build/test/CI cost if governance state,
# living specs, manifest, or entrypoint SHA-256 fingerprints are missing or stale.
set -euo pipefail
umask 077

export RELEASE_ONLY="${SOURCE_CI_RELEASE_ONLY:-${RELEASE_ONLY:-false}}"

python3 - <<'PY'
import hashlib
import json
import os
import re
import sys
from pathlib import Path

repo = Path(".").resolve()
release_only = os.environ.get("RELEASE_ONLY", "false").lower() == "true"
errors = []

HEX64_RE = re.compile(r"^[0-9a-fA-F]{64}$")
RELEASE_MARKER_RE = re.compile(r"<!--\s*decapod-release:\s*([^\s>]+)\s*-->")
FP_MARKER_RE = re.compile(r"<!--\s*decapod-fingerprint:\s*([0-9a-fA-F]{64})\s*-->")
SPEC_FP_RE = re.compile(r"Repository signal fingerprint:\s*`([0-9a-fA-F]{64})`")

def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()

# 1. .decapod/config.toml
config_path = repo / ".decapod" / "config.toml"
entrypoints = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"]
specs_required = True
if not config_path.is_file() or config_path.stat().st_size == 0:
    errors.append("config_schema: .decapod/config.toml is missing or empty")
else:
    raw_cfg = config_path.read_text(encoding="utf-8")
    try:
        import tomllib  # Python 3.11+
        cfg = tomllib.loads(raw_cfg)
        init_cfg = cfg.get("init", {})
        if isinstance(init_cfg.get("specs"), bool):
            specs_required = init_cfg["specs"]
        eps = init_cfg.get("entrypoints")
        if isinstance(eps, list) and eps:
            entrypoints = [str(x) for x in eps]
    except ImportError:
        pass
    except Exception as exc:
        errors.append(f"config_schema: .decapod/config.toml failed TOML parse: {exc}")

# 2. .decapod/governance.json
gov_path = repo / ".decapod" / "governance.json"
if not gov_path.is_file():
    errors.append("governance_document: .decapod/governance.json is missing")
else:
    try:
        gov = json.loads(gov_path.read_text(encoding="utf-8"))
        missing = []
        if "claims" not in gov:
            missing.append("claims")
        sections = gov.get("sections") if isinstance(gov, dict) else None
        for key in ("plan", "trajectory"):
            if not isinstance(sections, dict) or key not in sections:
                missing.append(key)
        if missing:
            errors.append(
                f"governance_document: .decapod/governance.json missing required section(s): {', '.join(missing)}"
            )
    except Exception as exc:
        errors.append(f"governance_document: .decapod/governance.json invalid JSON: {exc}")

# 3. Living specs layout + manifest SHA-256 verification (#1079, #1134)
required_specs = [
    "README.md",
    "INTENT.md",
    "ARCHITECTURE.md",
    "INTERFACES.md",
    "SECURITY.md",
    "OPERATIONS.md",
    "VALIDATION.md",
    "SEMANTICS.md",
]
manifest_data = None
if specs_required:
    specs_dir = repo / ".decapod" / "managed" / "specs"
    missing_specs = [
        name for name in required_specs
        if not (specs_dir / name).is_file() or (specs_dir / name).stat().st_size == 0
    ]
    if missing_specs:
        errors.append(
            f"living_specs_layout: missing or empty living spec(s): {', '.join(missing_specs)}"
        )

    canonical_manifest = repo / ".decapod" / "managed" / ".manifest.json"
    legacy_manifest = specs_dir / ".manifest.json"
    manifest_path = canonical_manifest if canonical_manifest.is_file() else (
        legacy_manifest if legacy_manifest.is_file() else None
    )
    if manifest_path is None:
        errors.append(
            "specs_manifest: missing .decapod/managed/.manifest.json (and legacy fallback)"
        )
    else:
        try:
            manifest_data = json.loads(manifest_path.read_text(encoding="utf-8"))
        except Exception as exc:
            errors.append(f"specs_manifest: invalid JSON in {manifest_path.relative_to(repo)}: {exc}")

    if isinstance(manifest_data, dict):
        repo_fp = str(manifest_data.get("repo_signal_fingerprint", ""))
        if not HEX64_RE.match(repo_fp):
            errors.append("specs_manifest: missing or invalid 64-hex repo_signal_fingerprint")
        for field in ("config_input_hash", "spec_input_hash"):
            val = str(manifest_data.get(field, ""))
            if not HEX64_RE.match(val):
                errors.append(f"specs_manifest: missing or invalid 64-hex {field}")
        if not str(manifest_data.get("decapod_release", "")).strip():
            errors.append("specs_manifest: missing decapod_release")

        # Verify spec files carry valid attestation fingerprints and match manifest content_hash
        for name in required_specs:
            p = specs_dir / name
            if not p.is_file():
                continue
            body = p.read_text(encoding="utf-8")
            m = SPEC_FP_RE.search(body)
            if not m:
                errors.append(
                    f"spec_fingerprint: .decapod/managed/specs/{name} missing 64-hex Repository signal fingerprint"
                )
            elif not release_only and HEX64_RE.match(repo_fp) and m.group(1) != repo_fp:
                errors.append(
                    f"spec_fingerprint: .decapod/managed/specs/{name} fingerprint {m.group(1)[:12]} != manifest {repo_fp[:12]}"
                )

        if not release_only:
            files_list = manifest_data.get("files", [])
            if not isinstance(files_list, list) or len(files_list) < len(required_specs):
                errors.append("specs_manifest: incomplete files inventory in manifest")
            else:
                for entry in files_list:
                    rel = entry.get("path", "")
                    expected_hash = str(entry.get("content_hash", ""))
                    if not HEX64_RE.match(expected_hash):
                        errors.append(f"specs_manifest: invalid content_hash for {rel}")
                        continue
                    target = repo / rel
                    if not target.is_file():
                        errors.append(f"specs_manifest: listed spec missing: {rel}")
                        continue
                    actual_hash = sha256_text(target.read_text(encoding="utf-8"))
                    if actual_hash != expected_hash:
                        errors.append(
                            f"specs_manifest: stale content_hash for {rel} (run `decapod rpc --op specs.refresh`)"
                        )

            ep_entries = manifest_data.get("entrypoints", [])
            if not isinstance(ep_entries, list) or not ep_entries:
                errors.append("specs_manifest: missing entrypoints inventory in manifest")
            else:
                for entry in ep_entries:
                    rel = entry.get("path", "")
                    fp = str(entry.get("fingerprint", ""))
                    expected_hash = str(entry.get("content_hash", ""))
                    if not HEX64_RE.match(fp):
                        errors.append(f"specs_manifest: missing/invalid entrypoint fingerprint for {rel}")
                    if not HEX64_RE.match(expected_hash):
                        errors.append(f"specs_manifest: missing/invalid entrypoint content_hash for {rel}")
                        continue
                    target = repo / rel
                    if not target.is_file():
                        errors.append(f"specs_manifest: listed entrypoint missing: {rel}")
                        continue
                    actual_hash = sha256_text(target.read_text(encoding="utf-8"))
                    if actual_hash != expected_hash:
                        errors.append(
                            f"specs_manifest: stale entrypoint content_hash for {rel} (refresh entrypoints + specs)"
                        )

# 4. Agent entrypoints release + SHA-256 fingerprint markers (#1134)
for ep in entrypoints:
    p = repo / ep
    if not p.is_file():
        errors.append(f"agent_entrypoints: missing {ep}")
        continue
    body = p.read_text(encoding="utf-8")
    if not RELEASE_MARKER_RE.search(body):
        errors.append(f"agent_entrypoints: {ep} missing <!-- decapod-release: ... --> marker")
    if not FP_MARKER_RE.search(body):
        errors.append(f"agent_entrypoints: {ep} missing valid 64-hex <!-- decapod-fingerprint: ... --> SHA")

if errors:
    for err in errors:
        print(f"::error::pre-build-validate: {err}", file=sys.stderr)
    print(
        f"FAIL: pre-build structural & fingerprint gate rejected {len(errors)} issue(s). "
        "Refresh living specs/entrypoints (`decapod rpc --op specs.refresh`) and governance artifacts before pushing.",
        file=sys.stderr,
    )
    sys.exit(1)

print("PASS: pre-build structural, governance, manifest, and SHA-256 fingerprint gate verified.")
PY
