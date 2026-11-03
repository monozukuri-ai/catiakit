#!/usr/bin/env python3
"""Rebuild decoded bodies in Open CASCADE and compare them with reference STEP files.

For every manifest CATPart of the development split, the script runs the
catiakit CLI, samples procedural carriers with the `sample_geometry` example,
builds a kernel shape through `catiakit_occt`, checks its validity and
compares volume, surface area, centroid and bounding box with the preferred
STEP reference of the same case. Kernel repairs, approximations and the
reference's revision status are recorded with every comparison. Models
without a complete body are reported as not built.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import subprocess
import sys
import tomllib
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import catiakit_occt  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
VOLUME_RELATIVE_TOLERANCE = 1.0e-4
AREA_RELATIVE_TOLERANCE = 1.0e-4
CENTROID_TOLERANCE_MM = 0.01
BOUNDING_BOX_TOLERANCE_MM = 0.01
MOMENT_RELATIVE_TOLERANCE = 1.0e-4


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def decode(cli, path, records, steps):
    result = subprocess.run(
        [str(cli), "decode", str(path), "--topology", "--json", "--max-topology-records", str(records), "--max-topology-steps", str(steps)],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=600,
    )
    return json.loads(result.stdout)


def sample(sampler, outcome, per_edge, grid_u, grid_v):
    result = subprocess.run([str(sampler), str(per_edge), str(grid_u), str(grid_v)], input=json.dumps(outcome).encode(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=600)
    return json.loads(result.stdout)


def reference_candidates(record, inventory):
    """Every STEP candidate of the case; none is trusted in advance."""
    seen = set()
    out = []
    for c in inventory.get(record["path"], []):
        if c["path"] in seen or c.get("status") == "tessellated_visualization_only":
            continue
        seen.add(c["path"])
        out.append((c["path"], c.get("status")))
    return out


def relative(a, b):
    return abs(a - b) / max(abs(b), 1.0e-12)


def compare(built, reference):
    """Compare in place and, separately, up to a rigid motion of the reference."""
    centroid = math.dist(built["centroid"], reference["centroid"])
    box = max(math.dist(built["bounding_box"][i], reference["bounding_box"][i]) for i in range(2))
    extents = max(abs(a - b) for a, b in zip(built["bounding_box_extents_sorted"], reference["bounding_box_extents_sorted"]))
    offset = abs(built["centroid_offset_from_box_centre"] - reference["centroid_offset_from_box_centre"])
    moments = max(relative(a, b) for a, b in zip(built["principal_moments"], reference["principal_moments"]))
    volume_ok = relative(built["volume"], reference["volume"]) <= VOLUME_RELATIVE_TOLERANCE
    area_ok = relative(built["area"], reference["area"]) <= AREA_RELATIVE_TOLERANCE
    return {
        "volume_relative_difference": relative(built["volume"], reference["volume"]),
        "area_relative_difference": relative(built["area"], reference["area"]),
        "centroid_distance_mm": centroid,
        "bounding_box_distance_mm": box,
        "bounding_box_extents_difference_mm": extents,
        "centroid_offset_difference_mm": offset,
        "principal_moments_relative_difference": moments,
        "face_count_difference": built["counts"]["faces"] - reference["counts"]["faces"],
        "volume_within_tolerance": volume_ok,
        "area_within_tolerance": area_ok,
        "centroid_within_tolerance": centroid <= CENTROID_TOLERANCE_MM,
        "bounding_box_within_tolerance": box <= BOUNDING_BOX_TOLERANCE_MM,
        "same_placement": centroid <= CENTROID_TOLERANCE_MM and box <= BOUNDING_BOX_TOLERANCE_MM,
        "invariants_within_tolerance": volume_ok and area_ok and extents <= BOUNDING_BOX_TOLERANCE_MM and offset <= CENTROID_TOLERANCE_MM and moments <= MOMENT_RELATIVE_TOLERANCE,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=ROOT / "target/release/catiakit")
    parser.add_argument("--sampler", type=Path, default=ROOT / "target/release/examples/sample_geometry")
    parser.add_argument("--output", type=Path, default=ROOT / ".internal/reports/m6/corpus-check.json")
    parser.add_argument("--step-dir", type=Path, default=None, help="write the rebuilt bodies as STEP files into this directory")
    parser.add_argument("--records", type=int, default=2_000_000)
    parser.add_argument("--steps", type=int, default=4_000_000_000)
    parser.add_argument("--case", action="append", default=[], help="restrict to these case ids")
    parser.add_argument("--samples-per-edge", type=int, default=65)
    parser.add_argument("--grid", type=int, nargs=2, default=(41, 21))
    args = parser.parse_args()
    manifest = tomllib.loads((ROOT / "tests/corpus-manifest.toml").read_text("utf-8"))
    inventory = {r["path"]: r.get("step_candidates", []) for r in json.loads((ROOT / "tests/corpus/inventory.json").read_text("utf-8"))["files"] if r["kind"] == "catpart"}
    records = [r for r in manifest["file"] if r["kind"] == "catpart" and r["split"] == "development" and (not args.case or r["case_id"] in args.case)]
    files = []
    for record in records:
        path = ROOT / record["path"]
        assert digest(path) == record["sha256"], f"input hash changed: {record['path']}"
        entry = {"case_id": record["case_id"], "path": record["path"], "sha256": record["sha256"], "authoring_version": record["authoring_version"]}
        outcome = decode(args.cli, path, args.records, args.steps)
        coverage = outcome["report"]["features"]["topology"]
        entry["topology_coverage"] = coverage
        if coverage != "complete":
            entry["status"] = "not_built"
            files.append(entry)
            print(json.dumps({k: entry[k] for k in ("case_id", "status")}))
            continue
        samples = sample(args.sampler, outcome, args.samples_per_edge, *args.grid)
        sample_errors = [e for e in samples["edges"] if e.get("error")] + [g for g in samples["surface_grids"] if g.get("error")]
        entry["sample_errors"] = [{"id": e["id"], "error": e["error"]} for e in sample_errors]
        try:
            shape, repaired, build = catiakit_occt.build_shape(outcome, samples)
        except Exception as error:  # noqa: BLE001 - kernel exceptions are not RuntimeError subclasses
            entry.update({"status": "build_failed", "error": f"{type(error).__name__}: {error}"})
            files.append(entry)
            print(json.dumps({k: entry[k] for k in ("case_id", "status", "error")}))
            continue
        entry["build"] = build.__dict__
        built = catiakit_occt.shape_metrics(shape)
        entry["built"] = built
        if repaired is not None:
            entry["repaired"] = catiakit_occt.shape_metrics(repaired)
        if args.step_dir:
            args.step_dir.mkdir(parents=True, exist_ok=True)
            catiakit_occt.write_step(shape, args.step_dir / f"{record['case_id']}-{record['authoring_version'].split()[0]}.stp")
        references = []
        for reference_path, status in reference_candidates(record, inventory):
            try:
                reference = catiakit_occt.shape_metrics(catiakit_occt.read_step(ROOT / reference_path))
            except Exception as error:  # noqa: BLE001
                references.append({"path": reference_path, "candidate_status": status, "error": f"{type(error).__name__}: {error}"})
                continue
            comparison = compare(built, reference)
            passes = sum(comparison[k] for k in ("volume_within_tolerance", "area_within_tolerance", "invariants_within_tolerance", "same_placement"))
            candidate = {"path": reference_path, "sha256": digest(ROOT / reference_path), "candidate_status": status, **reference, "comparison": comparison, "passes": passes}
            if repaired is not None:
                candidate["comparison_after_repair"] = compare(entry["repaired"], reference)
            references.append(candidate)
        entry["references"] = references
        comparable = [r for r in references if "comparison" in r]
        if comparable:
            best = max(comparable, key=lambda r: (r["passes"], -r["comparison"]["volume_relative_difference"]))
            entry["best_reference"] = best["path"]
            entry["comparison"] = best["comparison"]
            consistent = bool(build.shell_closed and not build.faces_failed and build.free_edges == 0)
            entry["shell_consistent"] = consistent
            if consistent and best["comparison"]["invariants_within_tolerance"]:
                entry["status"] = "matched" if best["comparison"]["same_placement"] else "matched_up_to_placement"
            else:
                entry["status"] = "differs"
        else:
            entry["status"] = "no_reference"
        files.append(entry)
        summary = {"case_id": entry["case_id"], "status": entry["status"], "closed": build.shell_closed, "consistent": entry.get("shell_consistent"), "valid": build.solid_valid, "faces": f"{build.faces_built}/{build.faces_built + len(build.faces_failed)}", "invalid_faces": build.raw_faces_invalid, "free_edges": build.free_edges, "max_dev": f"{build.max_pcurve_deviation:.1e}", "repaired_valid": build.repaired_solid_valid}
        if "comparison" in entry:
            summary.update({"reference": entry["best_reference"].split("/")[-1], "dV": f"{entry['comparison']['volume_relative_difference']:.2e}", "dA": f"{entry['comparison']['area_relative_difference']:.2e}", "dC": f"{entry['comparison']['centroid_distance_mm']:.4f}", "dI": f"{entry['comparison']['principal_moments_relative_difference']:.2e}", "dFaces": entry["comparison"]["face_count_difference"]})
        print(json.dumps(summary))
    report = {
        "schema_version": 1, "checked_at": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(),
        "occt_version": __import__("OCP").__version__, "cli_sha256": digest(args.cli), "sampler_sha256": digest(args.sampler),
        "budgets": {"records": args.records, "steps": args.steps}, "samples_per_edge": args.samples_per_edge, "grid": list(args.grid),
        "tolerances": {"volume_relative": VOLUME_RELATIVE_TOLERANCE, "area_relative": AREA_RELATIVE_TOLERANCE, "centroid_mm": CENTROID_TOLERANCE_MM, "bounding_box_mm": BOUNDING_BOX_TOLERANCE_MM, "principal_moments_relative": MOMENT_RELATIVE_TOLERANCE},
        "totals": {status: sum(1 for f in files if f["status"] == status) for status in ("matched", "matched_up_to_placement", "differs", "no_reference", "build_failed", "not_built")},
        "files": files,
        "evidence_limits": [
            "Every STEP candidate of a case is compared; the best match is reported and the candidates' revision statuses stay as recorded in the manifest.",
            "Kernel repairs (parametric curves, wire orientation, seams, degenerated edges) are applied by Open CASCADE and recorded per face; the decoded data is not modified.",
            "Rolling-ball surfaces and procedural curves are approximated from samples; their notes are listed per model.",
            "STEP references were produced by another CAD exporter; a difference does not identify which side deviates.",
            "Face counts differ by construction between exporters and are diagnostic only.",
            "A reference can be placed differently from the decoded model; volume, area, sorted bounding-box extents, centroid offset from the box centre and principal moments are compared up to that rigid motion, and the placement agreement is reported separately.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["totals"]))


if __name__ == "__main__":
    main()
