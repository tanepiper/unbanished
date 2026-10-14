#!/usr/bin/env python3
"""Export save-game external-reference IDs to their serialized paths.

The first block in a Banished save is metadata; the second is a compressed game
state. Its outer version-3 block begins with a nested reference table containing
sequential u32 IDs and serialized System::String paths.
"""
import argparse
import hashlib
import os
import struct
import sys

from crs.crs import BLOCK_END, BLOCK_MARKER, FormatError, Reader
from pkg_extract import read_block_stream


def save_payload(data):
    if len(data) < 0x1C:
        raise FormatError("save header is truncated")
    save_version = struct.unpack_from("<H", data, 0)[0]
    if save_version == 1:
        first_offset, first_size = struct.unpack_from("<II", data, 0x08)
        second_offset, second_size = struct.unpack_from("<II", data, 0x14)
    elif save_version == 2:
        first_offset = 0x24
        first_size = struct.unpack_from("<I", data, 0x14)[0]
        second_offset, second_size = struct.unpack_from("<II", data, 0x1C)
    else:
        raise FormatError(f"unsupported save container version {save_version}")
    if first_offset + first_size != second_offset:
        raise FormatError("metadata and game-state payloads are not contiguous")
    if second_offset + second_size != len(data):
        raise FormatError("game-state payload does not end at EOF")
    return read_block_stream(data[second_offset:second_offset + second_size])


def decode_reference_table(stream):
    if len(stream) < 0x20:
        raise FormatError("expanded game-state stream is truncated")
    if struct.unpack_from("<I", stream, 0)[0] != BLOCK_MARKER:
        raise FormatError("missing outer game-state block marker")
    outer_size, outer_version = struct.unpack_from("<II", stream, 4)
    outer_end = 12 + outer_size
    if outer_version != 3:
        raise FormatError(f"unsupported game-state serializer version {outer_version}")
    if outer_end != len(stream) or struct.unpack_from("<I", stream, outer_end - 4)[0] != BLOCK_END:
        raise FormatError("outer game-state block boundary or terminator is invalid")

    reader = Reader(stream, pos=12, end=outer_end - 4)
    with reader.block() as table:
        count = reader.u32()
        references = []
        for expected_id in range(1, count + 1):
            reference_id = reader.u32()
            if reference_id != expected_id:
                raise FormatError(
                    f"reference ID {reference_id} at row {expected_id} is not sequential"
                )
            references.append((reference_id, reader.string()))
    return references, table.version


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("saves", nargs="*", help="save files (default: save-game/*.sav)")
    parser.add_argument("-o", "--output", default="docs/SAVE-EXTERNAL-REFERENCES.tsv")
    args = parser.parse_args(argv)

    paths = args.saves or [
        os.path.join("save-game", name)
        for name in ("starter-save.sav", "Save2.sav", "Auto.sav", "allportez.sav")
    ]
    rows = []
    for path in paths:
        with open(path, "rb") as source:
            data = source.read()
        stream = save_payload(data)
        references, table_version = decode_reference_table(stream)
        digest = hashlib.sha256(data).hexdigest()
        for reference_id, resource_path in references:
            rows.append((os.path.basename(path), digest, reference_id, resource_path))
        print(
            f"{path}: {len(references)} references, table version {table_version}, "
            f"sha256={digest}"
        )

    with open(args.output, "w", encoding="utf-8", newline="") as output:
        output.write("save_file\tsave_sha256\treference_id\tresource_path\n")
        for save_file, digest, reference_id, resource_path in rows:
            output.write(f"{save_file}\t{digest}\t{reference_id}\t{resource_path}\n")
    print(f"wrote {len(rows)} rows to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
