use catiakit_core::{
    ParseLimits, ParseOptions, decode_brep, decode_geometry,
    geometry::{EvaluationError, UvJet},
    topology::{NativeUvModel, validate_native_uv},
};
const DATA: &[u8] = include_bytes!("../../../tests/fixtures/topology/native-uv.cfv2");
fn native() -> NativeUvModel {
    let out = decode_brep(DATA, &ParseOptions::default());
    assert_ne!(
        out.report.status,
        catiakit_core::ParseStatus::Rejected,
        "{out:?}"
    );
    let t = out.value.unwrap().topology.unwrap();
    assert!(t.bodies.iter().all(|b| b.export_eligible));
    t.native_uv.unwrap()
}
#[test]
fn serialized_jets_evaluate_the_exact_quintic_polynomial_across_spans() {
    let n = native();
    assert_eq!(
        (
            n.runs.len(),
            n.pcurves.len(),
            n.carriers.len(),
            n.nurbs_carriers.len(),
            n.jet_pairs.len()
        ),
        (1, 3, 4, 2, 1)
    );
    assert_eq!(n.pcurves[0].support_tag, 42);
    assert_eq!(n.pcurves[1].support_tag, 257);
    assert_eq!(n.pcurves[0].frame_flag, 19);
    assert_eq!(n.pcurves[1].frame_width, 2);
    assert!(
        n.pcurves
            .iter()
            .all(|p| p.source.file_ranges.iter().map(|r| r.length).sum::<u64>() == p.source.length)
    );
    for p in &n.pcurves {
        for i in 0..=96 {
            let t = -1. + i as f64 / 32.;
            let uv = p.jet.evaluate(t).unwrap();
            assert!((uv[0] - (t.powi(5) + 2. * t)).abs() < 1e-12);
            assert!((uv[1] - (3. * t.powi(4) - t + 1.)).abs() < 1e-12);
        }
    }
    assert_eq!(
        n.pcurves[0].jet.evaluate(3.),
        Err(EvaluationError::OutsideDomain)
    );
    assert_eq!(
        n.pcurves[0].jet.evaluate(f64::NAN),
        Err(EvaluationError::InvalidParameter)
    );
    let summary = validate_native_uv(&n, &ParseLimits::default()).unwrap();
    assert_eq!(summary.evaluated_uv_sites, 30);
    assert_eq!(summary.evaluated_surface_sites, 50);
    let round: NativeUvModel = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
    assert_eq!(
        summary,
        validate_native_uv(&round, &ParseLimits::default()).unwrap()
    );
}
#[test]
fn rational_control_grid_and_reflected_weight_rows_evaluate_without_fitting() {
    let n = native();
    for c in &n.nurbs_carriers {
        let s = &c.geometry;
        let degree = s.degrees[0];
        let size = degree + 1;
        assert!(s.is_valid());
        assert_eq!(s.evaluate([0., 0.]).unwrap(), [0., 0., 0.]);
        assert_eq!(
            s.evaluate([1., 1.]).unwrap(),
            [
                (size - 1) as f64,
                (size - 1) as f64,
                ((size - 1) * (size - 1)) as f64 / 4.
            ]
        );
        let bernstein = |t: f64| {
            if degree == 1 {
                vec![1. - t, t]
            } else {
                vec![
                    (1. - t).powi(3),
                    3. * t * (1. - t).powi(2),
                    3. * t * t * (1. - t),
                    t.powi(3),
                ]
            }
        };
        for uv in [[0.25, 0.5], [0.6, 0.75], [0.5, 0.5]] {
            let a = bernstein(uv[0]);
            let b = bernstein(uv[1]);
            let mut expected = [0.; 3];
            let mut den = 0.;
            for (i, a) in a.iter().enumerate() {
                for (j, b) in b.iter().enumerate() {
                    let w = a * b * s.weights[i * size + j];
                    den += w;
                    for (k, x) in expected.iter_mut().enumerate() {
                        *x += w * s.control_points[i * size + j][k];
                    }
                }
            }
            let actual = s.evaluate(uv).unwrap();
            for d in 0..3 {
                assert!((actual[d] - expected[d] / den).abs() < 1e-12);
            }
        }
        assert_eq!(s.evaluate([-0.01, 0.]), Err(EvaluationError::OutsideDomain));
        let mut broken = s.clone();
        broken.weights[0] = -1.;
        assert_eq!(
            broken.evaluate([0.5, 0.5]),
            Err(EvaluationError::InvalidGeometry)
        );
        let mut broken = s.clone();
        broken.control_counts[0] = usize::MAX;
        assert!(!broken.is_valid());
    }
}
#[test]
fn native_field_raw_byte_and_source_scope_forgery_is_rejected() {
    let n = native();
    let reject =
        |m: &NativeUvModel| assert!(validate_native_uv(m, &ParseLimits::default()).is_err());
    let mut m = n.clone();
    m.pcurves[0].jet.first[0][0] += 1.;
    reject(&m);
    let mut m = n.clone();
    m.pcurves[0].support_tag += 1;
    reject(&m);
    let mut m = n.clone();
    m.pcurves[0].source.logical_offset += 1;
    reject(&m);
    let mut m = n.clone();
    m.pcurves[0].source.file_ranges[0].offset += 1;
    reject(&m);
    let mut m = n.clone();
    m.pcurves[0].raw_frame_hex.replace_range(0..2, "ff");
    reject(&m);
    let mut m = n.clone();
    m.nurbs_carriers[0].geometry.weights[0] = 2.;
    reject(&m);
    let mut m = n.clone();
    m.nurbs_carriers[0].parameter_map[0][0] = 2.;
    reject(&m);
    let mut m = n.clone();
    m.carriers[0].uv_scale[0] = 3.;
    reject(&m);
    // A matching field edit and raw-byte edit must still obey the carrier grammar.
    let mut m = n.clone();
    m.carriers[0].native_bounds[3] = 7.;
    let mut bytes: Vec<_> = m.carriers[0]
        .raw_frame_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|q| u8::from_str_radix(std::str::from_utf8(q).unwrap(), 16).unwrap())
        .collect();
    bytes[5 + 16 * 8..5 + 17 * 8].copy_from_slice(&7f64.to_le_bytes());
    m.carriers[0].raw_frame_hex = bytes.iter().map(|x| format!("{x:02x}")).collect();
    reject(&m);
    let mut m = n.clone();
    m.jet_pairs[0].pcurves.swap(0, 1);
    reject(&m);
    let mut m = n.clone();
    m.jet_pairs[0].tolerance *= 2.;
    reject(&m);
    let mut m = n.clone();
    m.jet_pairs[0].raw_packet_hex.replace_range(0..2, "ff");
    reject(&m);
    let limits = ParseLimits {
        max_topology_records: 1,
        ..ParseLimits::default()
    };
    assert!(
        validate_native_uv(&n, &limits)
            .unwrap_err()
            .code
            .is_resource_limit()
    );
    let limits = ParseLimits {
        max_topology_bytes: 1,
        ..ParseLimits::default()
    };
    assert!(
        validate_native_uv(&n, &limits)
            .unwrap_err()
            .code
            .is_resource_limit()
    );
    let limits = ParseLimits {
        max_topology_steps: 1,
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
fn complete_opaque_frame_hides_payload_markers_and_geometry_default_is_unchanged() {
    assert!(
        decode_geometry(DATA, &ParseOptions::default())
            .value
            .unwrap()
            .topology
            .is_none()
    );
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-uv-decoy.cfv2"),
        &ParseOptions::default(),
    );
    let t = out.value.unwrap().topology.unwrap();
    assert!(t.native_uv.is_none());
    assert!(t.bodies[0].export_eligible);
    let mut invalid = UvJet {
        knots: vec![0., 1.],
        positions: vec![[0.; 2]; 2],
        first: vec![[0.; 2]; 2],
        second: vec![[0.; 2]; 2],
        parameter_range: [0., 1.],
    };
    assert!(invalid.is_valid());
    invalid.knots[1] = 0.;
    assert!(!invalid.is_valid());
}
#[test]
fn native_fixture_truncation_and_localized_mutations_remain_bounded() {
    let options = ParseOptions {
        limits: ParseLimits {
            max_topology_records: 8192,
            max_topology_steps: 512 * 1024,
            max_topology_bytes: 128 * 1024,
            ..ParseLimits::default()
        },
    };
    for n in (0..DATA.len()).step_by(53) {
        let _ = decode_brep(&DATA[..n], &options);
    }
    for p in (0..DATA.len()).step_by(23) {
        let mut data = DATA.to_vec();
        data[p] ^= 0x80;
        let out = decode_brep(&data, &options);
        if let Some(n) = out.value.and_then(|d| d.topology).and_then(|t| t.native_uv) {
            validate_native_uv(&n, &options.limits).unwrap();
        }
    }
}

#[test]
fn native_binary64_json_roundtrip_preserves_bits() {
    // A native lane value that loses one ULP without float_roundtrip parsing.
    for bits in [0x402966666666d61d, 0x402966666666d61c, 0x402966666666d61e] {
        let x = f64::from_bits(bits);
        let text = serde_json::to_string(&x).unwrap();
        let y: f64 = serde_json::from_str(&text).unwrap();
        assert_eq!(x.to_bits(), y.to_bits());
    }
}

#[test]
fn native_validation_uses_exact_serialized_domain_endpoints() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-uv-nonzero-domain.cfv2"),
        &ParseOptions::default(),
    );
    let n = out.value.unwrap().topology.unwrap().native_uv.unwrap();
    let ks = &n.nurbs_carriers[0].geometry.knots[0];
    let a = ks[0];
    let z = ks[ks.len() - 1];
    assert_ne!(a + (z - a), z);
    validate_native_uv(&n, &ParseLimits::default()).unwrap();
}
