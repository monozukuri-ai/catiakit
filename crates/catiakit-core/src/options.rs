use serde::{Deserialize, Serialize};

/// Limits enforced by the input loader and each binary read operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParseLimits {
    /// Maximum complete source length accepted by inspection entry points.
    pub max_input_bytes: u64,
    /// Maximum length of one borrowed binary read or subreader.
    pub max_read_bytes: u64,
    pub max_directory_bytes: u64,
    pub max_streams: u64,
    pub max_extents: u64,
    pub max_stream_bytes: u64,
    /// Sum of admitted logical lengths, including aliases and nested directories.
    pub max_total_stream_bytes: u64,
    pub max_nesting_depth: u32,
    /// Directory candidate positions, name code units, and extent rows examined.
    pub max_scan_steps: u64,
    pub max_name_bytes: u64,
    /// Total bytes reconstructed for metadata inspection, including aliases.
    pub max_metadata_bytes: u64,
    pub max_metadata_records: u64,
    pub max_metadata_steps: u64,
    pub max_property_bytes: u64,
    pub max_preview_bytes: u64,
    pub max_preview_pixels: u64,
    pub max_geometry_bytes: u64,
    pub max_geometry_records: u64,
    pub max_geometry_steps: u64,
    pub max_topology_bytes: u64,
    pub max_topology_records: u64,
    pub max_topology_steps: u64,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_read_bytes: 16 * 1024 * 1024,
            max_directory_bytes: 4 * 1024 * 1024,
            max_streams: 10_000,
            max_extents: 100_000,
            max_stream_bytes: 64 * 1024 * 1024,
            max_total_stream_bytes: 128 * 1024 * 1024,
            max_nesting_depth: 4,
            max_scan_steps: 8_000_000,
            max_name_bytes: 4096,
            max_metadata_bytes: 8 * 1024 * 1024,
            max_metadata_records: 4096,
            max_metadata_steps: 16_000_000,
            max_property_bytes: 64 * 1024,
            max_preview_bytes: 4 * 1024 * 1024,
            max_preview_pixels: 16 * 1024 * 1024,
            max_geometry_bytes: 32 * 1024 * 1024,
            max_geometry_records: 100_000,
            max_geometry_steps: 32_000_000,
            max_topology_bytes: 32 * 1024 * 1024,
            // The largest development model (CTC02, 635 faces) needs about
            // 710,000 records and 600,000,000 steps; the defaults leave a
            // margin of roughly three times.
            max_topology_records: 2_000_000,
            max_topology_steps: 2_000_000_000,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseOptions {
    pub limits: ParseLimits,
}
