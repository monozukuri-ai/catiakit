//! Source-defined rolling-ball carriers qualified against complete native face
//! packets. Geometric correspondence is derived; persistent identity is separate.
use super::*;
fn limit_station(surface: &RollingBallJet, support: &SurfaceGeometry, uv: [f64; 2]) -> Option<f64> {
    let phi = if uv[1] < 1e-4 {
        0.
    } else if uv[1] > 1. - 1e-4 {
        1.
    } else {
        return Some(uv[0]);
    };
    let [lo, hi] = surface.parameter_range;
    let width = (hi - lo) * 1e-3;
    let value = |t: f64| support.implicit_value(surface.evaluate_validated([t, phi]).ok()?);
    let mut a = (uv[0] - width).max(lo);
    let mut z = (uv[0] + width).min(hi);
    let (mut fa, fz) = (value(a)?, value(z)?);
    if fa * fz < 0. {
        for _ in 0..48 {
            let mid = (a + z) / 2.;
            let f = value(mid)?;
            if fa * f > 0. {
                a = mid;
                fa = f
            } else {
                z = mid
            }
        }
        return Some((a + z) / 2.);
    }
    // A tangential section can have a double root; bounded minimization must
    // still supply a checked zero of the analytic carrier equation.
    let golden = (5f64.sqrt() - 1.) / 2.;
    let mut c = z - golden * (z - a);
    let mut d = a + golden * (z - a);
    let mut fc = value(c)?.abs();
    let mut fd = value(d)?.abs();
    for _ in 0..48 {
        if fc < fd {
            z = d;
            d = c;
            fd = fc;
            c = z - golden * (z - a);
            fc = value(c)?.abs()
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + golden * (z - a);
            fd = value(d)?.abs()
        }
    }
    let t = (a + z) / 2.;
    (value(t)?.abs() < 1e-9).then_some(t)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn recover(
    surfaces: &mut [SurfaceRecord],
    curves: &mut [CurveRecord],
    points: &[PointRecord],
    wires: &[Wire],
    raw: &RawSource,
    binding: &EndpointBinding,
    native: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
) -> Result<()> {
    let Some(native) = native else { return Ok(()) };
    let table = &binding.0;
    let ends = &binding.1;
    // This optional production has a bounded preflight. Preserve the ordinary
    // analytic result when the caller has not budgeted the expensive native
    // inverse witnesses; an attempted operation still uses the global budget.
    let pending: BTreeSet<_> = surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            matches!(
                s.geometry,
                SurfaceGeometry::Unknown {
                    reason: Code::GeometryCarrierUnsupported
                }
            )
        })
        .map(|(i, _)| i)
        .collect();
    let paths: u64 = wires
        .iter()
        .filter(|w| pending.contains(&w.face))
        .flat_map(|w| &w.occurrences)
        .map(|o| o.path.len() as u64)
        .sum();
    let unit = native
        .rolling_ball_carriers
        .iter()
        .map(|n| n.geometry.inversion_work())
        .max()
        .unwrap_or(0);
    let estimated = unit.saturating_mul(
        (pending.len() as u64)
            .saturating_mul(native.rolling_ball_carriers.len() as u64)
            .saturating_add(paths.saturating_mul(2)),
    );
    if estimated > b.limits.max_topology_steps.saturating_sub(b.steps) {
        return Ok(());
    }

    for (face, s) in surfaces.iter_mut().enumerate() {
        if !matches!(
            s.geometry,
            SurfaceGeometry::Unknown {
                reason: Code::GeometryCarrierUnsupported
            }
        ) {
            continue;
        }
        let vertices: BTreeSet<_> = wires
            .iter()
            .filter(|w| w.face == face)
            .flat_map(|w| &w.occurrences)
            .flat_map(|o| ends[o.edge])
            .collect();
        let mut handles: BTreeSet<_> = wires
            .iter()
            .filter(|w| w.face == face)
            .flat_map(|w| &w.occurrences)
            .flat_map(|o| o.path.iter().copied())
            .collect();
        let interior: Vec<_> = raw.packets[face]
            .handles
            .iter()
            .copied()
            .filter(|h| !handles.contains(h))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !interior.is_empty() {
            for i in 0..8 {
                handles.insert(interior[i * (interior.len() - 1) / 7]);
            }
        }
        let mut candidates = Vec::new();
        for n in &native.rolling_ball_carriers {
            let mut valid = true;
            for v in &vertices {
                b.work(n.geometry.inversion_work())?;
                if n.geometry
                    .parameters_at_point(points[*v].position, 1e-3)
                    .is_none()
                {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            // Interior native packet witnesses distinguish two rolling-ball patches
            // that share exactly the same limiting curves and corner vertices.
            for h in &handles {
                b.work(n.geometry.inversion_work())?;
                if n.geometry
                    .parameters_at_point(table.coords[*h as usize], 1e-3)
                    .is_none()
                {
                    valid = false;
                    break;
                }
            }
            if valid {
                candidates.push(n)
            }
        }
        if candidates.len() != 1 {
            continue;
        }
        let n = candidates[0];
        b.add(n.geometry.knots.len().saturating_mul(31))?;
        s.geometry = SurfaceGeometry::RollingBall {
            surface: n.geometry.clone(),
        };
        s.domains = s.geometry.domains();
        s.parameterization = "derived_unique_native_rolling_ball_packet_correspondence".into();
        s.evidence.extend([
            n.source.clone(),
            raw.packets[face].source.clone(),
            table.source.clone(),
        ]);
    }
    // Exact stored limiting channels can supply split physical edge intervals.
    for (e, c) in curves.iter_mut().enumerate() {
        if !matches!(c.geometry, CurveGeometry::Unknown { .. })
            || !c.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let owners: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
            .map(|w| w.face)
            .collect();
        let mut candidates = Vec::new();
        for f in &owners {
            let SurfaceGeometry::RollingBall { surface } = &surfaces[*f].geometry else {
                continue;
            };
            for side in 0..2 {
                let Some(curve) = surface.limit_curve(side) else {
                    continue;
                };
                let mut range = [0.; 2];
                let mut valid = true;
                for (i, v) in ends[e].into_iter().enumerate() {
                    b.work(curve.inversion_work())?;
                    if let Some(t) = curve.parameter_at_point(points[v].position, 1e-3) {
                        range[i] = t
                    } else {
                        valid = false;
                        break;
                    }
                }
                if !valid || (range[1] - range[0]).abs() < 1e-10 {
                    continue;
                }
                for o in wires
                    .iter()
                    .flat_map(|w| &w.occurrences)
                    .filter(|o| o.edge == e)
                {
                    let mut old = None;
                    for h in &o.path {
                        b.work(curve.inversion_work())?;
                        let Some(t) = curve.parameter_at_point(table.coords[*h as usize], 1e-3)
                        else {
                            valid = false;
                            break;
                        };
                        let mut q = (t - range[0]) / (range[1] - range[0]);
                        if !o.forward {
                            q = 1. - q
                        }
                        if !(-1e-5..=1. + 1e-5).contains(&q) || old.is_some_and(|x| q < x - 1e-5) {
                            valid = false;
                            break;
                        }
                        old = Some(q);
                    }
                    if !valid {
                        break;
                    }
                }
                if valid {
                    candidates.push((curve, range, surfaces[*f].evidence.clone()))
                }
            }
        }
        if candidates.len() == 1 {
            let (curve, range, evidence) = candidates.remove(0);
            b.add(
                curve
                    .control_points
                    .len()
                    .saturating_mul(2)
                    .saturating_add(curve.knots.len()),
            )?;
            c.geometry = CurveGeometry::Nurbs { curve };
            c.domain = c.geometry.domain();
            c.parameter_interval = Some(range);
            c.parameterization = "derived_native_rolling_ball_limiting_channel".into();
            c.evidence.extend(evidence);
        }
    }
    for (e, c) in curves.iter_mut().enumerate() {
        if !matches!(c.geometry, CurveGeometry::Unknown { .. })
            || !c.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let owners: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
            .map(|w| w.face)
            .collect();
        if owners.len() != 2 {
            continue;
        }
        let mut candidates = Vec::new();
        for side in 0..2 {
            let SurfaceGeometry::RollingBall { surface } = &surfaces[owners[side]].geometry else {
                continue;
            };
            let standard_support = &surfaces[owners[1 - side]].geometry;
            let exact: Vec<_> = native
                .carriers
                .iter()
                .filter(|c| native_curve_binding::same_locus(&c.geometry, standard_support))
                .collect();
            let support = if exact.len() == 1 {
                &exact[0].geometry
            } else {
                standard_support
            };
            if !matches!(
                support,
                SurfaceGeometry::Plane { .. }
                    | SurfaceGeometry::Cylinder { .. }
                    | SurfaceGeometry::Torus { .. }
                    | SurfaceGeometry::Sphere { .. }
            ) {
                continue;
            }
            let mut uv = [[0.; 2]; 2];
            let mut valid = true;
            for (i, v) in ends[e].into_iter().enumerate() {
                b.work(surface.inversion_work())?;
                if let Some(p) = surface.parameters_at_point(points[v].position, 1e-3) {
                    uv[i] = p
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid || (uv[1][0] - uv[0][0]).abs() < 1e-6 {
                continue;
            }
            for point in &mut uv {
                b.work(8192)?;
                if let Some(t) = limit_station(surface, support, *point) {
                    point[0] = t
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            let curve = CurveGeometry::RollingIntersection {
                surface: Box::new(surface.clone()),
                support: Box::new(support.clone()),
                u_range: [uv[0][0], uv[1][0]],
                phi_hints: [uv[0][1], uv[1][1]],
            };
            for (i, v) in ends[e].into_iter().enumerate() {
                b.work(8192)?;
                if curve
                    .evaluate(i as f64)
                    .map_or(true, |p| distance(p, points[v].position) > 1e-3)
                {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            for o in wires
                .iter()
                .flat_map(|w| &w.occurrences)
                .filter(|o| o.edge == e)
            {
                let mut previous = None;
                for h in &o.path {
                    b.work(surface.inversion_work().saturating_add(8192))?;
                    let p = table.coords[*h as usize];
                    let Some(q) = surface.parameters_at_point(p, 1e-3) else {
                        valid = false;
                        break;
                    };
                    let t = (q[0] - uv[0][0]) / (uv[1][0] - uv[0][0]);
                    if !(-1e-5..=1. + 1e-5).contains(&t)
                        || curve
                            .evaluate(t.clamp(0., 1.))
                            .map_or(true, |z| distance(z, p) > 1e-3)
                    {
                        valid = false;
                        break;
                    }
                    let t = if o.forward { t } else { 1. - t };
                    if previous.is_some_and(|x| t < x - 1e-5) {
                        valid = false;
                        break;
                    }
                    previous = Some(t);
                }
                if !valid {
                    break;
                }
            }
            if valid {
                candidates.push((curve, surfaces[owners[side]].evidence.clone()))
            }
        }
        if candidates.len() == 1 {
            let (curve, evidence) = candidates.remove(0);
            b.add(charts::carrier_records(
                &SurfaceGeometry::Unknown {
                    reason: Code::GeometryCarrierUnsupported,
                },
                &curve,
            ))?;
            c.geometry = curve;
            c.domain = c.geometry.domain();
            c.parameter_interval = Some([0., 1.]);
            c.parameterization = "derived_native_rolling_ball_analytic_intersection".into();
            c.evidence.extend(evidence)
        }
    }
    Ok(())
}
