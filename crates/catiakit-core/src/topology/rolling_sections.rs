//! Constant-station circular sections of a serialized rolling-ball jet.
//! Endpoints supply a station witness; all paths and both carrier lifts qualify
//! the result. No circle is fitted to visualization points.
use super::*;
pub(super) fn recover(
    curves: &mut [CurveRecord],
    surfaces: &[SurfaceRecord],
    points: &[PointRecord],
    wires: &[Wire],
    binding: &EndpointBinding,
    b: &mut Budget<'_>,
) -> Result<()> {
    for (edge, c) in curves.iter_mut().enumerate() {
        if !matches!(c.geometry, CurveGeometry::Unknown { .. })
            || !c.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let owners: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == edge))
            .map(|w| w.face)
            .collect();
        let endpoints = binding.1[edge].map(|v| points[v].position);
        let mut candidates = Vec::new();
        for f in &owners {
            let SurfaceGeometry::RollingBall { surface } = &surfaces[*f].geometry else {
                continue;
            };
            let mut uv = [[0.; 2]; 2];
            let mut valid = true;
            for i in 0..2 {
                b.work(surface.inversion_work())?;
                if let Some(p) = surface.parameters_at_point(endpoints[i], 1e-3) {
                    uv[i] = p
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            // A station discrepancy is a rejection gate only. The first native
            // endpoint's inverse supplies the section; it is not averaged/fitted.
            if (uv[0][0] - uv[1][0]).abs() > 1e-3 {
                continue;
            }
            let row = surface
                .row_validated(uv[0][0])
                .map_err(|_| fail(Code::TopologyGeometryUnsupported, "native section station"))?;
            let origin = [row[6], row[7], row[8]];
            let a = sub([row[0], row[1], row[2]], origin);
            let z = sub([row[3], row[4], row[5]], origin);
            let radius = dot(a, a).sqrt();
            let (Some(x), Some(normal)) = (unit(a), unit(cross(a, z))) else {
                continue;
            };
            let curve = CurveGeometry::Circle {
                frame: Frame3 {
                    origin,
                    x_axis: x,
                    y_axis: cross(normal, x),
                    z_axis: normal,
                },
                radius,
            };
            let mut interval = None;
            for o in wires
                .iter()
                .flat_map(|w| &w.occurrences)
                .filter(|o| o.edge == edge)
            {
                let mut path = o.path.clone();
                if !o.forward {
                    path.reverse()
                }
                for h in &path {
                    b.work(1)?;
                    let p = binding.0.coords[*h as usize];
                    let a = curve.angular_parameter(p).ok_or_else(|| {
                        fail(Code::TopologyGeometryUnsupported, "native section angle")
                    })?;
                    if curve.evaluate(a).map_or(true, |q| distance(p, q) > 1e-3) {
                        valid = false;
                        break;
                    }
                }
                if !valid {
                    break;
                }
                let range = match circle_path(&curve, &path, &binding.0, b) {
                    Ok((a, sweep)) => [a, a + sweep],
                    Err(error) => {
                        if error.code.is_resource_limit() {
                            return Err(error);
                        }
                        valid = false;
                        break;
                    }
                };
                if interval.is_some_and(|v: [f64; 2]| {
                    delta_angle(v[0] - range[0]).abs() > 1e-6
                        || ((v[1] - v[0]) - (range[1] - range[0])).abs() > 1e-6
                }) {
                    valid = false;
                    break;
                }
                interval = Some(range);
            }
            let Some(range) = interval else { continue };
            if !valid {
                continue;
            }
            for i in 0..=16 {
                let t = range[0] + (range[1] - range[0]) * i as f64 / 16.;
                let p = curve.evaluate(t).map_err(|_| {
                    fail(Code::TopologyGeometryUnsupported, "native section circle")
                })?;
                for f in &owners {
                    match charts::inverse_metered(&surfaces[*f].geometry, p, b) {
                        Ok(_) => (),
                        Err(error) if error.code.is_resource_limit() => return Err(error),
                        Err(_) => {
                            valid = false;
                            break;
                        }
                    }
                }
                if !valid {
                    break;
                }
            }
            if valid {
                candidates.push((curve, range, surfaces[*f].evidence.clone()))
            }
        }
        let Some(first) = candidates.first() else {
            continue;
        };
        let mut equivalent = true;
        for (curve, range, _) in candidates.iter().skip(1) {
            for i in 0..=16 {
                b.work(2)?;
                let q = i as f64 / 16.;
                let a = first.0.evaluate(first.1[0] + q * (first.1[1] - first.1[0]));
                let z = curve.evaluate(range[0] + q * (range[1] - range[0]));
                if !matches!((a,z),(Ok(a),Ok(z)) if distance(a,z)<1e-4) {
                    equivalent = false;
                    break;
                }
            }
        }
        if !equivalent {
            continue;
        }
        let (curve, range, evidence) = candidates.remove(0);
        c.geometry = curve;
        c.domain = c.geometry.domain();
        c.parameter_interval = Some(range);
        c.parameterization = "derived_native_rolling_ball_constant_station_section".into();
        c.evidence.extend(evidence);
        c.evidence.push(binding.0.source.clone());
        for (_, _, evidence) in candidates {
            c.evidence.extend(evidence)
        }
    }
    Ok(())
}
