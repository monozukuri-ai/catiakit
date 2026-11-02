use catiakit_core::{
    DiagnosticCode as Code, DocumentKind, FeatureStatus, ParseLimits, ParseOptions, ParseStatus,
    SourceLocation, binary::ByteReader, inspect, inspect_reader,
};
use std::io::{self, Read};

// Deliberately not a valid CATPart: only a complete-length CFV2 header candidate.
fn header_candidate() -> Vec<u8> {
    let mut bytes = b"V5_CFV2\0".to_vec();
    bytes.resize(64, 0xAB);
    bytes
}

#[test]
fn binary_reads_have_explicit_endianness_and_preserve_file_positions() {
    let limits = ParseLimits::default();
    let mut reader = ByteReader::new(&[1, 2, 3, 4, 5, 6, 7, 8], 100, &limits).unwrap();
    assert_eq!(reader.read_u16_be().unwrap(), 0x0102);
    assert_eq!(reader.read_u16_le().unwrap(), 0x0403);
    assert_eq!(reader.file_offset(), 104);
    reader.seek(0).unwrap();
    assert_eq!(reader.read_u32_be().unwrap(), 0x01020304);
    reader.seek(0).unwrap();
    assert_eq!(reader.read_u32_le().unwrap(), 0x04030201);
    reader.seek(0).unwrap();
    assert_eq!(reader.read_u64_be().unwrap(), 0x0102030405060708);
    reader.seek(0).unwrap();
    assert_eq!(reader.read_u64_le().unwrap(), 0x0807060504030201);

    let mut floats = ByteReader::new(
        &[0, 0, 0xC0, 0x3F, 0, 0, 0, 0, 0, 0, 0xF8, 0x3F],
        0,
        &limits,
    )
    .unwrap();
    assert_eq!(floats.read_f32_le().unwrap(), 1.5);
    assert_eq!(floats.read_f64_le().unwrap(), 1.5);
}

#[test]
fn failed_reads_and_seeks_do_not_move_the_cursor() {
    let mut reader = ByteReader::new(&[10, 11, 12], 100, &ParseLimits::default()).unwrap();
    assert_eq!(reader.read_u8().unwrap(), 10);
    let failure = reader.read_u32_le().unwrap_err().into_diagnostic();
    assert_eq!(failure.code, Code::UnexpectedEof);
    assert_eq!(failure.location, Some(SourceLocation::File { offset: 101 }));
    assert_eq!(failure.details.expected_bytes, Some(4));
    assert_eq!(failure.details.available_bytes, Some(2));
    assert_eq!(reader.position(), 1);
    assert_eq!(
        reader.seek(4).unwrap_err().into_diagnostic().code,
        Code::InvalidPosition
    );
    assert_eq!(reader.position(), 1);
    let mut child = reader.subreader(1, 2).unwrap();
    assert_eq!(child.file_offset(), 101);
    assert_eq!(child.read_bytes(2).unwrap(), &[11, 12]);
    assert!(child.read_bytes(0).unwrap().is_empty());
    assert_eq!(reader.position(), 1);
}

#[test]
fn oversized_and_overflowing_ranges_are_errors() {
    let mut limits = ParseLimits {
        max_read_bytes: 2,
        ..ParseLimits::default()
    };
    let mut reader = ByteReader::new(&[1, 2, 3, 4], 0, &limits).unwrap();
    assert_eq!(
        reader.read_u32_be().unwrap_err().into_diagnostic().code,
        Code::ReadLimitExceeded
    );
    assert_eq!(reader.position(), 0);
    limits.max_read_bytes = u64::MAX;
    assert_eq!(
        ByteReader::new(&[1], u64::MAX, &limits)
            .unwrap_err()
            .into_diagnostic()
            .code,
        Code::OffsetOverflow
    );
    let mut reader = ByteReader::new(&[1, 2], 0, &limits).unwrap();
    reader.read_u8().unwrap();
    assert_eq!(
        reader
            .read_bytes(u64::MAX)
            .unwrap_err()
            .into_diagnostic()
            .code,
        Code::OffsetOverflow
    );
    assert_eq!(
        reader
            .subreader(u64::MAX, 1)
            .unwrap_err()
            .into_diagnostic()
            .code,
        Code::OffsetOverflow
    );
    assert_eq!(reader.position(), 1);
}

#[test]
fn every_truncated_header_is_rejected_without_guessing_part_identity() {
    let bytes = header_candidate();
    for length in 0..64 {
        let outcome = inspect(&bytes[..length], &ParseOptions::default());
        assert_eq!(
            outcome.report.status,
            ParseStatus::Rejected,
            "length {length}"
        );
        assert_eq!(outcome.report.features.container, FeatureStatus::Invalid);
        let expected = match length {
            0 => Code::EmptyInput,
            1..8 => Code::TruncatedSignature,
            _ => Code::TruncatedHeader,
        };
        assert_eq!(outcome.report.diagnostics[0].code, expected);
        if let Some(info) = outcome.value {
            assert_eq!(info.kind, DocumentKind::Unknown);
        }
    }
}

#[test]
fn unknown_format_is_distinct_from_truncated_catia() {
    for bytes in [
        b"hello".as_slice(),
        b"ISO-10303-21;",
        &[0xD0, 0xCF, 0x11, 0xE0],
    ] {
        let result = inspect(bytes, &ParseOptions::default());
        assert_eq!(result.report.diagnostics[0].code, Code::UnsupportedFormat);
        assert_eq!(result.report.features.container, FeatureStatus::Unsupported);
        assert!(result.value.is_none());
    }
    for index in 0..8 {
        let mut bytes = header_candidate();
        bytes[index] ^= 1;
        assert_eq!(
            inspect(&bytes, &ParseOptions::default()).report.diagnostics[0].code,
            Code::UnsupportedFormat
        );
    }
}

#[test]
fn signature_does_not_establish_directory_validity() {
    let result = inspect(&header_candidate(), &ParseOptions::default());
    assert_eq!(result.report.status, ParseStatus::Rejected);
    assert_eq!(result.value.unwrap().kind, DocumentKind::Unknown);
    assert_eq!(result.report.features.container, FeatureStatus::Invalid);
    assert_eq!(result.report.features.metadata, FeatureStatus::NotRequested);
    assert_eq!(result.report.features.geometry, FeatureStatus::NotRequested);
    assert_eq!(result.report.features.topology, FeatureStatus::NotRequested);
    assert_eq!(
        result.report.diagnostics[0].code,
        Code::DirectoryRangeInvalid
    );
}

#[test]
fn streaming_input_stops_at_the_limit_plus_one_byte() {
    struct Endless {
        read_bytes: usize,
    }
    impl Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            buffer.fill(0);
            self.read_bytes += buffer.len();
            Ok(buffer.len())
        }
    }
    let options = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: 16,
            ..ParseLimits::default()
        },
    };
    let mut input = Endless { read_bytes: 0 };
    let result = inspect_reader(&mut input, &options);
    assert_eq!(input.read_bytes, 17);
    assert_eq!(result.report.input_size_bytes, None);
    assert_eq!(result.report.diagnostics[0].code, Code::InputLimitExceeded);
    assert_eq!(
        result.report.features.container,
        FeatureStatus::NotRequested
    );

    let valid = include_bytes!("../../../tests/fixtures/synthetic/outer.cfv2");
    let exact = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: valid.len() as u64,
            ..ParseLimits::default()
        },
    };
    assert_eq!(
        inspect_reader(valid.as_slice(), &exact).report.status,
        ParseStatus::Partial
    );
    let mut oversized = valid.to_vec();
    oversized.push(0);
    assert_eq!(
        inspect(&oversized, &exact).report.diagnostics[0].code,
        Code::InputLimitExceeded
    );
}

#[test]
fn read_errors_are_not_misreported_as_truncated_files() {
    struct Failing {
        calls: u8,
    }
    impl Read for Failing {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            match self.calls {
                1 => Err(io::ErrorKind::Interrupted.into()),
                2 => {
                    buffer[..3].copy_from_slice(b"V5_");
                    Ok(3)
                }
                _ => Err(io::ErrorKind::PermissionDenied.into()),
            }
        }
    }
    let result = inspect_reader(Failing { calls: 0 }, &ParseOptions::default());
    let diagnostic = &result.report.diagnostics[0];
    assert_eq!(diagnostic.code, Code::ReadFailed);
    assert_eq!(
        diagnostic.location,
        Some(SourceLocation::File { offset: 3 })
    );
    assert_eq!(
        diagnostic.details.io_error_kind.as_deref(),
        Some("PermissionDenied")
    );
    assert_eq!(result.report.input_size_bytes, None);
}

#[test]
fn chunk_boundaries_do_not_change_inspection_or_json() {
    struct Chunks<'a>(&'a [u8]);
    impl Read for Chunks<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let count = self.0.len().min(buffer.len()).min(3);
            buffer[..count].copy_from_slice(&self.0[..count]);
            self.0 = &self.0[count..];
            Ok(count)
        }
    }
    let bytes = include_bytes!("../../../tests/fixtures/synthetic/outer.cfv2");
    let memory = inspect(bytes, &ParseOptions::default());
    let streaming = inspect_reader(Chunks(bytes), &ParseOptions::default());
    assert_eq!(
        serde_json::to_string(&memory).unwrap(),
        serde_json::to_string(&streaming).unwrap()
    );
    let encoded = serde_json::to_value(&memory).unwrap();
    assert_eq!(encoded["report"]["schema_version"], 2);
    assert_eq!(encoded["report"]["status"], "partial");
    assert_eq!(encoded["value"]["kind"], "unknown");
    let restored: catiakit_core::ParseOutcome<catiakit_core::DocumentInfo> =
        serde_json::from_value(encoded).unwrap();
    assert_eq!(restored, memory);
}

#[test]
fn read_limit_failure_is_separate_from_invalid_input() {
    let options = ParseOptions {
        limits: ParseLimits {
            max_read_bytes: 4,
            ..ParseLimits::default()
        },
    };
    let result = inspect(&header_candidate(), &options);
    assert_eq!(result.report.diagnostics[0].code, Code::ReadLimitExceeded);
    assert_eq!(
        result.report.features.container,
        FeatureStatus::NotRequested
    );
}
