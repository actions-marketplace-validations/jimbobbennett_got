#!/usr/bin/env python3
"""Update documentation examples to use a specific got Action release."""

from __future__ import annotations

import argparse
from pathlib import Path
import re

ACTION_REFERENCE = re.compile(
    r"(?P<prefix>cloudless-ai/got@)v[0-9A-Za-z][A-Za-z0-9._+-]*"
)
DOC_FILES = ("README.md", "docs/index.html")
VERSION_PATTERN = re.compile(
    r"v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?"
)


def update_documentation(root: Path, version: str) -> int:
    if not VERSION_PATTERN.fullmatch(version):
        raise ValueError(f"invalid release version: {version!r}")

    updated_files = 0
    for filename in DOC_FILES:
        path = root / filename
        content = path.read_text(encoding="utf-8")
        updated, replacements = ACTION_REFERENCE.subn(
            lambda match: f"{match.group('prefix')}{version}",
            content,
        )
        if replacements == 0:
            raise ValueError(f"no got Action reference found in {path}")
        if updated != content:
            path.write_text(updated, encoding="utf-8")
            updated_files += 1
    return updated_files


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="release tag, for example v0.1.0")
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root containing README.md and docs/index.html",
    )
    args = parser.parse_args()

    try:
        updated_files = update_documentation(args.root, args.version)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(f"Updated Action version in {updated_files} documentation file(s).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
