#![no_main]
use catiakit_core::{
    ParseLimits, ParseOptions, decode_geometry,
    geometry::{CurveGeometry, SurfaceGeometry},
    inspect,
};
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;
const BASE: &[u8] = include_bytes!("../../tests/fixtures/geometry/analytic.cfv2");
static EXTENTS: OnceLock<Vec<Vec<(usize, usize)>>> = OnceLock::new();
fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 || data.is_empty() {
        return;
    }
    let options = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: 128 * 1024,
            max_geometry_bytes: 128 * 1024,
            max_geometry_records: 256,
            max_geometry_steps: 256 * 1024,
            ..ParseLimits::default()
        },
    };
    let mut bytes = BASE.to_vec();
    let input = if data[0] % 3 == 0 {
        &data[1..]
    } else {
        let extents = EXTENTS.get_or_init(|| {
            let c = inspect(BASE, &ParseOptions::default())
                .value
                .unwrap()
                .container
                .unwrap();
            ["MainDataStream", "SurfacicReps"]
                .into_iter()
                .map(|name| {
                    c.streams
                        .iter()
                        .find(|s| s.name.as_deref() == Some(name))
                        .unwrap()
                        .extents
                        .iter()
                        .map(|e| (e.file_range.offset as usize, e.file_range.length as usize))
                        .collect()
                })
                .collect()
        });
        let mut cursor = 1;
        for &(offset, length) in &extents[(data[0] % 3 - 1) as usize] {
            let n = length.min(data.len().saturating_sub(cursor));
            bytes[offset..offset + n].copy_from_slice(&data[cursor..cursor + n]);
            cursor += n;
        }
        &bytes
    };
    let result = decode_geometry(input, &options);
    if let Some(value) = result.value {
        for s in &value.geometry.surfaces {
            if !matches!(s.geometry, SurfaceGeometry::Unknown { .. }) {
                assert!(
                    s.geometry
                        .evaluate([0., 0.])
                        .unwrap()
                        .iter()
                        .all(|v| v.is_finite())
                );
            }
        }
        for c in &value.geometry.curves {
            if !matches!(c.geometry, CurveGeometry::Unknown { .. }) {
                assert!(
                    c.geometry
                        .evaluate(0.)
                        .unwrap()
                        .iter()
                        .all(|v| v.is_finite())
                );
            }
        }
    }
});
