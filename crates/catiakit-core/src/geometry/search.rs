//! Bounded scalar membership witness for finite procedural curves.
use super::{CurveGeometry, Vec3, dot, sub};
pub(super) fn parameter(curve: &CurveGeometry, p: Vec3, tolerance: f64) -> Option<f64> {
    let domain = curve.domain()?;
    let lo = domain.minimum?;
    let hi = domain.maximum?;
    if !super::finite(p) || !tolerance.is_finite() || tolerance <= 0. || lo >= hi {
        return None;
    }
    let error = |t| {
        let d = sub(curve.evaluate(t).ok()?, p);
        Some(dot(d, d))
    };
    let grid: Vec<_> = (0..=32)
        .map(|i| {
            if i == 32 {
                hi
            } else {
                lo + (hi - lo) * i as f64 / 32.
            }
        })
        .collect();
    let mut ranked: Vec<_> = grid
        .iter()
        .enumerate()
        .filter_map(|(i, t)| Some((error(*t)?, i)))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best = *ranked.first()?;
    let mut station = grid[best.1];
    let golden = (5f64.sqrt() - 1.) / 2.;
    for &(_, i) in ranked.iter().take(3) {
        let mut a = grid[i.saturating_sub(1)];
        let mut b = grid[(i + 1).min(32)];
        let mut c = b - golden * (b - a);
        let mut d = a + golden * (b - a);
        let mut fc = error(c)?;
        let mut fd = error(d)?;
        for _ in 0..40 {
            if fc < fd {
                b = d;
                d = c;
                fd = fc;
                c = b - golden * (b - a);
                fc = error(c)?
            } else {
                a = c;
                c = d;
                fc = fd;
                d = a + golden * (b - a);
                fd = error(d)?
            }
        }
        let t = (a + b) / 2.;
        let e = error(t)?;
        if e < best.0 {
            best.0 = e;
            station = t
        }
    }
    (best.0 <= tolerance * tolerance).then_some(station)
}
