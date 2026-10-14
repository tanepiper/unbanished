#!/usr/bin/env python3
"""Export Banished package TOC identities without extracting package payloads."""
import argparse
import csv
import json
import os
import sys

from crs.crs import MAGIC, Reader, read_resource
import pkg_extract


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packages", nargs="*", help="package files; defaults to original WinData packages")
    parser.add_argument("-o", "--output", default="-", help="TSV output path (default: stdout)")
    parser.add_argument("--references-output", help="optional TSV of compiled-resource dependency edges")
    parser.add_argument("--type-map", default="tools/schema/types.json", help="observed type ID to class-name map")
    args = parser.parse_args()

    packages = args.packages or [
        os.path.join("game-data", "original", "WinData", name)
        for name in ("data0.pkg", "data1.pkg", "patch0.pkg", "patch1.pkg")
    ]
    type_map = json.load(open(args.type_map, encoding="utf-8")) if os.path.exists(args.type_map) else {}
    rows, references = [], []
    for package in packages:
        data, metadata, entries = pkg_extract.read_package(package)
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

            type_id = file_version = ""
            class_name = ""
            dependency_count = 0
            if kind == "compiled-resource":
                raw = pkg_extract.read_block_stream(data[offset:offset + size])
                parsed = read_resource(raw, decode=False)
                type_id = f"0x{parsed['type_id']:08x}"
                file_version = parsed["file_version"]
                names = type_map.get(type_id, {}).get("names", [])
                class_name = names[0] if len(names) == 1 else ""
                dependency_count = len(parsed["dependencies"])
                if args.references_output:
                    reader = Reader(raw)
                    with reader.block():
                        if reader.u32() != MAGIC:
                            raise ValueError(f"{package}:{normalized}: missing compiled-resource magic")
                        reader.u32()  # type ID
                        reader.u32()  # file version
                        count = reader.u32()
                        for dependency_index in range(count):
                            dependency_path = reader.string().replace("\\", "/")
                            filetime = reader.u64()
                            references.append((normalized, dependency_index, dependency_path, filetime))
                        reader.pos = reader.end  # dependency targets precede the class payload
            rows.append((
                os.path.basename(package), metadata["priority"], kind,
                normalized, offset, size, type_id, class_name, file_version, dependency_count,
            ))

    rows.sort(key=lambda row: (row[3].casefold(), row[1], row[0]))
    close_output = args.output != "-"
    out = open(args.output, "w", newline="", encoding="utf-8") if close_output else sys.stdout
    try:
        writer = csv.writer(out, delimiter="\t", lineterminator="\n")
        writer.writerow((
            "package", "priority", "kind", "resource_path", "package_offset", "stored_size",
            "type_id", "class_name", "file_version", "dependency_count",
        ))
        writer.writerows(rows)
    finally:
        if close_output:
            out.close()
    if args.references_output:
        references.sort(key=lambda row: (row[0].casefold(), row[1], row[2].casefold()))
        with open(args.references_output, "w", newline="", encoding="utf-8") as edge_out:
            writer = csv.writer(edge_out, delimiter="\t", lineterminator="\n")
            writer.writerow(("source_resource", "dependency_index", "dependency_path", "serialized_filetime"))
            writer.writerows(references)
    print(f"Exported {len(rows)} package entries from {len(packages)} packages to {args.output}", file=sys.stderr)
    if args.references_output:
        print(f"Exported {len(references)} resource references to {args.references_output}", file=sys.stderr)


if __name__ == "__main__":
    main()
