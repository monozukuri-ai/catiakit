use catiakit_core::{
    ParseOptions, decode_brep,
    geometry::EvaluationError,
    topology::{
        NativeParameterMapFields, NativeParameterQuery as Query,
        NativeParameterRelation as Relation, NativeTargetParameterQuery as TargetQuery,
    },
};

fn fields() -> NativeParameterMapFields {
    let out = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-parameter-maps.cfv2"),
        &ParseOptions::default(),
    );
    out.value
        .unwrap()
        .topology
        .unwrap()
        .native_uv
        .unwrap()
        .parameter_maps[5]
        .fields
        .clone()
}

#[test]
fn stored_samples_and_strict_interior_queries_have_different_results() {
    for sp in [vec![0., 2., 4.], vec![4., 2., 0.]] {
        let mut f = fields();
        f.target_range = [10., 30.];
        f.relation = Relation::Sampled {
            source_range: [0., 4.],
            source_parameters: sp.clone(),
            target_parameters: vec![10., 20., 30.],
        };
        assert_eq!(
            f.query_at_source(sp[1]).unwrap(),
            Query::Exact {
                source_parameter: sp[1],
                target_parameter: 20.,
                stored_sample: true
            }
        );
        let t = (sp[0] + sp[1]) / 2.;
        let q = f.query_at_source(t).unwrap();
        assert_eq!(
            q,
            Query::Bracket {
                source_parameter: t,
                first_sample_index: 0,
                source_parameters: [sp[0], sp[1]],
                target_parameters: [10., 20.]
            }
        );
        assert_eq!(f.image_at(t), None);
        let restored: Query = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(q, restored);
    }
}

#[test]
fn affine_identity_undefined_and_incomplete_coverage_are_explicit() {
    let mut f = fields();
    f.target_range = [10., 20.];
    f.relation = Relation::Affine {
        source_range: [0., 5.],
        scale: -2.,
        offset: 20.,
    };
    assert_eq!(
        f.query_at_source(2.).unwrap(),
        Query::Exact {
            source_parameter: 2.,
            target_parameter: 16.,
            stored_sample: false
        }
    );
    assert_eq!(f.query_at_source(6.), Err(EvaluationError::OutsideDomain));
    assert_eq!(
        f.query_at_source(f64::NAN),
        Err(EvaluationError::InvalidParameter)
    );
    f.target_range = [0., 5.];
    f.relation = Relation::Identity {
        source_range: [0., 5.],
    };
    assert!(matches!(
        f.query_at_source(3.),
        Ok(Query::Exact {
            target_parameter: 3.,
            stored_sample: false,
            ..
        })
    ));
    f.relation = Relation::Sampled {
        source_range: [0., 5.],
        source_parameters: vec![1., 4.],
        target_parameters: vec![1., 4.],
    };
    assert_eq!(
        f.query_at_source(0.5),
        Err(EvaluationError::UnresolvedGeometry)
    );
    f.relation = Relation::Unspecified;
    assert_eq!(
        f.query_at_source(2.),
        Err(EvaluationError::UnresolvedGeometry)
    );
}

#[test]
fn malformed_public_tables_and_signed_zero_aliases_never_invent_pairs() {
    let mut original = fields();
    original.target_range = [0., 1.];
    original.relation = Relation::Sampled {
        source_range: [0., 1.],
        source_parameters: vec![-0., 1.],
        target_parameters: vec![0., 1.],
    };
    assert!(matches!(
        original.query_at_source(-0.),
        Ok(Query::Exact { .. })
    ));
    assert_eq!(
        original.query_at_source(0.),
        Err(EvaluationError::UnresolvedGeometry)
    );
    for i in 0..7 {
        let mut f = original.clone();
        let Relation::Sampled {
            source_parameters: sp,
            target_parameters: tp,
            ..
        } = &mut f.relation
        else {
            unreachable!()
        };
        match i {
            0 => {
                sp.pop();
            }
            1 => sp[1] = 0.,
            2 => tp[1] = -1.,
            3 => sp[1] = f64::INFINITY,
            4 => tp[0] = f64::NAN,
            5 => {
                *sp = (0..128).map(f64::from).collect();
                *tp = sp.clone();
            }
            _ => f.target_range = [-f64::MAX, f64::MAX],
        }
        assert_eq!(
            f.query_at_source(0.5),
            Err(EvaluationError::InvalidGeometry)
        );
    }
}

#[test]
fn target_queries_retain_decreasing_pairs_and_never_interpolate() {
    for sp in [vec![0., 2., 4.], vec![4., 2., 0.]] {
        let mut f = fields();
        f.target_range = [10., 30.];
        f.relation = Relation::Sampled {
            source_range: [0., 4.],
            source_parameters: sp.clone(),
            target_parameters: vec![10., 20., 30.],
        };
        for (i, target) in [10., 20., 30.].into_iter().enumerate() {
            assert_eq!(
                f.query_at_target(target).unwrap(),
                TargetQuery::Exact {
                    source_parameter: sp[i],
                    target_parameter: target,
                    stored_sample: true,
                }
            );
        }
        let q = f.query_at_target(15.).unwrap();
        assert_eq!(
            q,
            TargetQuery::Bracket {
                target_parameter: 15.,
                first_sample_index: 0,
                source_parameters: [sp[0], sp[1]],
                target_parameters: [10., 20.]
            }
        );
        let round: TargetQuery = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(q, round);
    }
}

#[test]
fn target_queries_keep_declared_domains_table_tails_and_zero_bits_separate() {
    let mut f = fields();
    f.target_range = [-1., 2.];
    f.relation = Relation::Sampled {
        source_range: [0., 1.],
        source_parameters: vec![0., 1.],
        target_parameters: vec![-0., 1.],
    };
    assert!(matches!(
        f.query_at_target(-0.),
        Ok(TargetQuery::Exact { .. })
    ));
    assert_eq!(
        f.query_at_target(0.),
        Err(EvaluationError::UnresolvedGeometry)
    );
    assert_eq!(
        f.query_at_target(-0.5),
        Err(EvaluationError::UnresolvedGeometry)
    );
    assert_eq!(
        f.query_at_target(1.5),
        Err(EvaluationError::UnresolvedGeometry)
    );
    assert_eq!(f.query_at_target(-2.), Err(EvaluationError::OutsideDomain));
    assert_eq!(
        f.query_at_target(f64::NAN),
        Err(EvaluationError::InvalidParameter)
    );
    f.relation = Relation::Unspecified;
    assert_eq!(
        f.query_at_target(0.5),
        Err(EvaluationError::UnresolvedGeometry)
    );
}

#[test]
fn affine_target_candidates_must_replay_the_requested_bits() {
    let mut f = fields();
    f.target_range = [10., 20.];
    f.relation = Relation::Affine {
        source_range: [0., 5.],
        scale: -2.,
        offset: 20.,
    };
    assert_eq!(
        f.query_at_target(16.).unwrap(),
        TargetQuery::Exact {
            source_parameter: 2.,
            target_parameter: 16.,
            stored_sample: false,
        }
    );
    assert_eq!(f.query_at_target(21.), Err(EvaluationError::OutsideDomain));
    f.target_range = [-2., 4.];
    f.relation = Relation::Affine {
        source_range: [-1., 1.],
        scale: 3.,
        offset: 1.,
    };
    // Ordinary inverse arithmetic gives -0.3, whose forward image is a
    // different binary64 value. Do not label that as the requested pair.
    assert_ne!((-0.3_f64 * 3. + 1.).to_bits(), 0.1_f64.to_bits());
    assert_eq!(
        f.query_at_target(0.1),
        Err(EvaluationError::UnresolvedGeometry)
    );
    f.target_range = [0., 5.];
    f.relation = Relation::Identity {
        source_range: [0., 5.],
    };
    assert_eq!(
        f.query_at_target(3.).unwrap(),
        TargetQuery::Exact {
            source_parameter: 3.,
            target_parameter: 3.,
            stored_sample: false,
        }
    );
}

#[test]
fn malformed_target_tables_and_near_domain_samples_fail_closed() {
    let mut original = fields();
    original.target_range = [0., 1.];
    original.relation = Relation::Sampled {
        source_range: [0., 1.],
        source_parameters: vec![0., 1.],
        target_parameters: vec![0., 1.],
    };
    for i in 0..8 {
        let mut f = original.clone();
        let Relation::Sampled {
            source_parameters: sp,
            target_parameters: tp,
            source_range,
        } = &mut f.relation
        else {
            unreachable!()
        };
        match i {
            0 => {
                sp.pop();
            }
            1 => sp[1] = 0.,
            2 => tp[1] = 0.,
            3 => sp[1] = f64::INFINITY,
            4 => tp[0] = f64::NAN,
            5 => {
                *sp = (0..128).map(f64::from).collect();
                *tp = sp.clone();
            }
            6 => *source_range = [-f64::MAX, f64::MAX],
            _ => f.target_range = [-f64::MAX, f64::MAX],
        }
        assert_eq!(
            f.query_at_target(0.5),
            Err(EvaluationError::InvalidGeometry)
        );
    }
    if let Relation::Sampled {
        source_parameters, ..
    } = &mut original.relation
    {
        source_parameters[0] = -f64::EPSILON;
    }
    assert_eq!(
        original.query_at_target(0.),
        Err(EvaluationError::OutsideDomain)
    );
}

fn pair() -> (
    catiakit_core::topology::NativeBoundaryUv,
    catiakit_core::topology::NativeUvModel,
) {
    let d = decode_brep(
        include_bytes!("../../../tests/fixtures/topology/native-map-coordinate-extension.cfv2"),
        &ParseOptions::default(),
    )
    .value
    .unwrap();
    let top = d.topology.unwrap();
    let mut b = top.native_boundary_uv[0].clone();
    let a = b.uses[0].clone();
    let mut z = b.uses[1].clone();
    z.edge_tag = a.edge_tag;
    z.representation_tag = a.representation_tag;
    z.parameter_map = a.parameter_map.clone();
    z.map_operand = 1;
    b.uses = vec![a, z];
    // Evaluator-only construction. A native positive must also replay its
    // complete binding; real source-qualified pairs are audited separately.
    (b, top.native_uv.unwrap())
}

#[test]
fn map_gap_observes_a_saved_target_extension_without_changing_uv_point_api() {
    let (b, n) = pair();
    let edge = b.uses[0].edge_tag;
    let p = 1. / 6.;
    assert!(b.edge_gap_at(edge, p, &n).is_err());
    let pair = b.paired_edge(edge, &n).unwrap();
    assert_eq!(pair.coordinate_reference().map_operand, 0);
    assert_eq!(pair.second_representation().map_operand, 1);
    let gap = pair.gap_at_source(p).unwrap();
    assert!((gap.parameters[0] - 0.1).abs() < 1e-14);
    assert!(gap.distance > 0.001 && !gap.stored_sample);
    assert_eq!(gap, b.map_gap_at_source(edge, p, &n).unwrap());
}

#[test]
fn interior_brackets_cannot_be_evaluated_as_a_common_spatial_curve() {
    let (b, mut n) = pair();
    let id = b.uses[0].parameter_map.clone();
    let map = n.parameter_maps.iter_mut().find(|m| m.id == id).unwrap();
    map.fields.relation = Relation::Sampled {
        source_range: [0., 1.],
        source_parameters: vec![0., 0.5, 1.],
        target_parameters: vec![-0.05, 0.1, 1.05],
    };
    let p = b.paired_edge(b.uses[0].edge_tag, &n).unwrap();
    assert!(matches!(p.query_at_source(0.25), Ok(Query::Bracket { .. })));
    assert_eq!(
        p.gap_at_source(0.25),
        Err(EvaluationError::UnresolvedGeometry)
    );
    assert!(p.gap_at_source(0.5).unwrap().stored_sample);
    assert_eq!(p.gap_at_source(0.), Err(EvaluationError::OutsideDomain));
    assert_eq!(p.gap_at_target(0.1).unwrap(), p.gap_at_source(0.5).unwrap());
    assert_eq!(
        p.gap_at_target(0.),
        Err(EvaluationError::UnresolvedGeometry)
    );
    assert_eq!(
        b.map_gap_at_target(b.uses[0].edge_tag, 0.1, &n).unwrap(),
        p.gap_at_source(0.5).unwrap()
    );
}

#[test]
fn ambiguous_incomplete_or_duplicate_pairs_are_not_selected() {
    let (b, n) = pair();
    let edge = b.uses[0].edge_tag;
    for i in 0..6 {
        let mut changed = b.clone();
        match i {
            0 => {
                changed.uses.pop();
            }
            1 => changed.uses.push(changed.uses[0].clone()),
            2 => changed.uses[1].uv_tag = changed.uses[0].uv_tag,
            3 => changed.uses[1].map_operand = 0,
            4 => changed.uses[1].representation_tag += 1,
            _ => changed.uses[1].parameter_map.push('x'),
        }
        assert!(changed.paired_edge(edge, &n).is_err());
    }
    let mut duplicate = n.clone();
    duplicate.parameter_maps.push(
        duplicate
            .parameter_maps
            .iter()
            .find(|m| m.id == b.uses[0].parameter_map)
            .unwrap()
            .clone(),
    );
    assert!(b.paired_edge(edge, &duplicate).is_err());
}

fn endpoint_pair() -> (
    catiakit_core::topology::NativeBoundaryUv,
    catiakit_core::topology::NativeUvModel,
) {
    // An evaluator-only pair with intentionally different UV/map endpoints.
    // Full native-byte/identity replay is exercised by the development audit.
    let (mut b, n) = pair();
    let a = b.uses[0].clone();
    let z_tag = b.uses[1].uv_tag;
    let mut trim = a.edge_trim.clone().unwrap();
    for (i, p) in trim.endpoints.iter_mut().enumerate() {
        p.references = [a.representation_tag, a.uv_tag, z_tag];
        p.map_coordinate.as_mut().unwrap().parameter = [0.1, 0.8][i];
    }
    b.uses[0].edge_trim = Some(trim.clone());
    b.uses[1].edge_trim = Some(trim);
    b.uses[1].edge_trim.as_mut().unwrap().parameters = b.uses[1]
        .edge_trim
        .as_ref()
        .unwrap()
        .endpoints
        .each_ref()
        .map(|p| p.uv_parameters[1]);
    b.uses[1].parameter_range = [0., 1.];
    (b, n)
}

#[test]
fn endpoint_images_keep_map_gauge_separate_from_uv_points_and_saved_pairs() {
    let (b, mut n) = endpoint_pair();
    let edge = b.uses[0].edge_tag;
    let trim = b.uses[0].edge_trim.as_ref().unwrap();
    let map = n
        .parameter_maps
        .iter_mut()
        .find(|m| m.id == b.uses[0].parameter_map)
        .unwrap();
    map.fields.relation = Relation::Sampled {
        source_range: [0., 1.],
        target_parameters: vec![0.1, 0.8],
        source_parameters: vec![0.2, 0.8],
    };
    for i in 0..2 {
        let p = b.paired_edge(edge, &n).unwrap();
        let sample = p.endpoint_images(trim.vertex_tags[i]).unwrap();
        let u = trim.endpoints[i].uv_parameters;
        let t = trim.endpoints[i].map_coordinate.as_ref().unwrap().parameter;
        assert_eq!(sample.map_target_parameter.to_bits(), t.to_bits());
        assert_eq!(sample.uv_point_parameters, u);
        assert_eq!(
            sample.points[0],
            p.coordinate_reference().lift_map_target(t, &n).unwrap()
        );
        assert_eq!(
            sample.points[1],
            p.second_representation().lift(u[1], &n).unwrap()
        );
        assert_ne!(
            sample.points[0],
            p.coordinate_reference().lift(u[0], &n).unwrap()
        );
        assert!(sample.distance > 0.001);
        assert!(p.gap_at_source(u[1]).is_err()); // No synthetic map pair is invented.
        let mut reversed = b.clone();
        for v in &mut reversed.uses {
            for ep in &mut v.edge_trim.as_mut().unwrap().endpoints {
                ep.references.swap(1, 2);
                ep.uv_parameters.swap(0, 1);
            }
        }
        assert_eq!(
            sample,
            reversed
                .paired_edge(edge, &n)
                .unwrap()
                .endpoint_images(trim.vertex_tags[i])
                .unwrap()
        );
    }
}

#[test]
fn inconsistent_endpoint_ids_coordinates_and_missing_gauges_are_rejected() {
    let (b, n) = endpoint_pair();
    let edge = b.uses[0].edge_tag;
    let vertex = b.uses[0].edge_trim.as_ref().unwrap().vertex_tags[0];
    for kind in 0..9 {
        let mut changed = b.clone();
        match kind {
            0 => changed.uses[0].edge_trim = None,
            1 => changed.uses[1]
                .edge_trim
                .as_mut()
                .unwrap()
                .vertex_tags
                .swap(0, 1),
            2 => changed.uses[1].edge_trim.as_mut().unwrap().endpoints[0].allocation_tag += 1,
            3 => {
                for u in &mut changed.uses {
                    u.edge_trim.as_mut().unwrap().endpoints[0].references[1] += 100;
                }
            }
            4 => {
                for u in &mut changed.uses {
                    u.edge_trim.as_mut().unwrap().endpoints[0].map_coordinate = None;
                }
            }
            5 => changed.uses[1].edge_trim.as_mut().unwrap().parameters[0] += 0.1,
            6 => {
                for u in &mut changed.uses {
                    u.edge_trim.as_mut().unwrap().endpoints[0]
                        .map_coordinate
                        .as_mut()
                        .unwrap()
                        .parameter = f64::NAN;
                }
            }
            7 => changed.uses[1].parameter_range = [0.5, 1.],
            _ => {
                for u in &mut changed.uses {
                    u.edge_trim.as_mut().unwrap().vertex_tags[1] = vertex;
                }
            }
        }
        assert!(
            changed
                .paired_edge(edge, &n)
                .unwrap()
                .endpoint_images(vertex)
                .is_err(),
            "{kind}"
        );
    }
    assert!(
        b.paired_edge(edge, &n)
            .unwrap()
            .endpoint_images(u32::MAX)
            .is_err()
    );
}

#[test]
fn a_carrier_domain_failure_does_not_hide_the_other_endpoint() {
    let (mut b, n) = endpoint_pair();
    for u in &mut b.uses {
        u.edge_trim.as_mut().unwrap().endpoints[0]
            .map_coordinate
            .as_mut()
            .unwrap()
            .parameter = -0.05;
    }
    let p = b.paired_edge(b.uses[0].edge_tag, &n).unwrap();
    let vertices = b.uses[0].edge_trim.as_ref().unwrap().vertex_tags;
    assert_eq!(
        p.endpoint_images(vertices[0]),
        Err(EvaluationError::OutsideDomain)
    );
    assert!(p.endpoint_images(vertices[1]).is_ok());
}
