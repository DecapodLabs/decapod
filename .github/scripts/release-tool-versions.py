#!/usr/bin/env python3
"""Read the bounded shared release setup schema using Python's standard library."""
import pathlib
import re
import sys


def load(path):
    values = {}
    for line in pathlib.Path(path).read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = re.fullmatch(r'([a-z_]+) = "([A-Za-z0-9./@-]+)"', line)
        if not match or match[1] in values:
            raise ValueError("invalid or duplicate release tool setting")
        values[match[1]] = match[2]
    expected = {"rust", "release_plz", "cargo_dist", "release_plz_action", "semver_checks_constraint", "semver_checks_installer"}
    if values.keys() != expected:
        raise ValueError("missing or unknown release tool setting")
    for key in ("rust", "release_plz", "cargo_dist"):
        if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", values[key]):
            raise ValueError("release tool version must be exact")
    for key, repo in (("release_plz_action", "release-plz/action"), ("semver_checks_installer", "taiki-e/install-action")):
        if not re.fullmatch(re.escape(repo) + r"@[0-9a-f]{40}", values[key]):
            raise ValueError("release installer must use an immutable revision")
    if values["semver_checks_constraint"] != "0.51":
        raise ValueError("unsupported upstream semver-checks constraint")
    return values


if __name__ == "__main__":
    if len(sys.argv) != 1:
        raise SystemExit("release-tool-versions.py accepts no arguments")
    for key, value in load(".github/release-tools.toml").items():
        print(f"{key}={value}")
