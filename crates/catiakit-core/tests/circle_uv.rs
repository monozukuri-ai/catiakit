use catiakit_core::{
    GeometryDocument, ParseLimits, ParseOptions, decode_brep,
    geometry::EvaluationError,
    topology::{NativeBoundUvGeometry, validate_native_boundary_uv},
    validate_topology,
};

fn fixture(kind: &str) -> GeometryDocument {
    let path = format!(
        "{}/../../tests/fixtures/topology/native-uv-{kind}.cfv2",
        env!("CARGO_MANIFEST_DIR")
    );
    decode_brep(&std::fs::read(path).unwrap(), &ParseOptions::default())
        .value
        .unwrap()
}

#[test]
fn source_circle_keeps_its_parameter_gauge_and_unknown_word() {
    let d = fixture("circle");
    let t = d.topology.as_ref().unwrap();
    let b = &t.native_boundary_uv[0];
    assert_eq!(b.revision, 3);
    let u = b
        .uses
        .iter()
        .find(|u| matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }))
        .unwrap();
    let NativeBoundUvGeometry::Circle { fields } = &u.geometry else {
        unreachable!()
    };
    assert_eq!(fields.retained_parameter.to_bits(), (-0_f64).to_bits());
    for parameter in [0., 0.2, 0.7, 1.] {
        let p = fields.evaluate(parameter).unwrap();
        let theta = parameter / 0.25;
        assert!((p[0] - (0.5 + 0.25 * theta.cos())).abs() < 1e-15);
        assert!((p[1] - (0.5 + 0.25 * theta.sin())).abs() < 1e-15);
    }
    assert_eq!(
        fields.evaluate(-f64::EPSILON),
        Err(EvaluationError::OutsideDomain)
    );
    assert_eq!(
        fields.evaluate(f64::NAN),
        Err(EvaluationError::InvalidParameter)
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let summary = validate_native_boundary_uv(
        &d.geometry,
        &t.boundaries[0],
        b,
        t.native_uv.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    assert_eq!(summary.circles, 1);
    assert_eq!(summary.lines + summary.jets + summary.circles, summary.uses);
}

#[test]
fn circle_fields_and_unknown_word_are_replayed_bit_for_bit() {
    let d = fixture("circle");
    let t = d.topology.as_ref().unwrap();
    let b = &t.native_boundary_uv[0];
    for which in 0..5 {
        let mut changed = b.clone();
        let u = changed
            .uses
            .iter_mut()
            .find(|u| matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }))
            .unwrap();
        let NativeBoundUvGeometry::Circle { fields } = &mut u.geometry else {
            unreachable!()
        };
        match which {
            0 => fields.center[0] += 0.01,
            1 => fields.radius += 0.01,
            2 => fields.parameter_range[1] += 0.01,
            3 => fields.retained_parameter = 0.,
            _ => fields.control = 0,
        };
        assert!(
            validate_native_boundary_uv(
                &d.geometry,
                &t.boundaries[0],
                &changed,
                t.native_uv.as_ref().unwrap(),
                &ParseLimits::default()
            )
            .is_err()
        );
    }
}

#[test]
fn unsupported_circle_payloads_keep_the_other_source_uses() {
    for kind in [
        "negative-radius",
        "unknown-control",
        "trailing",
        "wrong-support",
        "nonfinite",
    ] {
        let d = fixture(&format!("circle-{kind}"));
        let t = d.topology.as_ref().unwrap();
        assert!(
            !t.native_boundary_uv[0]
                .uses
                .iter()
                .any(|u| matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }))
        );
        assert_eq!(t.native_boundary_uv[0].uses.len(), 4);
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn source_trim_can_select_another_arc_without_clamping_the_program_or_support() {
    let d = fixture("circle-extended-trim");
    let t = d.topology.as_ref().unwrap();
    let b = &t.native_boundary_uv[0];
    let u = b
        .uses
        .iter()
        .find(|u| matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }))
        .unwrap();
    let NativeBoundUvGeometry::Circle { fields } = &u.geometry else {
        unreachable!()
    };
    assert_eq!(fields.parameter_range, [0.3, 0.6]);
    assert_eq!(u.parameter_range, [0.2, 0.7]);
    assert_eq!(fields.evaluate(0.2), Err(EvaluationError::OutsideDomain));
    assert!(u.evaluate(0.2, t.native_uv.as_ref().unwrap()).is_ok());
    assert_eq!(
        u.evaluate(0.1, t.native_uv.as_ref().unwrap()),
        Err(EvaluationError::OutsideDomain)
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn old_binding_replays_without_claiming_circle_support() {
    let d = fixture("circle");
    let t = d.topology.as_ref().unwrap();
    let old = serde_json::from_str(include_str!(
        "../../../tests/fixtures/topology/legacy-circle-binding-v2.json"
    ))
    .unwrap();
    validate_native_boundary_uv(
        &d.geometry,
        &t.boundaries[0],
        &old,
        t.native_uv.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
}

#[test]
fn forward_program_support_map_and_point_references_are_qualified_by_role() {
    let d = fixture("circle-forward-refs");
    let t = d.topology.as_ref().unwrap();
    let b = &t.native_boundary_uv[0];
    assert_eq!(b.uses.len(), 5);
    let u = b
        .uses
        .iter()
        .find(|u| matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }))
        .unwrap();
    assert_eq!(u.uv_tag, 502);
    assert_eq!(u.support_tag, 700);
    assert!(u.edge_trim.is_some());
    assert_eq!(u.parameter_range, [0.2, 0.7]);
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    assert!(
        fixture("circle-ambiguous-forward")
            .topology
            .unwrap()
            .native_boundary_uv
            .is_empty()
    );
}

#[test]
fn wide_child_positions_require_a_unique_typed_loop_and_exact_incidence() {
    for (kind, tag) in [("u8", 300), ("u16", 501)] {
        let d = fixture(&format!("child-{kind}"));
        let t = d.topology.as_ref().unwrap();
        let b = &t.native_boundary_uv[0];
        assert_eq!(b.revision, 3);
        assert_eq!(b.uses.len(), 5);
        assert!(b.uses.iter().all(|u| u.loop_tag == tag));
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
        let mut changed = b.clone();
        changed.uses[0].loop_tag = 201;
        assert!(
            validate_native_boundary_uv(
                &d.geometry,
                &t.boundaries[0],
                &changed,
                t.native_uv.as_ref().unwrap(),
                &ParseLimits::default()
            )
            .is_err()
        );
    }
    assert!(
        fixture("child-ambiguous")
            .topology
            .unwrap()
            .native_boundary_uv
            .is_empty()
    );
}
