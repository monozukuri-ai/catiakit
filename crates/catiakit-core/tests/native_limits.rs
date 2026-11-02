use catiakit_core::{
    FeatureStatus, GeometryDocument, ParseLimits, ParseOptions, decode_brep,
    geometry::{CurveGeometry, SurfaceGeometry},
    topology::{validate_analytic_partial, validate_boundaries, validate_native_uv},
    validate_topology,
};

fn validate_partial(d: &GeometryDocument) {
    let t = d.topology.as_ref().unwrap();
    assert!(t.faces.is_empty() && !t.bodies[0].export_eligible);
    for boundary in &t.boundaries {
        validate_boundaries(&d.geometry, boundary, &ParseLimits::default()).unwrap();
    }
    for partial in &t.analytic_partials {
        let boundary = t
            .boundaries
            .iter()
            .find(|p| p.population_id == partial.population_id)
            .unwrap();
        validate_analytic_partial(&d.geometry, boundary, partial, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn a_source_limit_can_qualify_one_edge_without_qualifying_its_complete_patch() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-nurbs-independent-limit.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(out.report.features.topology, FeatureStatus::Partial);
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let partial = &t.analytic_partials[0];
    assert!(matches!(
        partial.surfaces[0].geometry,
        SurfaceGeometry::Unknown { .. }
    ));
    assert!(matches!(
        partial.curves[0].geometry,
        CurveGeometry::Nurbs { .. }
    ));
    assert_eq!(
        partial.curves[0].parameterization,
        "derived_native_nurbs_limit_with_complete_path_and_known_owner"
    );
    assert_eq!((partial.pcurves.len(), partial.unresolved.len()), (4, 4));
    assert!(
        partial
            .pcurves
            .iter()
            .any(|p| p.edge_ordinal == 0 && p.face_ordinal == 1)
    );
    let curve = &partial.curves[0].geometry;
    for t in [0., 0.25, 0.5, 0.75, 1.] {
        let p = curve.evaluate(t).unwrap();
        for (a, z) in p.into_iter().zip([3. * t, 0., 9. * t * t]) {
            assert!((a - z).abs() < 1e-12);
        }
    }
    validate_partial(&d);
    let round: GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    validate_partial(&round);
}

#[test]
fn off_path_or_distinct_matching_source_limits_remain_unresolved() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/ambiguous-nurbs-independent-limit.cfv2")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/topology/off-nurbs-independent-limit.cfv2")
            .as_slice(),
    ] {
        let out = decode_brep(bytes, &ParseOptions::default());
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        let d = out.value.unwrap();
        let p = &d.topology.as_ref().unwrap().analytic_partials[0];
        assert!(matches!(
            p.curves[0].geometry,
            CurveGeometry::Unknown { .. }
        ));
        assert_eq!((p.pcurves.len(), p.unresolved.len()), (3, 5));
        validate_partial(&d);
    }
}

#[test]
fn matching_mesh_stations_do_not_admit_a_limit_that_leaves_its_known_owner() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-nurbs-owner-gap.cfv2"),
        &ParseOptions::default(),
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let p = &t.analytic_partials[0];
    assert!(matches!(
        p.curves[0].geometry,
        CurveGeometry::Unknown { .. }
    ));
    assert_eq!((p.pcurves.len(), p.unresolved.len()), (3, 5));
    let n = &t.native_uv.as_ref().unwrap().nurbs_carriers[0].geometry;
    for u in [0., 0.5, 1.] {
        let point = n.evaluate([u, 0.]).unwrap();
        assert!((point[0].hypot(point[1]) - 10.).abs() < 1e-12);
    }
    let point = n.evaluate([0.25, 0.]).unwrap();
    assert!(point[0].hypot(point[1]) - 10. > 0.001);
    validate_partial(&d);
}

#[test]
fn full_and_zero_elided_cylinders_keep_frames_and_check_saved_closure() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-masked-cylinder-charts.cfv2"),
        &ParseOptions::default(),
    );
    let d = out.value.unwrap();
    let n = d.topology.as_ref().unwrap().native_uv.as_ref().unwrap();
    // Of six framed carriers, the zero frame and inconsistent shift are opaque.
    assert_eq!(n.carriers.len(), 4);
    let a = std::f64::consts::FRAC_1_SQRT_2;
    for (c, expected) in n.carriers.iter().zip([
        [3. * a, -2., 3. * a],
        [-3., 0., -2.],
        [0., 2., 3.],
        [-3., 0., -2.],
    ]) {
        assert_eq!(c.uv_scale, [1. / 3., 1.]);
        for (x, y) in c
            .geometry
            .evaluate([0., 2.])
            .unwrap()
            .into_iter()
            .zip(expected)
        {
            assert!((x - y).abs() < 1e-12);
        }
    }
    validate_native_uv(n, &ParseLimits::default()).unwrap();
    let mut forged = n.clone();
    if let SurfaceGeometry::Cylinder { frame, .. } = &mut forged.carriers[0].geometry {
        frame.origin[0] += 0.1;
    }
    assert!(validate_native_uv(&forged, &ParseLimits::default()).is_err());
}

#[test]
fn resolving_every_curve_on_a_body_with_an_unknown_face_keeps_partial_uv() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-unknown-cap.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(out.report.features.topology, FeatureStatus::Partial);
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let p = &t.analytic_partials[0];
    assert!(
        p.curves
            .iter()
            .all(|c| !matches!(c.geometry, CurveGeometry::Unknown { .. }))
    );
    assert!(matches!(
        p.surfaces[1].geometry,
        SurfaceGeometry::Unknown { .. }
    ));
    assert_eq!((p.pcurves.len(), p.unresolved.len()), (8, 4));
    assert_eq!(
        p.curves
            .iter()
            .filter(|c| matches!(c.geometry, CurveGeometry::SurfaceJet { .. }))
            .count(),
        4
    );
    validate_partial(&d);
}

#[test]
fn coelliptic_native_vertices_select_sparse_arcs_without_saved_uv() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/coelliptic-sparse-arcs.cfv2"),
        &ParseOptions::default(),
    );
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    assert!(d.topology.as_ref().unwrap().native_uv.is_none());
    let ellipses: Vec<_> = d
        .geometry
        .curves
        .iter()
        .filter(|c| matches!(c.geometry, CurveGeometry::Ellipse { .. }))
        .collect();
    assert_eq!(ellipses.len(), 4);
    for c in ellipses {
        let [a, z] = c.parameter_interval.unwrap();
        assert!(((z - a).abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!(!c.evidence.is_empty());
    }
    validate_topology(
        &d.geometry,
        d.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
}
