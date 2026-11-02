use super::*;

pub(super) fn connector(
    surface: &SurfaceGeometry,
    point: Vec3,
    a: [f64; 2],
    c: [f64; 2],
    b: &mut Budget<'_>,
) -> Result<()> {
    let rolling = if let SurfaceGeometry::RollingBall { surface } = surface {
        b.work(surface.knots.len() as u64 * 64)?;
        if !surface.is_valid() {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "invalid rolling connector carrier",
            ));
        }
        let limits = surface.parameter_range;
        let epsilon = 1e-5 * (limits[1] - limits[0]).max(1.);
        limits.into_iter().any(|u| {
            if (a[0] - u).abs() > epsilon
                || (c[0] - u).abs() > epsilon
                || !(0. ..=1.).contains(&a[1])
                || !(0. ..=1.).contains(&c[1])
                || (a[1] - c[1]).abs() < 1e-8
            {
                return false;
            }
            let Ok(row) = surface.row_validated(u) else {
                return false;
            };
            row[9] == 0.
                && distance([row[0], row[1], row[2]], [row[3], row[4], row[5]]) < 1e-8
                && distance([row[0], row[1], row[2]], point) <= 1e-3
        })
    } else {
        false
    };
    if !rolling
        && (!matches!(
            surface,
            SurfaceGeometry::Cone { .. } | SurfaceGeometry::Sphere { .. }
        ) || !charts::singular(surface, point)
            || (a[0] - c[0]).abs() < 1e-8
            || (a[1] - c[1]).abs() > 1e-8)
    {
        return Err(fail(
            Code::TopologyDegenerateUnresolved,
            "UV connector is not at one polar native vertex",
        ));
    }
    for j in 0..=8 {
        b.work(if let SurfaceGeometry::RollingBall { surface } = surface {
            surface.knots.len() as u64 * 64
        } else {
            1
        })?;
        let q = j as f64 / 8.;
        let uv = [a[0] + q * (c[0] - a[0]), a[1] + q * (c[1] - a[1])];
        let p = surface
            .evaluate(uv)
            .map_err(|_| fail(Code::TopologyPcurveInvalid, "polar connector evaluation"))?;
        if distance(p, point) > 1e-3 {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "polar UV connector leaves native vertex",
            ));
        }
    }
    Ok(())
}
pub(super) fn capture(
    t: &mut TopologyModel,
    surfaces: &[SurfaceRecord],
    points: &[PointRecord],
    b: &mut Budget<'_>,
) -> Result<()> {
    let pcs: BTreeMap<_, _> = t.pcurves.iter().map(|p| (&p.id, p)).collect();
    let ces: BTreeMap<_, _> = t.coedges.iter().map(|c| (&c.id, c)).collect();
    let edges: BTreeMap<_, _> = t.edges.iter().map(|e| (&e.id, e)).collect();
    let vertices: BTreeMap<_, _> = t.vertices.iter().map(|v| (&v.id, v)).collect();
    let ps: BTreeMap<_, _> = points.iter().map(|p| (&p.id, p)).collect();
    for boundary in &mut t.loops {
        for (i, id) in boundary.coedges.iter().enumerate() {
            b.work(1)?;
            let coedge = ces[id];
            let next = ces[&boundary.coedges[(i + 1) % boundary.coedges.len()]];
            let pc = pcs[&coedge.pcurve];
            let next_pc = pcs[&next.pcurve];
            let surface = &surfaces
                .iter()
                .find(|s| s.id == pc.surface)
                .ok_or_else(|| {
                    fail(
                        Code::TopologyValidationInvalid,
                        "polar face surface missing",
                    )
                })?
                .geometry;
            for p in [pc, next_pc] {
                b.work(match &p.geometry {
                    PcurveGeometry::AnalyticProjection {
                        surface,
                        curve,
                        phase_guides,
                        ..
                    } => charts::projection_cost(surface, curve)
                        .saturating_add(phase_guides.len() as u64),
                    _ => 1,
                })?;
            }
            let a = pc
                .geometry
                .evaluate(1.)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "polar incoming UV"))?;
            let c = next_pc
                .geometry
                .evaluate(0.)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "polar outgoing UV"))?;
            if uv_distance(surface, a, c) <= 1e-3 {
                continue;
            }
            let edge = edges[&coedge.edge];
            let vertex = vertices[&edge.vertices[usize::from(coedge.forward)]];
            let p = ps[&vertex.point];
            connector(surface, p.position, a, c, b).map_err(|mut error| {
                error.detail = format!(
                    "{} join {} -> {} at {:?} -> {:?}: {}",
                    pc.surface, coedge.id, next.id, a, c, error.detail
                );
                error.source = Some(pc.source.clone());
                error
            })?;
            b.add(1)?;
            boundary.singular_transitions.push(SingularTransition {
                after_coedge: id.clone(),
                vertex: vertex.id.clone(),
                uv_start: a,
                uv_end: c,
                source: p.source.clone(),
                evidence: if matches!(surface, SurfaceGeometry::RollingBall { .. }) {
                    "derived_chart_connector_at_counted_native_rolling_tip"
                } else {
                    "derived_chart_connector_at_counted_native_polar_vertex"
                }
                .into(),
            });
        }
    }
    Ok(())
}
