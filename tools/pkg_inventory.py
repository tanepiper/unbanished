#!/usr/bin/env python3
"""Export Banished package TOC identities without extracting package payloads."""
import argparse
import csv
import os
import sys

import pkg_extract


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packages", nargs="*", help="package files; defaults to original WinData packages")
    parser.add_argument("-o", "--output", default="-", help="TSV output path (default: stdout)")
    args = parser.parse_args()

    packages = args.packages or [
        os.path.join("game-data", "original", "WinData", name)
        for name in ("data0.pkg", "data1.pkg", "patch0.pkg", "patch1.pkg")
    ]
    rows = []
    for package in packages:
        _, metadata, entries = pkg_extract.read_package(package)
        for resource_path, offset, size in entries:
            normalized = resource_path.replace("\\", "/")
            extension = os.path.splitext(normalized)[1].lower()
            if normalized.lower().endswith(".index"):
                kind = "package-index"
            elif extension == ".crs":
                kind = "compiled-resource"
            elif extension == ".str":
                kind = "audio-stream"
            elif extension == ".sav":
                kind = "save-payload"
            else:
                kind = "other"
            rows.append((
                os.path.basename(package), metadata["priority"], kind,
                normalized, offset, size,
            ))

    rows.sort(key=lambda row: (row[3].casefold(), row[1], row[0]))
    close_output = args.output != "-"
    out = open(args.output, "w", newline="", encoding="utf-8") if close_output else sys.stdout
    try:
        writer = csv.writer(out, delimiter="\t", lineterminator="\n")
        writer.writerow(("package", "priority", "kind", "resource_path", "package_offset", "stored_size"))
        writer.writerows(rows)
    finally:
        if close_output:
            out.close()
    print(f"Exported {len(rows)} package entries from {len(packages)} packages to {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
