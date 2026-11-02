use catiakit_core::{
    FeatureStatus, ParseLimits, ParseOptions, decode_brep,
    geometry::{CurveGeometry, NurbsCurve, NurbsSurface, SurfaceGeometry},
    validate_topology,
};
fn decode(bytes: &[u8]) -> catiakit_core::ParseOutcome<catiakit_core::GeometryDocument> {
    decode_brep(bytes, &ParseOptions::default())
}
#[test]
fn complete_native_rectangle_with_split_sides_binds_without_fitting() {
    let out = decode(include_bytes!(
        "../../../tests/fixtures/topology/native-nurbs-rectangle.cfv2"
    ));
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    assert_eq!(
        (
            t.faces.len(),
            t.edges.len(),
            t.coedges.len(),
            t.vertices.len()
        ),
        (1, 5, 5, 5)
    );
    assert!(!t.bodies[0].solid_validated);
    assert!(t.bodies[0].export_eligible);
    let SurfaceGeometry::Nurbs { surface } = &d.geometry.surfaces[0].geometry else {
        panic!("missing native surface");
    };
    for uv in [[0., 0.], [0.25, 0.75], [1., 1.]] {
        let p = surface.evaluate(uv).unwrap();
        for (a, b) in p
            .into_iter()
            .zip([3. * uv[0], 3. * uv[1], 1.8 * uv[0] * uv[1]])
        {
            assert!((a - b).abs() < 1e-12);
        }
    }
    assert!(
        d.geometry
            .curves
            .iter()
            .all(|c| matches!(c.geometry, CurveGeometry::Nurbs { .. }))
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let round: catiakit_core::GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    validate_topology(
        &round.geometry,
        round.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
}
#[test]
fn matching_corners_and_borders_do_not_resolve_different_native_interiors() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/ambiguous-nurbs-rectangle.cfv2")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/topology/off-nurbs-rectangle.cfv2").as_slice(),
    ] {
        let out = decode(bytes);
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        let d = out.value.unwrap();
        let t = d.topology.unwrap();
        assert!(!t.bodies[0].export_eligible);
        assert!(t.faces.is_empty());
        assert!(t.boundaries.len() == 1);
    }
}

#[test]
fn native_triangle_inside_knot_rectangle_uses_full_surface_inverse() {
    let out = decode(include_bytes!(
        "../../../tests/fixtures/topology/native-nurbs-triangle.cfv2"
    ));
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let s = &d.geometry.surfaces[0];
    assert_eq!(
        s.parameterization,
        "derived_unique_native_nurbs_triangular_trim_correspondence"
    );
    assert!(matches!(s.geometry, SurfaceGeometry::Nurbs { .. }));
    assert!(
        d.geometry
            .curves
            .iter()
            .all(|c| matches!(c.geometry, CurveGeometry::Line { .. }))
    );
    let t = d.topology.as_ref().unwrap();
    assert_eq!((t.faces.len(), t.edges.len(), t.vertices.len()), (1, 3, 3));
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let round: catiakit_core::GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    validate_topology(
        &round.geometry,
        round.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    let low = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-nurbs-triangle.cfv2"),
        &ParseOptions {
            limits: ParseLimits {
                max_topology_steps: 1_000_000,
                ..ParseLimits::default()
            },
        },
    );
    assert_ne!(low.report.status, catiakit_core::ParseStatus::Rejected);
    assert_ne!(low.report.features.topology, FeatureStatus::Complete);
    assert!(!low.value.unwrap().topology.unwrap().boundaries.is_empty());
}

#[test]
fn ambiguous_or_off_surface_triangular_trims_remain_unresolved() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/ambiguous-nurbs-triangle.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/off-nurbs-triangle.cfv2").as_slice(),
    ] {
        let out = decode(bytes);
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        let d = out.value.unwrap();
        assert!(matches!(
            d.geometry.surfaces[0].geometry,
            SurfaceGeometry::Unknown { .. }
        ));
        let t = d.topology.unwrap();
        assert!(!t.bodies[0].export_eligible);
    }
}

#[test]
fn rational_surface_partials_and_full_inverse_keep_native_knot_origins() {
    let surface = NurbsSurface {
        degrees: [1, 1],
        control_counts: [2, 2],
        knots: [vec![5., 5., 7., 7.], vec![-3., -3., 2., 2.]],
        control_points: vec![[0., 0., 0.], [0., 3., 0.], [2., 0., 0.], [2., 3., 1.]],
        weights: vec![1., 2., 3., 4.],
    };
    for u in [0., 0.13, 0.51, 0.87, 1.] {
        for v in [0., 0.27, 0.73, 1.] {
            let uv = [5. + 2. * u, -3. + 5. * v];
            let weights = [(1. - u) * (1. - v), (1. - u) * v, u * (1. - v), u * v];
            let dw = [[-(1. - v), -v, 1. - v, v], [-(1. - u), 1. - u, -u, u]];
            let denominator: f64 = weights
                .iter()
                .zip(&surface.weights)
                .map(|(a, b)| a * b)
                .sum();
            let expected: [f64; 3] = std::array::from_fn(|d| {
                weights
                    .iter()
                    .zip(&surface.weights)
                    .zip(&surface.control_points)
                    .map(|((a, b), p)| a * b * p[d])
                    .sum::<f64>()
                    / denominator
            });
            let jet = surface.partials(uv).unwrap();
            for d in 0..3 {
                assert!((jet[0][d] - expected[d]).abs() < 1e-12);
            }
            for a in 0..2 {
                let wd: f64 = dw[a].iter().zip(&surface.weights).map(|(a, b)| a * b).sum();
                for d in 0..3 {
                    let pd: f64 = dw[a]
                        .iter()
                        .zip(&surface.weights)
                        .zip(&surface.control_points)
                        .map(|((a, b), p)| a * b * p[d])
                        .sum();
                    let expected = (pd - expected[d] * wd) / denominator / [2., 5.][a];
                    assert!((jet[a + 1][d] - expected).abs() < 1e-12);
                }
            }
            let found = surface.parameters_at_point(jet[0], 1e-8).unwrap();
            assert!(found.iter().zip(uv).all(|(a, b)| (a - b).abs() < 1e-7));
        }
    }
    assert!(surface.parameters_at_point([1., 1., 5.], 1e-3).is_none());
    assert!(
        surface
            .parameters_at_point([f64::NAN, 0., 0.], 1e-3)
            .is_none()
    );
    let mut singular = surface.clone();
    singular.control_points = vec![[0.; 3]; 4];
    assert!(singular.parameters_at_point([0.; 3], 1e-3).is_none());
}
#[test]
fn nurbs_values_derivatives_and_numeric_inverse_match_known_curves() {
    let curve = NurbsCurve {
        degree: 3,
        knots: vec![2., 2., 2., 2., 5., 5., 5., 5.],
        control_points: vec![
            [0., 0., 0.],
            [1. / 3., 0., 0.],
            [2. / 3., 1. / 3., 0.],
            [1., 1., 1.],
        ],
        weights: vec![1.; 4],
    };
    for i in 0..=20 {
        let u = i as f64 / 20.;
        let t = 2. + 3. * u;
        let p = curve.evaluate(t).unwrap();
        let tangent = curve.derivative(t).unwrap();
        for (a, b) in p.into_iter().zip([u, u * u, u * u * u]) {
            assert!((a - b).abs() < 1e-12);
        }
        for (a, b) in tangent.into_iter().zip([1. / 3., 2. * u / 3., u * u]) {
            assert!((a - b).abs() < 1e-12);
        }
        assert!((curve.parameter_at_point(p, 1e-9).unwrap() - t).abs() < 1e-9);
    }
    let circle = NurbsCurve {
        degree: 2,
        knots: vec![0., 0., 0., 1., 1., 1.],
        control_points: vec![[1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        weights: vec![1., std::f64::consts::FRAC_1_SQRT_2, 1.],
    };
    for i in 0..=16 {
        let t = i as f64 / 16.;
        let p = circle.evaluate(t).unwrap();
        let d = circle.derivative(t).unwrap();
        assert!((p[0] * p[0] + p[1] * p[1] - 1.).abs() < 1e-12);
        assert!((p[0] * d[0] + p[1] * d[1]).abs() < 1e-12);
        assert!((circle.parameter_at_point(p, 1e-9).unwrap() - t).abs() < 1e-9);
    }
    assert!(circle.parameter_at_point([2., 2., 0.], 1e-3).is_none());
}
#[test]
fn cocircular_native_incidence_selects_sparse_quarter_arcs() {
    let out = decode(include_bytes!(
        "../../../tests/fixtures/topology/sparse-circle-cylinder.cfv2"
    ));
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    assert_eq!(
        (
            t.vertices.len(),
            t.edges.len(),
            t.faces.len(),
            t.coedges.len()
        ),
        (8, 9, 3, 18)
    );
    for c in &d.geometry.curves {
        if matches!(c.geometry, CurveGeometry::Circle { .. }) {
            let [a, b] = c.parameter_interval.unwrap();
            assert!(((b - a).abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        }
    }
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn native_meridian_circle_and_regular_sphere_chart_are_explicit() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/coordinate-sphere-seam.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/native-meridian-tilted-sphere.cfv2")
            .as_slice(),
    ] {
        let out = decode(bytes);
        assert_eq!(
            out.report.features.topology,
            FeatureStatus::Complete,
            "{out:?}"
        );
        let d = out.value.unwrap();
        let t = d.topology.as_ref().unwrap();
        assert_eq!((t.faces.len(), t.edges.len(), t.vertices.len()), (2, 2, 2));
        assert!(
            d.geometry
                .surfaces
                .iter()
                .all(|s| matches!(s.geometry, SurfaceGeometry::SphereStereographic { .. }))
        );
        assert!(d.geometry.curves.iter().all(|c| {
            c.parameterization
                .starts_with("derived_circle_plane_from_source_closed")
        }));
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
        let native = t.native_uv.as_ref().unwrap();
        assert_eq!(native.sphere_meridians.len(), 1);
        catiakit_core::topology::validate_native_uv(native, &ParseLimits::default()).unwrap();
        let mut forged = native.clone();
        forged.sphere_meridians[0].normal[0] += 0.01;
        assert!(
            catiakit_core::topology::validate_native_uv(&forged, &ParseLimits::default()).is_err()
        );
    }
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/unresolved-tilted-sphere-seam.cfv2")
            .as_slice(),
        include_bytes!("../../../tests/fixtures/topology/invalid-native-sphere-domain.cfv2")
            .as_slice(),
    ] {
        let out = decode(bytes);
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        assert!(!out.value.unwrap().topology.unwrap().bodies[0].export_eligible);
    }
}

#[test]
fn malformed_sparse_arcs_cannot_retain_pcurves_with_wrong_native_endpoints() {
    // Localized ASan fuzz regression: a curve can lie in its plane while its
    // selected interval no longer ends at the counted native vertex.
    let mut bytes =
        include_bytes!("../../../tests/fixtures/topology/sparse-circle-cylinder.cfv2").to_vec();
    for (at, xor) in [
        (1015, 255),
        (1015, 255),
        (793, 255),
        (1015, 255),
        (811, 229),
        (785, 255),
        (253, 0),
        (760, 255),
        (809, 65),
    ] {
        bytes[at] ^= xor;
    }
    let out = decode(&bytes);
    assert_ne!(out.report.features.topology, FeatureStatus::Complete);
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    assert!(!t.bodies[0].export_eligible);
    for p in &t.analytic_partials {
        let boundary = t
            .boundaries
            .iter()
            .find(|b| b.population_id == p.population_id)
            .unwrap();
        catiakit_core::topology::validate_analytic_partial(
            &d.geometry,
            boundary,
            p,
            &ParseLimits::default(),
        )
        .unwrap();
    }
}
