use catiakit_core::{
    FeatureStatus, ParseLimits, ParseOptions, decode_brep,
    geometry::{CurveGeometry, Frame3, RollingBallJet, SurfaceGeometry, UvJet},
    topology::validate_native_uv,
    validate_topology,
};
fn jet(variable: bool) -> RollingBallJet {
    let mut positions = Vec::new();
    let mut first = Vec::new();
    for t in [0., 2.] {
        let r = 1. + if variable { 0.2 * t } else { 0. };
        positions.push([t, r, 0., t, 0., r, t, 0., 0., std::f64::consts::FRAC_PI_2]);
        first.push([
            1.,
            if variable { 0.2 } else { 0. },
            0.,
            1.,
            0.,
            if variable { 0.2 } else { 0. },
            1.,
            0.,
            0.,
            0.,
        ]);
    }
    RollingBallJet {
        knots: vec![0., 2.],
        positions,
        first,
        second: vec![[0.; 10]; 2],
        parameter_range: [0., 2.],
    }
}
#[test]
fn exact_constant_and_variable_radius_jets_have_known_values_and_inverses() {
    for variable in [false, true] {
        let rb = jet(variable);
        assert!(rb.is_valid());
        for i in 0..=8 {
            for j in 0..=4 {
                let u = i as f64 / 4.;
                let v = j as f64 / 4.;
                let r = 1. + if variable { 0.2 * u } else { 0. };
                let p = rb.evaluate([u, v]).unwrap();
                let expected = [
                    u,
                    r * (v * std::f64::consts::FRAC_PI_2).cos(),
                    r * (v * std::f64::consts::FRAC_PI_2).sin(),
                ];
                for d in 0..3 {
                    assert!((p[d] - expected[d]).abs() < 1e-12)
                }
                let uv = rb.parameters_at_point(p, 1e-8).unwrap();
                assert!((uv[0] - u).abs() < 1e-7 && (uv[1] - v).abs() < 1e-7);
            }
        }
        for side in 0..2 {
            let curve = rb.limit_curve(side).unwrap();
            for t in [0., 0.37, 1., 2.] {
                let p = curve.evaluate(t).unwrap();
                let q = rb.evaluate([t, side as f64]).unwrap();
                assert!(p.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-12));
            }
        }
    }
    let mut invalid = jet(false);
    invalid.positions[0][9] = 0.1;
    assert!(!invalid.is_valid());
    assert!(invalid.evaluate([0., 0.]).is_err());
}
#[test]
fn rolling_intersection_and_saved_surface_jet_keep_exact_geometry() {
    let rb = jet(false);
    let plane = SurfaceGeometry::Plane {
        frame: Frame3::from_axis([0.; 3], [0., 1., -1.]).unwrap(),
    };
    let c = CurveGeometry::RollingIntersection {
        surface: Box::new(rb.clone()),
        support: Box::new(plane),
        u_range: [0., 2.],
        phi_hints: [0.5, 0.5],
    };
    for q in [0., 0.2, 0.5, 0.9, 1.] {
        let p = c.evaluate(q).unwrap();
        assert!((p[0] - 2. * q).abs() < 1e-10);
        assert!((p[1] - 0.5f64.sqrt()).abs() < 1e-10);
        assert!((p[2] - p[1]).abs() < 1e-10);
    }
    let c = CurveGeometry::SurfaceJet {
        surface: Box::new(SurfaceGeometry::RollingBall { surface: rb }),
        jet: UvJet {
            knots: vec![0., 2.],
            positions: vec![[0., 0.5], [2., 0.5]],
            first: vec![[1., 0.]; 2],
            second: vec![[0.; 2]; 2],
            parameter_range: [0., 2.],
        },
        uv_scale: [1.; 2],
        uv_offset: [0.; 2],
    };
    assert!((c.evaluate(0.37).unwrap()[0] - 0.37).abs() < 1e-12);
    let mut old = serde_json::to_value(&c).unwrap();
    old.as_object_mut().unwrap().remove("uv_offset");
    let round: CurveGeometry = serde_json::from_value(old).unwrap();
    assert_eq!(round, c);
}
#[test]
fn native_rolling_frame_roundtrip_and_interior_witness_complete_a_patch() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-patch.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-interior-choice.cfv2")
            .as_slice(),
    ] {
        let out = decode_brep(bytes, &ParseOptions::default());
        assert_eq!(
            out.report.features.topology,
            FeatureStatus::Complete,
            "{out:?}"
        );
        let d = out.value.unwrap();
        let t = d.topology.as_ref().unwrap();
        assert_eq!((t.faces.len(), t.edges.len(), t.vertices.len()), (5, 9, 6));
        assert!(matches!(
            d.geometry.surfaces[0].geometry,
            SurfaceGeometry::RollingBall { .. }
        ));
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
        let n = t.native_uv.as_ref().unwrap();
        validate_native_uv(n, &ParseLimits::default()).unwrap();
        let mut forged = n.clone();
        forged.rolling_ball_carriers[0].geometry.first[0][0] += 0.1;
        assert!(validate_native_uv(&forged, &ParseLimits::default()).is_err());
        let serialized = serde_json::to_string(&d).unwrap();
        let round: catiakit_core::GeometryDocument = serde_json::from_str(&serialized).unwrap();
        validate_topology(
            &round.geometry,
            round.topology.as_ref().unwrap(),
            &ParseLimits::default(),
        )
        .unwrap();
    }
}
#[test]
fn scalar_tail_and_invalid_reference_copy_are_source_checked() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-variable.cfv2"),
        &ParseOptions::default(),
    );
    let d = out.value.unwrap();
    let n = d.topology.unwrap().native_uv.unwrap();
    assert_eq!(n.rolling_ball_carriers.len(), 1);
    let r = &n.rolling_ball_carriers[0];
    assert!(r.tail.scalar_jet.is_some());
    assert_eq!(r.tail.terminal, Some(1));
    assert!(r.geometry.is_valid());
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-invalid-tail.cfv2"),
        &ParseOptions::default(),
    );
    assert_ne!(out.report.features.topology, FeatureStatus::Complete);
    assert!(out.value.unwrap().topology.unwrap().native_uv.is_none());
}

#[test]
fn native_nurbs_limits_retain_local_spans_and_resolved_knot_coordinates() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-nurbs-local-limits.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.unwrap();
    let n = t.native_uv.unwrap();
    let c = &n.nurbs_carriers[0];
    assert_eq!(c.current_limits, [0., 2., 0., 5.]);
    assert_eq!(c.limit_span_indices, Some([1; 4]));
    assert_eq!(c.knot_limits, Some([5., 7., -3., 2.]));
    let mut forged = n.clone();
    forged.nurbs_carriers[0].knot_limits = Some([0., 2., 0., 5.]);
    assert!(validate_native_uv(&forged, &ParseLimits::default()).is_err());
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
}

#[test]
fn native_constant_station_sections_resolve_spline_rows_without_circle_hints() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-sections.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    for c in &d.geometry.curves[..2] {
        assert!(matches!(c.geometry, CurveGeometry::Circle { .. }));
        assert_eq!(
            c.parameterization,
            "derived_native_rolling_ball_constant_station_section"
        );
    }
    validate_topology(
        &d.geometry,
        d.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-bowed-section.cfv2"),
        &ParseOptions::default(),
    );
    assert_ne!(out.report.features.topology, FeatureStatus::Complete);
    let d = out.value.unwrap();
    let t = d.topology.unwrap();
    assert!(matches!(
        t.analytic_partials[0].curves[0].geometry,
        CurveGeometry::Unknown { .. }
    ));
}

#[test]
fn collapsed_native_rolling_tip_has_a_uv_connector_without_a_physical_zero_edge() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/rolling-ball-tip.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    assert_eq!(t.edges.len(), 3);
    assert!(t.edges.iter().all(|e| !e.degenerate));
    assert_eq!(t.loops[0].singular_transitions.len(), 1);
    assert_eq!(
        t.loops[0].singular_transitions[0].evidence,
        "derived_chart_connector_at_counted_native_rolling_tip"
    );
    for mutation in 0..3 {
        let mut bad = t.clone();
        match mutation {
            0 => bad.loops[0].singular_transitions.clear(),
            1 => bad.loops[0].singular_transitions[0].uv_end[0] = 1.,
            _ => bad.loops[0].singular_transitions[0].vertex = t.vertices[1].id.clone(),
        }
        assert!(validate_topology(&d.geometry, &bad, &ParseLimits::default()).is_err());
    }
}
