use catiakit_core::{
    ParseOptions, decode_brep,
    geometry::{EvaluationError, NurbsSurface},
    topology::NativeParameterLineGeometry,
};

fn surface() -> NurbsSurface {
    NurbsSurface {
        degrees: [2, 1],
        control_counts: [3, 2],
        knots: [vec![-2., -2., -2., 3., 3., 3.], vec![10., 10., 12., 12.]],
        control_points: (0..3)
            .flat_map(|i| (0..2).map(move |j| [i as f64, 2. * j as f64, (i * j) as f64 / 3.]))
            .collect(),
        weights: vec![1., 2., 3., 4., 2., 1.],
    }
}
fn fixture() -> catiakit_core::GeometryDocument {
    decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-edge-trim-explicit.cfv2"),
        &ParseOptions::default(),
    )
    .value
    .unwrap()
}
fn close(a: [f64; 3], b: [f64; 3]) {
    assert!(
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-11),
        "{a:?} {b:?}"
    );
}

#[test]
fn rational_interior_and_boundary_isocurves_preserve_values_and_partials() {
    let s = surface();
    assert!(s.is_valid());
    for (axis, fixed, parameters) in [
        (0, -0.7, vec![10., 10.3, 12.]),
        (1, 10.7, vec![-2., -0.3, 3.]),
    ] {
        let curve = s.isocurve(axis, fixed).unwrap();
        assert!(curve.is_valid());
        for t in parameters {
            let mut uv = [0.; 2];
            uv[axis] = fixed;
            uv[1 - axis] = t;
            let partials = s.partials(uv).unwrap();
            close(curve.evaluate(t).unwrap(), partials[0]);
            close(curve.derivative(t).unwrap(), partials[2 - axis]);
        }
    }
    for (axis, fixed, expected) in s.boundary_curves().unwrap() {
        assert_eq!(s.isocurve(axis, fixed).unwrap(), expected);
    }
}

#[test]
fn isocurve_domains_and_malformed_control_nets_are_not_repaired() {
    let s = surface();
    for (axis, t) in [
        (2, 0.),
        (0, f64::NAN),
        (1, f64::INFINITY),
        (0, -2.01),
        (1, 12.01),
    ] {
        assert!(s.isocurve(axis, t).is_err());
    }
    for edit in 0..3 {
        let mut bad = s.clone();
        match edit {
            0 => bad.weights[0] = 0.,
            1 => bad.control_counts[0] = usize::MAX,
            _ => bad.knots[0].clear(),
        };
        assert!(bad.isocurve(0, 0.).is_err());
    }
}

#[test]
fn bound_curve_images_keep_native_parameter_gauges_and_raw_edge_direction() {
    let d = fixture();
    let top = d.topology.unwrap();
    let n = top.native_uv.unwrap();
    for use_ in &top.native_boundary_uv[0].uses {
        let image = use_.nurbs_curve_image(&n).unwrap();
        assert_eq!(image.uv_source, use_.uv_source);
        let [a, z] = use_.parameter_range;
        for i in 0..=8 {
            let t = if i == 8 {
                z
            } else {
                a + (z - a) * i as f64 / 8.
            };
            close(image.evaluate(t).unwrap(), use_.lift(t, &n).unwrap());
        }
        let endpoints = use_.edge_trim.as_ref().unwrap().parameters;
        close(
            image.evaluate_on_edge(0.).unwrap(),
            use_.lift(endpoints[0], &n).unwrap(),
        );
        close(
            image.evaluate_on_edge(1.).unwrap(),
            use_.lift(endpoints[1], &n).unwrap(),
        );
        let restored: catiakit_core::topology::NativeNurbsCurveImage =
            serde_json::from_str(&serde_json::to_string(&image).unwrap()).unwrap();
        assert_eq!(restored, image);
        restored.validate_against(use_, &n).unwrap();
        let mut forged = restored;
        forged.curve.control_points[0][0] += 0.01;
        assert!(forged.validate_against(use_, &n).is_err());
    }
    let reversed = top.native_boundary_uv[0].uses[2]
        .nurbs_curve_image(&n)
        .unwrap();
    assert_eq!(reversed.parameter_scale, -1.);
    close(reversed.derivative(0.).unwrap(), [-3., 0., -1.8]);
}

#[test]
fn oblique_and_unresolved_supports_do_not_become_fitted_curve_images() {
    let d = fixture();
    let top = d.topology.unwrap();
    let n = top.native_uv.unwrap();
    let mut u = top.native_boundary_uv[0].uses[0].clone();
    let catiakit_core::topology::NativeBoundUvGeometry::Line { fields } = &mut u.geometry else {
        panic!()
    };
    fields.geometry = NativeParameterLineGeometry::Directed {
        origin: [0., 0.],
        direction: [0.6, 0.8],
        range: [0., 1.],
    };
    assert_eq!(
        u.nurbs_curve_image(&n).unwrap_err(),
        EvaluationError::UnresolvedGeometry
    );
    u.support = catiakit_core::topology::NativeBoundSupport::Nurbs {
        carrier: "missing".into(),
    };
    assert!(u.nurbs_curve_image(&n).is_err());
}

#[test]
fn public_curve_image_evaluation_rejects_nonfinite_or_inconsistent_domains() {
    let d = fixture();
    let top = d.topology.unwrap();
    let n = top.native_uv.unwrap();
    let original = top.native_boundary_uv[0].uses[0]
        .nurbs_curve_image(&n)
        .unwrap();
    for i in 0..7 {
        let mut image = original.clone();
        match i {
            0 => image.parameter_scale = f64::INFINITY,
            1 => image.parameter_range = [1., 0.],
            2 => image.parameter_range[0] = f64::NAN,
            3 => image.parameter_offset = f64::NAN,
            4 => image.fixed_axis = 2,
            5 => image.curve.weights[0] = 0.,
            _ => image.parameter_scale = 0.,
        };
        assert!(image.evaluate(0.5).is_err());
        assert!(image.derivative(0.5).is_err());
    }
    assert!(original.evaluate_on_edge(f64::NAN).is_err());
    assert!(original.evaluate_on_edge(1.01).is_err());
    let mut image = original;
    image.edge_parameters = None;
    assert!(image.evaluate_on_edge(0.).is_err());
}

#[test]
fn extracting_a_curve_does_not_extrapolate_its_surface_domain() {
    let d = fixture();
    let top = d.topology.unwrap();
    let n = top.native_uv.unwrap();
    let mut u = top.native_boundary_uv[0].uses[0].clone();
    u.parameter_range = [-0.25, 1.25];
    let image = u.nurbs_curve_image(&n).unwrap();
    assert!(image.evaluate(0.).is_ok());
    assert_eq!(image.evaluate(-0.25), Err(EvaluationError::OutsideDomain));
    assert_eq!(image.evaluate(1.25), Err(EvaluationError::OutsideDomain));
}
