#!/usr/bin/env bash
set -euo pipefail

mdbook build docs/book
python3 "${SOURCE_CI_EXECUTOR_ROOT:-.}/assets/tools/docs_seo.py" docs/book/book
