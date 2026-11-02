#!/usr/bin/env python3
"""Create synthetic metadata fixtures; Pillow is needed only for regeneration.

The fixtures exercise the admitted metadata grammars: typed property sets,
storage declarations (saved name, document type, containers), the document
link table and framed JPEG previews. They are constructed locally and are not
CATIA-authored documents.
"""
import hashlib
import io
import json
from pathlib import Path
import struct

from PIL import Image
from create_synthetic import directory

DEST = Path(__file__).resolve().parents[2] / "tests/fixtures/metadata"
STORAGE = b"\x34\x12CATStorageProperty\x80\x01\0\0\0\0\x22\x0c\0\0\0\x34\x01"
UNICODE = b"\x34\x10CATUnicodeString"
INSTANCE = b"\xa0\x02\0\0\0\0"
TRAILER = b"\x36\x08FINJPL  "
DOC_ID = bytes.fromhex("9a3c7a00000013a04f656324000009b4")
CONTAINER_ID = bytes.fromhex("9a3c7a00000013a04f656324000009b9")


def version(release="33"):
    return (f"<Version>5/<Version><Release>{release}/<Release><ServicePack>0/<ServicePack>"
            "<BuildDate>10-13-2022.18.00/<BuildDate><HotFix>0/<HotFix>").encode()


def value(type_code, payload):
    if type_code == 2:
        return struct.pack("<IH", 2, payload)
    if type_code == 3:
        return struct.pack("<Ii", 3, payload)
    if type_code in (13, 14):
        return struct.pack("<II", type_code, len(payload)) + payload
    if type_code == 12:
        return struct.pack("<II", 12, len(payload)) + b"".join(value(t, v) for t, v in payload)
    raise ValueError(type_code)


def prop(name, type_code, payload):
    return struct.pack("<H", len(name)) + name + value(type_code, payload)


def property_set(name, properties=None):
    """`properties=None` declares the set by name only, like `CATPreview`."""
    out = struct.pack("<H", len(name)) + name
    if properties is not None:
        out += struct.pack("<I", len(properties)) + b"".join(properties)
    return out


def sets(*groups):
    return b"\x01\x01\0" + struct.pack("<I", len(groups)) + b"".join(groups)


def summary(properties):
    return sets(property_set(b"CATSummaryInformation", properties))


def atom(text, short=False):
    if short:
        assert len(text) == 8
        return b"\x20" + text
    return bytes([0x34, len(text)]) + text


def storage(kind, first, second, ids, envelope=b"M", short=False, tail=b""):
    body = STORAGE + bytes([kind, 0]) + UNICODE
    body += INSTANCE + atom(first, short) + b"\x9f" + INSTANCE + atom(second, short) + b"\x9f"
    body += b"".join(b"\x3b\x10" + i for i in ids) + b"\x9f" + TRAILER + tail
    return envelope + bytes(125) + body


def saved_name(target=b"", envelope=b"M"):
    return storage(1, b"CATIA", target, [bytes(16)], envelope)


def document(kind_name=b"CATPart", envelope=b"M"):
    return storage(2, kind_name, kind_name, [bytes(16), DOC_ID], envelope)


def container(type_name, name=None, envelope=b"M", short=False):
    return storage(3, type_name, type_name if name is None else name, [CONTAINER_ID], envelope, short)


def unsupported_storage():
    """The kind-0 CameraStartupContainer variant observed in V5R14 files."""
    body = (b"\x34\x12CATStorageProperty\x80\x01\0\0\0\0\x22\x01\0\0\0\x22\x03\0\0\0\0"
            + UNICODE + INSTANCE + atom(b"CameraStartupContainer") + b"\x9f\0\x34\x0dCATOctetArray" + bytes(24))
    return b"N" + bytes(125) + body


def counted(text):
    return bytes([len(text)]) + text


def locator(kind, ident, document_type, path=None, word=0):
    tail = counted(b"File") + counted(document_type)
    if path is not None:
        tail += struct.pack("<H", len(path)) + path
    doc = counted(b"DocId") + struct.pack("<H", len(tail) + 1) + b"\x01" + tail
    return counted(ident) + struct.pack("<I", word) + counted(kind) + struct.pack("<I", len(doc)) + doc


def link_entry(payload, declare=False, values=(0, 0, 0), tags=(0x22, 0x22, 0x22), shorts=(0, 0), chunk=1):
    out = b""
    if declare:
        out += b"\0\x34\x11CATSymbolProperty"
    out += b"\xe0\x02\0\0\0\0" + b"\x22" + struct.pack("<i", 2)
    if declare:
        out += b"\0\x34\x0dCATOctetArray"
    out += b"\xe0\x03\0\0\0\0" + b"\x22" + struct.pack("<i", len(payload))
    for at in range(0, len(payload), chunk):
        piece = payload[at:at + chunk]
        out += b"\x3b" + bytes([len(piece)]) + piece
    out += b"\x9f"
    out += b"".join(bytes([t]) + struct.pack("<i", v) for t, v in zip(tags, values))
    out += b"".join(b"\x21" + struct.pack("<h", s) for s in shorts)
    return out + b"\x9f"


def links(entries, declared=2, index=(1, 2), index_tag=b"\x24", envelope=b"M"):
    body = b"\x34\x13CATDocumentProperty\x80\x01\0\0\0\0" + b"\x22" + struct.pack("<i", 2) + b"\x22" + struct.pack("<i", declared)
    body += b"".join(entries) + b"".join(index_tag + struct.pack("<I", i) for i in index) + b"\x9f" + TRAILER
    return envelope + bytes(125) + body


GENERIC = locator(b"GenericLocate", b"o", b"CATPart", b"E:\\models\\fixture.CATPart")
CATIA_UUID = locator(b"CATIA", b"\x03" + bytes(range(16)), b"feat", b"PartDesign.feat")
CATWIN = locator(b"CATWin", b"Undef", b"CATUnloadable", None, word=7)
NON_UTF8 = locator(b"CATIA", b"\x03" + bytes(16), b"CATProduct", b"C:\\\xff\xfe\\asm.CATProduct")
LINKS = links([
    link_entry(GENERIC, declare=True, values=(0, 0, 3), chunk=1),
    link_entry(CATIA_UUID, values=(1, 0, 0), tags=(0x22, 0x22, 0x23), shorts=(1, 0), chunk=5),
    link_entry(CATWIN, chunk=3),
    link_entry(NON_UTF8, chunk=4),
    link_entry(b"??", chunk=2),
], declared=3, index=(1, 2, 3))


def encode(name, streams):
    data = bytearray(b"V5_CFV2\0" + bytes(56))
    rows = []
    for stream_name, payload in streams:
        # Reverse physical placement and split every payload. Metadata records
        # and JPEG markers cross extents without changing their logical bytes.
        half = max(1, len(payload)//2)
        second = len(data)
        data.extend(payload[half:])
        data.extend(bytes(7))
        first = len(data)
        data.extend(payload[:half])
        data.extend(bytes(7))
        extents = [(first, half), (second, len(payload)-half)]
        rows.append((stream_name, extents))
    directory_offset = len(data)
    encoded, records = directory(rows)
    struct.pack_into(">II", data, 8, directory_offset, len(encoded))
    data.extend(encoded)
    (DEST / name).write_bytes(data)
    return {"path": name, "streams": [{**r, "descriptor_file_offset": directory_offset+r["descriptor_relative_offset"]} for r in records]}


def main():
    DEST.mkdir(parents=True, exist_ok=True)
    output = io.BytesIO()
    Image.new("RGB", (2,3), (30,100,180)).save(output, format="JPEG", quality=80)
    jpeg = output.getvalue()
    (DEST / "tiny.jpg").write_bytes(jpeg)
    output = io.BytesIO()
    Image.new("RGB", (2,3), (30,100,180)).save(output, format="JPEG", quality=80, progressive=True)
    (DEST / "progressive.jpg").write_bytes(output.getvalue())
    typed = sets(
        property_set(b"CATSummaryInformation", [
            prop(b"LastSaveVersion", 14, version()),
            prop(b"MinimalVersionToRead", 14, b"CATIAV5R33"),
            prop(b"OMBDocument_0001", 3, -2),
            prop(b"FirstStreamed", 14, version("24")),
            prop(b"IgnoredText", 14, b"\x0f\0LastSaveVersion\x0e\0\0\0" + struct.pack("<I", len(version("29"))) + version("29")),
            prop(b"NonAsciiText", 14, b"caf\xe9"),
        ]),
        property_set(b"CATPreview"),
        property_set(b"CATContainerLinksUpdateInformation", [
            prop(b"CATContainersForLinksNotUpdated", 12, [(14, b"CGMGeom"), (14, b"CATMFBRP")]),
            prop(b"NbCATContainersForLinksNotUpdated", 2, 2),
        ]),
    )
    flags = sets(property_set(b"CATProjectFlags", [prop(b"ProjectFlags", 12, [(13, bytes.fromhex("af221d0b11161754631b0a66"))])]))
    records = [
        encode("metadata.cfv2", [
            ("CATPropertySets", typed),
            ("CATPreview", bytes(4)+jpeg+b"opaque-after-image"),
            ("CATStorageProperty", saved_name(b"fixture-saved-name.CATPart")),
            ("Data", typed+saved_name(b"fake.CATPart")+bytes(4)+jpeg),
            ("CATPropertySets", summary([prop(b"LastSaveVersion", 14, version())])),
            ("CATStorageProperty", document()),
            ("CATStorageProperty", container(b"CATProdCont", envelope=b"N")),
            ("CATStorageProperty", container(b"CATMFBRP", short=True)),
            ("CATStorageProperty", container(b"CATCameraContainer", b"Camera")),
            ("CATStorageProperty", unsupported_storage()),
            ("CATPropertySets", flags),
            ("CATDocumentProperty", LINKS),
        ]),
        encode("absent.cfv2", [("CATPropertySets",summary([prop(b"FirstStreamed", 14, version())])), ("CATStorageProperty",saved_name())]),
        encode("conflict.cfv2", [("CATPropertySets",summary([prop(b"LastSaveVersion", 14, version())])), ("CATPropertySets",summary([prop(b"LastSaveVersion", 14, version("29"))]))]),
        encode("ambiguous.cfv2", [("CATPreview",bytes(4)+jpeg), ("CATPreview",bytes(4)+jpeg)]),
        encode("storage-conflict.cfv2", [("CATStorageProperty", document(b"CATPart")), ("CATStorageProperty", document(b"CATProduct")),
                                         ("CATStorageProperty", saved_name(b"a.CATPart")), ("CATStorageProperty", saved_name(b"b.CATPart"))]),
        encode("other-kind.cfv2", [("CATStorageProperty", document(b"CATProcess")), ("CATStorageProperty", saved_name(b"job.CATProcess", envelope=b"N")),
                                   ("CATPropertySets", summary([prop(b"LastSaveVersion", 14, version("12")[:version("12").index(b"<HotFix>")])])),
                                   ("CATDocumentProperty", links([link_entry(GENERIC, declare=True)], declared=1, index=(1,), index_tag=b"\x22", envelope=b"N"))]),
    ]
    (DEST / "expected.json").write_text(json.dumps({
        "fixtures": records,
        "jpeg_sha256": hashlib.sha256(jpeg).hexdigest(), "jpeg_bytes": len(jpeg), "width": 2, "height": 3,
        "saved_name": "fixture-saved-name.CATPart",
        "document_id_hex": DOC_ID.hex(), "container_id_hex": CONTAINER_ID.hex(),
        "generic_path": "E:\\models\\fixture.CATPart",
        "link_payload_hex": {"generic": GENERIC.hex(), "catia": CATIA_UUID.hex(), "catwin": CATWIN.hex(),
                             "non_utf8": NON_UTF8.hex(), "unknown": b"??".hex()},
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
