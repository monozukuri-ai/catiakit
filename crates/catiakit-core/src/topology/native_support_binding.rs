//! An allocation-linked support definition is independent of trim admission.
//! Only partial analytic evidence is refined; this never exports a body.
use super::*;

const GAUGE: &str = "source_native_allocation_nurbs_support";

fn support<'a>(
    surface: &SurfaceRecord,
    boundary: &BoundaryPopulation,
    binding: &NativeBoundaryUv,
    native: &'a NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<Option<(&'a NativeNurbsCarrier, StreamSpan)>> {
    b.work((binding.uses.len() + boundary.loops.len()) as u64)?;
    let uses: Vec<_> = binding
        .uses
        .iter()
        .filter(|u| u.face_ordinal == surface.record_ordinal)
        .collect();
    b.add(uses.len())?;
    let Some(&first) = uses.first() else {
        return Ok(None);
    };
    let NativeBoundSupport::Nurbs { carrier } = &first.support else {
        return Ok(None);
    };
    if uses.iter().any(|u| {
        u.face_tag != surface.native_carrier_tag
            || u.support_tag != first.support_tag
            || u.support != first.support
            || u.face_source != first.face_source
    }) {
        return Ok(None);
    }
    let mut expected = BTreeMap::<u32, usize>::new();
    for wire in boundary
        .loops
        .iter()
        .filter(|w| w.face_ordinal == surface.record_ordinal)
    {
        b.work(wire.occurrences.len() as u64)?;
        for o in &wire.occurrences {
            *expected.entry(o.edge_ordinal).or_default() += 1;
        }
    }
    b.add(expected.len())?;
    for u in uses {
        let Some(n) = expected.get_mut(&u.edge_ordinal) else {
            return Ok(None);
        };
        if *n == 0 {
            return Ok(None);
        }
        *n -= 1;
    }
    if expected.values().any(|&n| n != 0) {
        return Ok(None);
    }
    b.work(native.nurbs_carriers.len() as u64)?;
    let mut carriers = native
        .nurbs_carriers
        .iter()
        .filter(|n| &n.id == carrier && n.run == binding.run);
    let Some(n) = carriers.next() else {
        return Ok(None);
    };
    if carriers.next().is_some() {
        return Ok(None);
    }
    Ok(Some((n, first.face_source.clone())))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn refine(
    data: &[u8],
    geometry: &GeometryModel,
    surfaces: &mut [SurfaceRecord],
    boundary: &BoundaryPopulation,
    native: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
) -> Result<()> {
    let Some(native) = native else { return Ok(()) };
    if !surfaces
        .iter()
        .any(|s| matches!(s.geometry, SurfaceGeometry::Unknown { .. }))
    {
        return Ok(());
    }
    let Some(binding) =
        native_uv::allocations::recover_boundary(data, geometry, boundary, native, b)?
    else {
        return Ok(());
    };
    for s in surfaces.iter_mut().filter(|s| {
        matches!(
            s.geometry,
            SurfaceGeometry::Unknown {
                reason: Code::GeometryCarrierUnsupported
            }
        )
    }) {
        let Some((n, face_source)) = support(s, boundary, &binding, native, b)? else {
            continue;
        };
        b.add(
            n.geometry.control_points.len() * 2
                + n.geometry.knots.iter().map(Vec::len).sum::<usize>()
                + 3,
        )?;
        s.geometry = SurfaceGeometry::Nurbs {
            surface: n.geometry.clone(),
        };
        s.domains = s.geometry.domains();
        s.parameterization = GAUGE.into();
        s.evidence.extend([
            n.source.clone(),
            face_source,
            binding.factory_source.clone(),
        ]);
    }
    Ok(())
}

pub(super) fn validate(t: &TopologyModel, b: &mut Budget<'_>) -> Result<()> {
    for partial in &t.analytic_partials {
        for s in partial
            .surfaces
            .iter()
            .filter(|s| s.parameterization == GAUGE)
        {
            let invalid = || {
                fail(
                    Code::TopologyValidationInvalid,
                    "allocation-linked partial support",
                )
            };
            let native = t.native_uv.as_ref().ok_or_else(invalid)?;
            let binding = t
                .native_boundary_uv
                .iter()
                .find(|a| a.population_id == partial.population_id)
                .ok_or_else(invalid)?;
            let boundary = t
                .boundaries
                .iter()
                .find(|a| a.population_id == partial.population_id)
                .ok_or_else(invalid)?;
            let (n, face_source) = support(s, boundary, binding, native, b)?.ok_or_else(invalid)?;
            b.work(n.geometry.control_points.len() as u64 * 4)?;
            if !matches!(&s.geometry, SurfaceGeometry::Nurbs { surface } if *surface == n.geometry)
                || s.domains != s.geometry.domains()
                || ![&n.source, &face_source, &binding.factory_source]
                    .iter()
                    .all(|p| s.evidence.contains(p))
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
