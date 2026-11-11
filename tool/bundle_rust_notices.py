#!/usr/bin/env python3
"""Collect the license and notice files shipped by the Rust dependencies."""

import json
from pathlib import Path
import subprocess
import sys


def write_notices(cargo, output):
    host = subprocess.check_output(["rustc", "--print", "host-tuple"], text=True).strip()
    metadata = json.loads(subprocess.check_output(
        [cargo, "metadata", "--locked", "--format-version", "1", "--filter-platform", host]
    ))
    prefixes = ("license", "copying", "notice", "copyright", "unlicense")
    with output.open("w", encoding="utf-8") as notices:
        for package in sorted(metadata["packages"], key=lambda item: item["name"]):
            if package["source"] is None:
                continue
            root = Path(package["manifest_path"]).parent
            files = {
                path for path in root.iterdir()
                if path.is_file() and path.name.lower().startswith(prefixes)
            }
            if package["license_file"]:
                files.add(root / package["license_file"])
            print(f'{package["name"]} {package["version"]} ({package["license"]})',
                  file=notices)
            for path in sorted(files):
                print(f"\n{path.name}\n{path.read_text(encoding='utf-8', errors='replace')}",
                      file=notices)
            print("\n" + "-" * 72 + "\n", file=notices)


if __name__ == "__main__":
    write_notices(sys.argv[1], Path(sys.argv[2]))
