//! Fit-free rolling-ball carrier from the ten serialized quintic jet channels.
use super::{
    EvaluationError, NurbsCurve, RigidTransform, Vec3, add, cross, dot, finite, scale, sub,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RollingBallJet {
    pub knots: Vec<f64>,
    /// Limit1 XYZ, Limit2 XYZ, centre XYZ, included angle.
    pub positions: Vec<[f64; 10]>,
    pub first: Vec<[f64; 10]>,
    pub second: Vec<[f64; 10]>,
    pub parameter_range: [f64; 2],
}
impl RollingBallJet {
    pub fn is_valid(&self) -> bool {
        let n = self.knots.len();
        (2..=8192).contains(&n)
            && [self.positions.len(), self.first.len(), self.second.len()]
                .iter()
                .all(|k| *k == n)
            && self.knots.iter().all(|v| v.is_finite())
            && self
                .knots
                .windows(2)
                .all(|w| w[0] < w[1] && (w[1] - w[0]).is_finite())
            && self
                .positions
                .iter()
                .chain(&self.first)
                .chain(&self.second)
                .flatten()
                .all(|v| v.is_finite())
            && self.parameter_range.iter().all(|v| v.is_finite())
            && self.parameter_range[0] < self.parameter_range[1]
            && self.parameter_range[0] >= self.knots[0]
            && self.parameter_range[1] <= self.knots[n - 1]
            && self.positions.iter().all(|row| {
                let a = sub([row[0], row[1], row[2]], [row[6], row[7], row[8]]);
                let b = sub([row[3], row[4], row[5]], [row[6], row[7], row[8]]);
                let r = dot(a, a).sqrt();
                let z = dot(b, b).sqrt();
                r > 0.
                    && (r - z).abs() < 1e-8 * r.max(1.)
                    && (0. ..=std::f64::consts::PI).contains(&row[9])
                    && (dot(a, b) / (r * z)).clamp(-1., 1.).acos().is_finite()
                    && (row[9] - (dot(a, b) / (r * z)).clamp(-1., 1.).acos()).abs() < 1e-7
            })
            && (0..n - 1).all(|i| self.controls(i).iter().flatten().all(|v| v.is_finite()))
    }
    fn controls(&self, i: usize) -> [[f64; 10]; 6] {
        let h = self.knots[i + 1] - self.knots[i];
        std::array::from_fn(|k| {
            std::array::from_fn(|d| match k {
                0 => self.positions[i][d],
                1 => self.positions[i][d] + h * self.first[i][d] / 5.,
                2 => {
                    self.positions[i][d]
                        + 2. * h * self.first[i][d] / 5.
                        + h * h * self.second[i][d] / 20.
                }
                3 => {
                    self.positions[i + 1][d] - 2. * h * self.first[i + 1][d] / 5.
                        + h * h * self.second[i + 1][d] / 20.
                }
                4 => self.positions[i + 1][d] - h * self.first[i + 1][d] / 5.,
                _ => self.positions[i + 1][d],
            })
        })
    }
    pub(crate) fn row_validated(&self, t: f64) -> Result<[f64; 10], EvaluationError> {
        if !t.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        if t < self.parameter_range[0] || t > self.parameter_range[1] {
            return Err(EvaluationError::OutsideDomain);
        }
        let i = self
            .knots
            .partition_point(|k| *k <= t)
            .saturating_sub(1)
            .min(self.knots.len() - 2);
        let q = (t - self.knots[i]) / (self.knots[i + 1] - self.knots[i]);
        let mut c = self.controls(i);
        for width in (1..6).rev() {
            for k in 0..width {
                let next = c[k + 1];
                for (value, other) in c[k].iter_mut().zip(next) {
                    *value = (1. - q) * *value + q * other;
                }
            }
        }
        if c[0].iter().all(|v| v.is_finite()) {
            Ok(c[0])
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    pub(crate) fn evaluate_validated(&self, uv: [f64; 2]) -> Result<Vec3, EvaluationError> {
        let [t, phi] = uv;
        if !phi.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        if !(0. ..=1.).contains(&phi) {
            return Err(EvaluationError::OutsideDomain);
        }
        let row = self.row_validated(t)?;
        let c = [row[6], row[7], row[8]];
        let a = sub([row[0], row[1], row[2]], c);
        let b = sub([row[3], row[4], row[5]], c);
        let axis = cross(a, b);
        let n = dot(axis, axis).sqrt();
        if n <= 1e-20 {
            if row[9].abs() < 1e-12 {
                return Ok(add(c, a));
            }
            return Err(EvaluationError::InvalidGeometry);
        }
        let axis = scale(axis, 1. / n);
        let angle = phi * row[9];
        let p = add(
            c,
            add(
                scale(a, angle.cos()),
                add(
                    scale(cross(axis, a), angle.sin()),
                    scale(axis, dot(axis, a) * (1. - angle.cos())),
                ),
            ),
        );
        if finite(p) {
            Ok(p)
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    pub fn evaluate(&self, uv: [f64; 2]) -> Result<Vec3, EvaluationError> {
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        self.evaluate_validated(uv)
    }
    /// Exact quintic limiting channel, retaining all serialized spans.
    pub fn limit_curve(&self, side: usize) -> Option<NurbsCurve> {
        if !self.is_valid() || side > 1 {
            return None;
        }
        let mut knots = Vec::new();
        let mut points = Vec::new();
        for i in 0..self.knots.len() - 1 {
            knots.extend(std::iter::repeat_n(self.knots[i], 6));
            points.extend(
                self.controls(i)
                    .map(|p| [p[3 * side], p[3 * side + 1], p[3 * side + 2]]),
            );
        }
        knots.extend(std::iter::repeat_n(*self.knots.last()?, 6));
        let curve = NurbsCurve {
            degree: 5,
            knots,
            weights: vec![1.; points.len()],
            control_points: points,
        };
        curve.is_valid().then_some(curve)
    }
    #[must_use]
    pub fn inversion_work(&self) -> u64 {
        (self.knots.len() as u64)
            .saturating_mul(256)
            .saturating_add(16384)
    }
    /// Bounded scalar search of this serialized surface, never a surface fit.
    pub fn parameters_at_point(&self, p: Vec3, tolerance: f64) -> Option<[f64; 2]> {
        if !self.is_valid() || !finite(p) || !tolerance.is_finite() || tolerance <= 0. {
            return None;
        }
        let [lo, hi] = self.parameter_range;
        let project = |t: f64| -> Option<(f64, f64)> {
            let row = self.row_validated(t).ok()?;
            let c = [row[6], row[7], row[8]];
            let a = sub([row[0], row[1], row[2]], c);
            let z = sub([row[3], row[4], row[5]], c);
            let ax = cross(a, z);
            let length = dot(ax, ax).sqrt();
            let phi = if row[9].abs() < 1e-12 {
                0.
            } else {
                if length < 1e-20 {
                    return None;
                }
                let ax = scale(ax, 1. / length);
                let d = sub(p, c);
                (dot(cross(a, d), ax).atan2(dot(a, d)) / row[9]).clamp(0., 1.)
            };
            let delta = sub(self.evaluate_validated([t, phi]).ok()?, p);
            Some((dot(delta, delta), phi))
        };
        let mut grid: Vec<_> = (0..=64)
            .map(|i| {
                if i == 64 {
                    hi
                } else {
                    lo + (hi - lo) * i as f64 / 64.
                }
            })
            .collect();
        grid.extend(self.knots.iter().copied().filter(|t| *t >= lo && *t <= hi));
        grid.sort_by(f64::total_cmp);
        grid.dedup();
        let mut ranked: Vec<_> = grid
            .iter()
            .enumerate()
            .filter_map(|(i, t)| Some((project(*t)?.0, i)))
            .collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best = (f64::INFINITY, [0.; 2]);
        let golden = (5f64.sqrt() - 1.) / 2.;
        for &(_, i) in ranked.iter().take(4) {
            let t = grid[i];
            let (error, phi) = project(t)?;
            if error < best.0 {
                best = (error, [t, phi]);
            }
            let mut a = grid[i.saturating_sub(1)];
            let mut b = grid[(i + 1).min(grid.len() - 1)];
            let mut c = b - golden * (b - a);
            let mut d = a + golden * (b - a);
            let mut fc = project(c)?.0;
            let mut fd = project(d)?.0;
            for _ in 0..40 {
                if fc < fd {
                    b = d;
                    d = c;
                    fd = fc;
                    c = b - golden * (b - a);
                    fc = project(c)?.0;
                } else {
                    a = c;
                    c = d;
                    fc = fd;
                    d = a + golden * (b - a);
                    fd = project(d)?.0;
                }
            }
            let t = (a + b) / 2.;
            let (error, phi) = project(t)?;
            if error < best.0 {
                best = (error, [t, phi]);
            }
        }
        (best.0 <= tolerance * tolerance).then_some(best.1)
    }
    pub fn transformed(&self, t: &RigidTransform) -> Result<Self, EvaluationError> {
        if !self.is_valid() || !t.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let mut out = self.clone();
        for (kind, rows) in [&mut out.positions, &mut out.first, &mut out.second]
            .into_iter()
            .enumerate()
        {
            for row in rows {
                for at in [0, 3, 6] {
                    let v = [row[at], row[at + 1], row[at + 2]];
                    let v = if kind == 0 { t.point(v) } else { t.vector(v) };
                    row[at..at + 3].copy_from_slice(&v);
                }
            }
        }
        Ok(out)
    }
}
