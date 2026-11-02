#![no_main]
use catiakit_core::{FeatureStatus, ParseLimits, ParseOptions, decode_brep, validate_topology};
use libfuzzer_sys::fuzz_target;
const CASES: [&[u8]; 102] = [
    include_bytes!("../../tests/fixtures/topology/box.cfv2"),
    include_bytes!("../../tests/fixtures/topology/cylinder.cfv2"),
    include_bytes!("../../tests/fixtures/topology/through-hole.cfv2"),
    include_bytes!("../../tests/fixtures/topology/cone-pole.cfv2"),
    include_bytes!("../../tests/fixtures/topology/sphere-pole.cfv2"),
    include_bytes!("../../tests/fixtures/topology/cone-corner.cfv2"),
    include_bytes!("../../tests/fixtures/topology/slanted-cylinder.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-decoy.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-nurbs-rectangle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/ambiguous-nurbs-rectangle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/sparse-circle-cylinder.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-meridian-tilted-sphere.cfv2"),
    include_bytes!("../../tests/fixtures/topology/rolling-ball-patch.cfv2"),
    include_bytes!("../../tests/fixtures/topology/rolling-ball-variable.cfv2"),
    include_bytes!("../../tests/fixtures/topology/rolling-ball-sections.cfv2"),
    include_bytes!("../../tests/fixtures/topology/rolling-ball-bowed-section.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-nurbs-triangle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/ambiguous-nurbs-triangle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/off-nurbs-triangle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-ruled-patch.cfv2"),
    include_bytes!("../../tests/fixtures/topology/ambiguous-ruled-patch.cfv2"),
    include_bytes!("../../tests/fixtures/topology/off-ruled-patch.cfv2"),
    include_bytes!("../../tests/fixtures/topology/invalid-ruled-tail.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-cone-shift.cfv2"),
    include_bytes!("../../tests/fixtures/topology/rolling-ball-tip.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-cone-apex-chart.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-sparse-ellipse.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-nurbs-independent-limit.cfv2"),
    include_bytes!("../../tests/fixtures/topology/ambiguous-nurbs-independent-limit.cfv2"),
    include_bytes!("../../tests/fixtures/topology/off-nurbs-independent-limit.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-masked-cylinder-charts.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-unknown-cap.cfv2"),
    include_bytes!("../../tests/fixtures/topology/coelliptic-sparse-arcs.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-nurbs-owner-gap.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-parameter-maps.cfv2"),
    include_bytes!("../../tests/fixtures/topology/invalid-parameter-maps.cfv2"),
    include_bytes!("../../tests/fixtures/topology/parameter-maps-decoy.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-parameter-maps-only.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-asymmetric-parameter-lines.cfv2"),
    include_bytes!("../../tests/fixtures/topology/invalid-asymmetric-parameter-lines.cfv2"),
    include_bytes!("../../tests/fixtures/topology/asymmetric-parameter-lines-decoy.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-parameter-reference-contexts.cfv2"),
    include_bytes!("../../tests/fixtures/topology/invalid-parameter-reference-contexts.cfv2"),
    include_bytes!("../../tests/fixtures/topology/parameter-reference-contexts-decoy.cfv2"),
    include_bytes!("../../tests/fixtures/topology/separate-parameter-reference-contexts.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-allocation-uv.cfv2"),
    include_bytes!("../../tests/fixtures/topology/wrong-allocation-endpoint.cfv2"),
    include_bytes!("../../tests/fixtures/topology/wrong-allocation-support.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-allocation-uv-outside-domain.cfv2"),
    include_bytes!("../../tests/fixtures/topology/long-allocation-header.cfv2"),
    include_bytes!("../../tests/fixtures/topology/ambiguous-allocation-factories.cfv2"),
    include_bytes!("../../tests/fixtures/topology/allocation-opaque-decoy.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-sense.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-face-sense.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-compact-sense.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-reversed-edge.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-broken-cycle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-bad-count.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-bad-padding.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-loop-truncated-flags.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-explicit.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-reordered.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-line-extension.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-jet-span.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-bad-span.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-wrong-direction.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-wrong-map.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-point-trailing.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-unknown-control.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-plane.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-plane-trailing.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-descending.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-unknown-edge-control.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-compressed.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-edge-trim-context-default.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-scalar.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-reordered.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-inherited.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-range.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-span.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-bad-span.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-wrong-span.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-outside.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-line-index.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-extension.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-sphere.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-traversal-forward.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-reversed-edge.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-map-coordinate-sense-mismatch.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-extended-trim.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-negative-radius.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-unknown-control.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-trailing.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-wrong-support.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-nonfinite.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-child-u8.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-child-u16.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-child-ambiguous.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-forward-refs.cfv2"),
    include_bytes!("../../tests/fixtures/topology/native-uv-circle-ambiguous-forward.cfv2"),
];
fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > 64 * 1024 {
        return;
    }
    let options = ParseOptions {
        limits: ParseLimits {
            max_input_bytes: 128 * 1024,
            max_geometry_bytes: 128 * 1024,
            max_geometry_records: 512,
            max_geometry_steps: 256 * 1024,
            max_topology_bytes: 128 * 1024,
            max_topology_records: 8192,
            max_topology_steps: 8 * 1024 * 1024,
            ..ParseLimits::default()
        },
    };
    // Keep the base-case selector independent of the two branch bits, so every
    // fixture remains reachable when the case count is a multiple of four.
    let selector = data[0] as usize / 4 + 64 * usize::from(data.get(1).copied().unwrap_or(0));
    let mut changed = CASES[selector % CASES.len()].to_vec();
    let input = if data[0] % 4 == 0 {
        &data[1..]
    } else {
        // Localized mutations preserve most framing and reach deeper paths.
        for patch in data[2.min(data.len())..].chunks_exact(3) {
            let at = u16::from_le_bytes([patch[0], patch[1]]) as usize % changed.len();
            changed[at] ^= patch[2];
        }
        &changed
    };
    let out = decode_brep(input, &options);
    if let Some(d) = &out.value
        && let Some(t) = &d.topology
    {
        if let Some(n) = &t.native_uv {
            for map in n.parameter_maps.iter().take(16) {
                let _ = map.fields.query_at_source(0.5);
                let _ = map.fields.query_at_source(f64::NAN);
                let _ = map.fields.query_at_target(map.fields.target_range[0]);
                let _ = map.fields.query_at_target(0.5);
                let _ = map.fields.query_at_target(f64::NAN);
            }
            if let Err(e) = catiakit_core::topology::validate_native_uv(n, &options.limits) {
                assert!(e.code.is_resource_limit(), "{e}");
            }
            for c in n
                .nurbs_carriers
                .iter()
                .take(4)
                .filter(|c| c.geometry.is_valid())
            {
                for axis in 0..2 {
                    let s = &c.geometry;
                    let lo = s.knots[axis][s.degrees[axis]];
                    let hi = s.knots[axis][s.control_counts[axis]];
                    if let Ok(curve) = s.isocurve(axis, lo + (hi - lo) * 0.5) {
                        assert!(curve.is_valid());
                        let [a, z] = curve.range().unwrap();
                        let _ = curve.evaluate(a + (z - a) * 0.5);
                    }
                }
            }
        }
        for p in &t.boundaries {
            catiakit_core::topology::validate_boundaries(&d.geometry, p, &options.limits).unwrap();
        }
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
                &options.limits,
            )
            .unwrap();
        }
        for binding in &t.native_boundary_uv {
            let native = t.native_uv.as_ref().unwrap();
            for u in binding.uses.iter().take(16) {
                if let Ok(traversal) = binding.edge_map_traversal(u.edge_tag) {
                    for fraction in [0., 0.5, 1.] {
                        let parameter = traversal.map_target_at_fraction(fraction).unwrap();
                        let _ = traversal.fraction_at_map_target(parameter);
                    }
                    assert!(traversal.map_target_at_fraction(f64::NAN).is_err());
                }
                if let Some(trim) = &u.edge_trim {
                    let _ = trim.map_target_parameters();
                }
                if let Ok(image) = u.nurbs_curve_image(native) {
                    image.validate_against(u, native).unwrap();
                    let _ = image.evaluate_on_edge(0.5);
                    let _ = image.evaluate(u.parameter_range[0]);
                    let _ = image.derivative(u.parameter_range[1]);
                }
                for fraction in [0., 0.5, 1., f64::NAN] {
                    let _ = u.evaluate_in_loop(fraction, native);
                    if let Ok(parameter) = u.map_target_parameter_on_edge(fraction) {
                        let _ = u.evaluate_map_target(parameter, native);
                        let _ = u.lift_map_target(parameter, native);
                    }
                }
                let _ = binding.edge_gap_at(u.edge_tag, u.parameter_range[0], native);
                if let Ok(paired) = binding.paired_edge(u.edge_tag, native) {
                    let source = paired.second_representation().parameter_range[0];
                    let _ = paired.query_at_source(source);
                    let _ = paired.gap_at_source(source);
                    let target = paired.parameter_map().fields.target_range[0];
                    let _ = paired.query_at_target(target);
                    let _ = paired.gap_at_target(target);
                    if let Some(trim) = &paired.coordinate_reference().edge_trim {
                        for vertex in trim.vertex_tags {
                            let _ = paired.endpoint_images(vertex);
                        }
                    }
                    let _ = paired.endpoint_images(u32::MAX);
                }
            }
            let boundary = t
                .boundaries
                .iter()
                .find(|b| b.population_id == binding.population_id)
                .unwrap();
            if let Err(e) = catiakit_core::topology::validate_native_boundary_uv(
                &d.geometry,
                boundary,
                binding,
                t.native_uv.as_ref().unwrap(),
                &options.limits,
            ) {
                assert!(e.code.is_resource_limit(), "{e}");
            }
        }
    }
    if out.report.features.topology == FeatureStatus::Complete {
        let d = out.value.unwrap();
        let t = d.topology.unwrap();
        validate_topology(&d.geometry, &t, &options.limits).unwrap();
        assert!(t.unresolved.is_empty());
        assert!(
            t.bodies
                .iter()
                .all(|b| b.export_eligible && !b.solid_validated)
        );
    }
});
