use catiakit_core::{
    FeatureStatus, ParseLimits, ParseOptions, decode_brep,
    geometry::{CurveGeometry, RuledJet, SurfaceGeometry},
    topology::validate_native_uv,
    validate_topology,
};
fn jet() -> RuledJet {
    let knots = vec![0., 1., 2.];
    RuledJet {
        positions: knots
            .iter()
            .map(|u| [0., *u, u * u, 1., *u, u * u])
            .collect(),
        first: knots
            .iter()
            .map(|u| [0., 1., 2. * u, 0., 1., 2. * u])
            .collect(),
        second: vec![[0., 0., 2., 0., 0., 2.]; 3],
        knots,
        current_limits: [0., 2., 0., 2.],
        reference_limits: [0., 1.],
    }
}
#[test]
fn scaled_reference_jet_converts_exactly_to_a_trimmed_tensor_surface() {
    let mut j = jet();
    assert!(j.is_valid());
    let surface = j.surface().unwrap();
    for u in [0., 0.13, 0.97, 1., 1.73, 2.] {
        for v in [0., 0.27, 1., 1.67, 2.] {
            let expected = [v, u, u * u];
            let direct = j.evaluate([u, v]).unwrap();
            let native = surface.evaluate([u, v]).unwrap();
            assert!(
                direct
                    .iter()
                    .zip(expected)
                    .all(|(x, y)| (x - y).abs() < 1e-12)
            );
            assert!(
                native
                    .iter()
                    .zip(expected)
                    .all(|(x, y)| (x - y).abs() < 1e-12)
            );
            let uv = surface.parameters_at_point(expected, 1e-8).unwrap();
            assert!((uv[0] - u).abs() < 1e-7 && (uv[1] - v).abs() < 1e-7);
        }
    }
    assert!(surface.parameters_at_point([3., 1., 1.], 1e-3).is_none());
    j.current_limits[0] = 1.;
    let s = j.surface().unwrap();
    assert_eq!(s.knots[0][5], 1.);
    assert!(j.evaluate([0., 0.]).is_err());
    j.current_limits[0] = 0.5;
    assert!(j.surface().is_none());
    let mut j = jet();
    j.positions[1][3] += 0.1;
    assert!(!j.is_valid());
}
#[test]
fn native_ruled_frame_and_complete_packet_qualify_limits_without_fitting() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-ruled-patch.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let n = t.native_uv.as_ref().unwrap();
    assert_eq!(n.ruled_carriers.len(), 1);
    assert!(matches!(
        d.geometry.surfaces[0].geometry,
        SurfaceGeometry::Nurbs { .. }
    ));
    assert_eq!(
        d.geometry.surfaces[0].parameterization,
        "derived_unique_native_scaled_reference_ruled_jet"
    );
    assert!(matches!(
        d.geometry.curves[0].geometry,
        CurveGeometry::Nurbs { .. }
    ));
    assert!(matches!(
        d.geometry.curves[2].geometry,
        CurveGeometry::Nurbs { .. }
    ));
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let round: catiakit_core::GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    validate_topology(
        &round.geometry,
        round.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    let mut forged = n.clone();
    forged.ruled_carriers[0].geometry.first[1][1] += 0.1;
    assert!(validate_native_uv(&forged, &ParseLimits::default()).is_err());
}
#[test]
fn ambiguous_off_surface_and_nonclosing_ruled_programs_do_not_complete() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/ambiguous-ruled-patch.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/off-ruled-patch.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/invalid-ruled-tail.cfv2").as_slice(),
    ] {
        let out = decode_brep(bytes, &ParseOptions::default());
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        let d = out.value.unwrap();
        assert!(matches!(
            d.geometry.surfaces[0].geometry,
            SurfaceGeometry::Unknown { .. }
        ));
    }
}

#[test]
fn native_radial_plane_cone_chart_translates_to_the_apex_chart() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-cone-shift.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let n = t.native_uv.as_ref().unwrap();
    let cones: Vec<_> = n.carriers.iter().filter(|c| c.class == 41).collect();
    assert_eq!(cones.len(), 1);
    assert!((cones[0].uv_offset[1] - 4.).abs() < 1e-12);
    assert_eq!(n.carriers.iter().filter(|c| c.class == 40).count(), 3);
    let SurfaceGeometry::Cone { frame, half_angle } = &cones[0].geometry else {
        panic!("native cone")
    };
    assert!((frame.origin[2] - (6. - 2. * 3f64.sqrt())).abs() < 1e-12);
    assert!((*half_angle - std::f64::consts::PI / 6.).abs() < 1e-12);
    assert!(matches!(
        d.geometry.curves[1].geometry,
        CurveGeometry::SurfaceJet { .. }
    ));
    let CurveGeometry::SurfaceJet { jet, uv_offset, .. } = &d.geometry.curves[1].geometry else {
        unreachable!()
    };
    assert!(n.pcurves.iter().any(|p| p.jet == *jet));
    assert!((uv_offset[1] - 4.).abs() < 1e-12);
    for t in [0., 0.25, 0.5, 0.75, 1.] {
        let angle = std::f64::consts::PI / 3.;
        let theta = std::f64::consts::FRAC_PI_2 * (1. - t);
        let l = -1. + t;
        let expected = [
            4. + (2. + l * angle.cos()) * theta.cos(),
            5. + (2. + l * angle.cos()) * theta.sin(),
            6. + l * angle.sin(),
        ];
        assert!(
            d.geometry.curves[1]
                .geometry
                .evaluate(t)
                .unwrap()
                .iter()
                .zip(expected)
                .all(|(x, y)| (x - y).abs() < 1e-12)
        );
    }
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let mut forged = n.clone();
    forged.carriers[0].uv_offset[1] = 0.;
    assert!(validate_native_uv(&forged, &ParseLimits::default()).is_err());
}

#[test]
fn zero_reference_radius_keeps_a_finite_apex_chart_with_its_own_angular_scale() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-cone-apex-chart.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let n = d.topology.as_ref().unwrap().native_uv.as_ref().unwrap();
    let cone = n.carriers.iter().find(|c| c.class == 41).unwrap();
    assert_eq!(cone.uv_offset, [0., 0.]);
    assert_eq!(cone.uv_scale, [0.5, 1.]);
    let CurveGeometry::SurfaceJet { jet, uv_offset, .. } = &d.geometry.curves[1].geometry else {
        panic!("saved pcurve")
    };
    assert_eq!(*uv_offset, [0., 0.]);
    assert_eq!(jet.positions, vec![[std::f64::consts::PI, 3.], [0., 4.]]);
    validate_topology(
        &d.geometry,
        d.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
}

#[test]
fn saved_uv_jets_select_sparse_ellipse_arcs_on_the_resolved_analytic_locus() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-sparse-ellipse.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    assert_eq!((t.faces.len(), t.edges.len()), (3, 6));
    assert_eq!(
        d.geometry
            .curves
            .iter()
            .filter(|c| matches!(c.geometry, CurveGeometry::SurfaceJet { .. }))
            .count(),
        4
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
}
