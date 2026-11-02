//! Derived rectangle incidence: all four native carrier corners, every physical
//! boundary use and complete side coverage must agree. No fitted surface or
//! persistent identity is manufactured.
use super::*;
struct EdgeCandidate {
    edge: usize,
    curve: NurbsCurve,
    range: [f64; 2],
}
struct Rectangle {
    carrier: usize,
    edges: Vec<EdgeCandidate>,
    corners: Vec<usize>,
}
fn close(a: f64, c: f64) -> bool {
    (a - c).abs() <= 1e-9 * a.abs().max(c.abs()).max(1.)
}
fn equivalent(a: &NurbsCurve, c: &NurbsCurve) -> bool {
    if a.degree != c.degree
        || a.knots.len() != c.knots.len()
        || a.control_points.len() != c.control_points.len()
    {
        return false;
    }
    let Some([al, ah]) = a.range() else {
        return false;
    };
    let Some([cl, ch]) = c.range() else {
        return false;
    };
    for reversed in [false, true] {
        let n = a.control_points.len();
        let aw = a.weights[0];
        let cw = c.weights[if reversed { n - 1 } else { 0 }];
        let points = (0..n).all(|i| {
            let j = if reversed { n - 1 - i } else { i };
            (0..3).all(|d| close(a.control_points[i][d], c.control_points[j][d]))
                && close(a.weights[i] / aw, c.weights[j] / cw)
        });
        let knots = a.knots.iter().enumerate().all(|(i, k)| {
            let v = if reversed {
                (ch - c.knots[c.knots.len() - 1 - i]) / (ch - cl)
            } else {
                (c.knots[i] - cl) / (ch - cl)
            };
            close((*k - al) / (ah - al), v)
        });
        if points && knots {
            return true;
        }
    }
    false
}
fn rectangle(
    face: usize,
    surface: &NurbsSurface,
    wires: &[Wire],
    points: &[PointRecord],
    binding: &EndpointBinding,
    b: &mut Budget<'_>,
) -> Result<Option<(Vec<EdgeCandidate>, Vec<usize>)>> {
    let local: Vec<_> = wires.iter().filter(|w| w.face == face).collect();
    if local.len() != 1 {
        return Ok(None);
    }
    let w = local[0];
    let ends = &binding.1;
    let coords = &binding.0.coords;
    let vertices: BTreeSet<_> = w.occurrences.iter().flat_map(|o| ends[o.edge]).collect();
    let Some(boundaries) = surface.boundary_curves() else {
        return Ok(None);
    };
    let mut corners = Vec::new();
    for i in 0..2 {
        for j in 0..2 {
            b.work(surface.control_points.len() as u64 * 2 + 64)?;
            let uv = std::array::from_fn(|d| {
                surface.knots[d][if (if d == 0 { i } else { j }) == 0 {
                    surface.degrees[d]
                } else {
                    surface.control_counts[d]
                }]
            });
            let p = surface
                .evaluate(uv)
                .map_err(|_| fail(Code::TopologyGeometryUnsupported, "NURBS rectangle corner"))?;
            let hits: Vec<_> = vertices
                .iter()
                .filter(|v| distance(points[**v].position, p) <= 1e-3)
                .copied()
                .collect();
            if hits.len() != 1 {
                return Ok(None);
            }
            corners.push(hits[0]);
        }
    }
    if corners.iter().collect::<BTreeSet<_>>().len() != 4 {
        return Ok(None);
    }
    let mut edges = Vec::new();
    let mut coverage: [Vec<[f64; 2]>; 4] = std::array::from_fn(|_| Vec::new());
    for o in &w.occurrences {
        let mut accepted = Vec::new();
        for (side, (axis, fixed, c)) in boundaries.iter().enumerate() {
            let Some(domain) = c.range() else {
                continue;
            };
            let mut range = [0.; 2];
            let mut valid = true;
            for (end, v) in ends[o.edge].into_iter().enumerate() {
                b.work(c.inversion_work())?;
                let corner = corners.iter().position(|q| *q == v);
                let value = corner
                    .and_then(|corner| {
                        let coords = [corner / 2, corner % 2];
                        let fixed_end = if *fixed == surface.knots[*axis][surface.degrees[*axis]] {
                            0
                        } else {
                            1
                        };
                        (coords[*axis] == fixed_end).then_some(domain[coords[1 - *axis]])
                    })
                    .or_else(|| c.parameter_at_point(points[v].position, 1e-3));
                if let Some(t) = value {
                    range[end] = t;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid || (range[1] - range[0]).abs() < 1e-12 {
                continue;
            }
            let mut old = None;
            for h in &o.path {
                b.work(c.inversion_work())?;
                let Some(t) = c.parameter_at_point(coords[*h as usize], 1e-3) else {
                    valid = false;
                    break;
                };
                let q = (t - range[0]) / (range[1] - range[0]);
                let q = if o.forward { q } else { 1. - q };
                if !(-1e-6..=1. + 1e-6).contains(&q)
                    || old.is_some_and(|previous| q < previous - 1e-6)
                {
                    valid = false;
                    break;
                }
                old = Some(q);
            }
            if valid {
                accepted.push((side, c.clone(), range));
            }
        }
        if accepted.len() != 1 {
            return Ok(None);
        }
        let (side, curve, range) = accepted.remove(0);
        coverage[side].push([range[0].min(range[1]), range[0].max(range[1])]);
        edges.push(EdgeCandidate {
            edge: o.edge,
            curve,
            range,
        });
    }
    for (side, spans) in coverage.iter_mut().enumerate() {
        let [lo, hi] = boundaries[side].2.range().unwrap();
        let eps = 1e-6 * (hi - lo);
        spans.sort_by(|a, c| a[0].total_cmp(&c[0]));
        let mut end = lo;
        for span in spans {
            if (span[0] - end).abs() > eps {
                return Ok(None);
            }
            end = span[1];
        }
        if (end - hi).abs() > eps {
            return Ok(None);
        }
    }
    Ok(Some((edges, corners)))
}
pub(super) fn recover(
    surfaces: &mut [SurfaceRecord],
    curves: &mut [CurveRecord],
    points: &[PointRecord],
    wires: &[Wire],
    binding: &EndpointBinding,
    native: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
) -> Result<()> {
    let Some(native) = native else {
        return Ok(());
    };
    let mut proposals: BTreeMap<usize, Vec<(NurbsCurve, [f64; 2], StreamSpan)>> = BTreeMap::new();
    for (face, s) in surfaces.iter_mut().enumerate() {
        if !matches!(
            s.geometry,
            SurfaceGeometry::Unknown {
                reason: Code::GeometryCarrierUnsupported
            }
        ) {
            continue;
        }
        let mut accepted = Vec::new();
        for (index, n) in native.nurbs_carriers.iter().enumerate() {
            if let Some((edges, corners)) = rectangle(face, &n.geometry, wires, points, binding, b)?
            {
                accepted.push(Rectangle {
                    carrier: index,
                    edges,
                    corners,
                });
            }
        }
        if accepted.is_empty() {
            continue;
        }
        let first = &native.nurbs_carriers[accepted[0].carrier];
        if accepted
            .iter()
            .any(|c| native.nurbs_carriers[c.carrier].geometry != first.geometry)
        {
            continue;
        }
        let selected = accepted.remove(0);
        b.add(
            first.geometry.control_points.len() * 2
                + first.geometry.knots.iter().map(Vec::len).sum::<usize>(),
        )?;
        s.geometry = SurfaceGeometry::Nurbs {
            surface: first.geometry.clone(),
        };
        s.domains = s.geometry.domains();
        s.parameterization = "derived_unique_native_nurbs_rectangle_incidence".into();
        s.evidence.push(first.source.clone());
        s.evidence
            .extend(selected.corners.iter().map(|v| points[*v].source.clone()));
        for e in selected.edges {
            proposals
                .entry(e.edge)
                .or_default()
                .push((e.curve, e.range, first.source.clone()));
        }
    }
    for (edge, items) in proposals {
        let c = &mut curves[edge];
        if !matches!(c.geometry, CurveGeometry::Unknown { .. })
            || !c.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let first = &items[0];
        if items
            .iter()
            .any(|(curve, _, _)| !equivalent(&first.0, curve))
        {
            continue;
        }
        b.add(first.0.control_points.len() * 2 + first.0.knots.len())?;
        c.geometry = CurveGeometry::Nurbs {
            curve: first.0.clone(),
        };
        c.domain = c.geometry.domain();
        c.parameter_interval = Some(first.1);
        c.parameterization = "native_nurbs_boundary_curve_with_complete_rectangle_incidence".into();
        c.evidence
            .extend(items.into_iter().map(|(_, _, source)| source));
    }
    Ok(())
}
