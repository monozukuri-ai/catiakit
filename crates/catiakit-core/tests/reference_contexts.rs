use catiakit_core::{
    ParseLimits, ParseOptions, decode_brep,
    topology::{NativeAllocationEncoding, NativeUvModel, validate_native_uv},
};

fn native() -> NativeUvModel {
    decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-parameter-reference-contexts.cfv2"),
        &ParseOptions::default(),
    )
    .value
    .unwrap()
    .topology
    .unwrap()
    .native_uv
    .unwrap()
}

#[test]
fn counted_reference_lanes_preserve_wire_forms_and_opaque_tails() {
    let n = native();
    assert_eq!(n.parameter_map_contexts.len(), 2);
    assert!(
        n.parameter_map_contexts
            .iter()
            .any(|c| c.source.file_ranges.len() == 2)
    );
    let a = &n.parameter_map_contexts[0].following[0];
    assert_eq!(
        a.fields
            .references
            .iter()
            .map(|r| r.value)
            .collect::<Vec<_>>(),
        [1, 72, 690]
    );
    assert_eq!(a.fields.opaque_tail, [0x8a]);
    let a = &n.parameter_map_contexts[1].following[0];
    assert_eq!(a.frame_width, 3);
    assert_eq!(a.fields.references[0].value, a.fields.references[1].value);
    assert_eq!(
        a.fields.references[0].encoding,
        NativeAllocationEncoding::BackwardDistance
    );
    assert_eq!(
        a.fields.references[1].encoding,
        NativeAllocationEncoding::OwnedChild
    );
    assert_eq!(
        a.fields.references[2].encoding,
        NativeAllocationEncoding::Selector2
    );
    assert_eq!(a.fields.references[5].value, 70000);
    assert_eq!(a.fields.opaque_tail, [0x85, 1, 255]);
    assert_eq!(
        n.parameter_map_contexts[1].following[1].fields.references[1].value,
        u32::MAX
    );
    let summary = validate_native_uv(&n, &ParseLimits::default()).unwrap();
    assert_eq!(summary.parameter_map_contexts, 2);
    let round = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(
        summary,
        validate_native_uv(&round, &ParseLimits::default()).unwrap()
    );
    let mut old = serde_json::to_value(n).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("parameter_map_contexts");
    let old: NativeUvModel = serde_json::from_value(old).unwrap();
    assert_eq!(
        validate_native_uv(&old, &ParseLimits::default())
            .unwrap()
            .parameter_map_contexts,
        0
    );
}

#[test]
fn nonclosing_interrupted_decoy_and_cross_source_contexts_are_not_joined() {
    for data in [
        &include_bytes!(
            "../../../tests/fixtures/topology/invalid-parameter-reference-contexts.cfv2"
        )[..],
        &include_bytes!(
            "../../../tests/fixtures/topology/separate-parameter-reference-contexts.cfv2"
        )[..],
    ] {
        let n = decode_brep(data, &ParseOptions::default())
            .value
            .unwrap()
            .topology
            .unwrap()
            .native_uv
            .unwrap();
        assert!(n.parameter_map_contexts.is_empty());
        assert!(!n.parameter_maps.is_empty());
        validate_native_uv(&n, &ParseLimits::default()).unwrap();
    }
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/parameter-reference-contexts-decoy.cfv2"),
        &ParseOptions::default(),
    );
    assert!(out.value.unwrap().topology.unwrap().native_uv.is_none());
}

#[test]
fn forged_reference_context_fields_and_ownership_are_rejected() {
    let n = native();
    for mutation in 0..9 {
        let mut forged = n.clone();
        let context = &mut forged.parameter_map_contexts[0];
        match mutation {
            0 => {
                context.following[0].fields.references[0].encoding =
                    NativeAllocationEncoding::OwnedChild
            }
            1 => context.following[0].fields.references[0].value += 1,
            2 => context.following[0].fields.opaque_tail[0] ^= 1,
            3 => context.following[0].header_token[0] ^= 1,
            4 => context.following.swap(0, 1),
            5 => context.following[0].source.logical_offset += 1,
            6 => context.source.length += 1,
            7 => context.map = n.parameter_map_contexts[1].map.clone(),
            _ => forged
                .parameter_map_contexts
                .push(n.parameter_map_contexts[0].clone()),
        }
        assert!(
            validate_native_uv(&forged, &ParseLimits::default()).is_err(),
            "mutation {mutation}"
        );
    }
    let limits = ParseLimits {
        max_topology_records: 8,
        ..ParseLimits::default()
    };
    assert!(
        validate_native_uv(&n, &limits)
            .unwrap_err()
            .code
            .is_resource_limit()
    );
}
