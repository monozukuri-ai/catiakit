use serde::{Deserialize, Serialize};

pub const REPORT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Complete,
    Partial,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    NotRequested,
    Complete,
    Partial,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureCoverage {
    pub container: FeatureStatus,
    pub metadata: FeatureStatus,
    pub geometry: FeatureStatus,
    pub topology: FeatureStatus,
}

impl Default for FeatureCoverage {
    fn default() -> Self {
        Self {
            container: FeatureStatus::NotRequested,
            metadata: FeatureStatus::NotRequested,
            geometry: FeatureStatus::NotRequested,
            topology: FeatureStatus::NotRequested,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Wire names are stable; callers should branch on codes rather than messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticCode {
    #[serde(rename = "input.empty")]
    EmptyInput,
    #[serde(rename = "input.truncated_signature")]
    TruncatedSignature,
    #[serde(rename = "input.truncated_header")]
    TruncatedHeader,
    #[serde(rename = "format.unsupported")]
    UnsupportedFormat,
    #[serde(rename = "limits.input_bytes")]
    InputLimitExceeded,
    #[serde(rename = "limits.read_bytes")]
    ReadLimitExceeded,
    #[serde(rename = "limits.allocation_failed")]
    AllocationFailed,
    #[serde(rename = "binary.offset_overflow")]
    OffsetOverflow,
    #[serde(rename = "binary.unexpected_eof")]
    UnexpectedEof,
    #[serde(rename = "binary.invalid_position")]
    InvalidPosition,
    #[serde(rename = "io.open_failed")]
    OpenFailed,
    #[serde(rename = "io.read_failed")]
    ReadFailed,
    #[serde(rename = "container.directory_pending")]
    DirectoryInspectionPending,
    #[serde(rename = "container.directory_range_invalid")]
    DirectoryRangeInvalid,
    #[serde(rename = "container.directory_signature_invalid")]
    DirectorySignatureInvalid,
    #[serde(rename = "container.directory_sentinel_invalid")]
    DirectorySentinelInvalid,
    #[serde(rename = "container.directory_padding")]
    DirectoryPaddingRetained,
    #[serde(rename = "container.descriptor_invalid")]
    DescriptorInvalid,
    #[serde(rename = "container.extent_range_invalid")]
    ExtentRangeInvalid,
    #[serde(rename = "container.extent_logical_mismatch")]
    ExtentLogicalMismatch,
    #[serde(rename = "container.extents_overlap")]
    ExtentsOverlap,
    #[serde(rename = "container.empty_declaration_unverified")]
    EmptyDeclarationUnverified,
    #[serde(rename = "container.directory_grammar_unknown")]
    DirectoryGrammarUnknown,
    #[serde(rename = "container.name_grammar_unknown")]
    NameGrammarUnknown,
    #[serde(rename = "container.extent_word_unknown")]
    ExtentWordUnknown,
    #[serde(rename = "container.nested_storage_unsupported")]
    NestedStorageUnsupported,
    #[serde(rename = "container.geometry_layout_unclassified")]
    GeometryLayoutUnclassified,
    #[serde(rename = "container.stream_not_found")]
    StreamNotFound,
    #[serde(rename = "limits.directory_bytes")]
    DirectoryLimitExceeded,
    #[serde(rename = "limits.stream_count")]
    StreamCountExceeded,
    #[serde(rename = "limits.extent_count")]
    ExtentCountExceeded,
    #[serde(rename = "limits.stream_bytes")]
    StreamLimitExceeded,
    #[serde(rename = "limits.total_stream_bytes")]
    TotalStreamLimitExceeded,
    #[serde(rename = "limits.nesting_depth")]
    NestingLimitExceeded,
    #[serde(rename = "limits.scan_steps")]
    ScanLimitExceeded,
    #[serde(rename = "limits.name_bytes")]
    NameLimitExceeded,
    #[serde(rename = "io.output_failed")]
    OutputFailed,
    #[serde(rename = "metadata.scope_unsupported")]
    MetadataScopeUnsupported,
    #[serde(rename = "metadata.property_invalid")]
    PropertyInvalid,
    #[serde(rename = "metadata.version_invalid")]
    VersionInvalid,
    #[serde(rename = "metadata.version_conflict")]
    VersionConflict,
    #[serde(rename = "metadata.storage_invalid")]
    StorageInvalid,
    #[serde(rename = "metadata.storage_conflict")]
    StorageConflict,
    #[serde(rename = "metadata.link_invalid")]
    LinkInvalid,
    #[serde(rename = "preview.invalid_jpeg")]
    PreviewInvalid,
    #[serde(rename = "preview.unsupported_jpeg")]
    PreviewUnsupported,
    #[serde(rename = "preview.trailing_data")]
    PreviewTrailingData,
    #[serde(rename = "preview.not_found")]
    PreviewNotFound,
    #[serde(rename = "preview.ambiguous")]
    PreviewAmbiguous,
    #[serde(rename = "limits.metadata_bytes")]
    MetadataLimitExceeded,
    #[serde(rename = "limits.metadata_records")]
    MetadataRecordLimitExceeded,
    #[serde(rename = "limits.metadata_steps")]
    MetadataStepLimitExceeded,
    #[serde(rename = "limits.property_bytes")]
    PropertyLimitExceeded,
    #[serde(rename = "limits.preview_bytes")]
    PreviewLimitExceeded,
    #[serde(rename = "limits.preview_pixels")]
    PreviewPixelLimitExceeded,
    #[serde(rename = "geometry.layout_unsupported")]
    GeometryLayoutUnsupported,
    #[serde(rename = "geometry.population_ambiguous")]
    GeometryPopulationAmbiguous,
    #[serde(rename = "geometry.spine_invalid")]
    GeometrySpineInvalid,
    #[serde(rename = "geometry.trim_chain_unresolved")]
    GeometryTrimUnresolved,
    #[serde(rename = "geometry.surface_roster_unresolved")]
    GeometrySurfaceRosterUnresolved,
    #[serde(rename = "geometry.parameter_invalid")]
    GeometryParameterInvalid,
    #[serde(rename = "geometry.plane_location_unresolved")]
    GeometryPlaneLocationUnresolved,
    #[serde(rename = "geometry.curve_frame_unresolved")]
    GeometryCurveFrameUnresolved,
    #[serde(rename = "geometry.carrier_unsupported")]
    GeometryCarrierUnsupported,
    #[serde(rename = "geometry.native_identity_ambiguous")]
    GeometryNativeIdentityAmbiguous,
    #[serde(rename = "geometry.topology_not_decoded")]
    GeometryTopologyPending,
    #[serde(rename = "geometry.required_feature_unavailable")]
    RequiredGeometryUnavailable,
    #[serde(rename = "limits.geometry_bytes")]
    GeometryLimitExceeded,
    #[serde(rename = "limits.geometry_records")]
    GeometryRecordLimitExceeded,
    #[serde(rename = "limits.geometry_steps")]
    GeometryStepLimitExceeded,
    #[serde(rename = "topology.source_unsupported")]
    TopologySourceUnsupported,
    #[serde(rename = "topology.endpoint_ambiguous")]
    TopologyEndpointAmbiguous,
    #[serde(rename = "topology.boundary_invalid")]
    TopologyBoundaryInvalid,
    #[serde(rename = "topology.trim_unresolved")]
    TopologyTrimUnresolved,
    #[serde(rename = "topology.incidence_invalid")]
    TopologyIncidenceInvalid,
    #[serde(rename = "topology.orientation_conflict")]
    TopologyOrientationConflict,
    #[serde(rename = "topology.non_manifold")]
    TopologyNonManifold,
    #[serde(rename = "topology.uv_unresolved")]
    TopologyUvUnresolved,
    #[serde(rename = "topology.pcurve_invalid")]
    TopologyPcurveInvalid,
    #[serde(rename = "topology.degenerate_uv_unresolved")]
    TopologyDegenerateUnresolved,
    #[serde(rename = "topology.geometry_unsupported")]
    TopologyGeometryUnsupported,
    #[serde(rename = "topology.closure_mismatch")]
    TopologyClosureMismatch,
    #[serde(rename = "topology.validation_invalid")]
    TopologyValidationInvalid,
    #[serde(rename = "topology.unavailable")]
    TopologyUnavailable,
    #[serde(rename = "limits.topology_bytes")]
    TopologyByteLimitExceeded,
    #[serde(rename = "limits.topology_records")]
    TopologyRecordLimitExceeded,
    #[serde(rename = "limits.topology_steps")]
    TopologyStepLimitExceeded,
    #[serde(rename = "topology.required_feature_unavailable")]
    RequiredTopologyUnavailable,
}

impl DiagnosticCode {
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::EmptyInput => "input is empty",
            Self::TruncatedSignature => "input ends within a possible CATIA V5 signature",
            Self::TruncatedHeader => "CATIA V5 input ends before the outer header is complete",
            Self::UnsupportedFormat => "input does not have a recognized CATIA V5 signature",
            Self::InputLimitExceeded => "input exceeds the configured byte limit",
            Self::ReadLimitExceeded => "binary read exceeds the configured per-read byte limit",
            Self::AllocationFailed => "unable to reserve memory for requested storage",
            Self::OffsetOverflow => "byte range cannot be represented without integer overflow",
            Self::UnexpectedEof => "binary read exceeds the available bytes",
            Self::InvalidPosition => "binary position is outside the current view",
            Self::OpenFailed => "unable to open the input file",
            Self::ReadFailed => "unable to read the input",
            Self::DirectoryInspectionPending => {
                "only the signature and outer header length were checked; directory inspection is not implemented"
            }
            Self::DirectoryRangeInvalid => "directory range is outside its admitted container",
            Self::DirectorySignatureInvalid => "directory signature is invalid",
            Self::DirectorySentinelInvalid => "directory does not end with the admitted sentinel",
            Self::DirectoryPaddingRetained => {
                "zero padding after the directory sentinel is retained as opaque bytes"
            }
            Self::DescriptorInvalid => "named stream descriptor is truncated or inconsistent",
            Self::ExtentRangeInvalid => {
                "extent overlaps structural bytes or exceeds its container body"
            }
            Self::ExtentLogicalMismatch => {
                "extent lengths or logical offsets do not match the stream declaration"
            }
            Self::ExtentsOverlap => "physical extents of one stream overlap",
            Self::EmptyDeclarationUnverified => {
                "zero-length named declaration retained with unverified semantics"
            }
            Self::DirectoryGrammarUnknown => {
                "directory has no admitted nonempty stream descriptors"
            }
            Self::NameGrammarUnknown => {
                "stream name framing is not decoded; raw descriptor is retained"
            }
            Self::ExtentWordUnknown => {
                "unrecognized extent words are retained without changing reconstruction"
            }
            Self::NestedStorageUnsupported => {
                "nested header or directory is not contained in one owner extent"
            }
            Self::GeometryLayoutUnclassified => {
                "geometry storage grammar was not classified by container inspection"
            }
            Self::StreamNotFound => "requested stream id is not present in this inspection",
            Self::DirectoryLimitExceeded => "directory exceeds the configured byte limit",
            Self::StreamCountExceeded => "stream count exceeds the configured limit",
            Self::ExtentCountExceeded => "extent count exceeds the configured limit",
            Self::StreamLimitExceeded => "logical stream exceeds the configured byte limit",
            Self::TotalStreamLimitExceeded => {
                "sum of logical stream lengths exceeds the configured limit"
            }
            Self::NestingLimitExceeded => "nested container exceeds the configured depth limit",
            Self::ScanLimitExceeded => "directory scan exceeds the configured work limit",
            Self::NameLimitExceeded => "stream name exceeds the configured byte limit",
            Self::OutputFailed => "unable to create or write the output file",
            Self::MetadataScopeUnsupported => {
                "metadata stream framing is outside the supported grammar"
            }
            Self::PropertyInvalid => "metadata property framing is truncated or inconsistent",
            Self::VersionInvalid => "LastSaveVersion does not contain one complete supported tuple",
            Self::VersionConflict => "conflicting saved-by versions have no governing value",
            Self::StorageInvalid => "storage declaration atom framing is invalid",
            Self::StorageConflict => {
                "storage declarations of the same role disagree; no governing value"
            }
            Self::LinkInvalid => {
                "document link table or locator payload is outside the supported grammar"
            }
            Self::PreviewInvalid => "preview JPEG framing or dimensions are invalid",
            Self::PreviewUnsupported => "preview JPEG coding process is unsupported",
            Self::PreviewTrailingData => "bytes after the framed JPEG are retained as opaque data",
            Self::PreviewNotFound => "no admitted preview matches this request",
            Self::PreviewAmbiguous => "preview selection is ambiguous; specify a preview id",
            Self::MetadataLimitExceeded => "metadata streams exceed the configured byte budget",
            Self::MetadataRecordLimitExceeded => {
                "metadata records exceed the configured count budget"
            }
            Self::MetadataStepLimitExceeded => {
                "metadata inspection exceeds the configured work budget"
            }
            Self::PropertyLimitExceeded => "metadata property exceeds the configured byte limit",
            Self::PreviewLimitExceeded => "preview exceeds the configured byte limit",
            Self::PreviewPixelLimitExceeded => {
                "preview dimensions exceed the configured pixel limit"
            }
            Self::GeometryLayoutUnsupported => "no admitted standard geometry population was found",
            Self::GeometryPopulationAmbiguous => {
                "multiple candidates do not identify a unique geometry population"
            }
            Self::GeometrySpineInvalid => {
                "counted edge and vertex tables do not admit a unique supported walk"
            }
            Self::GeometryTrimUnresolved => {
                "no unique complete trim chain closes at the face population"
            }
            Self::GeometrySurfaceRosterUnresolved => {
                "no unique surface and support roster matches the counted population"
            }
            Self::GeometryParameterInvalid => {
                "native geometry parameters are non-finite or inconsistent"
            }
            Self::GeometryPlaneLocationUnresolved => {
                "plane location has no admitted geometric witness; bounds are not used as a definition"
            }
            Self::GeometryCurveFrameUnresolved => {
                "curve placement has no unique admitted carrier relation"
            }
            Self::GeometryCarrierUnsupported => {
                "geometry carrier is outside the supported analytic subset"
            }
            Self::GeometryNativeIdentityAmbiguous => {
                "native allocation tag selects multiple retained definitions"
            }
            Self::GeometryTopologyPending => {
                "carrier definitions do not establish trimmed faces, edges, or a body"
            }
            Self::RequiredGeometryUnavailable => {
                "complete geometry and required topology are unavailable"
            }
            Self::GeometryLimitExceeded => "geometry streams exceed the configured byte budget",
            Self::GeometryRecordLimitExceeded => {
                "geometry records exceed the configured count budget"
            }
            Self::TopologySourceUnsupported => {
                "no unique complete indexed endpoint relation was admitted"
            }
            Self::TopologyEndpointAmbiguous => {
                "indexed endpoints do not identify unique counted vertices"
            }
            Self::TopologyBoundaryInvalid => "triangle boundary is not an ordered manifold cycle",
            Self::TopologyTrimUnresolved => {
                "trim boundary does not uniquely cover serialized physical edges"
            }
            Self::TopologyIncidenceInvalid => {
                "recovered edge uses disagree with serialized incidence"
            }
            Self::TopologyOrientationConflict => "boundary orientation equations are inconsistent",
            Self::TopologyNonManifold => "edge or vertex links are non-manifold",
            Self::TopologyUvUnresolved => "trim domain or parameter-space boundary is unresolved",
            Self::TopologyPcurveInvalid => "pcurve lift disagrees with the three-dimensional edge",
            Self::TopologyDegenerateUnresolved => {
                "degenerate edge retained without an admitted parameter-space boundary"
            }
            Self::TopologyGeometryUnsupported => {
                "trimmed topology requires unsupported carrier geometry"
            }
            Self::TopologyClosureMismatch => {
                "recovered closure differs from the requested expectation"
            }
            Self::TopologyValidationInvalid => {
                "public topology has invalid identities or ownership"
            }
            Self::TopologyUnavailable => "no topology population was admitted",
            Self::TopologyByteLimitExceeded => "topology source bytes exceed the configured budget",
            Self::TopologyRecordLimitExceeded => {
                "topology elements exceed the configured count budget"
            }
            Self::TopologyStepLimitExceeded => {
                "topology recovery exceeds the configured work budget"
            }
            Self::RequiredTopologyUnavailable => "complete supported topology is unavailable",
            Self::GeometryStepLimitExceeded => {
                "geometry decoding exceeds the configured work budget"
            }
        }
    }

    #[must_use]
    pub const fn is_resource_limit(self) -> bool {
        matches!(
            self,
            Self::InputLimitExceeded
                | Self::ReadLimitExceeded
                | Self::AllocationFailed
                | Self::DirectoryLimitExceeded
                | Self::StreamCountExceeded
                | Self::ExtentCountExceeded
                | Self::StreamLimitExceeded
                | Self::TotalStreamLimitExceeded
                | Self::NestingLimitExceeded
                | Self::ScanLimitExceeded
                | Self::NameLimitExceeded
                | Self::MetadataLimitExceeded
                | Self::MetadataRecordLimitExceeded
                | Self::MetadataStepLimitExceeded
                | Self::PropertyLimitExceeded
                | Self::PreviewLimitExceeded
                | Self::PreviewPixelLimitExceeded
                | Self::GeometryLimitExceeded
                | Self::GeometryRecordLimitExceeded
                | Self::GeometryStepLimitExceeded
                | Self::TopologyByteLimitExceeded
                | Self::TopologyRecordLimitExceeded
                | Self::TopologyStepLimitExceeded
        )
    }
}

/// Offsets are byte positions, either in the file or in a named logical stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceLocation {
    File { offset: u64 },
    LogicalStream { stream_id: String, offset: u64 },
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
    pub details: DiagnosticDetails,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: DiagnosticCode, severity: Severity, offset: Option<u64>) -> Self {
        Self {
            code,
            severity,
            message: code.message().to_owned(),
            location: offset.map(|offset| SourceLocation::File { offset }),
            details: DiagnosticDetails::default(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(location) = &self.location {
            write!(f, " at {location:?}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseReport {
    pub schema_version: u32,
    pub status: ParseStatus,
    /// None when loading failed or stopped before the full input length was known.
    pub input_size_bytes: Option<u64>,
    pub features: FeatureCoverage,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseOutcome<T> {
    /// A rejected request can still contain partial document information.
    pub value: Option<T>,
    pub report: ParseReport,
}

impl<T> ParseOutcome<T> {
    #[must_use]
    pub fn rejected(input_size_bytes: Option<u64>, diagnostic: Diagnostic) -> Self {
        Self {
            value: None,
            report: ParseReport {
                schema_version: REPORT_SCHEMA_VERSION,
                status: ParseStatus::Rejected,
                input_size_bytes,
                features: FeatureCoverage::default(),
                diagnostics: vec![diagnostic],
            },
        }
    }
}
