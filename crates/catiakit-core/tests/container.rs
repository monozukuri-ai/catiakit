use catiakit_core::{
    DiagnosticCode as Code, FeatureStatus, ParseLimits, ParseOptions, ParseStatus,
    container::{ContainerInfo, ContainerLayout, ExtentWordForm, NameForm, reconstruct_stream},
    extract_stream, inspect,
};
use serde_json::Value;

const OUTER: &[u8] = include_bytes!("../../../tests/fixtures/synthetic/outer.cfv2");
const NESTED: &[u8] = include_bytes!("../../../tests/fixtures/synthetic/nested.cfv2");
const INLINE: &[u8] = include_bytes!("../../../tests/fixtures/synthetic/inline.cfv2");
const PADDED: &[u8] = include_bytes!("../../../tests/fixtures/synthetic/padded.cfv2");

fn parsed(data: &[u8]) -> ContainerInfo {
    let result = inspect(data, &ParseOptions::default());
    assert_ne!(
        result.report.status,
        ParseStatus::Rejected,
        "{:?}",
        result.report
    );
    result.value.unwrap().container.unwrap()
}

fn set(data: &mut [u8], offset: usize, word: u32) {
    data[offset..offset + 4].copy_from_slice(&word.to_be_bytes());
}

fn rejected(data: &[u8], options: ParseOptions, code: Code) {
    let result = inspect(data, &options);
    assert_eq!(
        result.report.status,
        ParseStatus::Rejected,
        "{:?}",
        result.report
    );
    assert_eq!(result.report.diagnostics.last().unwrap().code, code);
}

#[test]
fn static_fixtures_match_independent_golden_records() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/synthetic/expected.json"
    ))
    .unwrap();
    for (bytes, fixture) in [OUTER, NESTED, INLINE]
        .into_iter()
        .zip(expected.as_array().unwrap())
    {
        let container = parsed(bytes);
        let records: Vec<_> = fixture["streams"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| !s["extents"].as_array().unwrap().is_empty())
            .collect();
        assert_eq!(container.streams.len(), records.len());
        for (stream, expected) in container.streams.iter().zip(records) {
            assert_eq!(stream.name.as_deref(), expected["name"].as_str());
            assert_eq!(
                stream.descriptor_file_offset,
                expected["descriptor_file_offset"].as_u64().unwrap()
            );
            let directory = &container.directories[stream.directory_id as usize];
            assert_eq!(
                directory.container_offset,
                expected["base"].as_u64().unwrap()
            );
            assert_eq!(
                directory.file_range.offset,
                expected["directory_offset"].as_u64().unwrap()
            );
            for (extent, row) in stream
                .extents
                .iter()
                .zip(expected["extents"].as_array().unwrap())
            {
                assert_eq!(extent.physical_offset, row[0].as_u64().unwrap());
                assert_eq!(
                    extent.file_range.offset,
                    directory.container_offset + extent.physical_offset
                );
                assert_eq!(extent.file_range.length, row[1].as_u64().unwrap());
            }
            let payload = reconstruct_stream(bytes, stream, &ParseLimits::default()).unwrap();
            let hex: String = payload.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, expected["payload_hex"].as_str().unwrap());
        }
    }
}

#[test]
fn duplicate_names_preserve_ids_and_fragment_order() {
    let container = parsed(OUTER);
    assert_eq!(container.streams[0].name, container.streams[1].name);
    assert_ne!(container.streams[0].id, container.streams[1].id);
    assert_eq!(
        extract_stream(OUTER, 0, &ParseOptions::default())
            .value
            .unwrap(),
        b"ghi01"
    );
    assert_eq!(
        extract_stream(OUTER, 1, &ParseOptions::default())
            .value
            .unwrap(),
        b"4567"
    );
    let result = extract_stream(OUTER, 99, &ParseOptions::default());
    assert!(result.value.is_none());
    assert_eq!(
        result.report.diagnostics.last().unwrap().code,
        Code::StreamNotFound
    );
}

#[test]
fn nested_ownership_unknown_fields_and_empty_declarations_are_retained() {
    let result = inspect(NESTED, &ParseOptions::default());
    assert_eq!(result.report.features.container, FeatureStatus::Partial);
    let container = result.value.unwrap().container.unwrap();
    assert_eq!(container.layout, ContainerLayout::NestedDirectories);
    assert_eq!(container.directories[1].depth, 1);
    assert_eq!(container.directories[1].owner_stream_ids, [2]);
    let empty = &container.directories[0].unadmitted_descriptors[0];
    assert_eq!(empty.name, "Empty");
    assert_eq!(empty.reason, Code::EmptyDeclarationUnverified);
    assert_eq!(empty.raw_header_hex.len(), 168);
    assert_eq!(container.streams[0].raw_header_hex.len(), 168);
    assert!(!container.directories[0].opaque_ranges.is_empty());
    assert_eq!(container.raw_outer_header_tail_hex.len(), 96);
    assert_eq!(
        parsed(INLINE).streams[0].name_form,
        NameForm::InlineAsciiUtf16Le
    );
}

#[test]
fn outer_bounds_signature_and_sentinel_are_required() {
    for (offset, word) in [(8, 63), (8, u32::MAX), (12, 0), (12, u32::MAX)] {
        let mut bytes = OUTER.to_vec();
        set(&mut bytes, offset, word);
        rejected(&bytes, ParseOptions::default(), Code::DirectoryRangeInvalid);
    }
    let mut bytes = OUTER.to_vec();
    bytes[96] ^= 1;
    rejected(
        &bytes,
        ParseOptions::default(),
        Code::DirectorySignatureInvalid,
    );
    let mut bytes = OUTER.to_vec();
    *bytes.last_mut().unwrap() = 1;
    rejected(
        &bytes,
        ParseOptions::default(),
        Code::DirectorySentinelInvalid,
    );
    rejected(
        &OUTER[..OUTER.len() - 1],
        ParseOptions::default(),
        Code::DirectoryRangeInvalid,
    );
}

#[test]
fn invalid_named_extents_fail_closed_without_clipping_or_reordering() {
    // First descriptor at 159, two extent rows at 243 and 263.
    for (offset, word, code) in [
        (243, 0, Code::ExtentRangeInvalid),
        (243, 96, Code::ExtentRangeInvalid),
        (243, u32::MAX, Code::ExtentRangeInvalid),
        (247, 0, Code::ExtentRangeInvalid),
        (251, 4, Code::ExtentLogicalMismatch),
        (255, 1, Code::ExtentLogicalMismatch),
        (275, 0, Code::ExtentLogicalMismatch),
        (171, 6, Code::ExtentLogicalMismatch),
        (263, 81, Code::ExtentsOverlap),
        (239, u32::MAX, Code::DescriptorInvalid),
        (239, 0, Code::DescriptorInvalid),
    ] {
        let mut bytes = OUTER.to_vec();
        set(&mut bytes, offset, word);
        rejected(&bytes, ParseOptions::default(), code);
        assert!(
            extract_stream(&bytes, 0, &ParseOptions::default())
                .value
                .is_none()
        );
    }
}

#[test]
fn unknown_and_ordinal_extent_words_do_not_change_payload() {
    let mut ordinal = OUTER.to_vec();
    for (at, value) in [(259, 1), (279, 2), (398, 1)] {
        set(&mut ordinal, at, value);
    }
    assert_eq!(
        parsed(&ordinal).directories[0].extent_word_form,
        ExtentWordForm::Ordinal
    );
    set(&mut ordinal, 259, 0xfeed_beef);
    let result = inspect(&ordinal, &ParseOptions::default());
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ExtentWordUnknown)
    );
    assert_eq!(
        parsed(&ordinal).streams[0].extents[0].trailing_word,
        0xfeed_beef
    );
    assert_eq!(
        extract_stream(&ordinal, 0, &ParseOptions::default())
            .value
            .unwrap(),
        b"ghi01"
    );
}

#[test]
fn non_ascii_name_is_not_misreported_as_an_ascii_suffix() {
    let mut bytes = OUTER.to_vec();
    bytes[144..146].copy_from_slice(&0x3042u16.to_le_bytes());
    let container = parsed(&bytes);
    assert_eq!(container.streams.len(), 2);
    assert!(container.streams[0].name.is_none());
    assert_eq!(container.streams[0].name_form, NameForm::Unknown);
    assert!(
        container.directories[0]
            .opaque_ranges
            .iter()
            .any(|r| r.offset <= 144 && r.end().unwrap() >= 156)
    );
}

#[test]
fn markers_in_headers_gaps_and_middle_of_payload_do_not_select_nested_storage() {
    let mut bytes = OUTER.to_vec();
    for at in [16, 84, 112] {
        bytes[at..at + 8].copy_from_slice(b"V5_CFV2\0");
    }
    assert_eq!(parsed(&bytes).directories.len(), 1);
    let mut bytes = NESTED.to_vec();
    // Body now begins eight bytes before the real inner header.
    set(&mut bytes, 831, 382);
    set(&mut bytes, 903, 120);
    set(&mut bytes, 907, 382);
    set(&mut bytes, 911, 382);
    let container = parsed(&bytes);
    assert_eq!(container.layout, ContainerLayout::OuterDirectory);
    assert_eq!(container.directories.len(), 1);
}

#[test]
fn split_inner_signature_and_outside_owner_directory_are_unsupported() {
    let mut bytes = OUTER.to_vec();
    bytes[64..72].copy_from_slice(b"V5_CFV2\0");
    for (at, word) in [
        (171, 8),
        (243, 64),
        (247, 4),
        (251, 4),
        (263, 68),
        (267, 4),
        (271, 4),
        (275, 4),
    ] {
        set(&mut bytes, at, word);
    }
    for bytes in [bytes, {
        let mut b = NESTED.to_vec();
        set(&mut b, 136, u32::MAX);
        b
    }] {
        let result = inspect(&bytes, &ParseOptions::default());
        assert_eq!(result.report.status, ParseStatus::Partial);
        assert!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.code == Code::NestedStorageUnsupported)
        );
        assert_eq!(
            result.value.unwrap().container.unwrap().directories.len(),
            1
        );
    }
}

#[test]
fn invalid_inner_directory_keeps_outer_evidence_but_rejects_extraction() {
    let mut bytes = NESTED.to_vec();
    bytes[160] ^= 1;
    rejected(
        &bytes,
        ParseOptions::default(),
        Code::DirectorySignatureInvalid,
    );
    let container = inspect(&bytes, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    assert_eq!(container.directories.len(), 1);
    assert_eq!(container.streams.len(), 3);
    assert!(
        extract_stream(&bytes, 0, &ParseOptions::default())
            .value
            .is_none()
    );
}

#[test]
fn aliased_inner_container_is_parsed_once_with_both_owners() {
    let mut bytes = NESTED.to_vec();
    for (at, word) in [(716, 374), (788, 128), (792, 374), (796, 374)] {
        set(&mut bytes, at, word);
    }
    let container = parsed(&bytes);
    assert_eq!(container.directories.len(), 2);
    assert_eq!(container.directories[1].owner_stream_ids, [1, 2]);
}

#[test]
fn all_container_resource_budgets_are_enforced() {
    let defaults = ParseLimits::default();
    for (bytes, limits, code) in [
        (
            OUTER,
            ParseLimits {
                max_directory_bytes: 1,
                ..defaults
            },
            Code::DirectoryLimitExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_streams: 1,
                ..defaults
            },
            Code::StreamCountExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_extents: 1,
                ..defaults
            },
            Code::ExtentCountExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_stream_bytes: 4,
                ..defaults
            },
            Code::StreamLimitExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_total_stream_bytes: 8,
                ..defaults
            },
            Code::TotalStreamLimitExceeded,
        ),
        (
            NESTED,
            ParseLimits {
                max_nesting_depth: 0,
                ..defaults
            },
            Code::NestingLimitExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_scan_steps: 0,
                ..defaults
            },
            Code::ScanLimitExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_name_bytes: 11,
                ..defaults
            },
            Code::NameLimitExceeded,
        ),
        (
            OUTER,
            ParseLimits {
                max_read_bytes: 100,
                ..defaults
            },
            Code::ReadLimitExceeded,
        ),
    ] {
        rejected(bytes, ParseOptions { limits }, code);
    }
    let container = parsed(OUTER);
    let exact = ParseLimits {
        max_input_bytes: OUTER.len() as u64,
        max_directory_bytes: container.directories[0].file_range.length,
        max_streams: 2,
        max_extents: 3,
        max_stream_bytes: 5,
        max_total_stream_bytes: 9,
        max_name_bytes: 12,
        ..defaults
    };
    assert_eq!(
        inspect(OUTER, &ParseOptions { limits: exact })
            .report
            .status,
        ParseStatus::Partial
    );
}

#[test]
fn framing_without_admitted_descriptors_remains_unknown() {
    let mut bytes = OUTER[..96].to_vec();
    bytes.extend_from_slice(b"CATIA_V5 CB0001\0unknown directory bytesCB__END\0");
    let length = bytes.len() as u32 - 96;
    set(&mut bytes, 12, length);
    let result = inspect(&bytes, &ParseOptions::default());
    assert_eq!(result.report.features.container, FeatureStatus::Partial);
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::DirectoryGrammarUnknown)
    );
    let container = result.value.unwrap().container.unwrap();
    assert_eq!(container.layout, ContainerLayout::Unknown);
    assert!(container.streams.is_empty());
    assert_eq!(container.directories[0].opaque_ranges[0].length, 23);
}

#[test]
fn recursive_ownership_obeys_depth_limit_after_retaining_valid_directories() {
    // Wrap a complete fixture as the second stream of another fixture. The
    // inner header may retain 48 opaque bytes after its minimal 16-byte prefix.
    let mut bytes = OUTER.to_vec();
    for _ in 0..3 {
        let inner_len = bytes.len() as u32;
        let mut outer = INLINE[..96].to_vec();
        outer.extend_from_slice(&bytes);
        outer.extend_from_slice(&INLINE[96..]);
        set(&mut outer, 8, 96 + inner_len);
        let ds = 268 + inner_len as usize;
        for (at, value) in [
            (ds + 12, inner_len),
            (ds + 84, 96),
            (ds + 88, inner_len),
            (ds + 92, inner_len),
        ] {
            set(&mut outer, at, value);
        }
        bytes = outer;
    }
    let container = parsed(&bytes);
    assert_eq!(container.directories.len(), 4);
    assert_eq!(container.directories[3].depth, 3);
    let options = ParseOptions {
        limits: ParseLimits {
            max_nesting_depth: 2,
            ..ParseLimits::default()
        },
    };
    rejected(&bytes, options, Code::NestingLimitExceeded);
    let result = inspect(&bytes, &options);
    assert_eq!(result.report.features.container, FeatureStatus::Partial);
    assert_eq!(
        result.value.unwrap().container.unwrap().directories.len(),
        3
    );
}

#[test]
fn externally_modified_stream_metadata_is_revalidated() {
    let container = parsed(OUTER);
    assert_eq!(
        reconstruct_stream(
            OUTER,
            &container.streams[0],
            &ParseLimits {
                max_total_stream_bytes: 4,
                ..ParseLimits::default()
            }
        )
        .unwrap_err()
        .code,
        Code::TotalStreamLimitExceeded
    );
    assert_eq!(
        reconstruct_stream(
            OUTER,
            &container.streams[0],
            &ParseLimits {
                max_read_bytes: 2,
                ..ParseLimits::default()
            }
        )
        .unwrap_err()
        .code,
        Code::ReadLimitExceeded
    );
    for (offset, length, code) in [
        (u64::MAX, 3, Code::ExtentRangeInvalid),
        (80, u64::MAX, Code::ExtentRangeInvalid),
        (80, 2, Code::ExtentLogicalMismatch),
    ] {
        let mut stream = container.streams[0].clone();
        stream.extents[0].file_range.offset = offset;
        stream.extents[0].file_range.length = length;
        assert_eq!(
            reconstruct_stream(OUTER, &stream, &ParseLimits::default())
                .unwrap_err()
                .code,
            code
        );
    }
    let mut stream = container.streams[0].clone();
    stream.extents.clear();
    assert_eq!(
        reconstruct_stream(OUTER, &stream, &ParseLimits::default())
            .unwrap_err()
            .code,
        Code::ExtentLogicalMismatch
    );
    assert_eq!(
        reconstruct_stream(
            OUTER,
            &container.streams[0],
            &ParseLimits {
                max_stream_bytes: 4,
                ..ParseLimits::default()
            }
        )
        .unwrap_err()
        .code,
        Code::StreamLimitExceeded
    );
    assert_eq!(
        reconstruct_stream(
            OUTER,
            &container.streams[0],
            &ParseLimits {
                max_extents: 1,
                ..ParseLimits::default()
            }
        )
        .unwrap_err()
        .code,
        Code::ExtentCountExceeded
    );
}

#[test]
fn every_fixture_truncation_and_single_byte_mutation_stays_bounded() {
    let options = ParseOptions {
        limits: ParseLimits {
            max_scan_steps: 20_000,
            max_streams: 32,
            max_extents: 64,
            max_stream_bytes: 2048,
            max_total_stream_bytes: 8192,
            ..ParseLimits::default()
        },
    };
    for source in [OUTER, NESTED, INLINE] {
        for end in 0..source.len() {
            let _ = inspect(&source[..end], &options);
        }
        for at in 0..source.len() {
            let mut bytes = source.to_vec();
            bytes[at] ^= 0xff;
            let first = inspect(&bytes, &options);
            assert_eq!(first, inspect(&bytes, &options));
            if first.report.status != ParseStatus::Rejected {
                for stream in &first.value.unwrap().container.unwrap().streams {
                    reconstruct_stream(&bytes, stream, &options.limits).unwrap();
                }
            }
        }
    }
}

#[test]
fn zero_padding_after_the_sentinel_is_retained_as_opaque_bytes() {
    let result = inspect(PADDED, &ParseOptions::default());
    assert_eq!(result.report.status, ParseStatus::Partial);
    assert_eq!(result.report.features.container, FeatureStatus::Partial);
    assert!(
        result
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == Code::DirectoryPaddingRetained)
    );
    let container = result.value.unwrap().container.unwrap();
    let outer = parsed(OUTER);
    let names = |c: &ContainerInfo| c.streams.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&container), names(&outer));
    for (padded, plain) in container.streams.iter().zip(&outer.streams) {
        assert_eq!(
            reconstruct_stream(PADDED, padded, &ParseLimits::default()).unwrap(),
            reconstruct_stream(OUTER, plain, &ParseLimits::default()).unwrap()
        );
    }
    let directory = &container.directories[0];
    let padding = directory.opaque_ranges.last().unwrap();
    assert_eq!(padding.length, 24);
    assert_eq!(padding.end().unwrap(), directory.file_range.end().unwrap());
    assert!(PADDED[padding.offset as usize..].iter().all(|b| *b == 0));

    // A nonzero byte after the sentinel keeps the directory invalid.
    let mut corrupt = PADDED.to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] = 1;
    rejected(
        &corrupt,
        ParseOptions::default(),
        Code::DirectorySentinelInvalid,
    );
    // Padding alone, without any sentinel, is also invalid.
    let mut missing = PADDED.to_vec();
    let sentinel = missing.len() - 24 - 8;
    missing[sentinel..sentinel + 8].fill(0);
    rejected(
        &missing,
        ParseOptions::default(),
        Code::DirectorySentinelInvalid,
    );
}
