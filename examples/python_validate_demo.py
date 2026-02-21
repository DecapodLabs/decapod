#!/usr/bin/env python3
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "sdk", "python"))
from decapod_shim import run_validate  # type: ignore


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", help="Path to fixture envelope JSON")
    args = parser.parse_args()

    if args.fixture:
        with open(args.fixture, "r", encoding="utf-8") as f:
            data = json.load(f)
        print(json.dumps({"demo": "python", "status": data.get("status", "unknown")}, indent=2))
        return 0

    env = run_validate()
    print(json.dumps({"demo": "python", "status": env["status"], "exit_code": env["exit_code"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
