//! A vertex on the same circle or ellipse can exclude one complementary arc.
//! This uses serialized physical incidence and existing carriers, not a fit.
use super::*;
fn same_conic(a: &CurveGeometry, z: &CurveGeometry) -> bool {
    let (af, zf, radii) = match (a, z) {
        (
            CurveGeometry::Circle {
                frame: a,
                radius: r,
            },
            CurveGeometry::Circle {
                frame: z,
                radius: s,
            },
        ) => (a, z, r.to_bits() == s.to_bits()),
        (
            CurveGeometry::Ellipse {
                frame: a,
                major_radius: ar,
                minor_radius: br,
            },
            CurveGeometry::Ellipse {
                frame: z,
                major_radius: az,
                minor_radius: bz,
            },
        ) => (
            a,
            z,
            ar.to_bits() == az.to_bits()
                && br.to_bits() == bz.to_bits()
                && distance(cross(a.x_axis, z.x_axis), [0.; 3]) < 1e-10,
        ),
        _ => return false,
    };
    radii && af.origin == zf.origin && distance(cross(af.z_axis, zf.z_axis), [0.; 3]) < 1e-10
}
fn face_angle(curve: &CurveGeometry, surface: &SurfaceGeometry, witness: Vec3) -> Option<f64> {
    let CurveGeometry::Circle { frame, radius } = curve else {
        return None;
    };
    let normal = charts::normal_on(surface, frame.origin, *radius)?;
    if dot(cross(normal, frame.z_axis), cross(normal, frame.z_axis)) > 1e-10 {
        return None;
    }
    match surface {
        SurfaceGeometry::Cylinder { .. } => curve.angular_parameter(witness),
        SurfaceGeometry::Torus {
            frame: torus,
            major_radius,
            minor_radius,
        } if major_radius > minor_radius => {
            let aligned = dot(frame.z_axis, torus.z_axis).abs();
            if aligned > 1. - 1e-10 {
                return curve.angular_parameter(witness);
            }
            if aligned > 1e-8 {
                return None;
            }
            let delta = sub(witness, torus.origin);
            let z = dot(delta, torus.z_axis);
            let radial = sub(delta, scale(torus.z_axis, z));
            let rho = dot(radial, radial).sqrt();
            let center = sub(frame.origin, torus.origin);
            let radial_axis = unit(sub(center, scale(torus.z_axis, dot(center, torus.z_axis))))?;
            let p = add(
                frame.origin,
                add(
                    scale(radial_axis, rho - major_radius),
                    scale(torus.z_axis, z),
                ),
            );
            curve.angular_parameter(p)
        }
        _ => None,
    }
}
pub(super) struct ArcWitness {
    pub range: [f64; 2],
    pub evidence: Vec<StreamSpan>,
}
pub(super) fn recover(
    points: &[PointRecord],
    curves: &[CurveRecord],
    surfaces: &[SurfaceRecord],
    wires: &[Wire],
    ends: &[[usize; 2]],
    witnesses: &[Option<(Vec3, StreamSpan)>],
    b: &mut Budget<'_>,
) -> Result<BTreeMap<usize, ArcWitness>> {
    let mut out = BTreeMap::new();
    let mut owners = vec![BTreeSet::new(); curves.len()];
    for w in wires {
        for o in &w.occurrences {
            owners[o.edge].insert(w.face);
        }
    }
    for (e, c) in curves.iter().enumerate() {
        if !matches!(
            c.geometry,
            CurveGeometry::Circle { .. } | CurveGeometry::Ellipse { .. }
        ) {
            continue;
        }
        let [a, z] = ends[e];
        if a == z {
            continue;
        }
        let t0 = c.geometry.angular_parameter(points[a].position).unwrap();
        let t1 = c.geometry.angular_parameter(points[z].position).unwrap();
        let positive = (t1 - t0).rem_euclid(std::f64::consts::TAU);
        if positive < 1e-8 || std::f64::consts::TAU - positive < 1e-8 {
            continue;
        }
        let mut valid = [true, true];
        let mut evidence = Vec::new();
        for (j, other) in curves.iter().enumerate() {
            b.work(1)?;
            if e == j
                || owners[e].is_disjoint(&owners[j])
                || other.raw_parameters_hex != c.raw_parameters_hex
            {
                continue;
            }
            if !same_conic(&c.geometry, &other.geometry) {
                continue;
            }
            for v in ends[j] {
                if v == a || v == z {
                    continue;
                }
                let p = points[v].position;
                let t = c.geometry.angular_parameter(p).unwrap();
                if distance(
                    c.geometry.evaluate(t).map_err(|_| {
                        fail(Code::TopologyGeometryUnsupported, "co-circular witness")
                    })?,
                    p,
                ) > 1e-3
                {
                    continue;
                }
                let d = (t - t0).rem_euclid(std::f64::consts::TAU);
                if d > 1e-7 && d < positive - 1e-7 {
                    valid[0] = false;
                    evidence.extend([points[v].source.clone(), other.source.clone()]);
                }
                if d > positive + 1e-7 && d < std::f64::consts::TAU - 1e-7 {
                    valid[1] = false;
                    evidence.extend([points[v].source.clone(), other.source.clone()]);
                }
            }
        }
        if valid == [true, true] {
            for face in &owners[e] {
                let Some((p, source)) = witnesses.get(*face).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(t) = face_angle(&c.geometry, &surfaces[*face].geometry, *p) else {
                    continue;
                };
                let d = (t - t0).rem_euclid(std::f64::consts::TAU);
                if d > 1e-7 && d < positive - 1e-7 {
                    valid[1] = false;
                    evidence.push(source.clone());
                } else if d > positive + 1e-7 && d < std::f64::consts::TAU - 1e-7 {
                    valid[0] = false;
                    evidence.push(source.clone());
                }
            }
        }
        let sweep = match valid {
            [true, false] => positive,
            [false, true] => positive - std::f64::consts::TAU,
            _ => continue,
        };
        b.add(evidence.len())?;
        out.insert(
            e,
            ArcWitness {
                range: [t0, t0 + sweep],
                evidence,
            },
        );
    }
    Ok(out)
}
