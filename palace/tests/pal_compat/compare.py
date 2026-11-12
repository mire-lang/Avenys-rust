#!/usr/bin/env python3
"""Run one compatibility case against reference and candidate commands."""

from __future__ import annotations

import argparse
import difflib
import shlex
import subprocess
import sys


def run(command: str, timeout: float) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(shlex.split(command), capture_output=True, check=False, timeout=timeout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, help="command using the current C PAL")
    parser.add_argument("--candidate", required=True, help="command using the Mire PAL")
    parser.add_argument("--timeout", type=float, default=30.0, help="per-command timeout in seconds")
    args = parser.parse_args()

    try:
        reference = run(args.reference, args.timeout)
        candidate = run(args.candidate, args.timeout)
    except subprocess.TimeoutExpired as error:
        print(f"compatibility command exceeded {error.timeout:g}s", file=sys.stderr)
        return 124
    if (reference.returncode, reference.stdout, reference.stderr) == (
        candidate.returncode,
        candidate.stdout,
        candidate.stderr,
    ):
        print("compatible")
        return 0

    print(f"reference exit: {reference.returncode}; candidate exit: {candidate.returncode}")
    for stream_name in ("stdout", "stderr"):
        before = getattr(reference, stream_name).decode("utf-8", errors="backslashreplace").splitlines()
        after = getattr(candidate, stream_name).decode("utf-8", errors="backslashreplace").splitlines()
        if before != after:
            print(f"{stream_name} differs:")
            sys.stdout.writelines(
                difflib.unified_diff(before, after, fromfile=f"C/{stream_name}", tofile=f"Mire/{stream_name}", lineterm="\n")
            )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
