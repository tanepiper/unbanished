#!/usr/bin/env python3
"""Combine per-program ExportTypeStringReferences.java TSV exports deterministically."""
import argparse
import csv
import sys


HEADER = ("program", "class_name", "string_address", "reference_address", "function_address", "function_name", "reference_type")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", help="combined TSV output path")
    parser.add_argument("inputs", nargs="+", help="per-program Ghidra TSV exports")
    args = parser.parse_args()

    rows = []
    for path in args.inputs:
        with open(path, encoding="utf-8", newline="") as src:
            reader = csv.reader(src, delimiter="\t")
            header = tuple(next(reader, ()))
            if header != HEADER:
                raise ValueError(f"{path}: unexpected columns: {header!r}")
            for row in reader:
                if len(row) != len(HEADER):
                    raise ValueError(f"{path}: expected {len(HEADER)} columns, got {len(row)}")
                rows.append(row)

    rows.sort(key=lambda row: (row[0].casefold(), row[1].casefold(), row[2], row[3], row[4]))
    with open(args.output, "w", encoding="utf-8", newline="") as dst:
        writer = csv.writer(dst, delimiter="\t", lineterminator="\n")
        writer.writerow(HEADER)
        writer.writerows(rows)
    print(f"Combined {len(rows)} type-string reference rows from {len(args.inputs)} programs into {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
