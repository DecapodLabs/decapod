#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_MERGE_BASE:?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
set -euo pipefail
python3 - <<'PY'
import os, re, subprocess, sys
from pathlib import Path

base_ref = os.environ["CHANGE_BASE"]
head_ref = os.environ["CHANGE_HEAD"]
specs_dir = Path(".decapod/managed/specs")
names = [
    "INTENT.md",
    "ARCHITECTURE.md",
    "INTERFACES.md",
    "VALIDATION.md",
    "SEMANTICS.md",
    "OPERATIONS.md",
    "SECURITY.md",
    "README.md",
]
patterns = [
    re.compile(
        r"<!-- decapod:codebase-attestation:start -->.*?<!-- decapod:codebase-attestation:end -->",
        re.S,
    ),
    re.compile(
        r"<!-- decapod:declared-capabilities:start -->.*?<!-- decapod:declared-capabilities:end -->",
        re.S,
    ),
    re.compile(
        r"<!-- decapod:capability-overlay:[^:]+:start -->.*?<!-- decapod:capability-overlay:[^:]+:end -->",
        re.S,
    ),
]

def material(text: str) -> str:
    for pat in patterns:
        text = pat.sub("", text)
    return text

def git_show(revision: str, path: str):
    entry = subprocess.run(
        ["git", "--no-replace-objects", "ls-tree", revision, "--", path],
        capture_output=True, text=True, check=True,
    ).stdout
    if not entry:
        return None
    if not entry.startswith("100644 blob ") or not entry.endswith(f"\t{path}\n"):
        raise SystemExit(f"Living spec must be a regular non-executable blob: {path}")
    return subprocess.run(
        ["git", "--no-replace-objects", "show", f"{revision}:{path}"],
        capture_output=True, text=True, check=True,
    ).stdout

material_hits = []
for name in names:
    rel = f"{specs_dir.as_posix()}/{name}"
    head_text = git_show(head_ref, rel)
    if not head_text:
        continue
    base_mat = material(git_show(base_ref, rel) or "")
    head_mat = material(head_text)
    if base_mat != head_mat:
        print(f"✅ material change: {rel}")
        material_hits.append(rel)

if not material_hits:
    print(
        f"❌ FINGERPRINT_ONLY_SPECS: no material authored-content change under "
        f"{specs_dir}/*.md versus immutable {base_ref}."
    )
    print(
        "Fingerprint/attestation refresh alone is insufficient. "
        "Update at least one living spec with prose for this PR."
    )
    sys.exit(1)

print(
    f"✅ At least one living spec has a material rewrite versus immutable {base_ref}"
)
PY
