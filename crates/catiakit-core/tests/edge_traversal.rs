use catiakit_core::{GeometryDocument, ParseLimits, ParseOptions, decode_brep, validate_topology};

fn fixture(kind: &str) -> GeometryDocument {
    let path = format!(
        "{}/../../tests/fixtures/topology/native-map-coordinate-{kind}.cfv2",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = decode_brep(&std::fs::read(path).unwrap(), &ParseOptions::default())
        .value
        .unwrap();
    validate_topology(
        &doc.geometry,
        doc.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    doc
}

#[test]
fn edge_control_follows_the_first_map_coordinate_not_the_loop_or_uv_gauge() {
    for (kind, forward, coordinates, vertices) in [
        ("traversal-forward", true, [0.125, 0.875], [96, 97]),
        ("reversed-edge", false, [0.875, 0.125], [97, 96]),
    ] {
        let doc = fixture(kind);
        let binding = &doc.topology.as_ref().unwrap().native_boundary_uv[0];
        let t = binding.edge_map_traversal(100).unwrap();
        assert_eq!(t.map_forward, forward);
        assert_eq!(t.control, if forward { 0x21 } else { 0x22 });
        assert_eq!(t.map_target_parameters, coordinates);
        assert_eq!(t.vertex_tags, vertices);
        assert_eq!(t.point_tags, if forward { [20, 21] } else { [21, 20] });
        let u = binding.uses.iter().find(|u| u.edge_tag == 100).unwrap();
        assert!(u.orientation.as_ref().unwrap().uv_forward);
        assert_ne!(
            t.map_target_parameters,
            u.edge_trim.as_ref().unwrap().parameters
        );
        for (i, v) in coordinates.into_iter().enumerate() {
            assert_eq!(
                t.map_target_at_fraction(i as f64).unwrap().to_bits(),
                v.to_bits()
            );
            assert_eq!(t.fraction_at_map_target(v).unwrap(), i as f64);
        }
        assert_eq!(t.map_target_at_fraction(0.5).unwrap(), 0.5);
        let restored = serde_json::from_str::<catiakit_core::topology::NativeEdgeMapTraversal>(
            &serde_json::to_string(&t).unwrap(),
        )
        .unwrap();
        assert_eq!(restored, t);
    }
}

#[test]
fn unqualified_terminal_sense_does_not_discard_other_source_uv_evidence() {
    let d = fixture("sense-mismatch");
    let b = &d.topology.as_ref().unwrap().native_boundary_uv[0];
    assert!(
        b.uses
            .iter()
            .find(|u| u.edge_tag == 100)
            .unwrap()
            .edge_trim
            .is_some()
    );
    assert!(b.edge_map_traversal(100).is_err());
    assert!(
        fixture("scalar").topology.unwrap().native_boundary_uv[0]
            .edge_map_traversal(100)
            .is_err()
    );
}

#[test]
fn bad_sources_references_ambiguous_operands_and_ranges_never_choose_a_sense() {
    let d = fixture("traversal-forward");
    let original = &d.topology.as_ref().unwrap().native_boundary_uv[0];
    for mutation in 0..7 {
        let mut b = original.clone();
        let i = b.uses.iter().position(|u| u.edge_tag == 100).unwrap();
        match mutation {
            0 => b.uses.push(b.uses[i].clone()),
            1 => b.uses[i].edge_source.logical_offset += 1,
            2 => b.uses[i].edge_source.file_ranges[0].offset += 1,
            3 => b.uses[i].edge_source.length = u64::MAX,
            4 => b.uses[i].edge_trim.as_mut().unwrap().endpoints[0].allocation_tag += 1,
            5 => b.raw_factory_hex.clear(),
            6 => b.uses[i].edge_trim.as_mut().unwrap().endpoints[0].references[0] += 1,
            _ => unreachable!(),
        }
        assert!(b.edge_map_traversal(100).is_err(), "mutation {mutation}");
    }
    let t = original.edge_map_traversal(100).unwrap();
    for q in [f64::NAN, f64::INFINITY, -1., 2.] {
        assert!(t.map_target_at_fraction(q).is_err());
    }
    for q in [f64::NAN, f64::INFINITY, 0.1, 0.9] {
        assert!(t.fraction_at_map_target(q).is_err());
    }
    let mut t = t;
    t.control = 0x22;
    assert!(t.map_target_at_fraction(0.).is_err());
}
