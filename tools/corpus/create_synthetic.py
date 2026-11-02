#!/usr/bin/env python3
"""Encode small, explicitly synthetic CFV2 framing examples for container tests."""
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
DEST = ROOT / "tests/fixtures/synthetic"


def directory(streams, inline=False):
    data = bytearray(b"CATIA_V5 CB0001\0" + bytes(32))
    records = []
    for name, extents in streams:
        if not inline:
            data.extend(name.encode("utf-16le") + bytes(3))
        ds = len(data)
        header = bytearray(84)
        header[:12] = bytes([0xA5]) * 12  # uninterpreted header words
        struct.pack_into(">I", header, 12, sum(length for _, length in extents))
        if inline:
            encoded = name.encode("utf-16le")
            header[16:16 + len(encoded)] = encoded
        struct.pack_into(">I", header, 80, len(extents))
        data.extend(header)
        logical = 0
        for offset, length in extents:
            data.extend(struct.pack(">5I", offset, length, length, logical, 0))
            logical += length
        records.append({"name": name, "descriptor_relative_offset": ds, "extents": extents})
    data.extend(b"CB__END\0")
    return data, records


def encode(name, nested=False, inline=False, padding=0):
    data = bytearray(b"V5_CFV2\0" + bytes(56))
    data.extend(b"0123456789abcdefghijklmnopqrstuv")
    records = []
    if nested:
        inner = bytearray(b"V5_CFV2\0" + bytes(8) + b"ABC___DE" + bytes(8))
        inner_dir, inner_records = directory([("MainDataStream", [(22, 2), (16, 3)]), ("SurfacicReps", [(24, 4)])])
        struct.pack_into(">II", inner, 8, len(inner), len(inner_dir))
        inner.extend(inner_dir)
        data.extend(bytes(32))  # inner physical base = 128
        data.extend(inner)
        streams = [("Repeat", [(80, 3), (64, 2)]), ("Repeat", [(68, 4)]), ("Body", [(128, len(inner))]), ("Empty", [])]
        records += [{**r, "directory": "nested", "base": 128, "directory_offset": 160} for r in inner_records]
    else:
        streams = [("Repeat", [(80, 3), (64, 2)]), ("Repeat", [(68, 4)])]
    offset = len(data)
    outer_dir, outer_records = directory(streams, inline=inline)
    # Older documents can declare a directory length that continues with zero
    # bytes after the sentinel; the declared length includes that padding.
    outer_dir.extend(bytes(padding))
    struct.pack_into(">II", data, 8, offset, len(outer_dir))
    data.extend(outer_dir)
    records = [{**r, "directory": "outer", "base": 0, "directory_offset": offset} for r in outer_records] + records
    for record in records:
        record["descriptor_file_offset"] = record["directory_offset"] + record["descriptor_relative_offset"]
        record["payload_hex"] = b"".join(data[record["base"] + o:record["base"] + o + n] for o, n in record["extents"]).hex()
    DEST.mkdir(parents=True, exist_ok=True)
    (DEST / (name + ".cfv2")).write_bytes(data)
    return {"path": name + ".cfv2", "streams": records}


if __name__ == "__main__":
    images = [encode("outer"), encode("nested", nested=True), encode("inline", inline=True),
              encode("padded", padding=24)]
    (DEST / "expected.json").write_text(json.dumps(images, indent=2) + "\n")
