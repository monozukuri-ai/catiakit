use catiakit_core::{
    FeatureStatus, ParseLimits, ParseOptions, decode_brep,
    topology::{
        NativeAllocationEncoding, NativeParameterLineGeometry, NativeParameterRelation,
        validate_native_uv,
    },
};
const DATA: &[u8] = include_bytes!("../../../tests/fixtures/topology/native-parameter-maps.cfv2");
fn native() -> catiakit_core::topology::NativeUvModel {
    let out = decode_brep(DATA, &ParseOptions::default());
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{out:?}"
    );
    out.value.unwrap().topology.unwrap().native_uv.unwrap()
}
#[test]
fn native_affine_identity_and_sampled_relations_keep_their_own_domains() {
    let n = native();
    assert_eq!(n.parameter_maps.len(), 8);
    let f = &n.parameter_maps[0].fields;
    assert_eq!(
        f.source_reference.encoding,
        NativeAllocationEncoding::ExplicitU8
    );
    assert_eq!(f.source_reference.value, 71);
    assert_eq!(
        f.target_reference.encoding,
        NativeAllocationEncoding::ExplicitU16
    );
    assert_eq!(f.target_reference.value, 689);
    assert_eq!(f.image_at(12.), None);
    assert_eq!(n.parameter_maps[1].fields.image_at(12.), Some(12.));
    assert_eq!(n.parameter_maps[1].fields.image_at(21.), None);
    assert_eq!(
        n.parameter_maps[1].fields.signed_accuracy.to_bits(),
        (-0f64).to_bits()
    );
    for i in [2, 3] {
        let f = &n.parameter_maps[i].fields;
        assert_eq!(f.image_at(1.), Some(20.));
        assert_eq!(f.image_at(2.), Some(15.));
        assert_eq!(f.image_at(3.), Some(10.));
        assert_eq!(f.image_at(0.), None);
    }
    assert_eq!(n.parameter_maps[4].fields.image_at(4.), Some(14.));
    let f = &n.parameter_maps[5].fields;
    assert_eq!(f.image_at(8.), Some(11.));
    assert_eq!(f.image_at(5.), Some(14.));
    assert_eq!(f.image_at(3.), Some(19.));
    assert_eq!(f.image_at(6.), None);
    assert_eq!(
        n.parameter_maps[6].fields.source_reference.encoding,
        NativeAllocationEncoding::Selector2
    );
    assert_eq!(
        n.parameter_maps[6].fields.target_reference.encoding,
        NativeAllocationEncoding::WidthCoded
    );
    assert_eq!(n.parameter_maps[7].frame_width, 3);
    let NativeParameterRelation::Sampled {
        source_parameters, ..
    } = &n.parameter_maps[7].fields.relation
    else {
        panic!("sampled")
    };
    assert_eq!(source_parameters.len(), 21);
    let summary = validate_native_uv(&n, &ParseLimits::default()).unwrap();
    assert_eq!(summary.parameter_maps, 8);
    assert_eq!(summary.nurbs_parameter_lines, 4);
    let round = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(
        summary,
        validate_native_uv(&round, &ParseLimits::default()).unwrap()
    );
}
#[test]
fn native_parameter_lines_retain_implicit_sense_without_inventing_it() {
    let n = native();
    let context = &n.nurbs_parameter_objects[0];
    assert_eq!(context.lines.len(), 4);
    assert_eq!(
        context.lines[0].fields.geometry.images_at(0.25),
        vec![[0., 0.25]]
    );
    assert_eq!(
        context.lines[1].fields.geometry.images_at(0.25),
        vec![[0.25, 1.]]
    );
    assert_eq!(
        context.lines[2].fields.geometry.images_at(0.25),
        vec![[1., 0.25], [1., 0.75]]
    );
    assert_eq!(
        context.lines[3].fields.geometry.images_at(0.25),
        vec![[0.25, 0.], [0.75, 0.]]
    );
    assert!(context.lines[2].fields.geometry.images_at(1.).is_empty());
    let old = catiakit_core::topology::NativeUvModel::default();
    assert!(old.parameter_maps.is_empty());
    let mut json = serde_json::to_value(n).unwrap();
    json.as_object_mut().unwrap().remove("parameter_maps");
    json.as_object_mut()
        .unwrap()
        .remove("nurbs_parameter_objects");
    let old = serde_json::from_value::<catiakit_core::topology::NativeUvModel>(json).unwrap();
    assert!(old.parameter_maps.is_empty());
    validate_native_uv(&old, &ParseLimits::default()).unwrap();
}
#[test]
fn malformed_nonclosing_overflowing_and_embedded_map_programs_are_opaque() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/invalid-parameter-maps.cfv2"),
        &ParseOptions::default(),
    );
    let n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    assert!(n.parameter_maps.is_empty());
    assert!(n.nurbs_parameter_objects.is_empty());
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/parameter-maps-decoy.cfv2"),
        &ParseOptions::default(),
    );
    assert!(out.value.unwrap().topology.unwrap().native_uv.is_none());
}
#[test]
fn a_source_with_only_parameter_maps_is_retained_and_limits_still_apply() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-parameter-maps-only.cfv2"),
        &ParseOptions::default(),
    );
    let n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    assert_eq!(n.parameter_maps.len(), 8);
    assert!(n.carriers.is_empty() && n.pcurves.is_empty());
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
    let limits = ParseLimits {
        max_topology_records: 2,
        ..ParseLimits::default()
    };
    assert!(
        validate_native_uv(&n, &limits)
            .unwrap_err()
            .code
            .is_resource_limit()
    );
}
#[test]
fn source_validation_rejects_fields_sign_changes_context_and_reference_forgery() {
    let n = native();
    for which in 0..7 {
        let mut f = n.clone();
        match which {
            0 => f.parameter_maps[4].fields.target_range[1] += 1.,
            1 => f.parameter_maps[1].fields.signed_accuracy = 0.,
            2 => {
                f.parameter_maps[0].fields.source_reference.encoding =
                    NativeAllocationEncoding::BackwardDistance
            }
            3 => f.parameter_maps[7].fields.fit_tolerance = Some(0.2),
            4 => f.nurbs_parameter_objects[0].lines[0].ordinal = 2,
            5 => {
                if let NativeParameterLineGeometry::UndirectedAxis { axis, .. } =
                    &mut f.nurbs_parameter_objects[0].lines[2].fields.geometry
                {
                    *axis = 0;
                }
            }
            _ => f.nurbs_parameter_objects[0].source.logical_offset += 1,
        }
        assert!(
            validate_native_uv(&f, &ParseLimits::default()).is_err(),
            "mutation {which}"
        );
    }
}
