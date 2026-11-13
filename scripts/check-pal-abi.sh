#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

python3 palace/tools/extract_externs.py \
  --quiet \
  --output "$tmp_dir/pal_externs.mire" \
  --unsupported "$tmp_dir/pal_externs.unsupported"

diff -u src/pal/pal_externs.mire "$tmp_dir/pal_externs.mire"
diff -u src/pal/pal_externs.unsupported "$tmp_dir/pal_externs.unsupported"
python3 -m unittest discover -s palace/tests -v
