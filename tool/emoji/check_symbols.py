#!/usr/bin/env python3
"""Verify the curated Symbols catalog against pinned Unicode character names."""

import argparse
import json
import os
from pathlib import Path
import re

from build import ASSETS, TOOL, read_sources

CATALOG = TOOL.parents[1] / "lib" / "features" / "emoji" / "symbol_catalog.dart"
ROW = re.compile(r'\(\s*"([0-9A-F]{4,6})",\s*"([^"]+)",\s*"[^"]+",?\s*\)')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    parser.add_argument("--cache", type=Path, default=cache / "remya-emoji-sources")
    args = parser.parse_args()
    lock = json.loads((TOOL / "symbols-source.lock.json").read_bytes())
    sources = read_sources(lock, args.cache)
    names = dict(line.split(";", 2)[:2] for line in sources["unicodeData"].decode().splitlines())
    emoji_catalog = json.loads((ASSETS / "catalog.json").read_bytes())
    emoji = {row.split("|", 1)[0] for row in emoji_catalog["common"]}
    rows = ROW.findall(CATALOG.read_text(encoding="utf-8"))
    if not rows or len({point for point, _ in rows}) != len(rows):
        raise ValueError("Missing symbol records or duplicate code points")
    for point, name in rows:
        if names[point].lower() != name:
            raise ValueError(f"Incorrect Unicode name for U+{point}: {name}")
        if chr(int(point, 16)) in emoji:
            raise ValueError(f"Symbols and Emoji catalogs overlap at U+{point}")
    print(f"Verified {len(rows)} distinct Symbols entries against UnicodeData")


if __name__ == "__main__":
    main()
