#!/usr/bin/env python3
"""Synchronize bufrust version numbers before tagging a release."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FILES = {
    ROOT / "Cargo.toml": (
        re.compile(r'(?m)^(version\s*=\s*)"[^"]+"', re.MULTILINE),
        r'\g<1>"{version}"',
    ),
    ROOT / "pyproject.toml": (
        re.compile(r'(?m)^(version\s*=\s*)"[^"]+"', re.MULTILINE),
        r'\g<1>"{version}"',
    ),
    ROOT / "python" / "bufrust" / "__init__.py": (
        re.compile(r'(?m)^(__version__\s*=\s*)"[^"]+"', re.MULTILINE),
        r'\g<1>"{version}"',
    ),
}


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Update Cargo.toml, pyproject.toml, and bufrust.__version__."
    )
    parser.add_argument("version", help="Release version, for example 1.0.1 or v1.0.1")
    parser.add_argument(
        "--skip-lock",
        action="store_true",
        help="Do not run cargo check to refresh Cargo.lock.",
    )
    args = parser.parse_args()

    version = normalize_version(args.version)
    for path, (pattern, replacement) in FILES.items():
        update_file(path, pattern, replacement.format(version=version), version)

    if not args.skip_lock:
        subprocess.run(["cargo", "check", "--quiet"], cwd=ROOT, check=True)

    print(f"Updated bufrust version to {version}")


def normalize_version(value: str) -> str:
    version = value.removeprefix("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", version):
        raise SystemExit(f"invalid version: {value!r}")
    return version


def update_file(path: Path, pattern: re.Pattern[str], replacement: str, version: str) -> None:
    text = path.read_text(encoding="utf-8")
    updated, count = pattern.subn(replacement, text, count=1)
    if count != 1:
        raise SystemExit(f"could not update version in {path}")
    path.write_text(updated, encoding="utf-8", newline="")

    if version not in path.read_text(encoding="utf-8"):
        raise SystemExit(f"version check failed for {path}")


if __name__ == "__main__":
    main()
