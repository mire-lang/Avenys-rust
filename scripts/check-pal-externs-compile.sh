#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

python3 - "$tmp_dir/pal_abi.mire" <<'PY'
from pathlib import Path
import sys

declarations = Path("src/pal/pal_externs.mire").read_text()
Path(sys.argv[1]).write_text(declarations + "\npub fn main: () :i32 { return 0 :i32 }\n")
PY

target/release/mire build "$tmp_dir/pal_abi.mire" \
  --runtime none \
  --security-mode open \
  --output-dir "$tmp_dir/out"
