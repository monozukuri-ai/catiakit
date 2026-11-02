//! Exact saved UV jets lifted through a uniquely qualified native analytic
//! carrier. Native endpoints and every retained trim site qualify the interval.
use super::*;
pub(super) fn same_locus(a: &SurfaceGeometry, z: &SurfaceGeometry) -> bool {
    match (a, z) {
        (
            SurfaceGeometry::Cone {
                frame: a,
                half_angle: r,
            },
            SurfaceGeometry::Cone {
                frame: z,
                half_angle: s,
            },
        ) => {
            distance(a.origin, z.origin) < 1e-3
                && dot(a.z_axis, z.z_axis) > 1. - 1e-6
                && (r - s).abs() < 1e-6
        }
        (
            SurfaceGeometry::Sphere {
                frame: a,
                radius: r,
            },
            SurfaceGeometry::Sphere {
                frame: z,
                radius: s,
            },
        ) => distance(a.origin, z.origin) < 1e-3 && (r - s).abs() < 1e-3,
        (
            SurfaceGeometry::Cylinder {
                frame: a,
                radius: r,
            },
            SurfaceGeometry::Cylinder {
                frame: z,
                radius: s,
            },
        ) => {
            distance(cross(a.z_axis, z.z_axis), [0.; 3]) < 1e-6
                && distance(cross(sub(a.origin, z.origin), z.z_axis), [0.; 3]) < 1e-3
                && (r - s).abs() < 1e-3
        }
        (
            SurfaceGeometry::Torus {
                frame: a,
                major_radius: r,
                minor_radius: u,
            },
            SurfaceGeometry::Torus {
                frame: z,
                major_radius: s,
                minor_radius: v,
            },
        ) => {
            distance(a.origin, z.origin) < 1e-3
                && distance(cross(a.z_axis, z.z_axis), [0.; 3]) < 1e-6
                && (r - s).abs() < 1e-3
                && (u - v).abs() < 1e-3
        }
        _ => false,
    }
}
fn lifted(carrier: &NativeCarrier, pc: &NativePcurve) -> CurveGeometry {
    CurveGeometry::SurfaceJet {
        surface: Box::new(carrier.geometry.clone()),
        jet: pc.jet.clone(),
        uv_scale: carrier.uv_scale,
        uv_offset: carrier.uv_offset,
    }
}
fn member_parameter(
    curve: &CurveGeometry,
    p: Vec3,
    cache: &mut BTreeMap<[u32; 3], Option<f64>>,
    b: &mut Budget<'_>,
) -> Result<Option<f64>> {
    b.work(1)?;
    if let Some(t) = cache.get(&key(p)) {
        return Ok(*t);
    }
    b.work(charts::curve_cost(curve))?;
    let t = curve.bounded_parameter(p, 1e-3);
    b.add(1)?;
    cache.insert(key(p), t);
    Ok(t)
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
    if native.pcurves.is_empty() {
        return Ok(());
    }
    let mut sparse = BTreeSet::new();
    for o in wires.iter().flat_map(|w| &w.occurrences) {
        let mut sites = BTreeSet::new();
        for h in &o.path {
            b.work(1)?;
            sites.insert(key(binding.0.coords[*h as usize]));
            if sites.len() >= 3 {
                break;
            }
        }
        if sites.len() < 3 {
            sparse.insert(o.edge);
        }
    }
    b.add(sparse.len())?;
    let eligible = |e: usize, c: &CurveRecord| {
        c.raw_parameters_hex.starts_with("000000")
            && (matches!(c.geometry, CurveGeometry::Unknown { .. })
                || sparse.contains(&e)
                    && matches!(
                        c.geometry,
                        CurveGeometry::Circle { .. }
                            | CurveGeometry::Ellipse { .. }
                            | CurveGeometry::CylinderIntersection { .. }
                    ))
    };
    if !curves.iter().enumerate().any(|(e, c)| eligible(e, c)) {
        return Ok(());
    }
    b.add(native.pcurves.len().saturating_mul(2))?;
    let mut uv_endpoints = Vec::with_capacity(native.pcurves.len());
    for pc in &native.pcurves {
        b.work(pc.jet.knots.len() as u64 * 32)?;
        uv_endpoints.push(
            match (
                pc.jet.evaluate(pc.jet.parameter_range[0]),
                pc.jet.evaluate(pc.jet.parameter_range[1]),
            ) {
                (Ok(a), Ok(z)) => Some([a, z]),
                _ => None,
            },
        );
    }
    let mut endpoint_cache: BTreeMap<usize, Vec<Option<[Vec3; 2]>>> = BTreeMap::new();
    for (e, c) in curves.iter_mut().enumerate() {
        if !eligible(e, c) {
            continue;
        }
        let owners: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
            .map(|w| w.face)
            .collect();
        let endpoints = binding.1[e].map(|v| points[v].position);
        let mut candidates = Vec::new();
        for (ci, carrier) in native.carriers.iter().enumerate() {
            b.work(owners.len() as u64)?;
            if !owners
                .iter()
                .any(|f| same_locus(&carrier.geometry, &surfaces[*f].geometry))
            {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = endpoint_cache.entry(ci) {
                // The same source pair can be considered for many physical
                // edges. Evaluate its endpoints once, without caching a bound
                // curve or changing any ambiguity/membership gate.
                b.add(native.pcurves.len().saturating_mul(2).saturating_add(1))?;
                let mut ends = Vec::with_capacity(native.pcurves.len());
                for uv in &uv_endpoints {
                    b.work(4)?;
                    let Some(uv) = uv else {
                        ends.push(None);
                        continue;
                    };
                    let lift = |p: [f64; 2]| {
                        carrier.geometry.evaluate([
                            p[0] * carrier.uv_scale[0] + carrier.uv_offset[0],
                            p[1] * carrier.uv_scale[1] + carrier.uv_offset[1],
                        ])
                    };
                    ends.push(match (lift(uv[0]), lift(uv[1])) {
                        (Ok(a), Ok(z)) => Some([a, z]),
                        _ => None,
                    });
                }
                entry.insert(ends);
            }
            for (pi, pc) in native.pcurves.iter().enumerate() {
                b.work(16)?;
                let range = pc.jet.parameter_range;
                let Some([a, z]) = endpoint_cache[&ci][pi] else {
                    continue;
                };
                let forward = distance(a, endpoints[0]) < 1e-3 && distance(z, endpoints[1]) < 1e-3;
                let reverse = distance(a, endpoints[1]) < 1e-3 && distance(z, endpoints[0]) < 1e-3;
                if forward == reverse {
                    continue;
                }
                let curve = lifted(carrier, pc);
                let range = if forward { range } else { [range[1], range[0]] };
                let mut parameters = BTreeMap::from([
                    (key(endpoints[0]), Some(range[0])),
                    (key(endpoints[1]), Some(range[1])),
                ]);
                b.add(2)?;
                let mut valid = true;
                for i in 0..=16 {
                    b.work(pc.jet.knots.len() as u64 * 16)?;
                    let t = if i == 16 {
                        range[1]
                    } else {
                        range[0] + (range[1] - range[0]) * i as f64 / 16.
                    };
                    let Ok(p) = curve.evaluate(t) else {
                        valid = false;
                        break;
                    };
                    if !matches!(c.geometry, CurveGeometry::Unknown { .. }) {
                        b.work(1)?;
                        let Some(angle) = c.geometry.angular_parameter(p) else {
                            valid = false;
                            break;
                        };
                        if c.geometry
                            .evaluate(angle)
                            .map_or(true, |q| distance(q, p) > 1e-3)
                        {
                            valid = false;
                            break;
                        }
                    }
                    for f in &owners {
                        if matches!(surfaces[*f].geometry, SurfaceGeometry::Unknown { .. }) {
                            continue;
                        }
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
                if !valid {
                    continue;
                }
                // Reject a mismatching history curve at spread interior sites
                // before walking a densely sampled common endpoint region.
                for o in wires
                    .iter()
                    .flat_map(|w| &w.occurrences)
                    .filter(|o| o.edge == e)
                {
                    if o.path.len() < 2 {
                        valid = false;
                        break;
                    }
                    for j in 1..8 {
                        let h = o.path[j * (o.path.len() - 1) / 8];
                        if member_parameter(
                            &curve,
                            binding.0.coords[h as usize],
                            &mut parameters,
                            b,
                        )?
                        .is_none()
                        {
                            valid = false;
                            break;
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
                    .filter(|o| o.edge == e)
                {
                    let mut old = None;
                    for h in &o.path {
                        let Some(t) = member_parameter(
                            &curve,
                            binding.0.coords[*h as usize],
                            &mut parameters,
                            b,
                        )?
                        else {
                            valid = false;
                            break;
                        };
                        let mut q = (t - range[0]) / (range[1] - range[0]);
                        if !o.forward {
                            q = 1. - q
                        }
                        if !(-1e-5..=1. + 1e-5).contains(&q) || old.is_some_and(|v| q < v - 1e-5) {
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
                    candidates.push((
                        curve,
                        range,
                        vec![carrier.source.clone(), pc.source.clone()],
                    ))
                }
            }
        }
        let Some(first) = candidates.first() else {
            continue;
        };
        let work = candidates
            .iter()
            .skip(1)
            .map(|(curve, _, _)| {
                charts::curve_cost(curve)
                    .saturating_add(charts::curve_cost(&first.0))
                    .saturating_mul(17)
            })
            .fold(0u64, u64::saturating_add);
        b.work(work)?;
        let equivalent = candidates.iter().skip(1).all(|(curve, range, _)| {
            (0..=16).all(|i| {
                let q = i as f64 / 16.;
                let a = first.0.evaluate(first.1[0] + q * (first.1[1] - first.1[0]));
                let z = curve.evaluate(range[0] + q * (range[1] - range[0]));
                matches!((a,z),(Ok(a),Ok(z)) if distance(a,z)<1e-5)
            })
        });
        if !equivalent {
            continue;
        }
        let (curve, range, evidence) = candidates.remove(0);
        b.add(charts::carrier_records(
            &SurfaceGeometry::Unknown {
                reason: Code::GeometryCarrierUnsupported,
            },
            &curve,
        ))?;
        c.geometry = curve;
        c.domain = c.geometry.domain();
        c.parameter_interval = Some(range);
        c.parameterization = "derived_native_surface_jet_endpoint_and_complete_path".into();
        c.evidence.extend(evidence);
        for (_, _, evidence) in candidates {
            c.evidence.extend(evidence)
        }
    }
    Ok(())
}
