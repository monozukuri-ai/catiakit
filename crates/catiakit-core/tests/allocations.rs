use catiakit_core::{
    ParseLimits, ParseOptions, decode_brep,
    topology::{NativeBoundaryUv, NativeUvModel, validate_native_boundary_uv},
    validate_topology,
};

fn fixture(name: &str) -> catiakit_core::GeometryDocument {
    let path = format!(
        "{}/../../tests/fixtures/topology/{name}.cfv2",
        env!("CARGO_MANIFEST_DIR")
    );
    decode_brep(&std::fs::read(path).unwrap(), &ParseOptions::default())
        .value
        .unwrap()
}

fn valid() -> catiakit_core::GeometryDocument {
    fixture("native-allocation-uv")
}

#[test]
fn current_ids_endpoints_and_uv_map_ranges_are_replayed() {
    let d = valid();
    let t = d.topology.as_ref().unwrap();
    assert_eq!(t.native_boundary_uv.len(), 1);
    let binding = &t.native_boundary_uv[0];
    assert_eq!(binding.uses.len(), 5);
    assert_eq!(
        (
            binding.qualified_faces,
            binding.qualified_edges,
            binding.qualified_vertices
        ),
        (1, 5, 5)
    );
    let n = t.native_uv.as_ref().unwrap();
    let v = validate_native_boundary_uv(
        &d.geometry,
        &t.boundaries[0],
        binding,
        n,
        &ParseLimits::default(),
    )
    .unwrap();
    assert_eq!((v.lines, v.jets, v.evaluated_sites), (5, 0, 45));
    let u = &binding.uses[2];
    assert_eq!(u.parameter_range, [-0.5, 0.5]);
    assert_eq!(u.evaluate(-0.5, n).unwrap(), [1., 1.]);
    assert_eq!(u.evaluate(0.5, n).unwrap(), [0., 1.]);
    assert!(u.evaluate(0.6, n).is_err());
    // This source-bound evidence cannot override the independent spatial gate.
    assert!(t.faces.is_empty());
    assert!(!t.analytic_partials[0].unresolved.is_empty());
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn wrong_endpoints_support_and_colliding_factories_are_not_bindings() {
    for name in [
        "wrong-allocation-endpoint",
        "wrong-allocation-support",
        "ambiguous-allocation-factories",
        "allocation-opaque-decoy",
    ] {
        assert!(
            fixture(name)
                .topology
                .unwrap()
                .native_boundary_uv
                .is_empty(),
            "{name}"
        );
        assert!(
            fixture(name)
                .topology
                .unwrap()
                .analytic_partials
                .iter()
                .flat_map(|p| &p.surfaces)
                .all(|s| s.parameterization != "source_native_allocation_nurbs_support")
        );
    }
}

#[test]
fn four_byte_delta_header_remains_in_the_same_run() {
    let d = fixture("long-allocation-header");
    let t = d.topology.unwrap();
    assert_eq!(t.native_uv.as_ref().unwrap().runs.len(), 1);
    assert_eq!(t.native_boundary_uv[0].uses.len(), 5);
    validate_topology(&d.geometry, &t, &ParseLimits::default()).unwrap();
}

#[test]
fn source_bound_uv_does_not_authorize_surface_extrapolation() {
    let d = fixture("native-allocation-uv-outside-domain");
    let t = d.topology.as_ref().unwrap();
    let b = &t.native_boundary_uv[0];
    let n = t.native_uv.as_ref().unwrap();
    assert_eq!(b.uses[0].evaluate(0., n).unwrap(), [0., 2.]);
    assert!(b.uses[0].lift(0., n).is_err());
    let v =
        validate_native_boundary_uv(&d.geometry, &t.boundaries[0], b, n, &ParseLimits::default())
            .unwrap();
    assert_eq!(v.unresolved_lifts, 9);
    assert!(t.faces.is_empty());
}

#[test]
fn modified_public_ids_ranges_source_and_geometry_fail_validation() {
    let d = valid();
    let t = d.topology.as_ref().unwrap();
    let original = &t.native_boundary_uv[0];
    let n = t.native_uv.as_ref().unwrap();
    let check = |b: &NativeBoundaryUv, n: &NativeUvModel| {
        validate_native_boundary_uv(&d.geometry, &t.boundaries[0], b, n, &ParseLimits::default())
    };
    for index in 0..9 {
        let mut b = original.clone();
        match index {
            0 => b.uses[0].edge_tag += 1,
            1 => b.uses[0].support_tag += 1,
            2 => b.uses[0].parameter_range[0] = -0.1,
            3 => b.uses[0].map_operand ^= 1,
            4 => b.uses[0].uv_source.file_ranges[0].offset += 1,
            5 => b.uses[0].parameter_map.push('x'),
            6 => b.uses.push(b.uses[0].clone()),
            7 => b.raw_factory_hex.replace_range(0..2, "00"),
            _ => b.qualified_vertices += 1,
        }
        assert!(check(&b, n).is_err(), "{index}");
    }
    let mut changed = n.clone();
    changed.nurbs_carriers[0].geometry.control_points[0][0] += 1.;
    assert!(check(original, &changed).is_err());
    let mut changed = t.clone();
    changed.native_boundary_uv.push(original.clone());
    assert!(validate_topology(&d.geometry, &changed, &ParseLimits::default()).is_err());
}

#[test]
fn legacy_json_and_bounded_validation() {
    let d = valid();
    let t = d.topology.as_ref().unwrap();
    let mut json = serde_json::to_value(t).unwrap();
    json.as_object_mut().unwrap().remove("native_boundary_uv");
    let old: catiakit_core::topology::TopologyModel = serde_json::from_value(json).unwrap();
    // Missing optional fields still deserialize. Newly source-linked support
    // claims cannot remain valid after their mandatory evidence is deleted.
    assert!(old.native_boundary_uv.is_empty());
    assert!(validate_topology(&d.geometry, &old, &ParseLimits::default()).is_err());
    let limits = ParseLimits {
        max_topology_records: 2,
        ..ParseLimits::default()
    };
    assert!(
        validate_native_boundary_uv(
            &d.geometry,
            &t.boundaries[0],
            &t.native_boundary_uv[0],
            t.native_uv.as_ref().unwrap(),
            &limits
        )
        .is_err()
    );
}

#[test]
fn allocation_support_identity_is_independent_of_strict_trim_admission() {
    use catiakit_core::geometry::SurfaceGeometry;
    let d = valid();
    let t = d.topology.as_ref().unwrap();
    let s = &t.analytic_partials[0].surfaces[0];
    assert_eq!(s.parameterization, "source_native_allocation_nurbs_support");
    let native = t.native_uv.as_ref().unwrap();
    let c = &native.nurbs_carriers[0];
    assert!(matches!(&s.geometry, SurfaceGeometry::Nurbs { surface } if *surface == c.geometry));
    assert!(s.evidence.contains(&c.source));
    assert!(!t.analytic_partials[0].unresolved.is_empty());
    assert!(t.faces.is_empty() && !t.bodies[0].export_eligible);
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    for mutation in 0..4 {
        let mut changed = t.clone();
        let s = &mut changed.analytic_partials[0].surfaces[0];
        match mutation {
            0 => {
                let SurfaceGeometry::Nurbs { surface } = &mut s.geometry else {
                    panic!()
                };
                surface.control_points[0][0] += 0.1;
            }
            1 => s.evidence.clear(),
            2 => s.domains = None,
            3 => changed.native_boundary_uv.clear(),
            _ => unreachable!(),
        }
        assert!(
            validate_topology(&d.geometry, &changed, &ParseLimits::default()).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn forged_nonfinite_and_reversed_domains_never_panic() {
    let d = valid();
    let t = d.topology.as_ref().unwrap();
    let n = t.native_uv.as_ref().unwrap();
    for range in [
        [f64::NAN, 1.],
        [0., f64::NAN],
        [f64::NEG_INFINITY, 1.],
        [0., f64::INFINITY],
        [2., 1.],
        [0., 0.],
    ] {
        for field in 0..2 {
            let mut u = t.native_boundary_uv[0].uses[0].clone();
            if field == 0 {
                u.program_range = range;
            } else {
                u.parameter_range = range;
            }
            assert!(u.evaluate(0., n).is_err());
            assert!(u.lift(0., n).is_err());
        }
    }
}

#[test]
fn native_loop_flags_close_by_vertex_id_and_orient_uv_parameters() {
    for name in [
        "native-loop-sense",
        "native-loop-face-sense",
        "native-loop-compact-sense",
        "native-loop-reversed-edge",
    ] {
        let d = fixture(name);
        let t = d.topology.as_ref().unwrap();
        let b = &t.native_boundary_uv[0];
        let n = t.native_uv.as_ref().unwrap();
        let v = validate_native_boundary_uv(
            &d.geometry,
            &t.boundaries[0],
            b,
            n,
            &ParseLimits::default(),
        )
        .unwrap();
        assert_eq!((v.oriented_uses, v.oriented_loops), (5, 1));
        for (i, u) in b.uses.iter().enumerate() {
            let o = u.orientation.as_ref().unwrap();
            assert_eq!(
                o.end_vertex_tag,
                b.uses[(i + 1) % 5]
                    .orientation
                    .as_ref()
                    .unwrap()
                    .start_vertex_tag
            );
            assert_eq!(o.uv_forward, i < 3);
            assert_eq!(
                o.edge_forward,
                !(name == "native-loop-reversed-edge" && i == 2)
            );
            assert!(u.evaluate_in_loop(0., n).is_ok());
            assert!(u.evaluate_in_loop(1., n).is_ok());
            assert!(u.evaluate_in_loop(f64::NAN, n).is_err());
            assert!(u.evaluate_in_loop(1.01, n).is_err());
        }
        assert_eq!(b.uses[3].parameter_in_loop(0.).unwrap(), 1.);
        assert_eq!(b.uses[3].parameter_in_loop(1.).unwrap(), 0.5);
        let mut changed = b.clone();
        changed.uses[0].orientation.as_mut().unwrap().uv_forward ^= true;
        assert!(
            validate_native_boundary_uv(
                &d.geometry,
                &t.boundaries[0],
                &changed,
                n,
                &ParseLimits::default()
            )
            .is_err()
        );
        let mut legacy = b.clone();
        for u in &mut legacy.uses {
            u.orientation = None;
        }
        validate_native_boundary_uv(
            &d.geometry,
            &t.boundaries[0],
            &legacy,
            n,
            &ParseLimits::default(),
        )
        .unwrap();
    }
}

#[test]
fn malformed_or_unclosed_loop_flags_do_not_claim_sense() {
    for name in [
        "native-allocation-uv",
        "native-loop-broken-cycle",
        "native-loop-bad-count",
        "native-loop-bad-padding",
        "native-loop-truncated-flags",
    ] {
        let d = fixture(name);
        let t = d.topology.as_ref().unwrap();
        assert_eq!(t.native_boundary_uv[0].uses.len(), 5);
        assert!(
            t.native_boundary_uv[0]
                .uses
                .iter()
                .all(|u| u.orientation.is_none())
        );
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn gap_observations_require_two_operands_and_never_interpolate_sampled_maps() {
    use catiakit_core::topology::NativeParameterRelation;
    let d = valid();
    let t = d.topology.unwrap();
    let mut b = t.native_boundary_uv[0].clone();
    let mut n = t.native_uv.unwrap();
    assert!(b.edge_gap_at(b.uses[0].edge_tag, 0.5, &n).is_err());
    // Construct two representations to exercise the evaluator. This is not a
    // source-qualified positive fixture and cannot pass binding replay.
    let a = b.uses[0].clone();
    let mut z = b.uses[1].clone();
    z.edge_tag = a.edge_tag;
    z.representation_tag = a.representation_tag;
    z.parameter_map = a.parameter_map.clone();
    z.map_operand = 0;
    b.uses = vec![a, z];
    let sample = b.edge_gap_at(b.uses[0].edge_tag, 0.5, &n).unwrap();
    assert!(sample.distance > 0.001);
    assert!(!sample.stored_sample);
    assert_eq!(sample.parameters, [0.5, 0.5]);
    let map = n
        .parameter_maps
        .iter_mut()
        .find(|m| m.id == b.uses[0].parameter_map)
        .unwrap();
    map.fields.relation = NativeParameterRelation::Sampled {
        source_range: [0., 1.],
        source_parameters: vec![0., 1.],
        target_parameters: vec![0., 1.],
    };
    assert!(b.edge_gap_at(b.uses[0].edge_tag, 0.5, &n).is_err());
    assert!(
        b.edge_gap_at(b.uses[0].edge_tag, 0., &n)
            .unwrap()
            .stored_sample
    );
    assert!(b.edge_gap_at(b.uses[0].edge_tag, f64::NAN, &n).is_err());
    b.uses.push(b.uses[0].clone());
    assert!(b.edge_gap_at(b.uses[0].edge_tag, 0., &n).is_err());
}

#[test]
fn source_endpoints_are_read_in_reverse_order_without_guessing_the_map_gauge() {
    for name in ["explicit", "reordered", "descending"] {
        let d = fixture(&format!("native-edge-trim-{name}"));
        let t = d.topology.as_ref().unwrap();
        let b = &t.native_boundary_uv[0];
        let u = &b.uses[0];
        assert_eq!(b.revision, 3);
        assert_eq!(u.parameter_range, [0.2, 0.7]);
        let trim = u.edge_trim.as_ref().unwrap();
        assert_eq!(
            trim.parameters,
            if name == "descending" {
                [0.7, 0.2]
            } else {
                [0.2, 0.7]
            }
        );
        assert_eq!(
            trim.endpoints[0].opaque_map_coordinate_hex,
            "0000000000e05840"
        );
        assert_eq!(
            u.evaluate_in_loop(0., t.native_uv.as_ref().unwrap())
                .unwrap(),
            [trim.parameters[0], 0.]
        );
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
        let mut changed = b.clone();
        changed.uses[0].edge_trim.as_mut().unwrap().parameters[0] += 0.01;
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
        let mut changed = b.clone();
        changed.uses[0].edge_trim.as_mut().unwrap().endpoints[0]
            .opaque_map_coordinate_hex
            .push_str("00");
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
        let mut changed = b.clone();
        changed.revision = 4;
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
fn explicit_endpoints_can_extend_a_line_but_not_a_jet_or_an_unbound_line() {
    let d = fixture("native-edge-trim-line-extension");
    let t = d.topology.as_ref().unwrap();
    let n = t.native_uv.as_ref().unwrap();
    let u = &t.native_boundary_uv[0].uses[0];
    assert_eq!(u.parameter_range, [-0.25, 1.25]);
    assert_eq!(u.program_range, [0., 1.]);
    assert_eq!(u.evaluate(-0.25, n).unwrap(), [-0.25, 0.]);
    assert!(u.evaluate(1.26, n).is_err());
    let mut unbound = u.clone();
    unbound.edge_trim = None;
    assert!(unbound.evaluate(-0.25, n).is_err());
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    let d = fixture("native-edge-trim-jet-span");
    let t = d.topology.as_ref().unwrap();
    let u = &t.native_boundary_uv[0].uses[0];
    assert_eq!(u.edge_trim.as_ref().unwrap().parameters, [-0.25, 1.25]);
    assert!(
        u.evaluate_in_loop(0., t.native_uv.as_ref().unwrap())
            .is_ok()
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn unknown_point_controls_references_spans_and_conflicting_sense_remain_untrimmed() {
    for name in [
        "bad-span",
        "wrong-direction",
        "wrong-map",
        "point-trailing",
        "unknown-control",
        "unknown-edge-control",
    ] {
        let d = fixture(&format!("native-edge-trim-{name}"));
        let t = d.topology.as_ref().unwrap();
        assert!(
            t.native_boundary_uv[0].uses[0].edge_trim.is_none(),
            "{name}"
        );
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn source_plane_has_its_own_frame_without_invented_native_bounds() {
    use catiakit_core::topology::NativeBoundSupport;
    let d = fixture("native-edge-trim-plane");
    let t = d.topology.as_ref().unwrap();
    let u = &t.native_boundary_uv[0].uses[0];
    assert!(matches!(&u.support,NativeBoundSupport::Plane {plane} if plane.control==0x1def));
    assert_eq!(
        u.lift(0.2, t.native_uv.as_ref().unwrap()).unwrap(),
        [0.2, 0., 0.]
    );
    validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
    assert!(
        fixture("native-edge-trim-plane-trailing")
            .topology
            .unwrap()
            .native_boundary_uv
            .is_empty()
    );
}

#[test]
fn compressed_coordinate_order_and_contextual_defaults_select_the_current_interval() {
    for name in ["compressed", "context-default"] {
        let d = fixture(&format!("native-edge-trim-{name}"));
        let t = d.topology.as_ref().unwrap();
        let u = &t.native_boundary_uv[0].uses[0];
        assert_eq!(u.parameter_range, [0.2, 0.7]);
        let trim = u.edge_trim.as_ref().unwrap();
        assert_eq!(trim.parameters, [0.2, 0.7]);
        assert_eq!(
            trim.endpoints[0].uv_parameters,
            if name == "compressed" {
                [0.2, 0.4]
            } else {
                [0.2, 0.]
            }
        );
        assert_eq!(
            trim.endpoints[1].uv_parameters,
            if name == "compressed" {
                [0.7, 0.9]
            } else {
                [0.7, 1.]
            }
        );
        assert!(trim.endpoints[0].opaque_map_coordinate_hex.is_empty());
        validate_topology(&d.geometry, t, &ParseLimits::default()).unwrap();
        let mut changed = t.native_boundary_uv[0].clone();
        changed.uses[0].edge_trim.as_mut().unwrap().endpoints[0]
            .source
            .file_ranges[0]
            .offset += 1;
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
