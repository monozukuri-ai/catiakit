//! Derived analytic charts. No tessellation is used as a carrier definition.
use super::*;
pub(super) fn inverse_cost(surface: &SurfaceGeometry) -> u64 {
    if let SurfaceGeometry::RollingBall { surface } = surface {
        return surface.inversion_work();
    }
    if let SurfaceGeometry::Nurbs { surface } = surface {
        if surface.has_separable_ruled_chart() {
            return surface.inversion_work();
        }
        let mut work = (surface.control_points.len() as u64).saturating_mul(2);
        for axis in 0..2 {
            let n = surface.control_counts[axis] as u64;
            let p = surface.degrees[axis] as u64;
            let curve = 128u64
                .saturating_mul(n.saturating_add(p.saturating_mul(p)))
                .saturating_add(surface.knots[axis].len() as u64);
            work = work.saturating_add(curve.saturating_mul(2));
        }
        work
    } else {
        1
    }
}
pub(super) fn inverse_metered(
    surface: &SurfaceGeometry,
    p: Vec3,
    b: &mut Budget<'_>,
) -> Result<[f64; 2]> {
    b.work(inverse_cost(surface))?;
    if let SurfaceGeometry::Nurbs { surface } = surface {
        if surface.has_separable_ruled_chart() {
            return surface.parameters_at_point(p, 1e-3).ok_or_else(|| {
                fail(
                    Code::TopologyPcurveInvalid,
                    "no ruled chart membership witness",
                )
            });
        }
        if let Some(uv) = surface.boundary_parameters(p, 1e-3) {
            return Ok(uv);
        }
        b.work(surface.inversion_work())?;
        return surface.parameters_at_point(p, 1e-3).ok_or_else(|| {
            fail(
                Code::TopologyPcurveInvalid,
                "no native NURBS membership witness",
            )
        });
    }
    inverse(surface, p)
}
fn boundary_curve(surface: &NurbsSurface, curve: &CurveGeometry) -> bool {
    let CurveGeometry::Nurbs { curve } = curve else {
        return false;
    };
    let Some([lo, hi]) = curve.range() else {
        return false;
    };
    let Some(sides) = surface.boundary_curves() else {
        return false;
    };
    sides.into_iter().any(|(_, _, other)| {
        if other.degree != curve.degree
            || other.knots.len() != curve.knots.len()
            || other.control_points.len() != curve.control_points.len()
        {
            return false;
        }
        let Some([a, z]) = other.range() else {
            return false;
        };
        [false, true].into_iter().any(|reverse| {
            let n = other.control_points.len();
            if !curve.knots.iter().enumerate().all(|(i, k)| {
                let v = if reverse {
                    (z - other.knots[other.knots.len() - 1 - i]) / (z - a)
                } else {
                    (other.knots[i] - a) / (z - a)
                };
                (*k - lo) / (hi - lo) == v
            }) {
                return false;
            }
            let ratio = curve.weights[0] / other.weights[if reverse { n - 1 } else { 0 }];
            if !ratio.is_finite() || ratio <= 0. {
                return false;
            }
            let mut delta = 0f64;
            let mut epsilon = 0f64;
            let mut magnitude = 0f64;
            for i in 0..n {
                let j = if reverse { n - 1 - i } else { i };
                delta = delta.max(distance(curve.control_points[i], other.control_points[j]));
                let relative = curve.weights[i] / other.weights[j] / ratio;
                if !relative.is_finite() || relative <= 0. {
                    return false;
                }
                epsilon = epsilon.max((relative - 1.).abs());
                for x in other.control_points[j] {
                    magnitude = magnitude.max(x.abs())
                }
            }
            delta + 4. * magnitude * epsilon < 1e-6
        })
    })
}
pub(super) fn curve_cost(curve: &CurveGeometry) -> u64 {
    if let CurveGeometry::RollingIntersection { surface, .. } = curve {
        return surface.inversion_work().saturating_add(8192);
    }
    if let CurveGeometry::SurfaceJet { jet, .. } = curve {
        return (jet.knots.len() as u64).saturating_mul(256);
    }
    if let CurveGeometry::Nurbs { curve } = curve {
        curve.inversion_work()
    } else {
        1
    }
}

pub(super) fn carrier_records(surface: &SurfaceGeometry, curve: &CurveGeometry) -> usize {
    let s = if let SurfaceGeometry::RollingBall { surface } = surface {
        surface.knots.len().saturating_mul(31)
    } else if let SurfaceGeometry::Nurbs { surface } = surface {
        surface
            .control_points
            .len()
            .saturating_mul(2)
            .saturating_add(surface.knots[0].len())
            .saturating_add(surface.knots[1].len())
    } else {
        0
    };
    let c = if let CurveGeometry::RollingIntersection { surface, .. } = curve {
        surface.knots.len().saturating_mul(31)
    } else if let CurveGeometry::SurfaceJet { surface, jet, .. } = curve {
        jet.knots
            .len()
            .saturating_mul(7)
            .saturating_add(carrier_records(
                surface,
                &CurveGeometry::Point { position: [0.; 3] },
            ))
    } else if let CurveGeometry::Nurbs { curve } = curve {
        curve
            .control_points
            .len()
            .saturating_mul(2)
            .saturating_add(curve.knots.len())
    } else {
        0
    };
    s.saturating_add(c)
}
pub(super) fn projection_cost(surface: &SurfaceGeometry, curve: &CurveGeometry) -> u64 {
    let full = if let SurfaceGeometry::Nurbs { surface } = surface {
        if surface.has_separable_ruled_chart() || boundary_curve(surface, curve) {
            0
        } else {
            surface.inversion_work()
        }
    } else {
        0
    };
    inverse_cost(surface)
        .saturating_add(full)
        .saturating_add(carrier_records(surface, curve) as u64)
        .saturating_add(1)
}

pub(super) fn periodic(surface: &SurfaceGeometry) -> [bool; 2] {
    match surface {
        SurfaceGeometry::Plane { .. }
        | SurfaceGeometry::Unknown { .. }
        | SurfaceGeometry::Nurbs { .. }
        | SurfaceGeometry::RollingBall { .. } => [false; 2],
        SurfaceGeometry::SphereStereographic { .. } => [false; 2],
        SurfaceGeometry::Torus { .. } => [true; 2],
        _ => [true, false],
    }
}
pub(super) fn prepare(
    wire: &mut Wire,
    surface: &SurfaceGeometry,
    curves: &[CurveRecord],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<()> {
    let mut n = wire.handles.len();
    let mut polar: Vec<_> = wire
        .handles
        .iter()
        .map(|h| singular(surface, table.coords[*h as usize]))
        .collect();
    let mut virtual_corner = false;
    // Split the two chart incidences of one isolated polar corner. The native
    // boundary and physical edge set stay unchanged; only the UV polygon gains
    // its necessary constant-point connector.
    if polar.iter().filter(|x| **x).count() == 1 {
        let at = polar.iter().position(|x| *x).unwrap();
        let incident: Vec<_> = wire
            .occurrences
            .iter()
            .filter(|o| o.start == at || o.end == at)
            .collect();
        let qualified = incident.len() == 2
            && incident.iter().all(|o| {
                matches!(
                    (&curves[o.edge].geometry, surface),
                    (CurveGeometry::Line { .. }, SurfaceGeometry::Cone { .. })
                )
            })
            && wire
                .occurrences
                .iter()
                .all(|o| !o.path[1..o.path.len() - 1].contains(&wire.handles[at]));
        if qualified {
            wire.handles.insert(at + 1, wire.handles[at]);
            polar.insert(at + 1, true);
            n += 1;
            virtual_corner = true;
            for o in &mut wire.occurrences {
                if o.start >= at {
                    o.start += 1;
                }
                if o.end > at {
                    o.end += 1;
                }
            }
        }
    }
    wire.uv = vec![[0.; 2]; n];
    if polar.iter().any(|x| *x) {
        let starts: Vec<_> = (0..n)
            .filter(|i| polar[*i] && !polar[(*i + n - 1) % n])
            .collect();
        if starts.len() != 1 {
            return Err(fail(
                Code::TopologyDegenerateUnresolved,
                "polar boundary needs one explicit collapsed run",
            ));
        }
        let start = starts[0];
        let mut count = 0;
        while count < n && polar[(start + count) % n] {
            count += 1;
        }
        if count < 2 {
            return Err(fail(
                Code::TopologyDegenerateUnresolved,
                "pole has no distinct UV endpoint handles",
            ));
        }
        let point = table.coords[wire.handles[start] as usize];
        for j in 0..count {
            if key(table.coords[wire.handles[(start + j) % n] as usize]) != key(point) {
                return Err(fail(
                    Code::TopologyDegenerateUnresolved,
                    "collapsed run spans different pole points",
                ));
            }
        }
        for j in 0..count - 1 {
            let a = (start + j) % n;
            let c = (a + 1) % n;
            if !virtual_corner
                && !wire.occurrences.iter().any(|o| {
                    o.start == a
                        && o.end == c
                        && matches!(curves[o.edge].geometry, CurveGeometry::Point { .. })
                })
            {
                return Err(fail(
                    Code::TopologyDegenerateUnresolved,
                    "polar segment has no explicit collapsed edge",
                ));
            }
        }
        let next = (start + count) % n;
        let ordinary = n - count;
        for j in 0..ordinary {
            let i = (next + j) % n;
            let mut uv = inverse_metered(surface, table.coords[wire.handles[i] as usize], b)?;
            if j > 0 {
                let previous = wire.uv[(i + n - 1) % n];
                for d in 0..2 {
                    if periodic(surface)[d] {
                        uv[d] = previous[d] + delta_angle(uv[d] - previous[d]);
                    }
                }
            }
            wire.uv[i] = uv;
        }
        let left = wire.uv[(start + n - 1) % n][0];
        let right = wire.uv[next][0];
        let v = inverse(surface, point)?[1];
        for j in 0..count {
            wire.uv[(start + j) % n] = [left + (right - left) * j as f64 / (count - 1) as f64, v];
        }
    } else {
        for i in 0..n {
            let mut uv = inverse_metered(surface, table.coords[wire.handles[i] as usize], b)?;
            if i > 0 {
                for (d, value) in uv.iter_mut().enumerate() {
                    if periodic(surface)[d] {
                        *value = wire.uv[i - 1][d] + delta_angle(*value - wire.uv[i - 1][d]);
                    }
                }
            }
            wire.uv[i] = uv;
        }
    }
    for d in 0..2 {
        if periodic(surface)[d] {
            let first = wire.uv[0][d];
            let last = wire.uv[n - 1][d];
            if (last + delta_angle(first - last) - first).abs() > 1e-6 {
                return Err(fail(
                    Code::TopologyUvUnresolved,
                    "noncontractible periodic boundary requires an explicit seam",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn singular(surface: &SurfaceGeometry, p: Vec3) -> bool {
    match surface {
        SurfaceGeometry::Cone { frame, .. } | SurfaceGeometry::Sphere { frame, .. } => {
            let d = sub(p, frame.origin);
            dot(d, frame.x_axis).hypot(dot(d, frame.y_axis)) < 1e-10
        }
        _ => false,
    }
}
pub(super) fn inverse(surface: &SurfaceGeometry, p: Vec3) -> Result<[f64; 2]> {
    if let SurfaceGeometry::RollingBall { surface } = surface {
        return surface.parameters_at_point(p, 1e-3).ok_or_else(|| {
            fail(
                Code::TopologyPcurveInvalid,
                "point off native rolling-ball carrier",
            )
        });
    }
    if let SurfaceGeometry::SphereStereographic { frame, radius } = surface {
        let d = sub(p, frame.origin);
        let denominator = radius + dot(d, frame.z_axis);
        if !frame.is_valid() || denominator <= radius * 1e-10 || !denominator.is_finite() {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "excluded stereographic pole",
            ));
        }
        let uv = [
            dot(d, frame.x_axis) / denominator,
            dot(d, frame.y_axis) / denominator,
        ];
        if surface.evaluate(uv).map_or(true, |q| distance(q, p) > 1e-3) {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "point is off stereographic sphere",
            ));
        }
        return Ok(uv);
    }
    if let SurfaceGeometry::Nurbs { surface } = surface {
        let witness = if surface.has_separable_ruled_chart() {
            surface.parameters_at_point(p, 1e-3)
        } else {
            surface
                .boundary_parameters(p, 1e-3)
                .or_else(|| surface.parameters_at_point(p, 1e-3))
        };
        return witness.ok_or_else(|| {
            fail(
                Code::TopologyPcurveInvalid,
                "no native NURBS membership witness",
            )
        });
    }
    let (frame, uv) = match surface {
        SurfaceGeometry::Plane { frame } => {
            let d = sub(p, frame.origin);
            (frame, [dot(d, frame.x_axis), dot(d, frame.y_axis)])
        }
        SurfaceGeometry::Cylinder { frame, .. } => {
            let d = sub(p, frame.origin);
            (
                frame,
                [
                    dot(d, frame.y_axis).atan2(dot(d, frame.x_axis)),
                    dot(d, frame.z_axis),
                ],
            )
        }
        SurfaceGeometry::Cone { frame, half_angle } => {
            let d = sub(p, frame.origin);
            let z = dot(d, frame.z_axis);
            if z < -1e-3 {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "point is behind cone apex",
                ));
            }
            (
                frame,
                [
                    dot(d, frame.y_axis).atan2(dot(d, frame.x_axis)),
                    z.max(0.) / half_angle.cos(),
                ],
            )
        }
        SurfaceGeometry::Sphere { frame, .. } => {
            let d = sub(p, frame.origin);
            let x = dot(d, frame.x_axis);
            let y = dot(d, frame.y_axis);
            (frame, [y.atan2(x), dot(d, frame.z_axis).atan2(x.hypot(y))])
        }
        SurfaceGeometry::Torus {
            frame,
            major_radius,
            minor_radius,
        } if major_radius > minor_radius => {
            let d = sub(p, frame.origin);
            let x = dot(d, frame.x_axis);
            let y = dot(d, frame.y_axis);
            (
                frame,
                [
                    y.atan2(x),
                    dot(d, frame.z_axis).atan2(x.hypot(y) - major_radius),
                ],
            )
        }
        _ => {
            return Err(fail(
                Code::TopologyGeometryUnsupported,
                "unsupported or singular carrier chart",
            ));
        }
    };
    if !frame.is_valid()
        || uv.iter().any(|v| !v.is_finite())
        || surface.evaluate(uv).is_err()
        || distance(surface.evaluate(uv).unwrap(), p) > 1e-3
    {
        return Err(fail(
            Code::TopologyPcurveInvalid,
            "point is off analytic carrier",
        ));
    }
    Ok(uv)
}

pub(super) fn projected(
    surface: &SurfaceGeometry,
    curve: &CurveGeometry,
    range: [f64; 2],
    guides: &[[f64; 3]],
    q: f64,
) -> Result<[f64; 2]> {
    if guides.len() < 2
        || guides[0][0] != 0.
        || guides.last().unwrap()[0] != 1.
        || guides.iter().any(|g| g.iter().any(|v| !v.is_finite()))
        || guides.windows(2).any(|g| g[0][0] >= g[1][0])
    {
        return Err(fail(
            Code::TopologyPcurveInvalid,
            "invalid analytic projection guides",
        ));
    }
    let i = guides
        .partition_point(|g| g[0] < q)
        .clamp(1, guides.len() - 1);
    let a = guides[i - 1];
    let c = guides[i];
    let fraction = (q - a[0]) / (c[0] - a[0]);
    let hint = [
        a[1] + fraction * (c[1] - a[1]),
        a[2] + fraction * (c[2] - a[2]),
    ];
    // A saved limiting channel already names its transverse chart side. At a
    // collapsed rolling tip, a generic inverse would erase that incidence.
    if let (SurfaceGeometry::RollingBall { surface }, CurveGeometry::Nurbs { curve }) =
        (surface, curve)
    {
        for side in 0..2 {
            if surface.limit_curve(side).as_ref() == Some(curve) {
                let station = interval_parameter(range, q);
                if station < surface.parameter_range[0] || station > surface.parameter_range[1] {
                    return Err(fail(
                        Code::TopologyPcurveInvalid,
                        "native rolling limit station",
                    ));
                }
                return Ok([station, side as f64]);
            }
        }
    }
    let p = curve
        .evaluate(interval_parameter(range, q))
        .map_err(|_| fail(Code::TopologyPcurveInvalid, "projection edge evaluation"))?;
    let mut uv = if let SurfaceGeometry::Nurbs { surface: shape } = surface {
        if !shape.has_separable_ruled_chart() && boundary_curve(shape, curve) {
            shape.boundary_parameters(p, 1e-3).ok_or_else(|| {
                fail(
                    Code::TopologyPcurveInvalid,
                    "native NURBS boundary inversion",
                )
            })?
        } else {
            inverse(surface, p)?
        }
    } else {
        inverse(surface, p)?
    };
    for i in 0..2 {
        if periodic(surface)[i] {
            uv[i] = hint[i] + delta_angle(uv[i] - hint[i]);
        }
    }
    if singular(surface, p) {
        uv[0] = hint[0];
    }
    Ok(uv)
}

pub(super) fn guides(
    curve: &CurveGeometry,
    range: [f64; 2],
    path: &[u32],
    uv: &[[f64; 2]],
    table: &IndexTable,
) -> Result<Vec<[f64; 3]>> {
    let mut out = Vec::new();
    let mut previous_angle = 0.;
    let mut angle = range[0];
    for (i, (&h, uv)) in path.iter().zip(uv).enumerate() {
        let p = table.coords[h as usize];
        let mut q = match curve {
            CurveGeometry::RollingIntersection {
                surface, u_range, ..
            } => {
                let uv = surface.parameters_at_point(p, 1e-3).ok_or_else(|| {
                    fail(
                        Code::TopologyPcurveInvalid,
                        "rolling intersection parameter",
                    )
                })?;
                (uv[0] - u_range[0]) / (u_range[1] - u_range[0])
            }
            CurveGeometry::SurfaceJet { .. } => {
                let t = curve.bounded_parameter(p, 1e-3).ok_or_else(|| {
                    fail(Code::TopologyPcurveInvalid, "native surface-jet parameter")
                })?;
                (t - range[0]) / (range[1] - range[0])
            }
            CurveGeometry::Nurbs { curve } => {
                let t = curve.parameter_at_point(p, 1e-3).ok_or_else(|| {
                    fail(
                        Code::TopologyPcurveInvalid,
                        "native NURBS projection parameter",
                    )
                })?;
                (t - range[0]) / (range[1] - range[0])
            }
            CurveGeometry::Line { origin, direction } => {
                (dot(sub(p, *origin), *direction) - range[0]) / (range[1] - range[0])
            }
            CurveGeometry::Circle { .. }
            | CurveGeometry::Ellipse { .. }
            | CurveGeometry::CylinderIntersection { .. } => {
                let a = curve.angular_parameter(p).ok_or_else(|| {
                    fail(Code::TopologyPcurveInvalid, "angular projection parameter")
                })?;
                if i > 0 {
                    angle += delta_angle(a - previous_angle);
                }
                previous_angle = a;
                (angle - range[0]) / (range[1] - range[0])
            }
            CurveGeometry::Point { .. } => i as f64 / (path.len() - 1) as f64,
            _ => {
                return Err(fail(
                    Code::TopologyGeometryUnsupported,
                    "unsupported projection curve",
                ));
            }
        };
        if i == 0 {
            q = 0.;
        } else if i + 1 == path.len() {
            q = 1.;
        } else {
            q = q.clamp(0., 1.);
        }
        if out.last().is_some_and(|g: &[f64; 3]| q <= g[0]) {
            continue;
        }
        out.push([q, uv[0], uv[1]]);
    }
    if out.len() < 2 || out.last().unwrap()[0] != 1. {
        return Err(fail(
            Code::TopologyPcurveInvalid,
            "projection has no ordered interval",
        ));
    }
    Ok(out)
}

pub(super) fn normal_on(surface: &SurfaceGeometry, center: Vec3, radius: f64) -> Option<Vec3> {
    match surface {
        SurfaceGeometry::Plane { frame } => Some(frame.z_axis),
        SurfaceGeometry::Cylinder { frame, radius: r } => {
            let d = sub(center, frame.origin);
            ((radius - r).abs() < 1e-3
                && distance(d, scale(frame.z_axis, dot(d, frame.z_axis))) < 1e-3)
                .then_some(frame.z_axis)
        }
        SurfaceGeometry::Cone { frame, half_angle } => {
            let d = sub(center, frame.origin);
            let z = dot(d, frame.z_axis);
            (z > 0.
                && (radius - z * half_angle.tan()).abs() < 1e-3
                && distance(d, scale(frame.z_axis, z)) < 1e-3)
                .then_some(frame.z_axis)
        }
        SurfaceGeometry::Sphere { frame, radius: r } => {
            let d = sub(center, frame.origin);
            let n = dot(d, d).sqrt();
            ((radius * radius + n * n - r * r).abs() < 1e-3 * r.max(1.))
                .then(|| unit(d))
                .flatten()
        }
        SurfaceGeometry::Torus {
            frame,
            major_radius: r,
            minor_radius: s,
        } => {
            let d = sub(center, frame.origin);
            let z = dot(d, frame.z_axis);
            let radial = sub(d, scale(frame.z_axis, z));
            let rho = dot(radial, radial).sqrt();
            if rho < 1e-3 && ((radius - r).powi(2) + z * z - s * s).abs() < 1e-3 * s.max(1.) {
                Some(frame.z_axis)
            } else if z.abs() < 1e-3 && (rho - r).abs() < 1e-3 && (radius - s).abs() < 1e-3 {
                unit(cross(frame.z_axis, radial))
            } else {
                None
            }
        }
        _ => None,
    }
}
pub(super) fn resolve_circle(
    curve: &mut CurveRecord,
    faces: &[usize],
    surfaces: &[SurfaceRecord],
    endpoints: [Vec3; 2],
) -> Result<()> {
    if !matches!(curve.geometry, CurveGeometry::Unknown { .. })
        || !curve.raw_parameters_hex.starts_with("0012003337")
    {
        return Ok(());
    }
    let raw = &curve.raw_parameters_hex[10..];
    if raw.len() < 32 {
        return Ok(());
    }
    let mut values = [0.; 4];
    for (i, x) in values.iter_mut().enumerate() {
        *x = f32::from_bits(
            u32::from_str_radix(&raw[8 * i..8 * i + 8], 16)
                .map_err(|_| fail(Code::TopologyGeometryUnsupported, "circle parameter bits"))?,
        ) as f64;
    }
    let center = [values[0], values[1], values[2]];
    let radius = values[3];
    if radius <= 0. || !finite(center) || !radius.is_finite() {
        return Ok(());
    }
    let mut normal = None;
    for &face in faces {
        if let Some(n) = normal_on(&surfaces[face].geometry, center, radius) {
            if normal.is_some_and(|old| dot(cross(old, n), cross(old, n)) > 1e-8) {
                return Err(fail(
                    Code::TopologyGeometryUnsupported,
                    "contradictory circle carrier normals",
                ));
            }
            normal = Some(n);
        }
    }
    let cross_endpoints = cross(sub(endpoints[0], center), sub(endpoints[1], center));
    if normal.is_none() && dot(cross_endpoints, cross_endpoints).sqrt() > radius * radius * 1e-6 {
        normal = unit(cross_endpoints);
    }
    if let Some(normal) = normal {
        curve.geometry = CurveGeometry::Circle {
            frame: Frame3::from_axis(center, normal)
                .ok_or_else(|| fail(Code::TopologyGeometryUnsupported, "circle frame"))?,
            radius,
        };
        curve.domain = curve.geometry.domain();
        for &face in faces {
            curve.evidence.push(surfaces[face].source.clone());
        }
    }
    Ok(())
}
