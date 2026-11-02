#!/usr/bin/env python3
"""Reproducible corpus acquisition and byte inventory, not a CATIA decoder."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import struct
import subprocess
import tomllib
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[2]
SOURCE_FILE = Path(__file__).with_name("sources.json")
DATA = ROOT / "data/corpus"
REPORTS = ROOT / "tests/corpus"
# Reference-implementation outputs are development material outside the public tree.
REFERENCE = ROOT / ".internal/reports/m0/reference"
MANIFEST = ROOT / "tests/corpus-manifest.toml"
MAX_MEMBER = 128 * 1024 * 1024
MAX_EXPANDED = 512 * 1024 * 1024
SELECTED = {".catpart", ".stp", ".step", ".txt", ".pdf"}
MAGIC = b"V5_CFV2\0"
DIR_MAGIC = b"CATIA_V5 CB0001\0"


def digest(path: Path) -> str:
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def check_archive(path: Path, source: dict) -> None:
    if path.stat().st_size != source["size_bytes"] or digest(path) != source["sha256"]:
        raise ValueError(f"archive differs from pinned source: {path}")


def safe_member(info) -> PurePosixPath:
    name = info.filename
    p = PurePosixPath(name)
    if (p.is_absolute() or ".." in p.parts or "\\" in name or ":" in name
            or stat.S_ISLNK(info.external_attr >> 16)):
        raise ValueError(f"unsafe archive member: {name}")
    if info.file_size > MAX_MEMBER:
        raise ValueError(f"oversized archive member: {name}")
    return p


def selected_members(z: ZipFile):
    infos = z.infolist()
    if sum(i.file_size for i in infos) > MAX_EXPANDED:
        raise ValueError("archive expanded-size limit exceeded")
    seen = set()
    for info in infos:
        p = safe_member(info)
        if info.is_dir() or p.suffix.lower() not in SELECTED:
            continue
        if str(p) in seen:
            raise ValueError(f"duplicate selected member: {p}")
        seen.add(str(p))
        yield info, p


def fetch(sources: dict) -> None:
    for source in sources["archives"]:
        archive = DATA / "archives" / (source["id"] + ".zip")
        archive.parent.mkdir(parents=True, exist_ok=True)
        if not archive.exists():
            partial = archive.with_suffix(".zip.part")
            subprocess.run([
                "curl", "--fail", "--location", "--retry", "2", "--max-time", "180",
                "--user-agent", "Mozilla/5.0", "--output", str(partial), source["url"],
            ], check=True)
            check_archive(partial, source)
            partial.replace(archive)
        check_archive(archive, source)
        with ZipFile(archive) as z:
            for info, relative in selected_members(z):
                target = DATA / source["id"] / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                with z.open(info) as incoming, target.open("wb") as outgoing:
                    shutil.copyfileobj(incoming, outgoing)
        print(f"verified and extracted {source['id']}")


def case_info(name: str) -> dict:
    match = re.search(r"nist_(stc|ftc|ctc)_(\d{2})", name, re.I)
    if match:
        family, number = match.group(1).lower(), match.group(2)
        case = f"{family}-{number}"
        group = f"ftc-stc-{number}" if family in {"ftc", "stc"} else case
    elif "nist_htc" in name.lower():
        case, group = "htc", "htc"
    else:
        return {"case_id": "unknown", "geometry_group": "unknown", "revision": "unknown"}
    revision = re.search(r"_(r[a-z])(?:_|\.)", name, re.I)
    return {
        "case_id": case,
        "geometry_group": group,
        "revision": revision.group(1).lower() if revision else "unknown",
    }


def header_inventory(data: bytes) -> dict:
    result = {"magic_valid": data.startswith(MAGIC), "nested_candidates": []}
    if len(data) < 16 or not result["magic_valid"]:
        return result
    offset, length = struct.unpack_from(">II", data, 8)
    result.update({
        "directory_offset": offset, "directory_length": length,
        "directory_ends_at_eof": offset + length == len(data),
        "directory_magic_valid": data[offset:offset + len(DIR_MAGIC)] == DIR_MAGIC,
        "directory_end_sentinel_present": b"CB__END" in data[max(offset, len(data) - 32):],
        "header_flags_hex": data[56:64].hex(),
    })
    start = 8
    while (start := data.find(MAGIC, start)) >= 0:
        candidate = {"offset": start, "directory_valid": False}
        if start + 16 <= len(data):
            delta, size = struct.unpack_from(">II", data, start + 8)
            directory = start + delta
            candidate.update({"directory_offset": directory, "directory_length": size})
            candidate["directory_valid"] = (
                delta >= 16 and size >= len(DIR_MAGIC) and directory + size <= len(data)
                and data[directory:directory + len(DIR_MAGIC)] == DIR_MAGIC
                and b"CB__END" in data[max(directory, directory + size - 32):directory + size]
            )
        result["nested_candidates"].append(candidate)
        start += len(MAGIC)
    return result


def step_inventory(data: bytes) -> dict:
    text = data.decode("ascii", errors="replace")
    schema = re.search(r"FILE_SCHEMA\s*\(\s*\((.*?)\)\s*\)\s*;", text, re.S)
    body = re.search(r"\bDATA\s*;(.*?)\bENDSEC\s*;", text, re.S)
    byte_body = re.search(rb"\bDATA\s*;(.*?)\bENDSEC\s*;", data, re.S)
    entities = Counter(re.findall(r"#\d+\s*=\s*([A-Z][A-Z0-9_]*)\s*\(", body.group(1) if body else ""))
    return {
        "schema": re.findall(r"'([^']*)'", schema.group(1)) if schema else [],
        "simple_entity_count": sum(entities.values()),
        "entity_counts": dict(sorted(entities.items())),
        "si_unit_evidence": sorted(set(re.findall(r"SI_UNIT\s*\(([^)]*)\)", text))),
        "data_section_sha256": hashlib.sha256(byte_body.group(1)).hexdigest() if byte_body else "unknown",
        "tessellation_only_candidate": "ADVANCED_FACE" not in entities and any("TESSELLAT" in t for t in entities),
        "comparison_validation": "not_performed",
    }


def reference_inventory(sha: str) -> dict:
    path = REFERENCE / "inspect" / f"{sha}.json"
    if not path.exists():
        return {"status": "not_run"}
    raw = json.loads(path.read_text())["result"]
    if raw["status"] != "ok":
        return raw
    summary = raw["summary"]
    entries = summary["entries"]
    notes = summary["notes"]
    saved = next((n for n in notes if n.startswith("last saved by CATIA")), "unknown")
    return {
        "status": "ok", "identity": summary.get("identity", {}),
        "notes": notes, "saved_by": saved,
        "streams": [e for e in entries if e["role"] == "stream"],
        "previews": [e for e in entries if e["role"] == "preview"],
        "external_references": [e for e in entries if e["role"] == "external-reference"],
        "raw_result": path.relative_to(ROOT).as_posix(),
    }


def jpeg_dimensions(data: bytes) -> dict:
    """Read bounded JPEG marker headers; no image decoding or validity claim."""
    pos = 2
    while pos + 4 <= len(data):
        if data[pos] != 0xFF:
            return {}
        marker = data[pos + 1]
        if marker in {0xDA, 0xD9}:  # entropy-coded scan or end
            return {}
        length = int.from_bytes(data[pos + 2:pos + 4], "big")
        if length < 2 or pos + 2 + length > len(data):
            return {}
        if marker in {0xC0, 0xC1, 0xC2} and length >= 8:
            return {"height": int.from_bytes(data[pos + 5:pos + 7], "big"),
                    "width": int.from_bytes(data[pos + 7:pos + 9], "big")}
        pos += 2 + length
    return {}


def audit_reference_streams(data: bytes, header: dict, reference: dict, sha: str) -> dict:
    """Audit reference-selected descriptors against bytes; not independent enumeration."""
    result = {"descriptor_source": "pinned_cadmpeg_inspect", "streams": [],
              "last_save_properties": [], "previews": []}
    if reference.get("status") != "ok":
        return result
    nested = [n for n in header["nested_candidates"] if n["directory_valid"]]
    for stream in reference["streams"]:
        attrs = stream["attributes"]
        if attrs["directory"] == "outer":
            base, directory, directory_length = 0, header["directory_offset"], header["directory_length"]
        elif len(nested) == 1:
            n = nested[0]
            base, directory, directory_length = n["offset"], n["directory_offset"], n["directory_length"]
        else:
            raise ValueError("reference inner directory has no unique bounded candidate")
        ds = directory + int(attrs["desc_offset"])
        k = int(attrs["extent_count"])
        if not (directory <= ds and ds + 0x54 + 20 * k <= directory + directory_length):
            raise ValueError("descriptor exceeds directory")
        if struct.unpack_from(">I", data, ds + 0x50)[0] != k:
            raise ValueError("reference extent count differs from bytes")
        parts, extents, logical = [], [], 0
        for i in range(k):
            off, size, log_size, log_offset, word = struct.unpack_from(">5I", data, ds + 0x54 + i * 20)
            start = base + off
            if size == 0 or start + size > len(data) or log_size != size or log_offset != logical:
                raise ValueError("invalid extent framing")
            parts.append(data[start:start + size])
            extents.append({"file_offset": start, "size_bytes": size, "logical_offset": logical, "trailing_word": word})
            logical += size
        if logical != struct.unpack_from(">I", data, ds + 12)[0] or logical != stream["size"]["size"]:
            raise ValueError("reference stream length differs from bytes")
        payload = b"".join(parts)
        entry = {"name": stream["name"], "directory": attrs["directory"], "descriptor_file_offset": ds,
                 "size_bytes": logical, "sha256": hashlib.sha256(payload).hexdigest(), "extents": extents}
        result["streams"].append(entry)
        if attrs["directory"] != "outer":
            continue
        if stream["name"] == "CATPropertySets":
            key = b"\x0f\x00LastSaveVersion\x0e\x00\x00\x00"
            for match in re.finditer(re.escape(key), payload):
                start = match.end()
                if start + 4 > len(payload):
                    continue
                length = struct.unpack_from("<I", payload, start)[0]
                value = payload[start + 4:start + 4 + length]
                if len(value) != length:
                    continue
                names = ["Version", "Release", "ServicePack", "BuildDate", "HotFix"]
                fields = {}
                for name in names:
                    field = re.search(rb"<" + name.encode() + rb">([^<]*)/<" + name.encode() + rb">", value)
                    if field:
                        fields[name] = field.group(1).decode("ascii")
                if len(fields) == len(names):
                    result["last_save_properties"].append({"stream_descriptor_file_offset": ds,
                        "field_logical_offset": match.start(), "value_length": length, "fields": fields,
                        "field_file_offset": next((e["file_offset"] + match.start() - e["logical_offset"]
                            for e in extents if e["logical_offset"] <= match.start() < e["logical_offset"] + e["size_bytes"]), None),
                        "evidence": "labelled_length_bounded_property_in_reference_selected_stream"})
        if stream["name"] == "CATPreview" and payload.startswith(b"\0\0\0\0\xff\xd8") and payload.endswith(b"\xff\xd9"):
            jpeg = payload[4:]
            path = DATA / "previews" / f"{sha}-{ds}.jpg"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(jpeg)
            result["previews"].append({"stream_descriptor_file_offset": ds, "jpeg_logical_offset": 4,
                "path": path.relative_to(ROOT).as_posix(), "size_bytes": len(jpeg),
                "sha256": hashlib.sha256(jpeg).hexdigest(), **jpeg_dimensions(jpeg)})
    return result


def observed_version(record: dict) -> str:
    observations = record.get("stream_audit", {}).get("last_save_properties", [])
    tuples = {json.dumps(p["fields"], sort_keys=True) for p in observations}
    if len(tuples) != 1:
        return "unknown" if not tuples else "conflicting"
    f = json.loads(tuples.pop())
    return f"V{f['Version']}R{f['Release']} SP{f['ServicePack']} HF{f['HotFix']} ({f['BuildDate']})"


def decode_inventory(sha: str) -> dict:
    path = REFERENCE / "decode" / f"{sha}.json"
    if not path.exists():
        return {"status": "not_run"}
    result = json.loads(path.read_text())["result"]
    if result["status"] != "ok":
        return result
    report = result["report"]
    return {"status": "ok", "model_counts": result["model_counts"],
            "surface_kinds": result["surface_kinds"], "transfer": report["transfer"],
            "losses": [{"code": loss["code"], "severity": loss["severity"]} for loss in report["losses"]],
            "geometry_validation": "not_performed", "raw_result": path.relative_to(ROOT).as_posix()}


def pair_candidates(native: dict, steps: list[dict]) -> list[dict]:
    result = []
    for step in steps:
        if native["case_id"] == "unknown" or native["case_id"] != step["case_id"]:
            continue
        same_revision = native["revision"] == step["revision"] != "unknown"
        different_revision = native["revision"] != step["revision"] and "unknown" not in {native["revision"], step["revision"]}
        mesh = "-tg.stp" in step["path"].lower() or step["step"]["tessellation_only_candidate"]
        result.append({
            "path": step["path"], "sha256": step["sha256"],
            "status": "tessellated_visualization_only" if mesh else (
                "same_case_and_revision_unverified" if same_revision else
                "case_matches_revision_differs" if different_revision else "same_case_revision_unverified"),
            "preferred_geometry_reference": same_revision and "Definitions/" in step["path"] and not mesh,
        })
    return sorted(result, key=lambda p: (not p["preferred_geometry_reference"], p["path"]))


def manifest_text(sources: dict, records: list[dict]) -> str:
    # JSON's strings and scalar syntax used here are valid TOML basic strings/scalars.
    lines = ["schema_version = 1", f"acquired_on = {json.dumps(sources['acquired_on'])}",
             f"license_url = {json.dumps(sources['landing_page'])}", ""]
    for r in records:
        lines.append("[[file]]")
        entry = {k: r[k] for k in ["path", "kind", "archive_id", "archive_member", "source_url",
                                  "size_bytes", "sha256", "case_id", "geometry_group", "revision", "split"]}
        entry.update({"authoring_application": "CATIA" if r["kind"] == "catpart" else "unknown",
                      "authoring_version": observed_version(r),
                      "output_settings": "unknown", "expected_geometry": "not_validated",
                      "reference_paths": [p["path"] for p in r.get("step_candidates", [])],
                      "reference_candidate_statuses": [p["status"] for p in r.get("step_candidates", [])],
                      "geometry_reference_verified": False})
        if r["kind"] == "catpart":
            entry.update({"observed_reference_layout": r["reference"].get("identity", {}).get("dialects", {}).get("primary", {}).get("dialect", "unknown"),
                          "observed_stream_count": len(r["stream_audit"]["streams"]),
                          "observed_preview_count": len(r["stream_audit"]["previews"]),
                          "inspection_expectation": "bounded_container_metadata_and_preview",
                          "geometry_expectation": "not_validated"})
        for key, value in entry.items():
            lines.append(f"{key} = {json.dumps(value, ensure_ascii=False)}")
        lines.append("")
    return "\n".join(lines)


def survey(sources: dict) -> None:
    records, archives = [], []
    for source in sources["archives"]:
        archive = DATA / "archives" / (source["id"] + ".zip")
        check_archive(archive, source)
        with ZipFile(archive) as z:
            selected = {info.filename for info, _ in selected_members(z)}
            archives.append({**source, "members": [
                {"name": i.filename, "size_bytes": i.file_size, "crc32": f"{i.CRC:08x}",
                 "selected": i.filename in selected} for i in z.infolist()]})
            for info in sorted(z.infolist(), key=lambda i: i.filename):
                if info.filename not in selected:
                    continue
                path = DATA / source["id"] / info.filename
                data = path.read_bytes()
                sha = hashlib.sha256(data).hexdigest()
                if len(data) != info.file_size or sha != hashlib.sha256(z.read(info)).hexdigest():
                    raise ValueError(f"extracted file differs from pinned archive: {path}")
                ext = path.suffix.lower()
                kind = "catpart" if ext == ".catpart" else "step" if ext in {".step", ".stp"} else "documentation"
                record = {
                    "path": path.relative_to(ROOT).as_posix(), "kind": kind,
                    "archive_id": source["id"], "archive_member": info.filename,
                    "source_url": source["url"], "size_bytes": len(data), "sha256": sha,
                    **case_info(path.name),
                }
                record["split"] = "evaluation" if record["geometry_group"] in {"ftc-stc-08", "ctc-04", "htc"} else "development"
                if kind == "catpart":
                    record.update({"header": header_inventory(data), "reference": reference_inventory(sha)})
                    record["stream_audit"] = audit_reference_streams(data, record["header"], record["reference"], sha)
                    record["reference_decode"] = decode_inventory(sha)
                elif kind == "step":
                    record["step"] = step_inventory(data)
                records.append(record)
    natives = [r for r in records if r["kind"] == "catpart"]
    steps = [r for r in records if r["kind"] == "step"]
    for native in natives:
        native["step_candidates"] = pair_candidates(native, steps)
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(manifest_text(sources, records))
    write_json(REPORTS / "archive-inventory.json", {"schema_version": 1, "archives": archives})
    write_json(REPORTS / "inventory.json", {"schema_version": 1, "acquired_on": sources["acquired_on"], "files": records})
    summary = {
        "file_counts": dict(Counter(r["kind"] for r in records)),
        "native_saved_by": dict(Counter(r["reference"].get("saved_by", "unknown") for r in natives)),
        "native_observed_version": dict(Counter(observed_version(r) for r in natives)),
        "native_reference_dialect": dict(Counter(r["reference"].get("identity", {}).get("dialects", {}).get("primary", {}).get("dialect", "unknown") for r in natives)),
        "header_valid_count": sum(r["header"]["magic_valid"] and r["header"].get("directory_ends_at_eof", False) and r["header"].get("directory_magic_valid", False) for r in natives),
        "native_without_step_candidates": [r["path"] for r in natives if not r["step_candidates"]],
        "native_with_reference_preview": sum(bool(r["reference"].get("previews")) for r in natives),
        "native_with_audited_preview": sum(bool(r["stream_audit"]["previews"]) for r in natives),
        "audited_stream_count": sum(len(r["stream_audit"]["streams"]) for r in natives),
        "fragmented_stream_count": sum(len(s["extents"]) > 1 for r in natives for s in r["stream_audit"]["streams"]),
        "step_unique_data_sections": len({r["step"]["data_section_sha256"] for r in steps}),
        "reference_decode_status": dict(Counter(r["reference_decode"]["status"] for r in natives)),
        "reference_decode_loss_codes": dict(Counter(l["code"]["code"] for r in natives for l in r["reference_decode"].get("losses", []))),
        "geometry_comparison": "not_performed",
    }
    write_json(REPORTS / "summary.json", summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


def verify(sources: dict) -> None:
    expected = {}
    for source in sources["archives"]:
        archive = DATA / "archives" / (source["id"] + ".zip")
        check_archive(archive, source)
        with ZipFile(archive) as z:
            for info, relative in selected_members(z):
                path = (DATA / source["id"] / relative).relative_to(ROOT).as_posix()
                expected[path] = (info.file_size, hashlib.sha256(z.read(info)).hexdigest())
    manifest = tomllib.loads(MANIFEST.read_text())
    if {r["path"]: (r["size_bytes"], r["sha256"]) for r in manifest["file"]} != expected:
        raise ValueError("manifest differs from pinned archive membership or bytes")
    for item in manifest["file"]:
        path = ROOT / item["path"]
        if path.stat().st_size != item["size_bytes"] or digest(path) != item["sha256"]:
            raise ValueError(f"manifest mismatch: {item['path']}")
    inventory = json.loads((REPORTS / "inventory.json").read_text())["files"]
    if {(r["path"], r["sha256"]) for r in inventory} != {(r["path"], r["sha256"]) for r in manifest["file"]}:
        raise ValueError("inventory and manifest disagree")
    for item in inventory:
        if item["kind"] == "catpart":
            for preview in item.get("stream_audit", {}).get("previews", []):
                if digest(ROOT / preview["path"]) != preview["sha256"]:
                    raise ValueError("extracted preview hash mismatch")
    print(f"verified {len(sources['archives'])} archives and {len(manifest['file'])} extracted files")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["fetch", "survey", "verify"])
    args = parser.parse_args()
    sources = json.loads(SOURCE_FILE.read_text())
    {"fetch": fetch, "survey": survey, "verify": verify}[args.command](sources)


if __name__ == "__main__":
    main()
