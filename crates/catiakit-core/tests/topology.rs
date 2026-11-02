use catiakit_core::{
    ClosureExpectation, DiagnosticCode as Code, FeatureStatus, GeometryDocument, ParseLimits,
    ParseOptions, ParseStatus, TopologyOptions,
    container::reconstruct_stream,
    decode_brep, decode_brep_reader, decode_brep_with_options, decode_geometry, inspect,
    topology::{BodyKind, LoopRole, PcurveGeometry, ShellClosure},
    validate_topology,
};

const BOX: &[u8] = include_bytes!("../../../tests/fixtures/topology/box.cfv2");
const CYLINDER: &[u8] = include_bytes!("../../../tests/fixtures/topology/cylinder.cfv2");
const HOLE: &[u8] = include_bytes!("../../../tests/fixtures/topology/through-hole.cfv2");
const OPEN: &[u8] = include_bytes!("../../../tests/fixtures/topology/open-sheet.cfv2");

fn doc(bytes: &[u8]) -> GeometryDocument {
    let out = decode_brep(bytes, &ParseOptions::default());
    assert_eq!(
        out.report.features.topology,
        FeatureStatus::Complete,
        "{:?}",
        out
    );
    assert_ne!(out.report.status, ParseStatus::Rejected);
    let d = out.value.unwrap();
    validate_topology(
        &d.geometry,
        d.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    d
}
fn patch(data: &[u8], stream_id: u32, at: usize, bytes: &[u8]) -> Vec<u8> {
    let c = inspect(data, &ParseOptions::default())
        .value
        .unwrap()
        .container
        .unwrap();
    let stream = c.stream(stream_id).unwrap();
    let mut payload = reconstruct_stream(data, stream, &ParseLimits::default()).unwrap();
    payload[at..at + bytes.len()].copy_from_slice(bytes);
    let mut output = data.to_vec();
    for e in &stream.extents {
        output[e.file_range.offset as usize..e.file_range.end().unwrap() as usize].copy_from_slice(
            &payload[e.logical_offset as usize..(e.logical_offset + e.logical_length) as usize],
        );
    }
    output
}

#[test]
fn basic_shapes_have_closed_manifold_graphs_and_no_solid_claim() {
    for (bytes, expected) in [
        (BOX, (8, 12, 6, 6, 24, 2)),
        (CYLINDER, (2, 3, 3, 3, 6, 2)),
        (HOLE, (10, 15, 7, 9, 30, 0)),
    ] {
        let d = doc(bytes);
        let t = d.topology.unwrap();
        assert_eq!(
            (
                t.vertices.len(),
                t.edges.len(),
                t.faces.len(),
                t.loops.len(),
                t.coedges.len(),
                t.shells[0].euler_characteristic
            ),
            expected
        );
        assert_eq!(t.pcurves.len(), t.coedges.len());
        assert_eq!(t.bodies[0].kind, BodyKind::ClosedShell);
        assert!(t.bodies[0].export_eligible);
        assert!(!t.bodies[0].solid_validated);
        assert!(t.bodies[0].native_body_id.is_none());
        assert_eq!(t.shells[0].closure, ShellClosure::Closed);
        assert!(t.coedges.iter().all(|c| c.radial_next.is_some()));
        assert!(
            d.geometry
                .curves
                .iter()
                .all(|c| c.parameter_interval.is_some() && c.endpoint_point_ids.is_some())
        );
        assert!(d.geometry.unresolved.is_empty());
        assert!(
            d.geometry
                .surface_uses
                .iter()
                .all(|u| u.body_id.as_ref() == Some(&t.bodies[0].id))
        );
    }
}

#[test]
fn circular_edges_and_seam_keep_distinct_uv_branches() {
    let d = doc(CYLINDER);
    let t = d.topology.unwrap();
    let seam = t.edges.iter().find(|e| e.seam).unwrap();
    assert_ne!(seam.vertices[0], seam.vertices[1]);
    let uses: Vec<_> = t.coedges.iter().filter(|c| c.edge == seam.id).collect();
    assert_eq!(uses.len(), 2);
    assert_ne!(uses[0].forward, uses[1].forward);
    assert_eq!(uses[0].owner_loop, uses[1].owner_loop);
    let uv: Vec<_> = uses
        .iter()
        .map(|c| {
            t.pcurves
                .iter()
                .find(|p| p.id == c.pcurve)
                .unwrap()
                .geometry
                .evaluate(0.5)
                .unwrap()
        })
        .collect();
    assert!(((uv[0][0] - uv[1][0]).abs() - std::f64::consts::TAU).abs() < 1e-8);
    for e in t.edges.iter().filter(|e| !e.seam) {
        assert_eq!(e.vertices[0], e.vertices[1]);
        assert!(
            ((e.parameter_range[1] - e.parameter_range[0]).abs() - std::f64::consts::TAU).abs()
                < 1e-8
        );
    }
}

#[test]
fn wide_counts_and_periodic_uv_join_validation_are_independent() {
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/wide-cylinder.cfv2"
    ));
    let mut t = d.topology.unwrap();
    assert_eq!(t.edges.len(), 3);
    assert!(t.coedges.iter().any(|c| c.trim_handles.len() > 255));
    let seam = t.edges.iter().find(|e| e.seam).unwrap();
    let usage = t.coedges.iter().find(|c| c.edge == seam.id).unwrap();
    let pc = t.pcurves.iter_mut().find(|p| p.id == usage.pcurve).unwrap();
    let PcurveGeometry::Line { start, .. } = &mut pc.geometry else {
        panic!("seam pcurve")
    };
    // A periodic shift preserves its 3D lift but breaks the unwrapped UV chain.
    start[0] += std::f64::consts::TAU;
    assert!(validate_topology(&d.geometry, &t, &ParseLimits::default()).is_err());
}

#[test]
fn holes_are_inner_loops_with_opposite_chart_winding() {
    let d = doc(HOLE);
    let t = d.topology.unwrap();
    assert_eq!(
        t.loops.iter().filter(|l| l.role == LoopRole::Inner).count(),
        2
    );
    for face in &t.faces {
        for id in &face.loops {
            let l = t.loops.iter().find(|l| &l.id == id).unwrap();
            assert_eq!(
                l.signed_uv_area.signum() as i8,
                if l.role == LoopRole::Outer {
                    face.chart_sense
                } else {
                    -face.chart_sense
                }
            );
        }
    }
    assert_eq!(
        t.faces.iter().filter(|f| f.native_sense_byte == -1).count(),
        1
    );
}

#[test]
fn open_shell_is_explicit_and_caller_closure_mismatch_blocks_export() {
    let d = doc(OPEN);
    let t = d.topology.unwrap();
    assert_eq!(t.bodies[0].kind, BodyKind::OpenShell);
    assert!(t.coedges.iter().all(|c| c.radial_next.is_none()));
    let out = decode_brep_with_options(
        OPEN,
        &ParseOptions::default(),
        &TopologyOptions {
            expected_closure: ClosureExpectation::Closed,
        },
    );
    assert_eq!(out.report.features.topology, FeatureStatus::Partial);
    let t = out.value.unwrap().topology.unwrap();
    assert!(!t.bodies[0].export_eligible);
    assert_eq!(t.unresolved[0].code, Code::TopologyClosureMismatch);
    let out = decode_brep_with_options(
        OPEN,
        &ParseOptions::default(),
        &TopologyOptions {
            expected_closure: ClosureExpectation::Open,
        },
    );
    assert_eq!(out.report.features.topology, FeatureStatus::Complete);
}

#[test]
fn geometry_default_remains_independent_and_topology_failure_is_transactional() {
    let before = decode_geometry(BOX, &ParseOptions::default());
    assert!(before.value.as_ref().unwrap().topology.is_none());
    assert_eq!(before.report.features.topology, FeatureStatus::NotRequested);
    assert!(
        before
            .value
            .unwrap()
            .geometry
            .curves
            .iter()
            .all(|c| c.parameter_interval.is_none())
    );
    for (bytes, code) in [
        (
            include_bytes!("../../../tests/fixtures/topology/ambiguous-index.cfv2").as_slice(),
            Code::TopologySourceUnsupported,
        ),
        (
            include_bytes!("../../../tests/fixtures/topology/coincident-identities.cfv2")
                .as_slice(),
            Code::TopologyEndpointAmbiguous,
        ),
        (
            include_bytes!("../../../tests/fixtures/topology/nonmanifold-mesh.cfv2").as_slice(),
            Code::TopologyNonManifold,
        ),
        (
            include_bytes!("../../../tests/fixtures/topology/degenerate-no-uv.cfv2").as_slice(),
            Code::TopologyDegenerateUnresolved,
        ),
    ] {
        let base = decode_geometry(bytes, &ParseOptions::default())
            .value
            .unwrap()
            .geometry;
        let out = decode_brep(bytes, &ParseOptions::default());
        let d = out.value.unwrap();
        let t = d.topology.unwrap();
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        assert_eq!(t.unresolved[0].code, code, "{:?}", t.unresolved);
        assert!(t.unresolved[0].source.is_some());
        assert!(t.faces.is_empty());
        assert!(t.bodies.iter().all(|b| !b.export_eligible));
        assert_eq!(d.geometry, base);
    }
}

#[test]
fn indexed_controls_and_auxiliary_word_are_not_guessed_count_invariants() {
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/box-compressed.cfv2"
    ));
    let base = doc(BOX);
    assert_eq!(
        d.geometry
            .points
            .iter()
            .map(|p| p.position)
            .collect::<Vec<_>>(),
        base.geometry
            .points
            .iter()
            .map(|p| p.position)
            .collect::<Vec<_>>()
    );
    let t = d.topology.unwrap();
    let scope = &t.scopes[0];
    assert_eq!(
        scope.uninterpreted_auxiliary_word,
        scope.index_entry_count + 7
    );
    assert_eq!(t.edges.len(), 12);
}

#[test]
fn index_bounds_and_nonfinite_coordinates_do_not_supply_topology() {
    let d = doc(BOX);
    let t = d.topology.unwrap();
    let s = &t.scopes[0].index_table_source;
    for bytes in [f32::NAN.to_le_bytes(), f32::INFINITY.to_le_bytes()] {
        let changed = patch(BOX, s.stream_id, s.logical_offset as usize + 19, &bytes);
        let out = decode_brep(&changed, &ParseOptions::default());
        assert_ne!(out.report.features.topology, FeatureStatus::Complete);
        serde_json::to_string(&out).unwrap();
    }
    let edge = &t.edges[0];
    let changed = patch(
        BOX,
        edge.source.stream_id,
        edge.source.logical_offset as usize + 2,
        &u16::MAX.to_be_bytes(),
    );
    assert_ne!(
        decode_brep(&changed, &ParseOptions::default())
            .report
            .features
            .topology,
        FeatureStatus::Complete
    );
}

#[test]
fn topology_limits_reject_without_committing_half_a_body() {
    for (limits, code) in [
        (
            ParseLimits {
                max_topology_bytes: 1,
                ..ParseLimits::default()
            },
            Code::TopologyByteLimitExceeded,
        ),
        (
            ParseLimits {
                max_topology_records: 1,
                ..ParseLimits::default()
            },
            Code::TopologyRecordLimitExceeded,
        ),
        (
            ParseLimits {
                max_topology_steps: 0,
                ..ParseLimits::default()
            },
            Code::TopologyStepLimitExceeded,
        ),
    ] {
        let out = decode_brep(BOX, &ParseOptions { limits });
        assert_eq!(out.report.status, ParseStatus::Rejected);
        assert_eq!(out.report.diagnostics.last().unwrap().code, code);
        let d = out.value.unwrap();
        let t = d.topology.unwrap();
        assert!(t.faces.is_empty());
        assert!(
            d.geometry
                .curves
                .iter()
                .all(|c| c.parameter_interval.is_none())
        );
    }
}

#[test]
fn public_validator_rejects_broken_ownership_radials_domains_and_pcurves() {
    let d = doc(HOLE);
    let t = d.topology.as_ref().unwrap();
    for case in 0..11 {
        let mut broken = t.clone();
        let mut geometry = d.geometry.clone();
        match case {
            0 => broken.coedges[0].radial_next = None,
            1 => broken.loops[0].coedges.pop().map(|_| ()).unwrap(),
            2 => broken.coedges[0].forward = !broken.coedges[0].forward,
            3 => broken.edges[0].parameter_range[1] = f64::INFINITY,
            4 => broken.vertices[0].edges.clear(),
            5 => broken.shells[0].euler_characteristic += 2,
            6 => broken.bodies[0].solid_validated = true,
            7 => {
                broken.pcurves[0].geometry = PcurveGeometry::Line {
                    start: [1e10, 1e10],
                    delta: [0., 0.],
                }
            }
            8 => geometry.points[0].position[0] = f64::NAN,
            9 => {
                let l = broken
                    .loops
                    .iter_mut()
                    .find(|l| l.role == LoopRole::Inner)
                    .unwrap();
                l.role = LoopRole::Outer;
            }
            10 => broken.pcurves[0].edge_parameter_range.reverse(),
            _ => unreachable!(),
        }
        assert!(
            validate_topology(&geometry, &broken, &ParseLimits::default()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn public_validator_rejects_split_shells_and_orphan_references() {
    let d = doc(BOX);
    let mut t = d.topology.unwrap();
    let mut second = t.shells[0].clone();
    second.id.push_str(":other");
    second.faces = t.shells[0].faces.split_off(3);
    for f in &mut t.faces {
        if second.faces.contains(&f.id) {
            f.shell = second.id.clone();
        }
    }
    t.regions[0].shells.push(second.id.clone());
    t.shells.push(second);
    assert!(validate_topology(&d.geometry, &t, &ParseLimits::default()).is_err());
    let mut t = doc(BOX).topology.unwrap();
    let mut c = t.coedges[0].clone();
    c.id.push_str(":orphan");
    t.coedges.push(c);
    assert!(validate_topology(&d.geometry, &t, &ParseLimits::default()).is_err());
}

#[test]
fn reader_json_roundtrip_and_structured_mutations_remain_deterministic() {
    assert_eq!(
        decode_brep(BOX, &ParseOptions::default()),
        decode_brep_reader(BOX, &ParseOptions::default(), &TopologyOptions::default())
    );
    let d = doc(CYLINDER);
    let restored: GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    validate_topology(
        &restored.geometry,
        restored.topology.as_ref().unwrap(),
        &ParseLimits::default(),
    )
    .unwrap();
    let options = ParseOptions {
        limits: ParseLimits {
            max_topology_steps: 200_000,
            max_topology_records: 5000,
            ..ParseLimits::default()
        },
    };
    for at in (0..BOX.len()).step_by(7) {
        let mut changed = BOX.to_vec();
        changed[at] ^= 255;
        let out = decode_brep(&changed, &options);
        assert_eq!(out, decode_brep(&changed, &options));
        serde_json::to_string(&out).unwrap();
        if out.report.features.topology == FeatureStatus::Complete {
            let d = out.value.unwrap();
            validate_topology(&d.geometry, d.topology.as_ref().unwrap(), &options.limits).unwrap();
        }
    }
}

#[test]
fn pcurve_evaluation_rejects_bad_parameters_and_overflow() {
    let p = PcurveGeometry::Line {
        start: [f64::MAX, 0.],
        delta: [f64::MAX, 0.],
    };
    assert!(p.evaluate(1.).is_err());
    assert!(p.evaluate(-1.).is_err());
    assert!(p.evaluate(f64::NAN).is_err());
}

#[test]
fn exact_coordinate_aliases_and_repeated_slots_preserve_native_ownership() {
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/indexed-aliases.cfv2"
    ));
    let t = d.topology.unwrap();
    assert_eq!((t.vertices.len(), t.edges.len(), t.faces.len()), (2, 3, 3));
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/repeated-incidence.cfv2"
    ));
    let t = d.topology.unwrap();
    assert_eq!(t.coedges.len(), 24);
    assert!(t.edges.iter().all(|e| !e.seam));
    let slots = d.geometry.curve_uses[0].serialized_face_slots;
    assert_eq!(slots[0], slots[1]);
    let owners: std::collections::BTreeSet<_> = t
        .coedges
        .iter()
        .filter(|c| c.edge == t.edges[0].id)
        .map(|c| &t.loops.iter().find(|l| l.id == c.owner_loop).unwrap().face)
        .collect();
    assert_eq!(owners.len(), 2);
}

#[test]
fn unsupported_curves_retain_validated_complete_boundary_incidence() {
    let bytes = include_bytes!("../../../tests/fixtures/topology/unsupported-curve.cfv2");
    let out = decode_brep(bytes, &ParseOptions::default());
    assert_eq!(out.report.features.topology, FeatureStatus::Partial);
    let d = out.value.unwrap();
    let t = d.topology.unwrap();
    assert!(t.faces.is_empty());
    assert!(!t.bodies[0].export_eligible);
    let summary = catiakit_core::topology::validate_boundaries(
        &d.geometry,
        &t.boundaries[0],
        &ParseLimits::default(),
    )
    .unwrap();
    assert_eq!(
        (
            summary.vertices,
            summary.edges,
            summary.faces,
            summary.loops,
            summary.occurrences
        ),
        (8, 12, 6, 6, 24)
    );
    assert!(summary.closed);
    let mut broken = t.boundaries[0].clone();
    broken.loops[0].occurrences[0].trim_handles[0] = u32::MAX;
    assert!(
        catiakit_core::topology::validate_boundaries(&d.geometry, &broken, &ParseLimits::default())
            .is_err()
    );
    let mut broken = t.boundaries[0].clone();
    broken.loops[0].occurrences[0].forward = None;
    assert!(
        catiakit_core::topology::validate_boundaries(&d.geometry, &broken, &ParseLimits::default())
            .is_err()
    );
}

#[test]
fn cone_and_sphere_poles_have_nontrivial_uv_and_point_loci() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/topology/cone-pole.cfv2").as_slice(),
        include_bytes!("../../../tests/fixtures/topology/sphere-pole.cfv2").as_slice(),
    ] {
        let d = doc(bytes);
        let t = d.topology.as_ref().unwrap();
        assert_eq!((t.vertices.len(), t.edges.len(), t.faces.len()), (3, 4, 1));
        assert_eq!(t.shells[0].euler_characteristic, 1);
        let edge = t.edges.iter().find(|e| e.degenerate).unwrap();
        assert_eq!(edge.vertices[0], edge.vertices[1]);
        let curve = &d
            .geometry
            .curves
            .iter()
            .find(|c| c.id == edge.curve)
            .unwrap()
            .geometry;
        assert!(matches!(
            curve,
            catiakit_core::geometry::CurveGeometry::Point { .. }
        ));
        assert_eq!(curve.evaluate(0.).unwrap(), curve.evaluate(1.).unwrap());
        assert!(curve.evaluate(-1.).is_err());
        let coedge = t.coedges.iter().find(|c| c.edge == edge.id).unwrap();
        let pc = t.pcurves.iter().find(|p| p.id == coedge.pcurve).unwrap();
        let a = pc.geometry.evaluate(0.).unwrap();
        let c = pc.geometry.evaluate(1.).unwrap();
        assert!(((a[0] - c[0]).abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((a[1] - c[1]).abs() < 1e-10);
        let restored: GeometryDocument =
            serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        validate_topology(
            &restored.geometry,
            restored.topology.as_ref().unwrap(),
            &ParseLimits::default(),
        )
        .unwrap();
        let mut broken = t.clone();
        broken
            .edges
            .iter_mut()
            .find(|e| e.degenerate)
            .unwrap()
            .degenerate = false;
        assert!(validate_topology(&d.geometry, &broken, &ParseLimits::default()).is_err());
        let mut broken = t.clone();
        let p = broken
            .pcurves
            .iter_mut()
            .find(|p| p.id == coedge.pcurve)
            .unwrap();
        if let PcurveGeometry::AnalyticProjection { phase_guides, .. } = &mut p.geometry {
            phase_guides[1][1] = phase_guides[0][1];
        } else {
            panic!("projection");
        }
        assert!(validate_topology(&d.geometry, &broken, &ParseLimits::default()).is_err());
    }
}

#[test]
fn native_style_apex_corner_has_a_uv_connector_without_a_physical_edge() {
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/cone-corner.cfv2"
    ));
    let t = d.topology.as_ref().unwrap();
    assert_eq!(t.edges.len(), 3);
    assert!(t.edges.iter().all(|e| !e.degenerate));
    let transitions = &t.loops[0].singular_transitions;
    assert_eq!(transitions.len(), 1);
    let tr = &transitions[0];
    assert!(((tr.uv_end[0] - tr.uv_start[0]).abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert!(tr.uv_start[1].abs() < 1e-12);
    assert!(tr.uv_end[1].abs() < 1e-12);
    for mutation in 0..4 {
        let mut bad = t.clone();
        match mutation {
            0 => bad.loops[0].singular_transitions.clear(),
            1 => bad.loops[0].singular_transitions[0].uv_end[1] = 1.,
            2 => bad.loops[0].singular_transitions[0].vertex = t.vertices[0].id.clone(),
            3 => bad.loops[0].singular_transitions.push(tr.clone()),
            _ => unreachable!(),
        };
        assert!(
            validate_topology(&d.geometry, &bad, &ParseLimits::default()).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn partial_analytic_occurrences_are_revalidated_without_body_completion() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/unsupported-curve.cfv2"),
        &ParseOptions::default(),
    );
    let d = out.value.unwrap();
    let t = d.topology.as_ref().unwrap();
    let p = &t.analytic_partials[0];
    let b = &t.boundaries[0];
    let summary = catiakit_core::topology::validate_analytic_partial(
        &d.geometry,
        b,
        p,
        &ParseLimits::default(),
    )
    .unwrap();
    assert_eq!((summary.pcurves, summary.unresolved_uses), (22, 2));
    assert!(p.unresolved.iter().all(|u| !u.detail.is_empty()));
    assert!(!t.bodies[0].export_eligible);
    let mut bad = p.clone();
    bad.pcurves[0].parameter_range.reverse();
    assert!(
        catiakit_core::topology::validate_analytic_partial(
            &d.geometry,
            b,
            &bad,
            &ParseLimits::default()
        )
        .is_err()
    );
    let mut bad = p.clone();
    bad.pcurves[0].occurrence = 999;
    assert!(
        catiakit_core::topology::validate_analytic_partial(
            &d.geometry,
            b,
            &bad,
            &ParseLimits::default()
        )
        .is_err()
    );
    let restored: GeometryDocument =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    let t = restored.topology.unwrap();
    catiakit_core::topology::validate_analytic_partial(
        &restored.geometry,
        &t.boundaries[0],
        &t.analytic_partials[0],
        &ParseLimits::default(),
    )
    .unwrap();
}

#[test]
fn partial_issue_census_cannot_hide_a_missing_occurrence() {
    let d = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/unsupported-curve.cfv2"),
        &ParseOptions::default(),
    )
    .value
    .unwrap();
    let t = d.topology.as_ref().unwrap();
    let partial = &t.analytic_partials[0];
    assert_eq!(partial.unresolved.len(), 2);
    for mutation in 0..4 {
        let mut bad = partial.clone();
        match mutation {
            0 => bad.unresolved[1] = bad.unresolved[0].clone(),
            1 => bad.unresolved[0].source.logical_offset += 1,
            2 => bad.unresolved[0].face_ordinal = 999,
            3 => bad.unresolved[0].edge_ordinal = bad.pcurves[0].edge_ordinal,
            _ => unreachable!(),
        }
        assert!(
            catiakit_core::topology::validate_analytic_partial(
                &d.geometry,
                &t.boundaries[0],
                &bad,
                &ParseLimits::default()
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
    // The diagnostic field is optional when reading older parser snapshots.
    let mut legacy = serde_json::to_value(partial).unwrap();
    for issue in legacy["unresolved"].as_array_mut().unwrap() {
        issue.as_object_mut().unwrap().remove("detail");
    }
    let legacy = serde_json::from_value(legacy).unwrap();
    catiakit_core::topology::validate_analytic_partial(
        &d.geometry,
        &t.boundaries[0],
        &legacy,
        &ParseLimits::default(),
    )
    .unwrap();
}

#[test]
fn reversed_nurbs_projection_preserves_declared_endpoint_bits_without_clamping() {
    use catiakit_core::geometry::{CurveGeometry, Frame3, NurbsCurve, SurfaceGeometry};
    use catiakit_core::topology::PcurveGeometry;
    let lo = -0.029015772197335_f64;
    let hi = 0.12407112691379152_f64;
    // This common expression rounds one ULP below the saved low knot.
    assert!(hi + (lo - hi) < lo);
    let curve = CurveGeometry::Nurbs {
        curve: NurbsCurve {
            degree: 1,
            knots: vec![lo, lo, hi, hi],
            control_points: vec![[0., 0., 0.], [1., 0., 0.]],
            weights: vec![1., 1.],
        },
    };
    let pc = PcurveGeometry::AnalyticProjection {
        surface: Box::new(SurfaceGeometry::Plane {
            frame: Frame3::identity(),
        }),
        curve: Box::new(curve.clone()),
        parameter_range: [hi, lo],
        phase_guides: vec![[0., 1., 0.], [1., 0., 0.]],
    };
    assert_eq!(pc.evaluate(0.).unwrap(), [1., 0.]);
    assert_eq!(pc.evaluate(1.).unwrap(), [0., 0.]);
    assert!((pc.evaluate(0.5).unwrap()[0] - 0.5).abs() < 1e-14);
    assert!(pc.evaluate(f64::from_bits(1_f64.to_bits() + 1)).is_err());
    assert!(curve.evaluate(hi + (lo - hi)).is_err());
    assert!(curve.evaluate(f64::from_bits(lo.to_bits() + 1)).is_err());
}

#[test]
fn ellipse_and_intersection_carriers_preserve_exact_evaluation_and_domain() {
    use catiakit_core::geometry::{CurveGeometry, EvaluationError, Frame3, RigidTransform};
    let ellipse = CurveGeometry::Ellipse {
        frame: Frame3::identity(),
        major_radius: 3.,
        minor_radius: 2.,
    };
    assert_eq!(ellipse.evaluate(0.).unwrap(), [3., 0., 0.]);
    let p = ellipse.evaluate(0.7).unwrap();
    assert!((p[0] * p[0] / 9. + p[1] * p[1] / 4. - 1.).abs() < 1e-12);
    let mut primary = Frame3::identity();
    primary.origin = [0., 2.5, 0.];
    let curve = CurveGeometry::CylinderIntersection {
        primary,
        primary_radius: 2.,
        secondary: Frame3::from_axis([0.; 3], [1., 0., 0.]).unwrap(),
        secondary_radius: 1.,
        branch: 1,
    };
    assert_eq!(curve.evaluate(0.), Err(EvaluationError::OutsideDomain));
    let t = -std::f64::consts::FRAC_PI_2;
    let p = curve.evaluate(t).unwrap();
    assert!((p[1] * p[1] + p[2] * p[2] - 1.).abs() < 1e-12);
    assert!((p[0] * p[0] + (p[1] - 2.5).powi(2) - 4.).abs() < 1e-12);
    let transform = RigidTransform {
        translation: [3., -4., 5.],
        rotation: [[0., 1., 0.], [0., 0., 1.], [1., 0., 0.]],
    };
    let moved = curve.transformed(&transform).unwrap().evaluate(t).unwrap();
    assert!(
        moved
            .into_iter()
            .zip([p[2] + 3., p[0] - 4., p[1] + 5.])
            .all(|(a, b)| (a - b).abs() < 1e-12)
    );
}

#[test]
fn native_spline_support_from_cylinder_plane_becomes_an_exact_ellipse() {
    let d = doc(include_bytes!(
        "../../../tests/fixtures/topology/slanted-cylinder.cfv2"
    ));
    let t = d.topology.as_ref().unwrap();
    assert_eq!(t.edges.len(), 3);
    let curve = &d.geometry.curves[1].geometry;
    assert!(matches!(
        curve,
        catiakit_core::geometry::CurveGeometry::Ellipse { .. }
    ));
    for i in 0..17 {
        let p = curve
            .evaluate(i as f64 * std::f64::consts::TAU / 16.)
            .unwrap();
        assert!((p[0] * p[0] + p[1] * p[1] - 9.).abs() < 1e-6);
        assert!((p[2] - 5. - 0.5 * p[0]).abs() < 1e-5);
    }
}

#[test]
fn closed_partial_occurrences_retain_their_parameter_direction() {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/partial-closed.cfv2"),
        &ParseOptions::default(),
    );
    let d = out.value.unwrap();
    let t = d.topology.unwrap();
    assert_eq!(t.analytic_partials.len(), 1);
    let p = &t.analytic_partials[0];
    assert!(p.pcurves.iter().any(|p| !p.forward));
    catiakit_core::topology::validate_analytic_partial(
        &d.geometry,
        &t.boundaries[0],
        p,
        &ParseLimits::default(),
    )
    .unwrap();
}
