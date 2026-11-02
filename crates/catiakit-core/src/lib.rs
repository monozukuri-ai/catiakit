//! Read-only CATIA V5 inspection with bounded input and structured diagnostics.
//!
//! Inspects CFV2 directories, bounded metadata, and stored JPEG previews.
//! Separate requests decode supported analytic carriers and indexed topology.
//! The document kind follows the root storage declaration; complete geometry
//! coverage remains unclassified.
pub mod binary;
pub mod container;
pub mod geometry;
pub mod jpeg;
pub mod metadata;
pub mod model;
mod native;
pub mod options;
pub mod report;
pub mod topology;

pub use geometry::{GeometryDocument, GeometryModel};
pub use metadata::{MetadataInfo, MetadataState, PreviewImage, PreviewInfo};
pub use model::{ContainerFormat, DocumentInfo, DocumentKind};
pub use options::{ParseLimits, ParseOptions};
pub use report::{
    Diagnostic, DiagnosticCode, DiagnosticDetails, FeatureCoverage, FeatureStatus, ParseOutcome,
    ParseReport, ParseStatus, REPORT_SCHEMA_VERSION, Severity, SourceLocation,
};
pub use topology::{ClosureExpectation, TopologyModel, TopologyOptions, validate_topology};

use binary::ByteReader;
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

const MAGIC: &[u8; 8] = b"V5_CFV2\0";
const OUTER_HEADER_BYTES: u64 = 64;

fn rejection(size: Option<u64>, code: DiagnosticCode, offset: u64) -> ParseOutcome<DocumentInfo> {
    ParseOutcome::rejected(size, Diagnostic::new(code, Severity::Error, Some(offset)))
}

fn io_failure(
    code: DiagnosticCode,
    kind: io::ErrorKind,
    offset: Option<u64>,
) -> ParseOutcome<DocumentInfo> {
    let mut diagnostic = Diagnostic::new(code, Severity::Error, offset);
    diagnostic.details.io_error_kind = Some(format!("{kind:?}"));
    ParseOutcome::rejected(None, diagnostic)
}

fn input_limit(size: Option<u64>, observed: u64, limit: u64) -> ParseOutcome<DocumentInfo> {
    let mut result = rejection(size, DiagnosticCode::InputLimitExceeded, limit);
    result.report.diagnostics[0].details.available_bytes = Some(observed);
    result.report.diagnostics[0].details.limit_bytes = Some(limit);
    result
}

/// Inspect in-memory input. The input cap also applies when no file is opened.
#[must_use]
pub fn inspect(data: &[u8], options: &ParseOptions) -> ParseOutcome<DocumentInfo> {
    let Ok(size) = u64::try_from(data.len()) else {
        return rejection(None, DiagnosticCode::OffsetOverflow, 0);
    };
    if size > options.limits.max_input_bytes {
        return input_limit(Some(size), size, options.limits.max_input_bytes);
    }
    if data.is_empty() {
        let mut result = rejection(Some(0), DiagnosticCode::EmptyInput, 0);
        result.report.features.container = FeatureStatus::Invalid;
        return result;
    }
    let mut reader = match ByteReader::new(data, 0, &options.limits) {
        Ok(reader) => reader,
        Err(error) => return ParseOutcome::rejected(Some(size), error.into_diagnostic()),
    };
    let signature = match reader.read_bytes(size.min(MAGIC.len() as u64)) {
        Ok(bytes) => bytes,
        Err(error) => return ParseOutcome::rejected(Some(size), error.into_diagnostic()),
    };
    if size < MAGIC.len() as u64 && MAGIC.starts_with(signature) {
        let mut result = rejection(Some(size), DiagnosticCode::TruncatedSignature, size);
        result.report.features.container = FeatureStatus::Invalid;
        result.report.diagnostics[0].details.expected_bytes = Some(MAGIC.len() as u64);
        result.report.diagnostics[0].details.available_bytes = Some(size);
        return result;
    }
    if signature != MAGIC {
        let mut result = rejection(Some(size), DiagnosticCode::UnsupportedFormat, 0);
        result.report.features.container = FeatureStatus::Unsupported;
        return result;
    }
    let info = DocumentInfo {
        format: ContainerFormat::CatiaV5Cfv2,
        kind: DocumentKind::Unknown,
        declared_type: None,
        container: None,
        metadata: None,
    };
    if size < OUTER_HEADER_BYTES {
        let mut result = rejection(Some(size), DiagnosticCode::TruncatedHeader, size);
        result.value = Some(info);
        result.report.features.container = FeatureStatus::Invalid;
        result.report.diagnostics[0].details.expected_bytes = Some(OUTER_HEADER_BYTES);
        result.report.diagnostics[0].details.available_bytes = Some(size);
        return result;
    }
    if let Err(error) = reader.read_bytes(OUTER_HEADER_BYTES - MAGIC.len() as u64) {
        let mut result = ParseOutcome::rejected(Some(size), error.into_diagnostic());
        result.value = Some(info);
        return result;
    }
    let mut container = container::ContainerInfo::default();
    let mut diagnostics = Vec::new();
    let parsed = container::parse(data, &options.limits, &mut container, &mut diagnostics);
    let mut info = info;
    if let Err(diagnostic) = parsed {
        let coverage = if diagnostic.code.is_resource_limit() {
            if container.directories.is_empty() {
                FeatureStatus::NotRequested
            } else {
                FeatureStatus::Partial
            }
        } else {
            FeatureStatus::Invalid
        };
        if !container.directories.is_empty() {
            info.container = Some(container);
        }
        diagnostics.push(*diagnostic);
        let mut result = rejection(Some(size), DiagnosticCode::DirectoryRangeInvalid, 8);
        result.value = Some(info);
        result.report.features.container = coverage;
        result.report.diagnostics = diagnostics;
        return result;
    }
    let coverage = if diagnostics.iter().any(|d| d.severity == Severity::Warning)
        || container
            .directories
            .iter()
            .any(|d| !d.opaque_ranges.is_empty())
    {
        FeatureStatus::Partial
    } else {
        FeatureStatus::Complete
    };
    let mut metadata_info = MetadataInfo::default();
    let metadata_result = metadata::parse(
        data,
        &container,
        &options.limits,
        &mut metadata_info,
        &mut diagnostics,
    );
    if metadata_result.is_err() {
        metadata_info.saved_by.state = MetadataState::Incomplete;
        metadata_info.saved_by.value = None;
        metadata_info.storage.state = MetadataState::Incomplete;
        metadata_info.links.state = MetadataState::Incomplete;
        metadata_info.previews.state = MetadataState::Incomplete;
    }
    let metadata_coverage = if metadata_info.scopes.is_empty() {
        FeatureStatus::Unsupported
    } else if metadata_info
        .scopes
        .iter()
        .any(|s| s.state == MetadataState::Invalid)
        || [
            &metadata_info.saved_by.state,
            &metadata_info.storage.state,
            &metadata_info.links.state,
            &metadata_info.previews.state,
        ]
        .contains(&&MetadataState::Invalid)
    {
        FeatureStatus::Invalid
    } else {
        FeatureStatus::Partial
    };
    // The document kind follows the unique admitted root declaration only.
    // A declared type outside the mapped set stays visible as a string.
    if metadata_info.storage.state != MetadataState::Incomplete
        && let Some(document) = &metadata_info.storage.document
    {
        info.declared_type = Some(document.type_name.clone());
        info.kind = match document.type_name.as_str() {
            "CATPart" => DocumentKind::Part,
            "CATProduct" => DocumentKind::Product,
            "CATDrawing" => DocumentKind::Drawing,
            "CATShape" => DocumentKind::Shape,
            _ => DocumentKind::Unknown,
        };
    }
    info.container = Some(container);
    info.metadata = Some(metadata_info);
    let mut status = ParseStatus::Partial;
    let mut metadata_coverage = metadata_coverage;
    if let Err(error) = metadata_result {
        diagnostics.push(*error);
        metadata_coverage = FeatureStatus::Partial;
        status = ParseStatus::Rejected;
    }
    ParseOutcome {
        value: Some(info),
        report: ParseReport {
            schema_version: REPORT_SCHEMA_VERSION,
            status,
            input_size_bytes: Some(size),
            features: FeatureCoverage {
                container: coverage,
                metadata: metadata_coverage,
                ..FeatureCoverage::default()
            },
            diagnostics,
        },
    }
}

/// Read at most the input limit plus one byte, then inspect.
///
/// The extra byte establishes an over-limit input without reading the remainder.
/// Allocation is incremental and fallible. No allocation uses an input-declared
/// length. A load failure leaves `input_size_bytes` unknown.
fn load_input(mut reader: impl Read, options: &ParseOptions) -> Result<Vec<u8>, ParseReport> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let consumed = bytes.len() as u64;
        let budget = options.limits.max_input_bytes.saturating_sub(consumed);
        let request = budget.saturating_add(1).min(buffer.len() as u64) as usize;
        let count = match reader.read(&mut buffer[..request]) {
            Ok(0) => return Ok(bytes),
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(
                    io_failure(DiagnosticCode::ReadFailed, error.kind(), Some(consumed)).report,
                );
            }
        };
        let Some(observed) = consumed.checked_add(count as u64) else {
            return Err(rejection(None, DiagnosticCode::OffsetOverflow, consumed).report);
        };
        if observed > options.limits.max_input_bytes {
            return Err(input_limit(None, observed, options.limits.max_input_bytes).report);
        }
        if bytes.try_reserve(count).is_err() {
            return Err(rejection(None, DiagnosticCode::AllocationFailed, consumed).report);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
}

/// Inspect bounded input without retaining an owned copy in the result.
pub fn inspect_reader(reader: impl Read, options: &ParseOptions) -> ParseOutcome<DocumentInfo> {
    match load_input(reader, options) {
        Ok(bytes) => inspect(&bytes, options),
        Err(report) => ParseOutcome {
            value: None,
            report,
        },
    }
}

/// Inspect and reconstruct a stream from the same in-memory source image.
pub fn extract_stream(
    data: &[u8],
    stream_id: u32,
    options: &ParseOptions,
) -> ParseOutcome<Vec<u8>> {
    let outcome = inspect(data, options);
    let mut report = outcome.report;
    if report.status == ParseStatus::Rejected {
        return ParseOutcome {
            value: None,
            report,
        };
    }
    let stream = outcome
        .value
        .as_ref()
        .and_then(|v| v.container.as_ref())
        .and_then(|v| v.stream(stream_id));
    let result = match stream {
        Some(stream) => container::reconstruct_stream(data, stream, &options.limits),
        None => Err(Box::new(Diagnostic::new(
            DiagnosticCode::StreamNotFound,
            Severity::Error,
            None,
        ))),
    };
    match result {
        Ok(bytes) => ParseOutcome {
            value: Some(bytes),
            report,
        },
        Err(error) => {
            report.status = ParseStatus::Rejected;
            report.diagnostics.push(*error);
            ParseOutcome {
                value: None,
                report,
            }
        }
    }
}

/// Load once, then inspect and extract. No second read of a changing file occurs.
pub fn extract_stream_reader(
    reader: impl Read,
    stream_id: u32,
    options: &ParseOptions,
) -> ParseOutcome<Vec<u8>> {
    match load_input(reader, options) {
        Ok(bytes) => extract_stream(&bytes, stream_id, options),
        Err(report) => ParseOutcome {
            value: None,
            report,
        },
    }
}

pub fn extract_stream_file(
    path: impl AsRef<Path>,
    stream_id: u32,
    options: &ParseOptions,
) -> ParseOutcome<Vec<u8>> {
    match File::open(path) {
        Ok(file) => extract_stream_reader(file, stream_id, options),
        Err(error) => ParseOutcome {
            value: None,
            report: io_failure(DiagnosticCode::OpenFailed, error.kind(), None).report,
        },
    }
}

/// Extract a stored JPEG. Without an id, exactly one fully admitted candidate
/// and no invalid/unsupported preview candidate must be present.
pub fn extract_preview(
    data: &[u8],
    preview_id: Option<u32>,
    options: &ParseOptions,
) -> ParseOutcome<PreviewImage> {
    let outcome = inspect(data, options);
    let mut report = outcome.report;
    if report.status == ParseStatus::Rejected {
        return ParseOutcome {
            value: None,
            report,
        };
    }
    let doc = outcome.value.as_ref();
    let previews = doc.and_then(|d| d.metadata.as_ref()).map(|m| &m.previews);
    let selection = previews.and_then(|p| match preview_id {
        Some(id) => p.items.iter().find(|v| v.id == id),
        None if p.state == MetadataState::Present && p.items.len() == 1 => p.items.first(),
        _ => None,
    });
    let result = (|| {
        let selected = selection.ok_or_else(|| {
            Box::new(Diagnostic::new(
                if preview_id.is_none() && previews.is_some_and(|p| !p.items.is_empty()) {
                    DiagnosticCode::PreviewAmbiguous
                } else {
                    DiagnosticCode::PreviewNotFound
                },
                Severity::Error,
                None,
            ))
        })?;
        let stream = doc
            .and_then(|d| d.container.as_ref())
            .and_then(|c| c.stream(selected.source.stream_id))
            .ok_or_else(|| {
                Box::new(Diagnostic::new(
                    DiagnosticCode::StreamNotFound,
                    Severity::Error,
                    None,
                ))
            })?;
        let mut bytes = container::reconstruct_stream(data, stream, &options.limits)?;
        // Copy only the framed JPEG; opaque trailing bytes remain in the report.
        // Inspection and extraction share this source image.
        bytes.truncate((4 + selected.source.length) as usize);
        bytes.drain(..4);
        Ok::<_, Box<Diagnostic>>(PreviewImage {
            info: selected.clone(),
            bytes,
        })
    })();
    match result {
        Ok(image) => ParseOutcome {
            value: Some(image),
            report,
        },
        Err(error) => {
            report.status = ParseStatus::Rejected;
            report.diagnostics.push(*error);
            ParseOutcome {
                value: None,
                report,
            }
        }
    }
}

pub fn extract_preview_reader(
    reader: impl Read,
    preview_id: Option<u32>,
    options: &ParseOptions,
) -> ParseOutcome<PreviewImage> {
    match load_input(reader, options) {
        Ok(bytes) => extract_preview(&bytes, preview_id, options),
        Err(report) => ParseOutcome {
            value: None,
            report,
        },
    }
}

pub fn extract_preview_file(
    path: impl AsRef<Path>,
    preview_id: Option<u32>,
    options: &ParseOptions,
) -> ParseOutcome<PreviewImage> {
    match File::open(path) {
        Ok(file) => extract_preview_reader(file, preview_id, options),
        Err(error) => ParseOutcome {
            value: None,
            report: io_failure(DiagnosticCode::OpenFailed, error.kind(), None).report,
        },
    }
}

/// Open a local file. Paths and operating-system messages are not copied into JSON.
pub fn inspect_file(path: impl AsRef<Path>, options: &ParseOptions) -> ParseOutcome<DocumentInfo> {
    match File::open(path) {
        Ok(file) => inspect_reader(file, options),
        Err(error) => io_failure(DiagnosticCode::OpenFailed, error.kind(), None),
    }
}

/// Decode the admitted analytic subset. Carrier definitions are returned
/// separately from their serialized uses; no trimmed body is emitted.
pub fn decode_geometry(data: &[u8], options: &ParseOptions) -> ParseOutcome<GeometryDocument> {
    let inspection = inspect(data, options);
    let mut report = inspection.report;
    let Some(document) = inspection.value else {
        return ParseOutcome {
            value: None,
            report,
        };
    };
    if report.status == ParseStatus::Rejected {
        return ParseOutcome {
            value: Some(GeometryDocument {
                document,
                geometry: GeometryModel::default(),
                topology: None,
            }),
            report,
        };
    }
    let mut geometry = GeometryModel::default();
    let decoded = if let Some(container) = &document.container {
        native::decode(
            data,
            container,
            &options.limits,
            &mut geometry,
            &mut report.diagnostics,
        )
    } else {
        Ok(())
    };
    report.features.geometry = if geometry.populations.is_empty() {
        FeatureStatus::Unsupported
    } else if geometry
        .unresolved
        .iter()
        .any(|v| v.code == DiagnosticCode::GeometryParameterInvalid)
    {
        FeatureStatus::Invalid
    } else {
        FeatureStatus::Partial
    };
    if let Err(error) = decoded {
        report.status = ParseStatus::Rejected;
        report.features.geometry = FeatureStatus::Partial;
        report.diagnostics.push(*error);
    }
    ParseOutcome {
        value: Some(GeometryDocument {
            document,
            geometry,
            topology: None,
        }),
        report,
    }
}

pub fn decode_geometry_reader(
    reader: impl Read,
    options: &ParseOptions,
) -> ParseOutcome<GeometryDocument> {
    match load_input(reader, options) {
        Ok(bytes) => decode_geometry(&bytes, options),
        Err(report) => ParseOutcome {
            value: None,
            report,
        },
    }
}

pub fn decode_geometry_file(
    path: impl AsRef<Path>,
    options: &ParseOptions,
) -> ParseOutcome<GeometryDocument> {
    match File::open(path) {
        Ok(file) => decode_geometry_reader(file, options),
        Err(error) => ParseOutcome {
            value: None,
            report: io_failure(DiagnosticCode::OpenFailed, error.kind(), None).report,
        },
    }
}

/// Recover the supported indexed B-rep subset, retaining incomplete populations.
#[must_use]
pub fn decode_brep(data: &[u8], options: &ParseOptions) -> ParseOutcome<GeometryDocument> {
    decode_brep_with_options(data, options, &TopologyOptions::default())
}

#[must_use]
pub fn decode_brep_with_options(
    data: &[u8],
    options: &ParseOptions,
    topology_options: &TopologyOptions,
) -> ParseOutcome<GeometryDocument> {
    let mut outcome = decode_geometry(data, options);
    if outcome.report.status == ParseStatus::Rejected {
        return outcome;
    }
    let Some(value) = &mut outcome.value else {
        return outcome;
    };
    let Some(container) = &value.document.container else {
        return outcome;
    };
    let mut topology = TopologyModel::default();
    let result = topology::recover(
        data,
        container,
        &mut value.geometry,
        &options.limits,
        topology_options,
        &mut topology,
        &mut outcome.report.diagnostics,
    );
    outcome.report.features.topology = if !topology.bodies.is_empty()
        && topology.unresolved.is_empty()
        && topology.bodies.iter().all(|b| b.export_eligible)
    {
        FeatureStatus::Complete
    } else if topology.faces.is_empty() && topology.boundaries.is_empty() {
        FeatureStatus::Unsupported
    } else {
        FeatureStatus::Partial
    };
    if let Err(error) = result {
        outcome.report.status = ParseStatus::Rejected;
        outcome.report.features.topology = FeatureStatus::Partial;
        outcome.report.diagnostics.push(*error);
    }
    outcome.report.diagnostics.retain(|d| match d.code {
        DiagnosticCode::GeometryTopologyPending => {
            outcome.report.features.topology != FeatureStatus::Complete
        }
        DiagnosticCode::GeometryPlaneLocationUnresolved
        | DiagnosticCode::GeometryCurveFrameUnresolved => {
            value.geometry.unresolved.iter().any(|u| u.code == d.code)
        }
        _ => true,
    });
    value.topology = Some(topology);
    outcome
}

pub fn decode_brep_reader(
    reader: impl Read,
    options: &ParseOptions,
    topology_options: &TopologyOptions,
) -> ParseOutcome<GeometryDocument> {
    match load_input(reader, options) {
        Ok(bytes) => decode_brep_with_options(&bytes, options, topology_options),
        Err(report) => ParseOutcome {
            value: None,
            report,
        },
    }
}

pub fn decode_brep_file(
    path: impl AsRef<Path>,
    options: &ParseOptions,
    topology_options: &TopologyOptions,
) -> ParseOutcome<GeometryDocument> {
    match File::open(path) {
        Ok(file) => decode_brep_reader(file, options, topology_options),
        Err(error) => ParseOutcome {
            value: None,
            report: io_failure(DiagnosticCode::OpenFailed, error.kind(), None).report,
        },
    }
}
