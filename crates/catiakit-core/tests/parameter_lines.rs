use catiakit_core::{
    ParseLimits, ParseOptions, decode_brep,
    topology::{NativeParameterLineGeometry, validate_native_uv},
};

#[test]
fn explicit_intervals_are_not_replaced_by_symmetric_intervals() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-asymmetric-parameter-lines.cfv2"),
        &ParseOptions::default(),
    );
    let n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    assert_eq!(n.nurbs_parameter_objects.len(), 1);
    let lines = &n.nurbs_parameter_objects[0].lines;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].fields.geometry.range(), [-2., 1.]);
    let uv = lines[0].fields.geometry.images_at(-2.);
    assert!((uv[0][0] - 2.8).abs() < 1e-12);
    assert!((uv[0][1] - 6.6).abs() < 1e-12);
    assert!(lines[0].fields.geometry.images_at(1.5).is_empty());
    assert_eq!(
        lines[1].fields.geometry.images_at(1.),
        vec![[4., 4.], [4., 6.]]
    );
    assert_eq!(lines[2].fields.geometry.range(), [2., 3.]);
    assert_eq!(
        lines[2].fields.geometry.images_at(2.),
        vec![[2., 5.], [6., 5.]]
    );
    let mut changed = n.clone();
    if let NativeParameterLineGeometry::Directed { range, .. } =
        &mut changed.nurbs_parameter_objects[0].lines[0].fields.geometry
    {
        *range = [-2., 2.];
    }
    assert!(validate_native_uv(&changed, &ParseLimits::default()).is_err());
    assert_eq!(
        validate_native_uv(&n, &ParseLimits::default())
            .unwrap()
            .nurbs_parameter_lines,
        4
    );
}

#[test]
fn malformed_and_overflowing_explicit_lines_are_not_geometry() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/invalid-asymmetric-parameter-lines.cfv2"),
        &ParseOptions::default(),
    );
    let n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    assert!(n.nurbs_parameter_objects.is_empty());
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/asymmetric-parameter-lines-decoy.cfv2"),
        &ParseOptions::default(),
    );
    assert!(out.value.unwrap().topology.unwrap().native_uv.is_none());
}

#[test]
fn older_contexts_can_keep_newly_decoded_controls_opaque() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-asymmetric-parameter-lines.cfv2"),
        &ParseOptions::default(),
    );
    let mut n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    n.nurbs_parameter_objects[0]
        .lines
        .retain(|l| l.fields.control == 8);
    let legacy = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(
        validate_native_uv(&legacy, &ParseLimits::default())
            .unwrap()
            .nurbs_parameter_lines,
        1
    );
    n.nurbs_parameter_objects[0].lines[0].ordinal = 0;
    assert!(validate_native_uv(&n, &ParseLimits::default()).is_err());
}
