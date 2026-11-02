use catiakit_core::{
    DiagnosticCode as Code, FeatureStatus, ParseLimits, ParseOptions, ParseStatus,
    container::reconstruct_stream,
    decode_geometry, decode_geometry_reader,
    geometry::{
        CurveGeometry, EvaluationError, Frame3, GeometryDocument, GeometryLayout, RigidTransform,
        SurfaceGeometry,
    },
    inspect,
    metadata::StreamSpan,
};
const BASE: &[u8] = include_bytes!("../../../tests/fixtures/geometry/analytic.cfv2");
const PLACED: &[u8] = include_bytes!("../../../tests/fixtures/geometry/placed.cfv2");
fn doc(data: &[u8]) -> GeometryDocument {
    let out = decode_geometry(data, &ParseOptions::default());
    assert_ne!(out.report.status, ParseStatus::Rejected, "{:?}", out.report);
    out.value.unwrap()
}
fn close(a: [f64; 3], b: [f64; 3]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 1e-5, "{a} != {b}");
    }
}
fn patch(data: &[u8], stream_id: u32, at: usize, bytes: &[u8]) -> Vec<u8> {
    let c = inspect(data, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    let stream = c.stream(stream_id).unwrap();
    let mut payload = reconstruct_stream(data, stream, &ParseLimits::default()).unwrap();
    payload[at..at + bytes.len()].copy_from_slice(bytes);
    let mut output = data.to_vec();
    for e in &stream.extents {
        output[e.file_range.offset as usize..e.file_range.end().unwrap() as usize].copy_from_slice(
            &payload[e.logical_offset as usize..(e.logical_offset + e.logical_length) as usize],
        );
    }
    output
}
fn mapped(data: &[u8], span: &StreamSpan) -> Vec<u8> {
    span.file_ranges
        .iter()
        .flat_map(|r| {
            data[r.offset as usize..r.end().unwrap() as usize]
                .iter()
                .copied()
        })
        .collect()
}

#[test]
fn standard_population_preserves_identity_domains_uses_and_fragmented_sources() {
    let out = decode_geometry(BASE, &ParseOptions::default());
    assert_eq!(out.report.features.geometry, FeatureStatus::Partial);
    assert_eq!(out.report.features.topology, FeatureStatus::NotRequested);
    let g = out.value.unwrap().geometry;
    assert_eq!(g.layout, GeometryLayout::StandardNested);
    assert_eq!(
        (g.points.len(), g.surfaces.len(), g.curves.len()),
        (4, 8, 3)
    );
    assert_eq!(
        (
            g.populations[0].face_count,
            g.populations[0].edge_count,
            g.populations[0].point_count
        ),
        (8, 3, 4)
    );
    assert_eq!(g.units.length, "millimeter");
    assert!(g.units.native_unit_declaration.is_none());
    assert!(g.points.iter().all(|p| p.native_allocation_id.is_none()));
    for (i, s) in g.surfaces.iter().enumerate() {
        assert_eq!(s.record_ordinal, i as u32);
        assert_eq!(s.native_carrier_tag, 200 + i as u32);
        assert_eq!(mapped(BASE, &s.source).len() as u64, s.source.length);
    }
    assert!(g.surfaces.iter().any(|s| s.source.file_ranges.len() == 2));
    assert!(g.surface_uses.iter().all(|u| u.body_id.is_none()));
    assert!(
        g.curves
            .iter()
            .all(|c| c.parameter_interval.is_none() && c.endpoint_point_ids.is_none())
    );
    assert_eq!(g.curve_uses[2].serialized_face_slots, [0, 1]);
    assert_eq!(g.curves[0].native_allocation_tag, 100);
    assert!(
        g.opaque_ranges
            .iter()
            .any(|r| mapped(BASE, r).starts_with(b"native-prefix"))
    );
}

#[test]
fn analytic_records_evaluate_known_dimensions_without_bounds_as_definitions() {
    let g = doc(BASE).geometry;
    close(
        g.surfaces[0].geometry.evaluate([2., 3.]).unwrap(),
        [2., 3., 10.],
    );
    close(
        g.surfaces[1].geometry.evaluate([2., 3.]).unwrap(),
        [2., 20., -3.],
    );
    close(
        g.surfaces[2]
            .geometry
            .evaluate([std::f64::consts::FRAC_PI_2, 20.])
            .unwrap(),
        [0., 5., 20.],
    );
    close(
        g.surfaces[4].geometry.evaluate([0., 2.]).unwrap(),
        [11., 0., 3f64.sqrt()],
    );
    close(
        g.surfaces[5].geometry.evaluate([0., 0.]).unwrap(),
        [-8., 0., 0.],
    );
    close(
        g.surfaces[6].geometry.evaluate([0., 0.]).unwrap(),
        [5., -10., 0.],
    );
    close(g.curves[0].geometry.evaluate(0.).unwrap(), [5., 0., 10.]);
    close(
        g.curves[1]
            .geometry
            .evaluate(std::f64::consts::FRAC_PI_2)
            .unwrap(),
        [0., 20., -3.],
    );
    close(g.curves[2].geometry.evaluate(2.).unwrap(), [-2., 20., 10.]);
    assert!(matches!(
        g.surfaces[7].geometry,
        SurfaceGeometry::Unknown {
            reason: Code::GeometryCarrierUnsupported
        }
    ));
    assert_eq!(
        g.surfaces[2].domains.unwrap()[0].period,
        Some(std::f64::consts::TAU)
    );
}

#[test]
fn stored_translation_and_rotation_preserve_world_carriers() {
    let g = doc(PLACED).geometry;
    close(g.points[0].position, [7., -11., 13.]);
    close(g.points[1].position, [7., -1., 13.]);
    close(g.points[2].position, [7., -11., 33.]);
    close(g.points[3].position, [37., -11., 13.]);
    let SurfaceGeometry::Plane { frame } = &g.surfaces[0].geometry else {
        panic!("plane")
    };
    close(frame.origin, [17., -11., 13.]);
    close(frame.z_axis, [1., 0., 0.]);
    close(
        g.surfaces[2]
            .geometry
            .evaluate([std::f64::consts::FRAC_PI_2, 20.])
            .unwrap(),
        [27., -11., 18.],
    );
    let CurveGeometry::Line { origin, direction } = &g.curves[2].geometry else {
        panic!("line")
    };
    let witness = [17., -11., 33.];
    let t: f64 = (0..3)
        .map(|i| (witness[i] - origin[i]) * direction[i])
        .sum();
    close(g.curves[2].geometry.evaluate(t).unwrap(), witness);
}

#[test]
fn rigid_transform_preserves_parameterized_evaluation_for_all_resolved_kinds() {
    let g = doc(BASE).geometry;
    let t = RigidTransform {
        translation: [7., -11., 13.],
        rotation: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
    };
    let expected = |p: [f64; 3]| [p[2] + 7., p[0] - 11., p[1] + 13.];
    for s in &g.surfaces {
        if let Ok(p) = s.geometry.evaluate([0.3, 0.4]) {
            close(
                s.geometry
                    .transformed(&t)
                    .unwrap()
                    .evaluate([0.3, 0.4])
                    .unwrap(),
                expected(p),
            );
        }
    }
    for c in &g.curves {
        let p = c.geometry.evaluate(0.7).unwrap();
        close(
            c.geometry.transformed(&t).unwrap().evaluate(0.7).unwrap(),
            expected(p),
        );
    }
}

#[test]
fn missing_relations_and_ambiguous_identities_remain_unresolved() {
    let g = doc(include_bytes!(
        "../../../tests/fixtures/geometry/unresolved.cfv2"
    ))
    .geometry;
    assert!(g.surfaces[..2].iter().all(|s| matches!(
        s.geometry,
        SurfaceGeometry::Unknown {
            reason: Code::GeometryPlaneLocationUnresolved
        }
    )));
    assert!(
        g.curves
            .iter()
            .all(|c| matches!(c.geometry, CurveGeometry::Unknown { .. }))
    );
    let g = doc(include_bytes!(
        "../../../tests/fixtures/geometry/duplicate-tag.cfv2"
    ))
    .geometry;
    let unresolved = g
        .unresolved
        .iter()
        .find(|u| u.code == Code::GeometryNativeIdentityAmbiguous)
        .unwrap();
    assert_eq!(unresolved.native_tag, Some(200));
    assert_eq!(unresolved.candidates.len(), 2);
    assert_ne!(unresolved.candidates[0], unresolved.candidates[1]);
}

#[test]
fn ambiguous_populations_and_streams_do_not_select_the_first_candidate() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/geometry/ambiguous-stream.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/geometry/ambiguous-population.cfv2").as_slice(),
    ] {
        let g = doc(bytes).geometry;
        assert!(g.populations.is_empty());
        assert!(g.points.is_empty());
        assert_eq!(g.layout, GeometryLayout::Unknown);
        assert_eq!(g.units.length, "unknown");
        assert!(
            g.unresolved
                .iter()
                .any(|u| u.code == Code::GeometryPopulationAmbiguous)
        );
    }
    assert_eq!(
        doc(include_bytes!(
            "../../../tests/fixtures/geometry/compact-wide.cfv2"
        ))
        .geometry
        .populations[0]
            .handle_width,
        3
    );
}

#[test]
fn bounds_and_embedded_signatures_do_not_supply_geometry() {
    let d = doc(BASE);
    let g = d.geometry;
    let surface = &g.surfaces[0];
    let p = surface.source.logical_offset as usize + 8;
    let bytes = patch(
        BASE,
        surface.source.stream_id,
        p,
        &[0, 0x33, 0x33, 0, 0x05, 0x08, 0x01, 0],
    );
    let after = doc(&bytes).geometry;
    assert_eq!(after.surfaces.len(), 8);
    assert_eq!(after.surfaces[0].geometry, surface.geometry);
    assert_eq!(after.points.len(), 4);
    assert!(
        !after
            .points
            .iter()
            .any(|p| p.position == [1000., 2000., 3000.])
    );
    let short_false_run = [0x30, 4, 4, 255, 0, 0, 0, 0, 0x30, 4, 4, 255, 1, 0, 0, 0];
    let bytes = patch(BASE, g.populations[0].main_stream_id, 0, &short_false_run);
    assert_eq!(doc(&bytes).geometry.points.len(), 4);
}

#[test]
fn invalid_parameters_retain_raw_bits_and_never_serialize_nan_geometry() {
    let d = doc(BASE);
    let surface = &d.geometry.surfaces[2];
    let bytes = patch(
        BASE,
        surface.source.stream_id,
        surface.source.logical_offset as usize + 28,
        &f32::NAN.to_be_bytes(),
    );
    let out = decode_geometry(&bytes, &ParseOptions::default());
    assert_eq!(out.report.features.geometry, FeatureStatus::Invalid);
    let g = &out.value.as_ref().unwrap().geometry;
    assert!(matches!(
        g.surfaces[2].geometry,
        SurfaceGeometry::Unknown {
            reason: Code::GeometryParameterInvalid
        }
    ));
    assert!(g.surfaces[2].raw_parameters_hex.ends_with("7fc00000"));
    serde_json::to_string(&out).unwrap();
}

#[test]
fn work_count_and_byte_limits_reject_without_committing_half_a_population() {
    for (limits, code) in [
        (
            ParseLimits {
                max_geometry_bytes: 1,
                ..ParseLimits::default()
            },
            Code::GeometryLimitExceeded,
        ),
        (
            ParseLimits {
                max_geometry_steps: 0,
                ..ParseLimits::default()
            },
            Code::GeometryStepLimitExceeded,
        ),
        (
            ParseLimits {
                max_geometry_records: 8,
                ..ParseLimits::default()
            },
            Code::GeometryRecordLimitExceeded,
        ),
    ] {
        let out = decode_geometry(BASE, &ParseOptions { limits });
        assert_eq!(out.report.status, ParseStatus::Rejected);
        assert_eq!(out.report.diagnostics.last().unwrap().code, code);
        let g = out.value.unwrap().geometry;
        assert!(g.populations.is_empty());
        assert!(g.points.is_empty());
        assert!(g.surfaces.is_empty());
    }
    let g = doc(BASE).geometry;
    let exact = g.populations[0].main_stream_id;
    let c = inspect(BASE, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    let bytes = c.stream(exact).unwrap().logical_length
        + c.stream(g.populations[0].surface_stream_id)
            .unwrap()
            .logical_length;
    assert_eq!(
        decode_geometry(
            BASE,
            &ParseOptions {
                limits: ParseLimits {
                    max_geometry_bytes: bytes,
                    max_geometry_records: 26,
                    ..ParseLimits::default()
                }
            }
        )
        .report
        .status,
        ParseStatus::Partial
    );
}

#[test]
fn externally_constructed_geometry_is_checked_before_evaluation() {
    assert_eq!(
        CurveGeometry::Line {
            origin: [0.; 3],
            direction: [0.; 3]
        }
        .evaluate(1.),
        Err(EvaluationError::InvalidGeometry)
    );
    assert_eq!(
        CurveGeometry::Circle {
            frame: Frame3::identity(),
            radius: f64::NAN
        }
        .evaluate(1.),
        Err(EvaluationError::InvalidGeometry)
    );
    assert_eq!(
        doc(BASE).geometry.curves[0]
            .geometry
            .evaluate(f64::INFINITY),
        Err(EvaluationError::InvalidParameter)
    );
    assert_eq!(
        doc(BASE).geometry.surfaces[4].geometry.evaluate([0., -1.]),
        Err(EvaluationError::OutsideDomain)
    );
    let reflection = RigidTransform {
        translation: [0.; 3],
        rotation: [[-1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
    };
    assert!(!reflection.is_valid());
    assert!(
        Frame3::from_axis([0.; 3], [1e-320, 0., 0.])
            .unwrap()
            .is_valid()
    );
    let huge = RigidTransform {
        translation: [f64::MAX, 0., 0.],
        rotation: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
    };
    let line = CurveGeometry::Line {
        origin: [f64::MAX, 0., 0.],
        direction: [1., 0., 0.],
    };
    assert_eq!(
        line.transformed(&huge),
        Err(EvaluationError::InvalidGeometry)
    );
}

#[test]
fn reader_chunks_and_mutations_preserve_determinism_and_bound_work() {
    assert_eq!(
        decode_geometry(BASE, &ParseOptions::default()),
        decode_geometry_reader(BASE, &ParseOptions::default())
    );
    let options = ParseOptions {
        limits: ParseLimits {
            max_geometry_steps: 200_000,
            max_geometry_records: 1000,
            ..ParseLimits::default()
        },
    };
    for at in (0..BASE.len()).step_by(3) {
        let mut bytes = BASE.to_vec();
        bytes[at] ^= 0xff;
        let a = decode_geometry(&bytes, &options);
        assert_eq!(a, decode_geometry(&bytes, &options));
        serde_json::to_string(&a).unwrap();
    }
    for end in 0..BASE.len() {
        let out = decode_geometry(&BASE[..end], &options);
        assert_eq!(out.report.status, ParseStatus::Rejected);
    }
}
