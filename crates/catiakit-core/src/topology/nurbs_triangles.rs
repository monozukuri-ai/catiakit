//! Native NURBS carriers whose current trim is a triangle inside the knot
//! rectangle. Full native boundary witnesses qualify a unique stored carrier.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn recover(
    surfaces: &mut [SurfaceRecord],
    points: &[PointRecord],
    wires: &[Wire],
    raw: &RawSource,
    binding: &EndpointBinding,
    native: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
) -> Result<()> {
    let Some(native) = native else { return Ok(()) };
    let ends = &binding.1;
    let table = &binding.0;
    // Reserve the optional binding, subsequent occurrence construction and its
    // public revalidation together. A newly resolved face must not consume the
    // budget needed to retain ordinary analytic occurrences.
    let pending: Vec<_> = surfaces
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
        .filter_map(|(face, _)| {
            let local: Vec<_> = wires.iter().filter(|w| w.face == face).collect();
            let vs: BTreeSet<_> = local
                .iter()
                .flat_map(|w| &w.occurrences)
                .flat_map(|o| ends[o.edge])
                .collect();
            (local.len() == 1 && vs.len() == 3).then_some(local)
        })
        .collect();
    let paths: u64 = pending
        .iter()
        .flatten()
        .flat_map(|w| &w.occurrences)
        .map(|o| o.path.len() as u64)
        .sum();
    let occurrences: u64 = pending
        .iter()
        .flatten()
        .map(|w| w.occurrences.len() as u64)
        .sum();
    let unit = native
        .nurbs_carriers
        .iter()
        .map(|n| n.geometry.inversion_work())
        .max()
        .unwrap_or(0);
    let estimated = unit.saturating_mul(
        paths
            .saturating_mul(native.nurbs_carriers.len() as u64 + 2)
            .saturating_add(occurrences.saturating_mul(68))
            .saturating_add(
                (pending.len() as u64)
                    .saturating_mul(11)
                    .saturating_mul(native.nurbs_carriers.len() as u64),
            ),
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
        let local: Vec<_> = wires.iter().filter(|w| w.face == face).collect();
        if local.len() != 1 {
            continue;
        }
        let vs: BTreeSet<_> = local
            .iter()
            .flat_map(|w| &w.occurrences)
            .flat_map(|o| ends[o.edge])
            .collect();
        if vs.len() != 3 {
            continue;
        }
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
        for n in &native.nurbs_carriers {
            b.work(n.geometry.control_points.len() as u64 * 6 + vs.len() as u64 * 3)?;
            let bounds: [[f64; 2]; 3] = std::array::from_fn(|d| {
                n.geometry
                    .control_points
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
                .chain(handles.iter().map(|h| table.coords[*h as usize]))
            {
                b.work(n.geometry.inversion_work())?;
                if n.geometry.parameters_at_point(p, 1e-3).is_none() {
                    valid = false;
                    break;
                }
            }
            if valid {
                accepted.push(n)
            }
        }
        let Some(first) = accepted.first() else {
            continue;
        };
        if accepted.iter().any(|c| c.geometry != first.geometry) {
            continue;
        }
        b.add(
            first
                .geometry
                .control_points
                .len()
                .saturating_mul(2)
                .saturating_add(first.geometry.knots.iter().map(Vec::len).sum::<usize>()),
        )?;
        s.geometry = SurfaceGeometry::Nurbs {
            surface: first.geometry.clone(),
        };
        s.domains = s.geometry.domains();
        s.parameterization = "derived_unique_native_nurbs_triangular_trim_correspondence".into();
        s.evidence.extend([
            first.source.clone(),
            raw.packets[face].source.clone(),
            table.source.clone(),
        ]);
    }
    Ok(())
}
