use catiakit_core::{
    GeometryDocument, ParseLimits, ParseOptions, decode_brep,
    topology::{NativeMapCoordinateEncoding as Encoding, validate_native_boundary_uv},
    validate_topology,
};

fn fixture(kind: &str) -> GeometryDocument {
    let path = format!(
        "{}/../../tests/fixtures/topology/native-map-coordinate-{kind}.cfv2",
        env!("CARGO_MANIFEST_DIR")
    );
    decode_brep(&std::fs::read(path).unwrap(), &ParseOptions::default())
        .value
        .unwrap()
}

#[test]
fn map_coordinate_is_distinct_from_both_uv_endpoint_coordinates() {
    for kind in ["scalar", "reordered"] {
        let doc = fixture(kind);
        let t = doc.topology.as_ref().unwrap();
        let b = &t.native_boundary_uv[0];
        assert_eq!(b.revision, 3);
        let u = &b.uses[0];
        let trim = u.edge_trim.as_ref().unwrap();
        assert_eq!(trim.parameters, [0.2, 0.7]);
        assert_eq!(trim.map_target_parameters(), Some([0.125, 0.875]));
        for p in &trim.endpoints {
            let m = p.map_coordinate.as_ref().unwrap();
            assert_eq!(m.target_uv_tag, 3);
            assert_eq!(m.encoding, Encoding::Explicit);
            assert_eq!(m.literal, Some(m.parameter));
            assert_eq!(m.span_index, None);
            assert!(!p.uv_parameters.contains(&m.parameter));
        }
        validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn inherited_and_range_codes_preserve_their_different_sources() {
    for (kind, parameters, encoding) in [
        ("inherited", [0.4, 0.9], Encoding::InheritedTarget),
        ("range", [0., 1.], Encoding::RangeStart),
    ] {
        let doc = fixture(kind);
        let t = doc.topology.as_ref().unwrap();
        let trim = t.native_boundary_uv[0].uses[0].edge_trim.as_ref().unwrap();
        assert_eq!(trim.map_target_parameters(), Some(parameters));
        assert_eq!(
            trim.endpoints[0].map_coordinate.as_ref().unwrap().encoding,
            encoding
        );
        if kind == "range" {
            assert_eq!(
                trim.endpoints[1].map_coordinate.as_ref().unwrap().encoding,
                Encoding::RangeEnd
            );
        }
        assert_eq!(trim.parameters, [0.2, 0.7]);
        validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
    }
}

#[test]
fn explicit_map_span_uses_the_first_operand_jet_knots() {
    let doc = fixture("span");
    let t = doc.topology.as_ref().unwrap();
    let trim = t.native_boundary_uv[0].uses[0].edge_trim.as_ref().unwrap();
    assert_eq!(trim.map_target_parameters(), Some([0.75, 1.25]));
    for (i, p) in trim.endpoints.iter().enumerate() {
        let m = p.map_coordinate.as_ref().unwrap();
        assert_eq!(m.span_index, Some(2));
        assert_eq!(m.literal, Some([0.25, 0.75][i]));
        assert_eq!(m.parameter, 0.5 + m.literal.unwrap());
    }
    validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn unsupported_map_words_do_not_discard_valid_uv_coordinates() {
    for kind in ["wrong-span", "outside", "line-index"] {
        let doc = fixture(kind);
        let t = doc.topology.as_ref().unwrap();
        let trim = t.native_boundary_uv[0].uses[0].edge_trim.as_ref().unwrap();
        assert_eq!(trim.parameters, [0.2, 0.7]);
        assert_eq!(trim.map_target_parameters(), None);
        assert!(trim.endpoints.iter().all(|p| p.map_coordinate.is_none()));
        validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
    }
    assert!(
        fixture("bad-span").topology.unwrap().native_boundary_uv[0].uses[0]
            .edge_trim
            .is_none()
    );
}

#[test]
fn map_claims_and_encoding_bits_are_replayed_from_source() {
    let doc = fixture("scalar");
    let t = doc.topology.as_ref().unwrap();
    let original = &t.native_boundary_uv[0];
    let native = t.native_uv.as_ref().unwrap();
    for mutation in 0..7 {
        let mut changed = original.clone();
        let point = &mut changed.uses[0].edge_trim.as_mut().unwrap().endpoints[0];
        let m = point.map_coordinate.as_mut().unwrap();
        match mutation {
            0 => m.parameter += 0.01,
            1 => m.target_uv_tag += 1,
            2 => m.encoding = Encoding::RangeStart,
            3 => m.literal = Some(-0.),
            4 => m.span_index = Some(1),
            5 => point.map_coordinate = None,
            _ => changed.revision = 1,
        }
        assert!(
            validate_native_boundary_uv(
                &doc.geometry,
                &t.boundaries[0],
                &changed,
                native,
                &ParseLimits::default()
            )
            .is_err()
        );
    }
    let mut a = original.uses[0].edge_trim.as_ref().unwrap().endpoints[0]
        .map_coordinate
        .clone()
        .unwrap();
    a.literal = Some(0.);
    let mut z = a.clone();
    z.literal = Some(-0.);
    assert_ne!(a, z);
    z = a.clone();
    a.parameter = 0.;
    z.parameter = -0.;
    assert_ne!(a, z);
}

#[test]
fn revision_one_without_map_claims_replays_its_original_scope() {
    let doc = fixture("scalar");
    let mut value = serde_json::to_value(&doc).unwrap();
    for b in value["topology"]["native_boundary_uv"]
        .as_array_mut()
        .unwrap()
    {
        b["revision"] = 1.into();
        for u in b["uses"].as_array_mut().unwrap() {
            u.as_object_mut().unwrap().remove("support_chart");
            if let Some(points) = u["edge_trim"]["endpoints"].as_array_mut() {
                for p in points {
                    p.as_object_mut().unwrap().remove("map_coordinate");
                }
            }
        }
    }
    let old: GeometryDocument = serde_json::from_value(value).unwrap();
    validate_topology(
        &old.geometry,
        old.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
}

#[test]
fn target_map_evaluation_keeps_uv_point_and_support_domains_separate() {
    let doc = fixture("extension");
    let t = doc.topology.as_ref().unwrap();
    let u = &t.native_boundary_uv[0].uses[0];
    let n = t.native_uv.as_ref().unwrap();
    assert_eq!(u.parameter_range, [0.2, 0.7]);
    assert_eq!(u.map_target_parameter_on_edge(0.).unwrap(), -0.05);
    assert_eq!(u.map_target_parameter_on_edge(1.).unwrap(), 1.05);
    assert_eq!(u.evaluate_map_target(-0.05, n).unwrap(), [-0.05, 0.]);
    assert_eq!(u.evaluate_map_target(1.05, n).unwrap(), [1.05, 0.]);
    assert!(u.evaluate(-0.05, n).is_err());
    assert!(u.evaluate_map_target(-0.051, n).is_err());
    assert!(u.evaluate_map_target(f64::NAN, n).is_err());
    assert!(u.map_target_parameter_on_edge(1.01).is_err());
    assert!(u.map_target_parameter_on_edge(f64::NAN).is_err());
    assert!(u.lift_map_target(-0.05, n).is_err());
    assert_eq!(u.lift_map_target(0.5, n).unwrap(), u.lift(0.5, n).unwrap());
    let mut source = u.clone();
    source.map_operand = 1;
    assert!(source.evaluate_map_target(0.5, n).is_err());
    let mut wrong = u.clone();
    wrong.edge_trim.as_mut().unwrap().endpoints[1]
        .map_coordinate
        .as_mut()
        .unwrap()
        .target_uv_tag += 1;
    assert!(wrong.evaluate_map_target(0.5, n).is_err());
    validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
}

#[test]
fn bound_sphere_chart_converts_length_uvs_using_the_source_radius() {
    let doc = fixture("sphere");
    let t = doc.topology.as_ref().unwrap();
    let binding = &t.native_boundary_uv[0];
    let u = &binding.uses[0];
    let n = t.native_uv.as_ref().unwrap();
    assert_eq!(u.support_chart.as_ref().unwrap().scale, [0.5, 0.5]);
    let p = u.lift(0.5, n).unwrap();
    for (a, z) in p
        .into_iter()
        .zip([2. * 0.25_f64.cos(), 2. * 0.25_f64.sin(), 0.])
    {
        assert!((a - z).abs() < 1e-14);
    }
    assert!(t.faces.is_empty());
    validate_topology(&doc.geometry, t, &ParseLimits::default()).unwrap();
    for clear in [true, false] {
        let mut changed = binding.clone();
        if clear {
            changed.uses[0].support_chart = None;
        } else {
            changed.uses[0].support_chart.as_mut().unwrap().scale = [1., 1.];
        }
        assert!(
            validate_native_boundary_uv(
                &doc.geometry,
                &t.boundaries[0],
                &changed,
                n,
                &ParseLimits::default()
            )
            .is_err()
        );
    }
}
