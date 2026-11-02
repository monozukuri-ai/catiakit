//! Per-occurrence analytic evidence, with no claim of a complete trimmed body.
use super::*;

fn use_pcurve(
    surface: &SurfaceGeometry,
    curve: &CurveGeometry,
    range: [f64; 2],
    path: &[u32],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<PcurveGeometry> {
    let mut uv = Vec::new();
    for (i, h) in path.iter().enumerate() {
        let p = table.coords[*h as usize];
        let mut q = charts::inverse_metered(surface, p, b)?;
        if charts::singular(surface, p) {
            if i != 0 && i + 1 != path.len() {
                return Err(fail(
                    Code::TopologyDegenerateUnresolved,
                    "interior pole requires a chart split",
                ));
            }
            let endpoint = if i == 0 { range[0] } else { range[1] };
            let tangent = curve.tangent(endpoint).ok_or_else(|| {
                fail(
                    Code::TopologyDegenerateUnresolved,
                    "polar occurrence has no analytic tangent",
                )
            })?;
            let direction = scale(
                tangent,
                (range[1] - range[0]).signum() * if i == 0 { 1. } else { -1. },
            );
            let frame = match surface {
                SurfaceGeometry::Cone { frame, .. } | SurfaceGeometry::Sphere { frame, .. } => {
                    frame
                }
                _ => return Err(fail(Code::TopologyDegenerateUnresolved, "polar carrier")),
            };
            q[0] = dot(direction, frame.y_axis).atan2(dot(direction, frame.x_axis));
        }
        if let Some(previous) = uv.last() {
            let previous: &[f64; 2] = previous;
            for (d, value) in q.iter_mut().enumerate() {
                if charts::periodic(surface)[d] {
                    *value = previous[d] + delta_angle(*value - previous[d]);
                }
            }
        }
        uv.push(q);
    }
    b.work((path.len() as u64).saturating_mul(charts::curve_cost(curve)))?;
    let guides = charts::guides(curve, range, path, &uv, table)?;
    b.add(
        guides
            .len()
            .saturating_add(charts::carrier_records(surface, curve)),
    )?;
    let pc = PcurveGeometry::AnalyticProjection {
        surface: Box::new(surface.clone()),
        curve: Box::new(curve.clone()),
        parameter_range: range,
        phase_guides: guides,
    };
    for j in 0..=16 {
        b.work((path.len() as u64).saturating_add(charts::projection_cost(surface, curve)))?;
        let q = j as f64 / 16.;
        let uv = pc
            .evaluate(q)
            .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial pcurve evaluation"))?;
        let lifted = surface
            .evaluate(uv)
            .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial support evaluation"))?;
        let expected = curve
            .evaluate(interval_parameter(range, q))
            .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial edge evaluation"))?;
        if (j == 0 || j == 16)
            && distance(
                expected,
                table.coords[path[if j == 0 { 0 } else { path.len() - 1 }] as usize],
            ) > 1e-3
        {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "partial endpoint binding",
            ));
        }
        if distance(lifted, expected) > 1e-3 {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "partial analytic lift mismatch",
            ));
        }
    }
    Ok(pc)
}

pub(super) fn recover(
    surfaces: &[SurfaceRecord],
    curves: &[CurveRecord],
    wires: &[Wire],
    raw: &RawSource,
    binding: &EndpointBinding,
    b: &mut Budget<'_>,
) -> Result<AnalyticPartial> {
    let pid = surfaces[0].population_id;
    b.add(surfaces.len() + curves.len())?;
    let mut out = AnalyticPartial {
        population_id: pid,
        surfaces: surfaces.to_vec(),
        curves: curves.to_vec(),
        pcurves: Vec::new(),
        unresolved: Vec::new(),
    };
    for (wi, w) in wires.iter().enumerate() {
        for (oi, o) in w.occurrences.iter().enumerate() {
            b.work(1)?;
            let curve = &curves[o.edge];
            let surface = &surfaces[w.face];
            let result = (|| {
                let range = curve.parameter_interval.ok_or_else(|| {
                    fail(
                        Code::TopologyGeometryUnsupported,
                        "unresolved analytic edge",
                    )
                })?;
                let range = if o.forward {
                    range
                } else {
                    [range[1], range[0]]
                };
                if matches!(surface.geometry, SurfaceGeometry::Unknown { .. }) {
                    return Err(fail(
                        Code::TopologyGeometryUnsupported,
                        "unresolved analytic surface",
                    ));
                }
                Ok((
                    range,
                    use_pcurve(
                        &surface.geometry,
                        &curve.geometry,
                        range,
                        &o.path,
                        &binding.0,
                        b,
                    )?,
                ))
            })();
            b.add(1)?;
            match result {
                Ok((range, pcurve)) => out.pcurves.push(AnalyticUse {
                    face_ordinal: w.face as u32,
                    boundary_loop: wi as u32,
                    occurrence: oi as u32,
                    edge_ordinal: o.edge as u32,
                    forward: o.forward,
                    curve: curve.id.clone(),
                    surface: surface.id.clone(),
                    parameter_range: range,
                    pcurve,
                    source: raw.packets[w.face].source.clone(),
                }),
                Err(e) => {
                    if e.code.is_resource_limit() {
                        return Err(e);
                    }
                    out.unresolved.push(AnalyticUseIssue {
                        face_ordinal: w.face as u32,
                        edge_ordinal: o.edge as u32,
                        code: e.code,
                        detail: e.detail,
                        source: raw.packets[w.face].source.clone(),
                    });
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticValidationSummary {
    pub known_surfaces: usize,
    pub known_curves: usize,
    pub pcurves: usize,
    pub unresolved_uses: usize,
    pub samples: usize,
    pub max_lift_residual_mm: f64,
}

pub(super) fn validate(
    g: &GeometryModel,
    boundary: &BoundaryPopulation,
    p: &AnalyticPartial,
    b: &mut Budget<'_>,
) -> Result<AnalyticValidationSummary> {
    if p.population_id != boundary.population_id
        || p.surfaces.len()
            != boundary
                .loops
                .iter()
                .map(|l| l.face_ordinal)
                .collect::<BTreeSet<_>>()
                .len()
        || p.curves.len() != boundary.edges.len()
    {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "partial analytic population",
        ));
    }
    b.add(
        p.surfaces.len()
            + p.curves.len()
            + p.pcurves.len()
            + p.unresolved.len()
            + g.points.len()
            + g.surfaces.len()
            + g.curves.len(),
    )?;
    let surfaces = unique(&p.surfaces, |s| &s.id)?;
    let curves = unique(&p.curves, |c| &c.id)?;
    let points = unique(&g.points, |p| &p.id)?;
    let original_curves = unique(&g.curves, |c| &c.id)?;
    let original_surfaces = unique(&g.surfaces, |s| &s.id)?;
    for s in &p.surfaces {
        let original = *member(&original_surfaces, &s.id)?;
        if s.population_id != p.population_id
            || s.record_ordinal != original.record_ordinal
            || original.population_id != p.population_id
            || s.source != original.source
            || s.native_carrier_tag != original.native_carrier_tag
            || s.raw_parameters_hex != original.raw_parameters_hex
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "partial surface identity",
            ));
        }
    }
    for c in &p.curves {
        let original = *member(&original_curves, &c.id)?;
        if c.population_id != p.population_id
            || c.record_ordinal != original.record_ordinal
            || original.population_id != p.population_id
            || c.source != original.source
            || c.native_allocation_tag != original.native_allocation_tag
            || c.raw_parameters_hex != original.raw_parameters_hex
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "partial curve identity",
            ));
        }
    }
    let mut samples = 0;
    let mut maximum: f64 = 0.;
    let mut occurrences = BTreeSet::new();
    for u in &p.pcurves {
        b.work(1)?;
        if !occurrences.insert((u.boundary_loop, u.occurrence)) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "duplicate partial occurrence",
            ));
        }
        let wire = boundary
            .loops
            .get(u.boundary_loop as usize)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "partial loop reference"))?;
        let occ = wire.occurrences.get(u.occurrence as usize).ok_or_else(|| {
            fail(
                Code::TopologyValidationInvalid,
                "partial occurrence reference",
            )
        })?;
        let edge = boundary
            .edges
            .get(u.edge_ordinal as usize)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "partial edge reference"))?;
        let surface = *member(&surfaces, &u.surface)?;
        let curve = *member(&curves, &u.curve)?;
        if wire.face_ordinal != u.face_ordinal
            || occ.edge_ordinal != u.edge_ordinal
            || edge.curve != curve.id
            || surface.record_ordinal != u.face_ordinal
            || u.source != wire.source
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "partial native incidence",
            ));
        }
        let native_range = curve
            .parameter_interval
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "partial native interval"))?;
        if occ.forward.is_some_and(|forward| forward != u.forward) {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "partial boundary direction",
            ));
        }
        let range = if u.forward {
            native_range
        } else {
            [native_range[1], native_range[0]]
        };
        if u.parameter_range != range {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "partial interval direction",
            ));
        }
        match &u.pcurve {
            PcurveGeometry::AnalyticProjection {
                surface: s,
                curve: c,
                parameter_range,
                phase_guides,
            } => {
                if **s != surface.geometry || **c != curve.geometry || *parameter_range != range {
                    return Err(fail(
                        Code::TopologyValidationInvalid,
                        "partial embedded carrier",
                    ));
                }
                b.add(
                    phase_guides
                        .len()
                        .saturating_add(charts::carrier_records(s, c)),
                )?;
                b.work(phase_guides.len() as u64 * 17)?;
            }
            _ => {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "unsupported partial pcurve representation",
                ));
            }
        }
        for j in 0..=16 {
            b.work(charts::projection_cost(&surface.geometry, &curve.geometry))?;
            let q = j as f64 / 16.;
            let uv = u
                .pcurve
                .evaluate(q)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial UV evaluation"))?;
            let a = surface
                .geometry
                .evaluate(uv)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial lift evaluation"))?;
            let c = curve
                .geometry
                .evaluate(interval_parameter(range, q))
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "partial curve evaluation"))?;
            let residual = distance(a, c);
            if residual > 1e-3 {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "partial embedding residual",
                ));
            }
            maximum = maximum.max(residual);
            samples += 1;
            if j == 0 || j == 16 {
                let vertex = if j == 0 {
                    usize::from(!u.forward)
                } else {
                    usize::from(u.forward)
                };
                let point = *member(
                    &points,
                    &boundary.vertex_points[edge.vertices[vertex] as usize],
                )?;
                if distance(c, point.position) > 1e-3 {
                    return Err(fail(
                        Code::TopologyPcurveInvalid,
                        "partial endpoint binding",
                    ));
                }
            }
        }
    }
    // An unresolved use must account for an actual remaining occurrence. A
    // census alone lets duplicate issues hide an omitted face/edge use. Keep
    // multiplicity: a seam may use the same edge twice on the same face.
    let mut remaining = BTreeMap::<(u32, u32), Vec<&StreamSpan>>::new();
    for (wi, wire) in boundary.loops.iter().enumerate() {
        for (oi, occ) in wire.occurrences.iter().enumerate() {
            b.work(1)?;
            if !occurrences.contains(&(wi as u32, oi as u32)) {
                b.add(1)?;
                remaining
                    .entry((wire.face_ordinal, occ.edge_ordinal))
                    .or_default()
                    .push(&wire.source);
            }
        }
    }
    b.add(remaining.len())?;
    for issue in &p.unresolved {
        b.work(1)?;
        let sources = remaining
            .get_mut(&(issue.face_ordinal, issue.edge_ordinal))
            .ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "partial unresolved occurrence",
                )
            })?;
        b.work(sources.len() as u64)?;
        let position = sources.iter().position(|s| *s == &issue.source);
        if position.is_none() || issue.code.is_resource_limit() {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "partial unresolved identity",
            ));
        }
        sources.swap_remove(position.unwrap());
    }
    let total: usize = boundary.loops.iter().map(|l| l.occurrences.len()).sum();
    if p.pcurves.len() + p.unresolved.len() != total {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "partial occurrence census",
        ));
    }
    if remaining.values().any(|sources| !sources.is_empty()) {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "partial unresolved coverage",
        ));
    }
    Ok(AnalyticValidationSummary {
        known_surfaces: p
            .surfaces
            .iter()
            .filter(|s| !matches!(s.geometry, SurfaceGeometry::Unknown { .. }))
            .count(),
        known_curves: p
            .curves
            .iter()
            .filter(|c| !matches!(c.geometry, CurveGeometry::Unknown { .. }))
            .count(),
        pcurves: p.pcurves.len(),
        unresolved_uses: p.unresolved.len(),
        samples,
        max_lift_residual_mm: maximum,
    })
}
