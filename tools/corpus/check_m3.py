#!/usr/bin/env python3
"""Validate metadata and JPEG bytes against the frozen inventory and Pillow; no native CAD claim.

Requires Pillow only in this optional development tool. Controlled native-file
mutations are identified separately from unmodified CATIA-authored samples.
"""
import argparse
import hashlib
import io
import json
import platform
import struct
import subprocess
from datetime import datetime, timezone
from pathlib import Path

import PIL
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def parts(data, ranges):
    return b"".join(data[r["offset"]:r["offset"]+r["length"]] for r in ranges)


def payload(data, stream):
    return b"".join(data[r["file_offset"]:r["file_offset"]+r["size_bytes"]] for r in stream["extents"])


def replace(data, stream, value):
    assert len(value) == stream["size_bytes"]
    result = bytearray(data)
    for extent in stream["extents"]:
        a, n, logical = extent["file_offset"], extent["size_bytes"], extent["logical_offset"]
        result[a:a+n] = value[logical:logical+n]
    return bytes(result)


def rename(data, stream):
    result = bytearray(data)
    name = stream["name"].encode("utf-16le")
    start = stream["descriptor_file_offset"] - 3 - len(name)
    assert result[start:start+len(name)] == name
    result[start] = ord("X")
    return bytes(result)


def inspect_document(data, cli):
    result = subprocess.run([str(cli), "inspect", "-", "--json"], input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=30)
    return json.loads(result.stdout)["value"]


def inspect(data, cli):
    return inspect_document(data, cli)["metadata"]


def derived_cases(data, record, cli):
    streams = record["stream_audit"]["streams"]
    preview = next(s for s in streams if s["name"] == "CATPreview")
    version_property = record["stream_audit"]["last_save_properties"][0]
    summary = next(s for s in streams if s["descriptor_file_offset"] == version_property["stream_descriptor_file_offset"])
    assert inspect(rename(data, preview), cli)["previews"]["state"] == "not_found"
    assert inspect(rename(data, summary), cli)["saved_by"]["state"] == "not_found"

    source = payload(data, summary)
    first_end = 34 + 25 + version_property["value_length"]
    packet = bytearray(source[:first_end])
    packet[3:7] = struct.pack("<I", 1)
    packet[30:34] = struct.pack("<I", 1)
    pos = packet.index(b"/<Release>")-1
    packet[pos] = ord("0") if packet[pos] != ord("0") else ord("1")
    other = next(s for s in streams if s["name"] == "CATPropertySets" and s is not summary and s["size_bytes"] >= len(packet))
    before = payload(data, other)
    mutated = replace(data, other, packet+before[len(packet):])
    saved = inspect(mutated, cli)["saved_by"]
    assert saved["state"] == "conflicting" and saved["value"] is None

    anchor = b"\x34\x05CATIA\x9f\xa0\x02\0\0\0\0\x34\0\x9f"
    saved = next(s for s in streams if s["name"] == "CATStorageProperty" and anchor in payload(data, s))
    value = bytearray(payload(data, saved))
    at = value.index(anchor) + 2
    value[at:at+5] = b"CATIB"  # Unsupported owner atom; the slot cannot grow in place.
    document = inspect_document(replace(data, saved, value), cli)
    storage = document["metadata"]["storage"]
    saved_id = next(s["id"] for s in document["container"]["streams"] if s["descriptor_file_offset"] == saved["descriptor_file_offset"])
    scope = next(s for s in document["metadata"]["scopes"] if s["stream_id"] == saved_id)
    assert scope["state"] == "unsupported" and storage.get("saved_name") is None
    assert storage["state"] == "present" and document["kind"] == "part"
    return {"preview_scope_removed": "not_found", "summary_scope_removed": "not_found",
            "summary_conflict_injected": "conflicting", "saved_name_owner_corrupted": "unsupported"}


def check(record, helper, cli):
    data = (ROOT / record["path"]).read_bytes()
    assert digest(data) == record["sha256"]
    result = json.loads(subprocess.check_output([str(helper), str(ROOT / record["path"])], timeout=30))
    outcome = result["inspection"]
    assert outcome["report"]["status"] == "partial"
    info = outcome["value"]["metadata"]
    assert info["saved_by"]["state"] == "present"
    expected = record["stream_audit"]["last_save_properties"]
    assert len(info["saved_by"]["observations"]) == len(expected) == 1
    value = info["saved_by"]["value"]
    mapping = {"version":"Version", "release":"Release", "service_pack":"ServicePack", "build_date":"BuildDate", "hot_fix":"HotFix"}
    assert {mapping[k]:v for k,v in value.items()} == expected[0]["fields"]
    observation = info["saved_by"]["observations"][0]
    assert observation["source"]["logical_offset"] == expected[0]["field_logical_offset"]
    assert observation["source"]["file_ranges"][0]["offset"] == expected[0]["field_file_offset"]
    assert parts(data, observation["source"]["file_ranges"]).startswith(b"\x0f\0LastSaveVersion")
    assert outcome["value"]["kind"] == "part" and outcome["value"]["declared_type"] == "CATPart"
    assert info["storage"]["state"] == "present" and info["storage"]["document"]["type_name"] == "CATPart"
    assert info["storage"]["saved_name"]["name"] == ""
    assert info["links"]["state"] == "present" and info["links"]["items"]
    assert all(s["state"] == "present" for s in info["scopes"])
    assert info["previews"]["state"] == "present" and len(info["previews"]["items"]) == 1
    preview = info["previews"]["items"][0]
    image = parts(data, preview["source"]["file_ranges"])
    extracted = result["preview_hashes"][0]
    assert len(image) == preview["source"]["length"] == extracted["size_bytes"]
    assert digest(image) == extracted["sha256"]
    baseline = record["stream_audit"]["previews"][0]
    stored = (ROOT / baseline["path"]).read_bytes()
    assert digest(stored) == baseline["sha256"]
    assert stored.startswith(image) and image.endswith(b"\xff\xd9")
    with Image.open(io.BytesIO(image)) as decoded:
        decoded.load()
        assert decoded.size == (preview["width"], preview["height"]) == (baseline["width"], baseline["height"])
    return {"path":record["path"], "sha256":record["sha256"], "status":"passed",
            "saved_by":value, "kind":outcome["value"]["kind"], "saved_name":info["storage"]["saved_name"]["name"],
            "link_count":len(info["links"]["items"]),
            "preview":{"sha256":digest(image), "size_bytes":len(image), "width":preview["width"], "height":preview["height"],
                       "m0_stored_size_bytes":len(stored), "opaque_trailing_bytes":len(stored)-len(image),
                       "comparison":"identical" if image == stored else "framed_prefix_with_opaque_tail"},
            "native_derived_cases":derived_cases(data, record, cli)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--helper",type=Path,default=ROOT/"target/debug/examples/metadata_inventory")
    parser.add_argument("--cli",type=Path,default=ROOT/"target/debug/catiakit")
    parser.add_argument("--output",type=Path,default=ROOT/".internal/reports/m3/corpus-check.json")
    args = parser.parse_args()
    inventory = ROOT/"tests/corpus/inventory.json"
    records = [r for r in json.loads(inventory.read_text())["files"] if r["kind"] == "catpart"]
    files = [check(r,args.helper.resolve(),args.cli.resolve()) for r in records]
    report = {"schema_version":1,"checked_at":datetime.now(timezone.utc).isoformat(),"status":"passed",
              "platform":platform.platform(),"pillow_version":PIL.__version__,
              "m0_inventory_sha256":digest(inventory.read_bytes()),"helper_sha256":digest(args.helper.read_bytes()),
              "cli_sha256":digest(args.cli.read_bytes()),"cargo_lock_sha256":digest((ROOT/"Cargo.lock").read_bytes()),
              "totals":{"native_files":len(files),"versions_matched":len(files),"jpeg_frames_extracted_and_decoded":len(files),
                        "previews_matching_full_m0_payload":sum(f["preview"]["comparison"]=="identical" for f in files),
                        "previews_with_opaque_tail":sum(f["preview"]["opaque_trailing_bytes"]>0 for f in files),
                        "native_derived_cases":sum(len(f["native_derived_cases"]) for f in files)},
              "files":files,
              "evidence_limits":["Original native files all contain versions and previews and have empty saved-name slots.",
                  "Absence, conflict, and unsupported-owner cases above are controlled byte mutations, not additional CATIA-authored documents.",
                  "Pillow acceptance does not establish the intended native preview or correctness of all entropy-coded coefficients.",
                  "Remote CI, native CAD application, geometry and topology were not validated."]}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report["totals"]))


if __name__ == "__main__":
    main()
