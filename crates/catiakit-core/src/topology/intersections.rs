//! Analytic constructions from resolved serialized incident carriers.
//! Tessellation tests candidate membership; it supplies no fitted carrier.
use super::*;

fn cylinder_plane(cylinder: &SurfaceGeometry, plane: &SurfaceGeometry) -> Option<CurveGeometry> {
    let SurfaceGeometry::Cylinder { frame: c, radius } = cylinder else {
        return None;
    };
    let SurfaceGeometry::Plane { frame: p } = plane else {
        return None;
    };
    let cosine = dot(c.z_axis, p.z_axis);
    if cosine.abs() < 1e-8 {
        return None;
    }
    let origin = add(
        c.origin,
        scale(c.z_axis, dot(sub(p.origin, c.origin), p.z_axis) / cosine),
    );
    if (cosine.abs() - 1.).abs() < 1e-12 {
        return Some(CurveGeometry::Circle {
            frame: Frame3::from_axis(origin, p.z_axis)?,
            radius: *radius,
        });
    }
    let x_axis = unit(sub(c.z_axis, scale(p.z_axis, cosine)))?;
    let y_axis = cross(p.z_axis, x_axis);
    Some(CurveGeometry::Ellipse {
        frame: Frame3 {
            origin,
            x_axis,
            y_axis,
            z_axis: p.z_axis,
        },
        major_radius: radius / cosine.abs(),
        minor_radius: *radius,
    })
}

fn candidates(a: &SurfaceGeometry, c: &SurfaceGeometry) -> Vec<CurveGeometry> {
    let mut out = Vec::new();
    if let Some(curve) = cylinder_plane(a, c).or_else(|| cylinder_plane(c, a)) {
        out.push(curve);
    }
    for (sphere, plane) in [(a, c), (c, a)] {
        if let (SurfaceGeometry::Sphere { frame: s, radius }, SurfaceGeometry::Plane { frame: p }) =
            (sphere, plane)
        {
            let h = dot(sub(s.origin, p.origin), p.z_axis);
            let r2 = radius * radius - h * h;
            if r2 > 0.
                && let Some(frame) = Frame3::from_axis(sub(s.origin, scale(p.z_axis, h)), p.z_axis)
            {
                out.push(CurveGeometry::Circle {
                    frame,
                    radius: r2.sqrt(),
                });
            }
        }
    }
    if let (
        SurfaceGeometry::Cylinder {
            frame: a,
            radius: ra,
        },
        SurfaceGeometry::Cylinder {
            frame: c,
            radius: rc,
        },
    ) = (a, c)
    {
        let cosine = dot(a.z_axis, c.z_axis);
        let denominator = 1. - cosine * cosine;
        if denominator > 1e-12 {
            if ra.to_bits() == rc.to_bits() {
                let d = sub(c.origin, a.origin);
                let t = (dot(d, a.z_axis) - cosine * dot(d, c.z_axis)) / denominator;
                let u = (cosine * dot(d, a.z_axis) - dot(d, c.z_axis)) / denominator;
                let center = add(a.origin, scale(a.z_axis, t));
                if distance(center, add(c.origin, scale(c.z_axis, u))) < 1e-8 {
                    for n in [add(a.z_axis, c.z_axis), sub(a.z_axis, c.z_axis)] {
                        if let Some(frame) = Frame3::from_axis(center, n) {
                            let plane = SurfaceGeometry::Plane { frame };
                            let cylinder = SurfaceGeometry::Cylinder {
                                frame: a.clone(),
                                radius: *ra,
                            };
                            if let Some(curve) = cylinder_plane(&cylinder, &plane) {
                                out.push(curve);
                            }
                        }
                    }
                }
            }
            // Generic unequal-radius or skew axes retain the exact intersection
            // construction. Each branch is qualified against every occurrence.
            if out.is_empty() {
                for branch in [-1, 1] {
                    out.push(CurveGeometry::CylinderIntersection {
                        primary: a.clone(),
                        primary_radius: *ra,
                        secondary: c.clone(),
                        secondary_radius: *rc,
                        branch,
                    });
                }
            }
        }
    }
    out
}

pub(super) fn resolve(
    curve: &mut CurveRecord,
    owners: &[usize],
    surfaces: &[SurfaceRecord],
    paths: &[&[u32]],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<()> {
    if !matches!(curve.geometry, CurveGeometry::Unknown { .. })
        || !curve.raw_parameters_hex.starts_with("000000")
    {
        return Ok(());
    }
    let owners: BTreeSet<_> = owners.iter().copied().collect();
    if owners.len() != 2 {
        return Ok(());
    }
    let faces: Vec<_> = owners.into_iter().collect();
    let mut accepted = Vec::new();
    for (a, c) in [(faces[0], faces[1]), (faces[1], faces[0])] {
        for candidate in candidates(&surfaces[a].geometry, &surfaces[c].geometry) {
            let mut valid = true;
            for path in paths {
                for h in *path {
                    b.work(1)?;
                    let p = table.coords[*h as usize];
                    let Some(t) = candidate.angular_parameter(p) else {
                        valid = false;
                        break;
                    };
                    if candidate.evaluate(t).is_err()
                        || distance(candidate.evaluate(t).unwrap(), p) > 1e-3
                    {
                        valid = false;
                        break;
                    }
                }
                if !valid {
                    break;
                }
            }
            if valid {
                // A valid locus can still turn back in this angular chart. The
                // alternate cylinder chart may represent the same trim as one root.
                for path in paths {
                    if path.len() > 2
                        && let Err(error) = circle_path(&candidate, path, table, b)
                    {
                        if error.code.is_resource_limit() {
                            return Err(error);
                        }
                        valid = false;
                        break;
                    }
                }
            }
            if valid {
                accepted.push(candidate);
            }
        }
        if !accepted.is_empty() {
            break;
        }
    }
    if accepted.len() == 1 {
        curve.geometry = accepted.remove(0);
        curve.domain = curve.geometry.domain();
        curve.parameterization = "analytic_intersection_of_serialized_incident_carriers".into();
        for f in faces {
            curve.evidence.push(surfaces[f].source.clone());
        }
    }
    Ok(())
}

pub(super) fn rechart_spheres(
    surfaces: &mut [SurfaceRecord],
    curves: &[CurveRecord],
    wires: &[Wire],
    raw: &RawSource,
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<()> {
    for (face, s) in surfaces.iter_mut().enumerate() {
        let SurfaceGeometry::Sphere { frame, radius } = &s.geometry else {
            continue;
        };
        let local: Vec<_> = wires.iter().filter(|w| w.face == face).collect();
        if local
            .iter()
            .flat_map(|w| &w.occurrences)
            .any(|o| matches!(curves[o.edge].geometry, CurveGeometry::Point { .. }))
        {
            continue;
        }
        // A chart gauge can move a sphere's poles away from an observed strict
        // hemisphere. Its center/radius and the 3D geometry remain unchanged.
        let mut mean = [0.; 3];
        for h in &raw.packets[face].handles {
            b.work(1)?;
            mean = add(mean, sub(table.coords[*h as usize], frame.origin));
        }
        let mut candidates: Vec<_> = unit(mean).into_iter().collect();
        // An antipodal boundary makes its sampled mean sensitive to native
        // tessellation density. Serialized circle normals provide additional
        // chart gauges. Every candidate still has to contain the entire face
        // packet in its closed hemisphere.
        for o in local.iter().flat_map(|w| &w.occurrences) {
            if let CurveGeometry::Circle { frame, .. } = &curves[o.edge].geometry {
                candidates.extend([frame.z_axis, scale(frame.z_axis, -1.)]);
            }
        }
        b.work((candidates.len() as u64).saturating_mul(raw.packets[face].handles.len() as u64))?;
        let pole = candidates.into_iter().find(|axis| {
            raw.packets[face]
                .handles
                .iter()
                .all(|h| dot(sub(table.coords[*h as usize], frame.origin), *axis) >= -radius * 1e-6)
        });
        let Some(new_frame) = pole.and_then(|axis| Frame3::from_axis(frame.origin, axis)) else {
            continue;
        };
        s.geometry = SurfaceGeometry::SphereStereographic {
            frame: new_frame,
            radius: *radius,
        };
        s.domains = s.geometry.domains();
        s.parameterization =
            "derived_stereographic_chart_with_excluded_pole_outside_trim_hemisphere".into();
        s.evidence.push(raw.packets[face].source.clone());
    }
    Ok(())
}
