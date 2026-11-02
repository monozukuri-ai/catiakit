//! Source boundary curves can qualify an edge without qualifying the complete
//! source rectangle as its owning face. At least one resolved current owner,
//! every native endpoint and every occurrence path must agree.
use super::*;

fn injective_coordinate(curve: &NurbsCurve) -> bool {
    if curve.weights.iter().any(|w| *w != curve.weights[0]) {
        return false;
    }
    (0..3).any(|axis| {
        [1., -1.].into_iter().any(|sense| {
            curve
                .control_points
                .windows(2)
                .all(|q| sense * (q[1][axis] - q[0][axis]) >= 0.)
                && curve
                    .control_points
                    .windows(curve.degree + 1)
                    .all(|q| sense * (q[curve.degree][axis] - q[0][axis]) > 0.)
        })
    })
}

fn parameter(
    curve: &NurbsCurve,
    point: Vec3,
    cache: &mut BTreeMap<[u32; 3], Option<f64>>,
    b: &mut Budget<'_>,
) -> Result<Option<f64>> {
    b.work(1)?;
    if let Some(value) = cache.get(&key(point)) {
        return Ok(*value);
    }
    b.work(curve.inversion_work())?;
    let value = curve.parameter_at_point(point, 1e-3);
    b.add(1)?;
    cache.insert(key(point), value);
    Ok(value)
}

pub(super) fn recover(
    curves: &mut [CurveRecord],
    surfaces: &[SurfaceRecord],
    points: &[PointRecord],
    wires: &[Wire],
    binding: &EndpointBinding,
    native: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
) -> Result<()> {
    let Some(native) = native else { return Ok(()) };
    for (edge, current) in curves.iter_mut().enumerate() {
        if !matches!(current.geometry, CurveGeometry::Unknown { .. })
            || !current.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let owners: BTreeSet<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == edge))
            .map(|w| w.face)
            .filter(|f| !matches!(surfaces[*f].geometry, SurfaceGeometry::Unknown { .. }))
            .collect();
        if owners.is_empty() {
            continue;
        }
        let endpoints = binding.1[edge].map(|v| points[v].position);
        let mut accepted = Vec::new();
        for n in &native.nurbs_carriers {
            b.work(n.geometry.control_points.len() as u64 * 2)?;
            let Some(sides) = n.geometry.boundary_curves() else {
                continue;
            };
            b.add(
                sides
                    .iter()
                    .map(|(_, _, c)| c.control_points.len() * 2 + c.knots.len() + 1)
                    .sum(),
            )?;
            for (_, _, curve) in sides {
                b.work(curve.control_points.len() as u64 * 12)?;
                if !injective_coordinate(&curve) {
                    continue;
                }
                let bounds: [[f64; 2]; 3] = std::array::from_fn(|d| {
                    curve
                        .control_points
                        .iter()
                        .fold([f64::INFINITY, f64::NEG_INFINITY], |[a, z], p| {
                            [a.min(p[d]), z.max(p[d])]
                        })
                });
                if endpoints.iter().any(|p| {
                    (0..3).any(|d| p[d] < bounds[d][0] - 1e-3 || p[d] > bounds[d][1] + 1e-3)
                }) {
                    continue;
                }
                let mut cache = BTreeMap::new();
                let Some(a) = parameter(&curve, endpoints[0], &mut cache, b)? else {
                    continue;
                };
                let Some(z) = parameter(&curve, endpoints[1], &mut cache, b)? else {
                    continue;
                };
                if (a - z).abs() < 1e-12 {
                    continue;
                }
                let mut valid = true;
                for i in 0..=16 {
                    b.work(curve.control_points.len() as u64 * 2 + 64)?;
                    let t = if i == 16 {
                        z
                    } else {
                        a + (z - a) * i as f64 / 16.
                    };
                    let Ok(p) = curve.evaluate(t) else {
                        valid = false;
                        break;
                    };
                    for owner in &owners {
                        match charts::inverse_metered(&surfaces[*owner].geometry, p, b) {
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
                if !valid {
                    continue;
                }
                for o in wires
                    .iter()
                    .flat_map(|w| &w.occurrences)
                    .filter(|o| o.edge == edge)
                {
                    let mut previous = None;
                    for h in &o.path {
                        let Some(t) =
                            parameter(&curve, binding.0.coords[*h as usize], &mut cache, b)?
                        else {
                            valid = false;
                            break;
                        };
                        let mut q = (t - a) / (z - a);
                        if !o.forward {
                            q = 1. - q;
                        }
                        if !(-1e-6..=1. + 1e-6).contains(&q)
                            || previous.is_some_and(|p| q < p - 1e-6)
                        {
                            valid = false;
                            break;
                        }
                        previous = Some(q);
                    }
                    if !valid {
                        break;
                    }
                }
                if valid {
                    b.add(curve.control_points.len() * 2 + curve.knots.len() + 1)?;
                    accepted.push((curve, [a, z], n.source.clone()));
                }
            }
        }
        let Some(first) = accepted.first() else {
            continue;
        };
        b.work(
            accepted
                .iter()
                .map(|(c, _, _)| c.control_points.len() as u64 * 128 + 1024)
                .sum(),
        )?;
        if !accepted.iter().skip(1).all(|(c, range, _)| {
            (0..=16).all(|i| {
                let q = i as f64 / 16.;
                matches!((first.0.evaluate(first.1[0] + q * (first.1[1] - first.1[0])),
                    c.evaluate(range[0] + q * (range[1] - range[0]))),
                    (Ok(a), Ok(z)) if distance(a, z) < 1e-5)
            })
        }) {
            continue;
        }
        let (curve, range, _) = accepted[0].clone();
        b.add(curve.control_points.len() * 2 + curve.knots.len())?;
        current.geometry = CurveGeometry::Nurbs { curve };
        current.domain = current.geometry.domain();
        current.parameter_interval = Some(range);
        current.parameterization =
            "derived_native_nurbs_limit_with_complete_path_and_known_owner".into();
        current
            .evidence
            .extend(accepted.into_iter().map(|(_, _, source)| source));
    }
    Ok(())
}
