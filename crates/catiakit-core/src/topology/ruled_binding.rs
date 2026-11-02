//! Source-derived scaled-reference ruled jets qualified by every native boundary
//! and interior witnesses. Carrier correspondence does not assign persistent tags.
use super::*;
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
    if native.ruled_carriers.is_empty() {
        return Ok(());
    }
    let mut carriers = Vec::new();
    for n in &native.ruled_carriers {
        b.work(n.geometry.knots.len() as u64 * 128)?;
        if let Some(s) = n.geometry.surface() {
            carriers.push((n, s))
        }
    }
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
    let unit = carriers
        .iter()
        .map(|(_, s)| s.inversion_work())
        .max()
        .unwrap_or(0);
    let estimated = unit.saturating_mul(
        paths
            .saturating_mul(4)
            .saturating_add((pending.len() as u64).saturating_mul(carriers.len() as u64 + 8)),
    );
    if estimated > b.limits.max_topology_steps.saturating_sub(b.steps) {
        return Ok(());
    }
    let mut sources = BTreeMap::new();
    for face in pending {
        let local: Vec<_> = wires.iter().filter(|w| w.face == face).collect();
        if local.len() != 1 {
            continue;
        }
        let vs: BTreeSet<_> = local
            .iter()
            .flat_map(|w| &w.occurrences)
            .flat_map(|o| binding.1[o.edge])
            .collect();
        let mut handles: BTreeSet<_> = local
            .iter()
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
        let mut accepted = Vec::new();
        for (n, s) in &carriers {
            b.work(s.control_points.len() as u64 * 6 + vs.len() as u64 * 3)?;
            let bounds: [[f64; 2]; 3] = std::array::from_fn(|d| {
                s.control_points
                    .iter()
                    .fold([f64::INFINITY, f64::NEG_INFINITY], |[a, z], p| {
                        [a.min(p[d]), z.max(p[d])]
                    })
            });
            if vs.iter().any(|v| {
                (0..3).any(|d| {
                    points[*v].position[d] < bounds[d][0] - 1e-3
                        || points[*v].position[d] > bounds[d][1] + 1e-3
                })
            }) {
                continue;
            }
            let mut valid = true;
            for p in vs
                .iter()
                .map(|v| points[*v].position)
                .chain(handles.iter().map(|h| binding.0.coords[*h as usize]))
            {
                b.work(s.inversion_work())?;
                if s.parameters_at_point(p, 1e-3).is_none() {
                    valid = false;
                    break;
                }
            }
            if valid {
                accepted.push((*n, s))
            }
        }
        let Some(first) = accepted.first() else {
            continue;
        };
        if accepted.iter().any(|(_, s)| *s != first.1) {
            continue;
        }
        let geometry = SurfaceGeometry::Nurbs {
            surface: first.1.clone(),
        };
        b.add(charts::carrier_records(
            &geometry,
            &CurveGeometry::Point { position: [0.; 3] },
        ))?;
        let s = &mut surfaces[face];
        s.geometry = geometry;
        s.domains = s.geometry.domains();
        s.parameterization = "derived_unique_native_scaled_reference_ruled_jet".into();
        s.evidence.extend([
            first.0.source.clone(),
            raw.packets[face].source.clone(),
            binding.0.source.clone(),
        ]);
        sources.insert(face, first.0.source.clone());
    }
    for (e, c) in curves.iter_mut().enumerate() {
        if !matches!(c.geometry, CurveGeometry::Unknown { .. })
            || !c.raw_parameters_hex.starts_with("000000")
        {
            continue;
        }
        let mut accepted = Vec::new();
        for (face, source) in &sources {
            if !wires
                .iter()
                .filter(|w| w.face == *face)
                .any(|w| w.occurrences.iter().any(|o| o.edge == e))
            {
                continue;
            }
            let SurfaceGeometry::Nurbs { surface } = &surfaces[*face].geometry else {
                continue;
            };
            let Some(sides) = surface.boundary_curves() else {
                continue;
            };
            for (axis, _fixed, curve) in sides {
                // Both V limits are supplied by the closed source chart. Other
                // current trim curves require their own serialized definition.
                if axis != 1 {
                    continue;
                }
                let mut range = [0.; 2];
                let mut valid = true;
                for (i, v) in binding.1[e].into_iter().enumerate() {
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
                        let Some(t) = curve.parameter_at_point(binding.0.coords[*h as usize], 1e-3)
                        else {
                            valid = false;
                            break;
                        };
                        let q = (t - range[0]) / (range[1] - range[0]);
                        let q = if o.forward { q } else { 1. - q };
                        if !(-1e-6..=1. + 1e-6).contains(&q) || old.is_some_and(|p| q < p - 1e-6) {
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
                    accepted.push((curve, range, source.clone()))
                }
            }
        }
        let Some(first) = accepted.first() else {
            continue;
        };
        if accepted
            .iter()
            .any(|(curve, range, _)| *curve != first.0 || *range != first.1)
        {
            continue;
        }
        b.add(first.0.control_points.len() * 2 + first.0.knots.len())?;
        c.geometry = CurveGeometry::Nurbs {
            curve: first.0.clone(),
        };
        c.domain = c.geometry.domain();
        c.parameter_interval = Some(first.1);
        c.parameterization = "derived_native_ruled_limit_with_complete_path".into();
        c.evidence.extend(accepted.into_iter().map(|(_, _, s)| s));
    }
    Ok(())
}
