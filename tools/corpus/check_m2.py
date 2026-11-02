#!/usr/bin/env python3
"""Compare catiakit's admitted streams with the frozen byte inventory.

No corpus download or reference implementation is invoked by this command.
"""
import argparse
import hashlib
import json
import platform
import subprocess
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_file(record, binary):
    path = ROOT / record["path"]
    if sha256(path) != record["sha256"]:
        raise ValueError(f"input hash changed: {record['path']}")
    actual = json.loads(subprocess.check_output([str(binary), str(path)], timeout=30))
    outcome = actual["inspection"]
    if outcome["report"]["status"] == "rejected":
        raise ValueError(f"rejected: {record['path']}")
    container = outcome["value"]["container"]
    streams = container["streams"]
    expected = record["stream_audit"]["streams"]
    by_descriptor = {s["descriptor_file_offset"]: s for s in streams}
    expected_offsets = {s["descriptor_file_offset"] for s in expected}
    if len(by_descriptor) != len(streams) or set(by_descriptor) != expected_offsets:
        raise ValueError(f"descriptor set mismatch: {record['path']}")
    hashes = {s["stream_id"]: s["sha256"] for s in actual["stream_hashes"]}
    fragmented = 0
    extents = 0
    for reference in expected:
        stream = by_descriptor[reference["descriptor_file_offset"]]
        directory = container["directories"][stream["directory_id"]]
        directory_kind = "outer" if directory["kind"] == "outer" else "inner"
        rows = [{
            "file_offset": e["file_range"]["offset"],
            "size_bytes": e["file_range"]["length"],
            "logical_offset": e["logical_offset"],
            "trailing_word": e["trailing_word"],
        } for e in stream["extents"]]
        comparisons = {
            "name": stream["name"] == reference["name"],
            "directory": directory_kind == reference["directory"],
            "length": stream["logical_length"] == reference["size_bytes"],
            "extents": rows == reference["extents"],
            "payload_sha256": hashes[stream["id"]] == reference["sha256"],
        }
        if not all(comparisons.values()):
            raise ValueError(f"{record['path']} at {stream['descriptor_file_offset']}: {comparisons}")
        extents += len(rows)
        fragmented += len(rows) > 1
    directories = container["directories"]
    observed = [(d["container_offset"], d["file_range"]["offset"], d["file_range"]["length"])
                for d in directories if d["kind"] == "nested"]
    expected_inner = [(c["offset"], c["directory_offset"], c["directory_length"])
                      for c in record["header"]["nested_candidates"] if c["directory_valid"]]
    if observed != expected_inner:
        raise ValueError(f"nested directory bounds mismatch: {record['path']}")
    return {
        "path": record["path"], "sha256": record["sha256"], "status": "matched",
        "directories": len(directories), "streams": len(streams),
        "extents": extents, "fragmented_streams": fragmented,
        "unadmitted_declarations": sum(len(d["unadmitted_descriptors"]) for d in directories),
        "layout": container["layout"], "coverage": outcome["report"]["features"]["container"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/examples/stream_inventory")
    parser.add_argument("--output", type=Path, default=ROOT / ".internal/reports/m2/corpus-check.json")
    args = parser.parse_args()
    inventory = ROOT / "tests/corpus/inventory.json"
    records = [r for r in json.loads(inventory.read_text())["files"] if r["kind"] == "catpart"]
    files = [check_file(record, args.binary.resolve()) for record in records]
    report = {
        "schema_version": 1, "checked_at": datetime.now(timezone.utc).isoformat(),
        "scope": "directory framing, admitted stream descriptors and reconstructed bytes",
        "source_inventory_sha256": sha256(inventory),
        "reference_commit": "6289a0d31901aae9445fb99c4bbf7c8effd1d3f1",
        "binary_sha256": sha256(args.binary), "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "platform": platform.platform(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "totals": {key: sum(f[key] for f in files) for key in (
            "directories", "streams", "extents", "fragmented_streams", "unadmitted_declarations")},
        "file_count": len(files), "status": "matched", "files": files,
        "not_validated": ["remote CI", "CATIA application loading", "metadata semantics", "geometry", "topology"],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps({"status": report["status"], "files": len(files), **report["totals"]}))


if __name__ == "__main__":
    main()
