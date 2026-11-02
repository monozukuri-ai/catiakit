//! Scalar root of a native rolling-ball patch and an analytic carrier.
use super::*;
pub(super) fn implicit(surface: &SurfaceGeometry, p: Vec3) -> Option<f64> {
    let (frame, kind) = match surface {
        SurfaceGeometry::Plane { frame } => (frame, 0),
        SurfaceGeometry::Cylinder { frame, .. } => (frame, 1),
        SurfaceGeometry::Sphere { frame, .. } => (frame, 2),
        SurfaceGeometry::Torus { frame, .. } => (frame, 3),
        SurfaceGeometry::Cone { frame, .. } => (frame, 4),
        _ => return None,
    };
    if !frame.is_valid() || !finite(p) {
        return None;
    }
    let d = sub(p, frame.origin);
    let z = dot(d, frame.z_axis);
    let rho = dot(d, frame.x_axis).hypot(dot(d, frame.y_axis));
    Some(match (kind, surface) {
        (0, _) => z,
        (1, SurfaceGeometry::Cylinder { radius, .. }) if radius.is_finite() && *radius > 0. => {
            rho - radius
        }
        (2, SurfaceGeometry::Sphere { radius, .. }) if radius.is_finite() && *radius > 0. => {
            rho.hypot(z) - radius
        }
        (
            3,
            SurfaceGeometry::Torus {
                major_radius,
                minor_radius,
                ..
            },
        ) if major_radius.is_finite()
            && minor_radius.is_finite()
            && *minor_radius > 0.
            && major_radius > minor_radius =>
        {
            (rho - major_radius).hypot(z) - minor_radius
        }
        (4, SurfaceGeometry::Cone { half_angle, .. })
            if half_angle.is_finite()
                && *half_angle > 0.
                && *half_angle < std::f64::consts::FRAC_PI_2
                && z >= -1e-3 =>
        {
            rho * half_angle.cos() - z * half_angle.sin()
        }
        _ => return None,
    })
}
pub(super) fn rolling(
    surface: &RollingBallJet,
    support: &SurfaceGeometry,
    u_range: [f64; 2],
    hints: [f64; 2],
    q: f64,
) -> Result<Vec3, EvaluationError> {
    if !(0. ..=1.).contains(&q)
        || u_range.iter().chain(&hints).any(|x| !x.is_finite())
        || !surface.is_valid()
    {
        return Err(EvaluationError::InvalidGeometry);
    }
    let u = if q == 1. {
        u_range[1]
    } else {
        u_range[0] + q * (u_range[1] - u_range[0])
    };
    let hint = hints[0] + q * (hints[1] - hints[0]);
    let value = |phi| {
        let p = surface.evaluate_validated([u, phi]).ok()?;
        Some((implicit(support, p)?, p))
    };
    let mut roots = Vec::new();
    let mut previous = value(0.).ok_or(EvaluationError::InvalidGeometry)?;
    if previous.0.abs() <= 1e-9 {
        roots.push((0., previous.1))
    }
    for i in 1..=32 {
        let phi = i as f64 / 32.;
        let current = value(phi).ok_or(EvaluationError::InvalidGeometry)?;
        if current.0.abs() <= 1e-9 {
            roots.push((phi, current.1));
        }
        if current.0 * previous.0 < 0. {
            let mut a = (i - 1) as f64 / 32.;
            let mut z = phi;
            let mut sign = previous.0;
            for _ in 0..40 {
                let mid = (a + z) / 2.;
                let f = value(mid).ok_or(EvaluationError::InvalidGeometry)?.0;
                if f * sign > 0. {
                    a = mid;
                    sign = f
                } else {
                    z = mid
                }
            }
            let mid = (a + z) / 2.;
            roots.push((mid, value(mid).ok_or(EvaluationError::InvalidGeometry)?.1));
        }
        previous = current;
    }
    roots.sort_by(|a, b| (a.0 - hint).abs().total_cmp(&(b.0 - hint).abs()));
    let first = roots.first().ok_or(EvaluationError::InvalidGeometry)?;
    if roots.iter().skip(1).any(|r| {
        (r.0 - first.0).abs() > 1e-6 && ((r.0 - hint).abs() - (first.0 - hint).abs()).abs() < 1e-6
    }) {
        return Err(EvaluationError::InvalidGeometry);
    }
    Ok(first.1)
}
