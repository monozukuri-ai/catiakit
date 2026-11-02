use catiakit_core::{
    DiagnosticCode as Code, DocumentKind, FeatureStatus, MetadataState as State, ParseLimits,
    ParseOptions, ParseStatus,
    container::reconstruct_stream,
    extract_preview, extract_preview_reader, inspect, inspect_reader,
    jpeg::{inspect_jpeg, inspect_jpeg_prefix},
    metadata::{PropertyValue, StorageRole, StreamSpan},
};
use serde_json::Value;

const META: &[u8] = include_bytes!("../../../tests/fixtures/metadata/metadata.cfv2");
const ABSENT: &[u8] = include_bytes!("../../../tests/fixtures/metadata/absent.cfv2");
const CONFLICT: &[u8] = include_bytes!("../../../tests/fixtures/metadata/conflict.cfv2");
const AMBIGUOUS: &[u8] = include_bytes!("../../../tests/fixtures/metadata/ambiguous.cfv2");
const STORAGE_CONFLICT: &[u8] =
    include_bytes!("../../../tests/fixtures/metadata/storage-conflict.cfv2");
const OTHER_KIND: &[u8] = include_bytes!("../../../tests/fixtures/metadata/other-kind.cfv2");
const JPEG: &[u8] = include_bytes!("../../../tests/fixtures/metadata/tiny.jpg");

fn expected() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/metadata/expected.json"
    ))
    .unwrap()
}

fn mapped(bytes: &[u8], source: &StreamSpan) -> Vec<u8> {
    source
        .file_ranges
        .iter()
        .flat_map(|r| {
            bytes[r.offset as usize..r.end().unwrap() as usize]
                .iter()
                .copied()
        })
        .collect()
}

fn payload(bytes: &[u8], id: u32) -> Vec<u8> {
    let c = inspect(bytes, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    reconstruct_stream(bytes, c.stream(id).unwrap(), &ParseLimits::default()).unwrap()
}

fn change(bytes: &[u8], id: u32, mutate: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let c = inspect(bytes, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    let stream = c.stream(id).unwrap();
    let mut p = reconstruct_stream(bytes, stream, &ParseLimits::default()).unwrap();
    mutate(&mut p);
    let mut output = bytes.to_vec();
    for e in &stream.extents {
        output[e.file_range.offset as usize..e.file_range.end().unwrap() as usize].copy_from_slice(
            &p[e.logical_offset as usize..(e.logical_offset + e.logical_length) as usize],
        );
    }
    output
}

#[test]
fn typed_metadata_and_fragmented_provenance_match_golden_bytes() {
    let result = inspect(META, &ParseOptions::default());
    assert_eq!(result.report.status, ParseStatus::Partial);
    assert_eq!(result.report.features.metadata, FeatureStatus::Partial);
    let document = result.value.unwrap();
    assert_eq!(document.kind, DocumentKind::Part);
    assert_eq!(document.declared_type.as_deref(), Some("CATPart"));
    let info = document.metadata.unwrap();
    assert_eq!(info.saved_by.state, State::Present);
    let v = info.saved_by.value.unwrap();
    assert_eq!(
        (
            v.version.as_str(),
            v.release.as_str(),
            v.service_pack.as_str(),
            v.hot_fix.as_deref()
        ),
        ("5", "33", "0", Some("0"))
    );
    assert_eq!(info.saved_by.observations.len(), 2);
    assert!(
        info.saved_by
            .observations
            .iter()
            .any(|o| o.source.file_ranges.len() == 2)
    );
    for o in &info.saved_by.observations {
        assert!(mapped(META, &o.source).starts_with(b"\x0f\0LastSaveVersion"));
    }
    assert_eq!(info.previews.state, State::Present);
    assert_eq!(info.previews.items.len(), 1);
    let p = &info.previews.items[0];
    assert_eq!((p.width, p.height, p.components), (2, 3, 3));
    assert_eq!(p.source.file_ranges.len(), 2);
    assert_eq!(mapped(META, &p.source), JPEG);
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::PreviewTrailingData)
    );
    let opaque = info
        .scopes
        .iter()
        .find(|s| s.stream_id == 1)
        .unwrap()
        .opaque_ranges
        .last()
        .unwrap();
    assert_eq!(mapped(META, opaque), b"opaque-after-image");
    // The `Data` stream carries byte-identical decoys that are never admitted.
    assert!(info.scopes.iter().all(|s| s.stream_id != 3));
    assert!(info.property_sets.iter().all(|s| s.stream_id != 3));
    assert!(info.storage.declarations.iter().all(|d| d.stream_id != 3));
}

#[test]
fn property_sets_decode_every_observed_value_type_and_name_only_sets() {
    let info = inspect(META, &ParseOptions::default())
        .value
        .unwrap()
        .metadata
        .unwrap();
    let names: Vec<_> = info
        .property_sets
        .iter()
        .map(|s| (s.stream_id, s.name.as_str(), s.inline))
        .collect();
    assert_eq!(
        names,
        [
            (0, "CATSummaryInformation", true),
            (0, "CATPreview", false),
            (0, "CATContainerLinksUpdateInformation", true),
            (4, "CATSummaryInformation", true),
            (10, "CATProjectFlags", true),
        ]
    );
    let summary = &info.property_sets[0];
    let by_name = |name: &str| {
        summary
            .properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(
        by_name("MinimalVersionToRead").value,
        PropertyValue::Text("CATIAV5R33".into())
    );
    assert_eq!(by_name("OMBDocument_0001").value, PropertyValue::I32(-2));
    assert_eq!(by_name("OMBDocument_0001").type_code, 3);
    // Text outside printable ASCII is retained as bytes, never decoded.
    assert_eq!(
        by_name("NonAsciiText").value,
        PropertyValue::Bytes("636166e9".into())
    );
    // An embedded field inside a string value never becomes a saved version;
    // its control bytes keep the whole value as bytes.
    assert!(matches!(
        by_name("IgnoredText").value,
        PropertyValue::Bytes(_)
    ));
    assert_eq!(info.saved_by.observations.len(), 2);
    for property in &summary.properties {
        let bytes = mapped(META, &property.source);
        assert!(bytes[2..].starts_with(property.name.as_bytes()));
    }
    assert!(info.property_sets[1].properties.is_empty());
    let links = &info.property_sets[2];
    assert_eq!(
        links.properties[0].value,
        PropertyValue::List(vec![
            PropertyValue::Text("CGMGeom".into()),
            PropertyValue::Text("CATMFBRP".into())
        ])
    );
    assert_eq!(links.properties[1].value, PropertyValue::U16(2));
    assert_eq!(
        info.property_sets[4].properties[0].value,
        PropertyValue::List(vec![PropertyValue::Bytes(
            "af221d0b11161754631b0a66".into()
        )])
    );
    // Every set is a contiguous span of its stream and the stream is fully consumed.
    let first_scope = info.scopes.iter().find(|s| s.stream_id == 0).unwrap();
    assert!(first_scope.opaque_ranges.is_empty());
    assert_eq!(first_scope.state, State::Present);
    assert_eq!(first_scope.grammar, "property_sets");
}

#[test]
fn storage_declarations_classify_document_saved_name_and_containers() {
    let result = inspect(META, &ParseOptions::default());
    let document = result.value.unwrap();
    let info = document.metadata.unwrap();
    let storage = &info.storage;
    assert_eq!(storage.state, State::Present);
    let doc = storage.document.as_ref().unwrap();
    assert_eq!(
        (doc.type_name.as_str(), doc.name.as_str()),
        ("CATPart", "CATPart")
    );
    assert_eq!(doc.document_id_hex, expected()["document_id_hex"]);
    assert_eq!(doc.stream_id, 5);
    let saved = storage.saved_name.as_ref().unwrap();
    assert_eq!(saved.name, expected()["saved_name"]);
    assert_eq!(mapped(META, &saved.source), saved.name.as_bytes());
    assert_eq!(saved.stream_id, 2);
    let containers: Vec<_> = storage
        .declarations
        .iter()
        .filter(|d| d.role == StorageRole::Container)
        .map(|d| {
            (
                d.stream_id,
                d.type_name.as_str(),
                d.name.as_str(),
                d.envelope_byte,
            )
        })
        .collect();
    assert_eq!(
        containers,
        [
            (6, "CATProdCont", "CATProdCont", b'N'),
            (7, "CATMFBRP", "CATMFBRP", b'M'),
            (8, "CATCameraContainer", "Camera", b'M'),
        ]
    );
    for d in &storage.declarations {
        assert!(mapped(META, &d.source).starts_with(b"\x34\x12CATStorageProperty"));
        assert!(mapped(META, &d.source).ends_with(b"FINJPL  "));
        assert_eq!(mapped(META, &d.name_source), d.name.as_bytes());
        assert_eq!(
            d.ids_hex.len(),
            if d.role == StorageRole::Document {
                2
            } else {
                1
            }
        );
    }
    assert_eq!(
        storage.declarations[1].ids_hex,
        [
            "00000000000000000000000000000000",
            expected()["document_id_hex"].as_str().unwrap()
        ]
    );
    // The kind-0 variant stays an unsupported scope without affecting the others.
    let unsupported = info.scopes.iter().find(|s| s.stream_id == 9).unwrap();
    assert_eq!(unsupported.state, State::Unsupported);
    assert_eq!(unsupported.opaque_ranges.len(), 1);
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::MetadataScopeUnsupported)
    );
    assert_eq!(result.report.features.metadata, FeatureStatus::Partial);
}

#[test]
fn document_links_decode_locator_families_and_retain_unknown_payloads() {
    let result = inspect(META, &ParseOptions::default());
    let info = result.value.unwrap().metadata.unwrap();
    let links = &info.links;
    assert_eq!(links.state, State::Present);
    assert_eq!(links.declared_count, Some(3));
    assert_eq!(links.index_table, [1, 2, 3]);
    assert_eq!(links.items.len(), 5);
    let generic = links.items[0].locator.as_ref().unwrap();
    assert_eq!(generic.kind, "GenericLocate");
    assert_eq!(generic.id_hex, "6f");
    assert_eq!(generic.document_type, "CATPart");
    assert_eq!(generic.path.as_deref(), expected()["generic_path"].as_str());
    assert_eq!(links.items[0].values, [0, 0, 3]);
    assert_eq!(links.items[0].value_tags, [0x22, 0x22, 0x22]);
    let catia = links.items[1].locator.as_ref().unwrap();
    assert_eq!(catia.kind, "CATIA");
    assert_eq!(catia.id_hex, "03000102030405060708090a0b0c0d0e0f");
    assert_eq!(
        (catia.document_type.as_str(), catia.path.as_deref()),
        ("feat", Some("PartDesign.feat"))
    );
    assert_eq!(links.items[1].value_tags, [0x22, 0x22, 0x23]);
    assert_eq!(links.items[1].shorts, [1, 0]);
    let win = links.items[2].locator.as_ref().unwrap();
    assert_eq!((win.kind.as_str(), win.word), ("CATWin", 7));
    assert_eq!(win.document_type, "CATUnloadable");
    assert!(win.path.is_none() && win.path_hex.is_none());
    let raw = links.items[3].locator.as_ref().unwrap();
    assert!(raw.path.is_none());
    assert_eq!(
        raw.path_hex.as_deref(),
        Some("433a5cfffe5c61736d2e43415450726f64756374")
    );
    assert!(links.items[4].locator.is_none());
    assert_eq!(links.items[4].payload_hex.as_deref(), Some("3f3f"));
    assert_eq!(
        result
            .report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::LinkInvalid)
            .count(),
        1
    );
    for (ordinal, link) in links.items.iter().enumerate() {
        assert_eq!(link.ordinal as usize, ordinal);
        let bytes = mapped(META, &link.source);
        assert!(bytes.starts_with(b"\xe0\x02\0\0\0\0"));
        assert!(bytes.ends_with(b"\x9f"));
    }
    let scope = info.scopes.iter().find(|s| s.stream_id == 11).unwrap();
    assert_eq!(scope.grammar, "document_links");
    assert_eq!(scope.opaque_ranges.len(), 1);
    assert_eq!(scope.opaque_ranges[0].length, 126);
}

#[test]
fn absent_fields_do_not_inherit_first_streamed_or_untyped_strings() {
    let document = inspect(ABSENT, &ParseOptions::default()).value.unwrap();
    assert_eq!(document.kind, DocumentKind::Unknown);
    assert!(document.declared_type.is_none());
    let info = document.metadata.unwrap();
    assert_eq!(info.saved_by.state, State::NotFound);
    assert!(info.saved_by.value.is_none());
    assert_eq!(info.storage.state, State::Present);
    assert!(info.storage.document.is_none());
    assert_eq!(info.storage.saved_name.as_ref().unwrap().name, "");
    assert_eq!(info.links.state, State::NotFound);
    assert_eq!(info.previews.state, State::NotFound);
    let out = extract_preview(ABSENT, None, &ParseOptions::default());
    assert!(out.value.is_none());
    assert_eq!(
        out.report.diagnostics.last().unwrap().code,
        Code::PreviewNotFound
    );
}

#[test]
fn conflicting_declarations_and_unmapped_types_leave_the_kind_unknown() {
    let result = inspect(STORAGE_CONFLICT, &ParseOptions::default());
    let document = result.value.unwrap();
    assert_eq!(document.kind, DocumentKind::Unknown);
    assert!(document.declared_type.is_none());
    let storage = document.metadata.unwrap().storage;
    assert_eq!(storage.state, State::Conflicting);
    assert!(storage.document.is_none() && storage.saved_name.is_none());
    assert_eq!(storage.declarations.len(), 4);
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::StorageConflict)
    );
    assert_eq!(result.report.features.metadata, FeatureStatus::Partial);

    let document = inspect(OTHER_KIND, &ParseOptions::default()).value.unwrap();
    assert_eq!(document.kind, DocumentKind::Unknown);
    assert_eq!(document.declared_type.as_deref(), Some("CATProcess"));
    let info = document.metadata.unwrap();
    assert_eq!(info.storage.state, State::Present);
    assert_eq!(
        info.storage.saved_name.as_ref().unwrap().name,
        "job.CATProcess"
    );
    assert_eq!(info.storage.declarations[1].envelope_byte, b'N');
    // V5R12 forms: a version tuple without HotFix and a `22`-tagged index table.
    let v = info.saved_by.value.as_ref().unwrap();
    assert_eq!((v.release.as_str(), v.hot_fix.as_deref()), ("12", None));
    assert_eq!(info.links.state, State::Present);
    assert_eq!(
        (info.links.index_tag, info.links.index_table.as_slice()),
        (Some(0x22), &[1][..])
    );
    assert_eq!(info.links.items.len(), 1);
}

#[test]
fn conflicting_and_invalid_versions_have_no_governing_value() {
    let info = inspect(CONFLICT, &ParseOptions::default())
        .value
        .unwrap()
        .metadata
        .unwrap();
    assert_eq!(info.saved_by.state, State::Conflicting);
    assert_eq!(info.saved_by.observations.len(), 2);
    assert!(info.saved_by.value.is_none());
    let bytes = change(META, 0, |p| p[59] = b'x');
    let result = inspect(&bytes, &ParseOptions::default());
    assert_eq!(result.report.features.metadata, FeatureStatus::Invalid);
    let v = result.value.unwrap().metadata.unwrap().saved_by;
    assert_eq!(v.state, State::Invalid);
    assert!(v.value.is_none());
    assert_eq!(v.observations.len(), 1);
}

#[test]
fn property_walker_does_not_resynchronize_inside_an_unsupported_value() {
    // Unknown type for the first property; the embedded complete-looking field
    // in a later string must never become a saved version.
    let bytes = change(META, 0, |p| p[51..55].copy_from_slice(&99u32.to_le_bytes()));
    let result = inspect(&bytes, &ParseOptions::default());
    let info = result.value.unwrap().metadata.unwrap();
    assert_eq!(info.saved_by.state, State::Unsupported);
    assert!(info.saved_by.value.is_none());
    assert_eq!(info.saved_by.observations.len(), 1); // Separate second named stream.
    assert!(info.property_sets.iter().all(|s| s.stream_id != 0));
    let bytes = change(META, 0, |p| {
        p[30..34].copy_from_slice(&u32::MAX.to_le_bytes())
    });
    let result = inspect(&bytes, &ParseOptions::default());
    assert_eq!(result.report.status, ParseStatus::Rejected);
    assert_eq!(
        result.report.diagnostics.last().unwrap().code,
        Code::MetadataRecordLimitExceeded
    );
}

#[test]
fn malformed_storage_links_and_fake_atoms_are_not_admitted() {
    let bytes = change(META, 2, |p| *p.last_mut().unwrap() = 0);
    let result = inspect(&bytes, &ParseOptions::default());
    let storage = result.value.unwrap().metadata.unwrap().storage;
    assert_eq!(storage.state, State::Invalid);
    assert!(storage.saved_name.is_none());
    assert!(storage.document.is_some());
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::StorageInvalid)
    );
    // Changing the root atom keeps every string intact but invalidates the scope.
    let bytes = change(META, 2, |p| p[126] = 0);
    let info = inspect(&bytes, &ParseOptions::default())
        .value
        .unwrap()
        .metadata
        .unwrap();
    assert_eq!(info.storage.state, State::Present);
    assert!(info.storage.saved_name.is_none());
    assert_eq!(
        info.scopes.iter().find(|s| s.stream_id == 2).unwrap().state,
        State::Unsupported
    );
    // A broken link trailer invalidates the whole table rather than part of it.
    let bytes = change(META, 11, |p| *p.last_mut().unwrap() = b'x');
    let result = inspect(&bytes, &ParseOptions::default());
    let links = result.value.unwrap().metadata.unwrap().links;
    assert_eq!(links.state, State::Invalid);
    assert!(links.items.is_empty());
    assert_eq!(result.report.features.metadata, FeatureStatus::Invalid);
}

#[test]
fn metadata_json_round_trips_through_the_public_types() {
    let result = inspect(META, &ParseOptions::default());
    let json = serde_json::to_string(&result).unwrap();
    let back: catiakit_core::ParseOutcome<catiakit_core::DocumentInfo> =
        serde_json::from_str(&json).unwrap();
    assert_eq!(back, result);
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["report"]["schema_version"], 2);
    assert_eq!(value["value"]["kind"], "part");
    assert_eq!(value["value"]["declared_type"], "CATPart");
    assert_eq!(
        value["value"]["metadata"]["property_sets"][0]["properties"][2]["value"],
        serde_json::json!({"type": "i32", "value": -2})
    );
}

#[test]
fn extraction_selects_explicit_candidates_and_uses_the_same_input_image() {
    let first = extract_preview(META, None, &ParseOptions::default());
    let streamed = extract_preview_reader(META, None, &ParseOptions::default());
    assert_eq!(first, streamed);
    assert_eq!(first.value.unwrap().bytes, JPEG);
    let out = extract_preview(AMBIGUOUS, None, &ParseOptions::default());
    assert_eq!(
        out.report.diagnostics.last().unwrap().code,
        Code::PreviewAmbiguous
    );
    assert!(out.value.is_none());
    assert_eq!(
        extract_preview(AMBIGUOUS, Some(1), &ParseOptions::default())
            .value
            .unwrap()
            .bytes,
        JPEG
    );
    assert!(
        extract_preview(META, Some(99), &ParseOptions::default())
            .value
            .is_none()
    );
    assert_eq!(
        inspect(META, &ParseOptions::default()),
        inspect_reader(META, &ParseOptions::default())
    );
}

#[test]
fn metadata_limits_fail_separately_from_structural_corruption() {
    let defaults = ParseLimits::default();
    for (limits, code) in [
        (
            ParseLimits {
                max_metadata_bytes: 1,
                ..defaults
            },
            Code::MetadataLimitExceeded,
        ),
        (
            ParseLimits {
                max_metadata_records: 0,
                ..defaults
            },
            Code::MetadataRecordLimitExceeded,
        ),
        (
            ParseLimits {
                max_metadata_steps: 0,
                ..defaults
            },
            Code::MetadataStepLimitExceeded,
        ),
        (
            ParseLimits {
                max_property_bytes: 64,
                ..defaults
            },
            Code::PropertyLimitExceeded,
        ),
        (
            ParseLimits {
                max_preview_bytes: 100,
                ..defaults
            },
            Code::PreviewLimitExceeded,
        ),
        (
            ParseLimits {
                max_preview_pixels: 5,
                ..defaults
            },
            Code::PreviewPixelLimitExceeded,
        ),
    ] {
        let result = inspect(META, &ParseOptions { limits });
        assert_eq!(result.report.status, ParseStatus::Rejected);
        assert_eq!(result.report.diagnostics.last().unwrap().code, code);
        assert_ne!(result.report.features.container, FeatureStatus::Invalid);
        assert_eq!(result.report.features.metadata, FeatureStatus::Partial);
        let document = result.value.unwrap();
        let metadata = document.metadata.as_ref().unwrap();
        assert_eq!(metadata.saved_by.state, State::Incomplete);
        assert_eq!(metadata.storage.state, State::Incomplete);
        assert_eq!(metadata.links.state, State::Incomplete);
        // An interrupted inspection never promotes a document kind.
        assert_eq!(document.kind, DocumentKind::Unknown);
        assert!(document.container.is_some());
    }
}

#[test]
fn jpeg_walker_closes_segments_before_interpreting_eoi_bytes() {
    let mut bytes = JPEG[..2].to_vec();
    bytes.extend_from_slice(b"\xff\xfe\0\x06\xff\xd9xx"); // EOI inside COM payload.
    bytes.extend_from_slice(&JPEG[2..]);
    let info = inspect_jpeg(&bytes, &ParseLimits::default()).unwrap();
    assert_eq!((info.width, info.height), (2, 3));
    let mut tail = bytes.clone();
    tail.extend_from_slice(b"opaque");
    assert!(inspect_jpeg(&tail, &ParseLimits::default()).is_err());
    assert_eq!(
        inspect_jpeg_prefix(&tail, &ParseLimits::default())
            .unwrap()
            .byte_length,
        bytes.len() as u64
    );
    assert_eq!(
        inspect_jpeg(
            include_bytes!("../../../tests/fixtures/metadata/progressive.jpg"),
            &ParseLimits::default()
        )
        .unwrap()
        .sof_marker,
        0xc2
    );
}

#[test]
fn malformed_jpeg_markers_dimensions_and_entropy_termination_are_rejected() {
    let sof = JPEG.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
    let sos = JPEG.windows(2).position(|w| w == [0xff, 0xda]).unwrap();
    for (at, value, code) in [
        (sof + 1, 0xc1, Code::PreviewUnsupported),
        (sof + 3, 0, Code::PreviewInvalid),
        (sof + 4, 12, Code::PreviewUnsupported),
        (sof + 6, 0, Code::PreviewInvalid),
        (sof + 8, 0, Code::PreviewInvalid),
        (sos + 4, 0, Code::PreviewInvalid),
    ] {
        let mut bytes = JPEG.to_vec();
        bytes[at] = value;
        assert_eq!(
            inspect_jpeg(&bytes, &ParseLimits::default())
                .unwrap_err()
                .code,
            code
        );
    }
    for end in 0..JPEG.len() {
        assert!(inspect_jpeg(&JPEG[..end], &ParseLimits::default()).is_err());
    }
    assert!(inspect_jpeg(b"\xff\xd8\xff\xd9", &ParseLimits::default()).is_err());
    let bytes = change(META, 1, |p| p[4] = 0);
    let result = inspect(&bytes, &ParseOptions::default());
    assert_eq!(result.report.features.metadata, FeatureStatus::Invalid);
    assert!(
        extract_preview(&bytes, None, &ParseOptions::default())
            .value
            .is_none()
    );
}

#[test]
fn metadata_fixture_mutations_never_panic_or_change_repeated_reports() {
    for source in [
        META,
        ABSENT,
        CONFLICT,
        AMBIGUOUS,
        STORAGE_CONFLICT,
        OTHER_KIND,
    ] {
        for at in 0..source.len() {
            let mut bytes = source.to_vec();
            bytes[at] ^= 0xff;
            let a = inspect(&bytes, &ParseOptions::default());
            assert_eq!(a, inspect(&bytes, &ParseOptions::default()));
        }
    }
    assert!(!payload(META, 0).is_empty());
}
