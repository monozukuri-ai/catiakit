use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_catiakit"))
}

fn input(bytes: &[u8], extra: &[&str]) -> Output {
    let mut child = command()
        .args(["inspect", "-", "--json"])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}

fn container_fixture() -> Vec<u8> {
    include_bytes!("../../../tests/fixtures/synthetic/outer.cfv2").to_vec()
}

fn dump_stdin(bytes: &[u8], id: &str, path: &std::path::Path) -> Output {
    let mut child = command()
        .args(["dump-stream", "-", "--stream-id", id, "--json", "-o"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn dump_reassembles_bytes_and_preserves_existing_outputs() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("stream.bin");
    let first = dump_stdin(&container_fixture(), "0", &path);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"ghi01");
    let result: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["value"]["bytes_written"], 5);
    assert_eq!(result["report"]["status"], "partial");
    let again = dump_stdin(&container_fixture(), "1", &path);
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), b"ghi01");
    let result: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert!(result["value"].is_null());
    assert_eq!(
        result["report"]["diagnostics"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["code"],
        "io.output_failed"
    );
}

#[test]
fn dump_creates_no_file_for_bad_input_or_missing_stream() {
    let sandbox = Sandbox::new();
    for (bytes, id) in [
        (container_fixture(), "99"),
        (b"not a container".to_vec(), "0"),
    ] {
        let path = sandbox.0.join("absent.bin");
        let output = dump_stdin(&bytes, id, &path);
        assert_eq!(output.status.code(), Some(1));
        assert!(!path.exists());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(result["value"].is_null());
    }
}

#[test]
fn inspect_text_and_file_dump_show_stream_ids() {
    let sandbox = Sandbox::new();
    let source = sandbox.0.join("input.bin");
    let destination = sandbox.0.join("output.bin");
    fs::write(&source, container_fixture()).unwrap();
    let output = command().arg("inspect").arg(&source).output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("stream 0: Repeat (5 bytes, 2 extents)")
    );
    let output = command()
        .arg("dump-stream")
        .arg(source)
        .args(["--stream-id", "1", "-o"])
        .arg(&destination)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read(destination).unwrap(), b"4567");
}

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "catiakit-cli-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create test directory: {error}"),
            }
        }
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn stdin_recognition_is_partial_and_json_is_repeatable() {
    let first = input(&container_fixture(), &[]);
    let second = input(&container_fixture(), &[]);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let value: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(value["report"]["status"], "partial");
    assert_eq!(value["value"]["kind"], "unknown");
    assert_eq!(value["report"]["features"]["metadata"], "unsupported");
}

#[test]
fn rejected_inputs_have_machine_readable_codes_and_nonzero_exit() {
    for (data, code, feature) in [
        (b"".as_slice(), "input.empty", "invalid"),
        (b"V5_CFV2", "input.truncated_signature", "invalid"),
        (b"V5_CFV2\0short", "input.truncated_header", "invalid"),
        (b"ordinary text", "format.unsupported", "unsupported"),
    ] {
        let output = input(data, &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["report"]["diagnostics"][0]["code"], code);
        assert_eq!(value["report"]["features"]["container"], feature);
    }
}

#[test]
fn file_extensions_do_not_establish_the_format_or_document_kind() {
    let sandbox = Sandbox::new();
    let wrong_extension = sandbox.0.join("input.CATProduct");
    fs::write(&wrong_extension, container_fixture()).unwrap();
    let file = command()
        .arg("inspect")
        .arg(&wrong_extension)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(file.status.code(), Some(0));
    assert_eq!(file.stdout, input(&container_fixture(), &[]).stdout);

    let misleading = sandbox.0.join("input.CATPart");
    fs::write(&misleading, b"plain text").unwrap();
    let output = command()
        .arg("inspect")
        .arg(misleading)
        .arg("--json")
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        value["report"]["diagnostics"][0]["code"],
        "format.unsupported"
    );
}

#[test]
fn missing_files_produce_json_io_diagnostics() {
    let sandbox = Sandbox::new();
    let output = command()
        .arg("inspect")
        .arg(sandbox.0.join("missing.CATPart"))
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["report"]["diagnostics"][0]["code"], "io.open_failed");
    assert_eq!(value["report"]["input_size_bytes"], Value::Null);
    assert!(output.stderr.is_empty());
}

#[test]
fn limits_are_enforced_from_cli_options() {
    for (args, code) in [
        (["--max-input-bytes", "63"], "limits.input_bytes"),
        (["--max-read-bytes", "4"], "limits.read_bytes"),
    ] {
        let output = input(&container_fixture(), &args);
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(value["report"]["diagnostics"][0]["code"], code);
    }
}

#[test]
fn usage_errors_and_help_follow_cli_conventions() {
    let bad = command()
        .args(["inspect", "-", "--max-input-bytes", "invalid"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());
    assert!(!bad.stderr.is_empty());
    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("inspect"));
    let version = command().arg("--version").output().unwrap();
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );
}

fn preview_stdin(bytes: &[u8], path: &std::path::Path, extra: &[&str]) -> Output {
    let mut child = command()
        .args(["extract-preview", "-", "--json", "-o"])
        .arg(path)
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn preview_extraction_reports_dimensions_and_preserves_existing_file() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("preview.jpg");
    let bytes = include_bytes!("../../../tests/fixtures/metadata/metadata.cfv2");
    let first = preview_stdin(bytes, &path, &[]);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());
    assert_eq!(
        fs::read(&path).unwrap(),
        include_bytes!("../../../tests/fixtures/metadata/tiny.jpg")
    );
    let value: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(value["value"]["preview"]["width"], 2);
    assert_eq!(value["value"]["preview"]["height"], 3);
    assert!(value["value"].get("bytes").is_none());
    let second = preview_stdin(bytes, &path, &[]);
    assert_eq!(second.status.code(), Some(2));
    assert_eq!(
        fs::read(&path).unwrap(),
        include_bytes!("../../../tests/fixtures/metadata/tiny.jpg")
    );
}

#[test]
fn missing_ambiguous_and_limited_previews_create_no_output() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("absent.jpg");
    let absent = include_bytes!("../../../tests/fixtures/metadata/absent.cfv2");
    let ambiguous = include_bytes!("../../../tests/fixtures/metadata/ambiguous.cfv2");
    for (bytes, extra, code) in [
        (absent.as_slice(), vec![], "preview.not_found"),
        (ambiguous.as_slice(), vec![], "preview.ambiguous"),
        (
            ambiguous.as_slice(),
            vec!["--max-preview-pixels", "1"],
            "limits.preview_pixels",
        ),
    ] {
        let out = preview_stdin(bytes, &path, &extra);
        assert_eq!(out.status.code(), Some(1));
        assert!(!path.exists());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["report"]["diagnostics"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["code"],
            code
        );
    }
    assert!(
        preview_stdin(ambiguous, &path, &["--preview-id", "1"])
            .status
            .success()
    );
}

#[test]
fn metadata_inspection_and_preview_file_input_are_consistent() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("model.bin");
    let dest = sandbox.0.join("preview.jpg");
    let bytes = include_bytes!("../../../tests/fixtures/metadata/metadata.cfv2");
    fs::write(&path, bytes).unwrap();
    let out = command().arg("inspect").arg(&path).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("V5R33 SP0 HF0"));
    assert!(text.contains("document kind: Part (declared CATPart)"));
    assert!(text.contains("saved name: \"fixture-saved-name.CATPart\""));
    assert!(text.contains("containers: 3 (CATProdCont, CATMFBRP, CATCameraContainer)"));
    assert!(text.contains("property sets: CATSummaryInformation, CATPreview"));
    assert!(text.contains("link 0: GenericLocate CATPart E:\\models\\fixture.CATPart"));
    assert!(text.contains("link 4: undecoded payload"));
    assert!(!text.contains("link 1:")); // Feature catalogs stay out of the text summary.
    assert!(text.contains("preview 0: 2 x 3"));
    let out = command()
        .arg("extract-preview")
        .arg(path)
        .args(["-o"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        fs::read(dest).unwrap(),
        include_bytes!("../../../tests/fixtures/metadata/tiny.jpg")
    );
}

fn geometry_stdin(bytes: &[u8], extra: &[&str]) -> Output {
    let mut child = command()
        .args(["decode", "-", "--json"])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn topology_is_opt_in_and_requirement_implies_decoding() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/cylinder.cfv2");
    let plain: Value = serde_json::from_slice(&geometry_stdin(bytes, &[]).stdout).unwrap();
    assert!(plain["value"].get("topology").is_none());
    let out = geometry_stdin(bytes, &["--require", "topology"]);
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["report"]["features"]["topology"], "complete");
    assert_eq!(
        value["value"]["topology"]["bodies"][0]["kind"],
        "closed_shell"
    );
    assert_eq!(
        value["value"]["topology"]["bodies"][0]["solid_validated"],
        false
    );
    assert_eq!(out.stdout, geometry_stdin(bytes, &["--topology"]).stdout);
    assert_eq!(
        geometry_stdin(bytes, &["--topology", "--require", "geometry"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn topology_require_and_closure_mismatch_keep_reviewable_partial_json() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/open-sheet.cfv2");
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("topology.json");
    let out = geometry_stdin(
        bytes,
        &[
            "--topology",
            "--expect-closure",
            "closed",
            "--require",
            "topology",
            "-o",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value,
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap()
    );
    assert_eq!(
        value["value"]["topology"]["bodies"][0]["kind"],
        "open_shell"
    );
    assert_eq!(
        value["value"]["topology"]["bodies"][0]["export_eligible"],
        false
    );
    assert_eq!(
        value["report"]["diagnostics"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["code"],
        "topology.required_feature_unavailable"
    );
    let before = fs::read(&path).unwrap();
    assert_eq!(
        geometry_stdin(bytes, &["--topology", "-o", path.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn topology_limits_are_reported_as_rejections() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/box.cfv2");
    for (flag, code) in [
        ("--max-topology-bytes", "limits.topology_bytes"),
        ("--max-topology-records", "limits.topology_records"),
        ("--max-topology-steps", "limits.topology_steps"),
    ] {
        let out = geometry_stdin(bytes, &["--topology", flag, "0"]);
        assert_eq!(out.status.code(), Some(1));
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            value["report"]["diagnostics"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["code"],
            code
        );
    }
}

#[test]
fn incomplete_uv_keeps_boundary_evidence_and_cannot_satisfy_requirement() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/unsupported-curve.cfv2");
    let out = geometry_stdin(bytes, &["--require", "topology"]);
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["report"]["features"]["topology"], "partial");
    assert_eq!(
        value["value"]["topology"]["boundaries"][0]["edges"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    assert_eq!(
        value["value"]["topology"]["bodies"][0]["export_eligible"],
        false
    );
}

#[test]
fn polar_collapsed_edge_is_reported_as_a_point_with_uv() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/cone-pole.cfv2");
    let out = geometry_stdin(bytes, &["--require", "topology"]);
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["value"]["geometry"]["curves"][2]["geometry"]["kind"],
        "point"
    );
    assert_eq!(value["value"]["topology"]["edges"][2]["degenerate"], true);
    assert_eq!(
        value["value"]["topology"]["shells"][0]["euler_characteristic"],
        1
    );
}

#[test]
fn geometry_decode_is_partial_and_does_not_change_inspection_defaults() {
    let bytes = include_bytes!("../../../tests/fixtures/geometry/analytic.cfv2");
    let out = geometry_stdin(bytes, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, geometry_stdin(bytes, &[]).stdout);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["report"]["features"]["geometry"], "partial");
    assert_eq!(value["report"]["features"]["topology"], "not_requested");
    assert_eq!(
        value["value"]["geometry"]["points"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let inspected: Value = serde_json::from_slice(&input(bytes, &[]).stdout).unwrap();
    assert_eq!(inspected["report"]["features"]["geometry"], "not_requested");
}

#[test]
fn geometry_require_retains_partial_json_and_preserves_existing_artifacts() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("geometry.json");
    let path = path.to_str().unwrap();
    let bytes = include_bytes!("../../../tests/fixtures/geometry/analytic.cfv2");
    let first = geometry_stdin(bytes, &["--require", "geometry", "-o", path]);
    assert_eq!(first.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&first.stdout).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value, saved);
    assert_eq!(
        value["report"]["diagnostics"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["code"],
        "geometry.required_feature_unavailable"
    );
    assert!(
        value["value"]["geometry"]["surfaces"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    let before = fs::read(path).unwrap();
    let again = geometry_stdin(bytes, &["-o", path]);
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn geometry_limits_and_missing_files_have_stable_reports() {
    let bytes = include_bytes!("../../../tests/fixtures/geometry/analytic.cfv2");
    let out = geometry_stdin(bytes, &["--max-geometry-bytes", "0"]);
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["report"]["diagnostics"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["code"],
        "limits.geometry_bytes"
    );
    let sandbox = Sandbox::new();
    let out = command()
        .arg("decode")
        .arg(sandbox.0.join("absent.CATPart"))
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["report"]["diagnostics"][0]["code"], "io.open_failed");
}
