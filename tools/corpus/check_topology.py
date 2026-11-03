#!/usr/bin/env python3
"""Compare the decoded topology of the development CATParts with frozen expectations.

The CLI runs with its default budgets. For every manifest CATPart of the
development split, the check records the coverage of each feature, the
population counts of the recovered graph and of the retained native UV
records, the body kinds and the diagnostic codes, and compares them with
`tests/corpus/topology-expectations.json`. `--write` regenerates that file
from the current decode. The counts describe the decoded graph only; they do
not establish geometric correctness, which the external comparison in
`tools/occt/` addresses separately. The reserved evaluation split is never
decoded by this check.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
import time
import tomllib
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMPARED_FIELDS = ("status", "features", "counts", "native_uv_counts", "bodies", "diagnostics")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def decode(cli, path, timeout):
    started = time.monotonic()
    result = subprocess.run([str(cli), "decode", str(path), "--topology", "--json"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=timeout)
    elapsed = time.monotonic() - started
    if not result.stdout:
        raise RuntimeError(f"no JSON output (exit {result.returncode}): {result.stderr.decode(errors='replace').strip()}")
    return json.loads(result.stdout), elapsed


def observe(outcome):
    """Reduce a decode outcome to the compared fields."""
    report = outcome["report"]
    topology = ((outcome.get("value") or {}).get("topology")) or {}
    native_uv = topology.get("native_uv") or {}
    return {
        "status": report["status"],
        "features": dict(report["features"]),
        "counts": {key: len(value) for key, value in topology.items() if isinstance(value, list)},
        "native_uv_counts": {key: len(value) for key, value in native_uv.items() if isinstance(value, list)},
        "bodies": [
            {"kind": body["kind"], "regions": len(body["regions"]), "export_eligible": body["export_eligible"], "solid_validated": body["solid_validated"]}
            for body in topology.get("bodies", [])
        ],
        "diagnostics": sorted({diagnostic["code"] for diagnostic in report.get("diagnostics", [])}),
    }


def differences(expected, observed):
    return [
        {"field": field, "expected": expected.get(field), "observed": observed.get(field)}
        for field in COMPARED_FIELDS
        if expected.get(field) != observed.get(field)
    ]


def load_expectations(path):
    if not path.exists():
        return {"schema_version": 1, "report_schema_version": None, "budgets": "defaults", "files": []}
    return json.loads(path.read_text("utf-8"))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/catiakit")
    parser.add_argument("--expectations", type=Path, default=ROOT / "tests/corpus/topology-expectations.json")
    parser.add_argument("--output", type=Path, default=ROOT / ".internal/reports/m6/topology-check.json")
    parser.add_argument("--write", action="store_true", help="regenerate the expectations from the current decode instead of comparing")
    parser.add_argument("--timeout", type=int, default=1800, help="seconds per decode")
    parser.add_argument("--case", action="append", default=[], help="restrict to these case ids")
    args = parser.parse_args()
    manifest = tomllib.loads((ROOT / "tests/corpus-manifest.toml").read_text("utf-8"))
    records = [r for r in manifest["file"] if r["kind"] == "catpart" and r["split"] == "development" and (not args.case or r["case_id"] in args.case)]
    expectations = load_expectations(args.expectations)
    expected_by_path = {entry["path"]: entry for entry in expectations["files"]}
    files = []
    report_schema_version = expectations.get("report_schema_version")
    for record in records:
        path = ROOT / record["path"]
        assert digest(path) == record["sha256"], f"input hash changed: {record['path']}"
        entry = {"case_id": record["case_id"], "path": record["path"], "sha256": record["sha256"], "authoring_version": record["authoring_version"]}
        try:
            outcome, elapsed = decode(args.cli, path, args.timeout)
        except (RuntimeError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
            entry.update({"result": "decode_failed", "error": str(error)})
            files.append(entry)
            print(json.dumps({k: entry[k] for k in ("case_id", "result", "error")}))
            continue
        report_schema_version = outcome["report"]["schema_version"]
        observed = observe(outcome)
        entry.update(observed)
        entry["decode_seconds"] = round(elapsed, 3)
        expected = expected_by_path.get(record["path"])
        if args.write:
            entry["result"] = "written"
        elif expected is None:
            entry["result"] = "missing_expectation"
        elif expected["sha256"] != record["sha256"]:
            entry.update({"result": "mismatch", "differences": [{"field": "sha256", "expected": expected["sha256"], "observed": record["sha256"]}]})
        else:
            entry["differences"] = differences(expected, observed)
            entry["result"] = "mismatch" if entry["differences"] else "matched"
        files.append(entry)
        summary = {"case_id": entry["case_id"], "result": entry["result"], "topology": observed["features"]["topology"], "faces": observed["counts"].get("faces", 0), "edges": observed["counts"].get("edges", 0), "bodies": [b["kind"] for b in observed["bodies"]], "seconds": entry["decode_seconds"]}
        if entry.get("differences"):
            summary["differences"] = [d["field"] for d in entry["differences"]]
        print(json.dumps(summary))
    if args.write:
        written = {entry["path"]: entry for entry in files if entry["result"] == "written"}
        merged = [written.pop(e["path"], e) for e in expectations["files"]] + list(written.values())
        for entry in merged:
            entry.pop("result", None)
            entry.pop("decode_seconds", None)
            entry.pop("differences", None)
        expectations.update({"schema_version": 1, "report_schema_version": report_schema_version, "budgets": "defaults", "files": sorted(merged, key=lambda e: e["path"])})
        args.expectations.write_text(json.dumps(expectations, indent=2, sort_keys=False) + "\n", "utf-8")
    report = {
        "schema_version": 1,
        "checked_at": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(),
        "cli_sha256": digest(args.cli),
        "report_schema_version": report_schema_version,
        "budgets": "defaults",
        "expectations": str(args.expectations.relative_to(ROOT)) if args.expectations.is_relative_to(ROOT) else str(args.expectations),
        "totals": {result: sum(1 for f in files if f["result"] == result) for result in ("matched", "mismatch", "missing_expectation", "decode_failed", "written")},
        "coverage": {coverage: sum(1 for f in files if f.get("features", {}).get("topology") == coverage) for coverage in ("complete", "partial", "invalid", "unsupported", "not_requested")},
        "files": files,
        "evidence_limits": [
            "Counts and coverage describe the decoded graph under the default budgets; they do not establish geometric correctness.",
            "Diagnostic codes are compared as sets; messages and positions are not compared.",
            "The reserved evaluation split is not decoded.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", "utf-8")
    print(json.dumps({"totals": report["totals"], "coverage": report["coverage"]}))
    if not args.write and (report["totals"]["mismatch"] or report["totals"]["missing_expectation"] or report["totals"]["decode_failed"]):
        sys.exit(1)


if __name__ == "__main__":
    main()
