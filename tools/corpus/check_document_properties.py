#!/usr/bin/env python3
"""Check document kind, storage declarations, property sets and links on the corpus.

Every CATPart in the manifest must declare `CATPart`, decode all admitted
metadata scopes and expose its own locator in the link table. The optional
`--extra` globs add local files whose hashes are not pinned; they are reported
separately and never fail the check on their own kind.
"""
import argparse
import glob
import hashlib
import json
import platform
import subprocess
import tomllib
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inspect(cli, path):
    result = subprocess.run([str(cli), "inspect", str(path), "--json"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60, check=False)
    return json.loads(result.stdout)


def summarize(outcome):
    value = outcome["value"]
    metadata = value["metadata"]
    storage = metadata["storage"]
    links = metadata["links"]
    locators = [l["locator"] for l in links["items"] if l.get("locator")]
    own = [l for l in locators if l["kind"] == "GenericLocate" and l["document_type"] == value.get("declared_type")]
    return {
        "status": outcome["report"]["status"],
        "metadata_coverage": outcome["report"]["features"]["metadata"],
        "kind": value["kind"],
        "declared_type": value.get("declared_type"),
        "storage_state": storage["state"],
        "saved_name": (storage.get("saved_name") or {}).get("name"),
        "envelope_bytes": sorted({chr(d["envelope_byte"]) for d in storage["declarations"]}),
        "containers": [d["type_name"] for d in storage["declarations"] if d["role"] == "container"],
        "property_sets": [{"name": s["name"], "inline": s["inline"], "properties": [p["name"] for p in s["properties"]]} for s in metadata["property_sets"]],
        "scope_states": dict(Counter(f"{s['grammar']}:{s['state']}" for s in metadata["scopes"])),
        "links_state": links["state"],
        "link_count": len(links["items"]),
        "undecoded_links": sum(1 for l in links["items"] if not l.get("locator")),
        "link_types": dict(Counter(l["document_type"] for l in locators)),
        "own_locator_path": own[0].get("path") if own else None,
        "external_documents": sorted({l["path"] for l in locators if l["document_type"] not in ("feat", "CATUnloadable") and l.get("path") and l not in own}),
        "diagnostics": sorted({d["code"] for d in outcome["report"]["diagnostics"] if d["code"].startswith("metadata")}),
    }


def check_manifest_file(record, cli):
    path = ROOT / record["path"]
    assert digest(path) == record["sha256"], f"input hash changed: {record['path']}"
    summary = summarize(inspect(cli, path))
    assert summary["status"] == "partial", record["path"]
    assert summary["metadata_coverage"] == "partial", (record["path"], summary["diagnostics"])
    assert summary["kind"] == "part" and summary["declared_type"] == "CATPart", record["path"]
    assert summary["storage_state"] == "present" and summary["saved_name"] == "", record["path"]
    assert all(state.endswith(":present") for state in summary["scope_states"]), (record["path"], summary["scope_states"])
    names = {s["name"]: s for s in summary["property_sets"]}
    assert names["CATSummaryInformation"]["inline"] and "LastSaveVersion" in names["CATSummaryInformation"]["properties"]
    assert not names["CATPreview"]["inline"]
    assert summary["links_state"] == "present" and summary["undecoded_links"] == 0, record["path"]
    assert summary["own_locator_path"] and summary["own_locator_path"].lower().endswith(".catpart"), record["path"]
    assert "CATProdCont" in summary["containers"] and "CATPrtCont" in summary["containers"], record["path"]
    return {"path": record["path"], "sha256": record["sha256"], "case_id": record.get("case_id"), **summary}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/catiakit")
    parser.add_argument("--output", type=Path, default=ROOT / ".internal/reports/m3-document/corpus-check.json")
    parser.add_argument("--extra", action="append", default=[], help="glob of additional local files to summarize without hash pinning")
    args = parser.parse_args()
    manifest = tomllib.loads((ROOT / "tests/corpus-manifest.toml").read_text("utf-8"))
    records = [r for r in manifest["file"] if r["kind"] == "catpart"]
    cli = args.cli.resolve()
    files = [check_manifest_file(r, cli) for r in records]
    extra = []
    for pattern in args.extra:
        for path in sorted(glob.glob(pattern, recursive=True)):
            summary = summarize(inspect(cli, Path(path)))
            extra.append({"path": path, "size_bytes": Path(path).stat().st_size, **summary})
    report = {
        "schema_version": 1, "checked_at": datetime.now(timezone.utc).isoformat(), "status": "passed",
        "platform": platform.platform(), "cli_sha256": digest(cli), "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
        "totals": {
            "manifest_files": len(files),
            "kinds": dict(Counter(f["kind"] for f in files)),
            "containers": dict(Counter(c for f in files for c in f["containers"])),
            "link_types": dict(Counter(t for f in files for t in f["link_types"])),
            "extra_files": len(extra),
            "extra_kinds": dict(Counter(f["kind"] for f in extra)),
            "extra_metadata_coverage": dict(Counter(f["metadata_coverage"] for f in extra)),
        },
        "files": files,
        "extra": extra,
        "evidence_limits": [
            "Document kind follows the kind-2 root storage declaration; no CATIA application confirmed the mapping.",
            "Saved names are empty in every manifest file; positive saved names come from synthetic fixtures and unpinned local samples.",
            "Link locators are stored strings; no path was resolved or opened.",
            "Extra files are summarized without hash pinning and are not part of the pass/fail decision.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(report["totals"]))


if __name__ == "__main__":
    main()
