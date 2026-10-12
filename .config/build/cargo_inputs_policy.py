"""Verify real generated dependency inputs are exactly bound by Bazel's lock."""

import hashlib
import json
import pathlib
import sys

manifest, cargo_lock, module_lock = map(pathlib.Path, sys.argv[1:])
record = json.loads(module_lock.read_text())["moduleExtensions"][
    "@@rules_rust+//crate_universe:extensions.bzl%crate"
]["general"]
inputs = record["recordedInputs"]
for path in [manifest, cargo_lock]:
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    matches = [line for line in inputs if line.startswith("FILE:") and line.split(" ")[0].endswith("//" + path.name)]
    assert len(matches) == 1, (path.name, matches)
    assert "decapod_cargo_inputs" in matches[0], matches[0]
    assert matches[0].endswith(" " + digest), matches[0]
assert not any(line.startswith("FILE:@@//Cargo.") for line in inputs)
print("Version-neutral Cargo inputs match the committed strict Bazel lock")
