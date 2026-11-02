//! Scaled reference-direction quintic jet. Its admitted chart is a ruled
//! surface: P(u) + v (Q(u)-P(u))/reference_distance. All six channels are kept.
use super::{EvaluationError, NurbsSurface, Vec3, dot, finite, sub};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuledJet {
    pub knots: Vec<f64>,
    /// Guide point P and reference point Q, in serialized order.
    pub positions: Vec<[f64; 6]>,
    pub first: Vec<[f64; 6]>,
    pub second: Vec<[f64; 6]>,
    pub current_limits: [f64; 4],
    pub reference_limits: [f64; 2],
}
impl RuledJet {
    pub fn is_valid(&self) -> bool {
        let n = self.knots.len();
        let r = self.reference_limits[1];
        (2..=8192).contains(&n)
            && [self.positions.len(), self.first.len(), self.second.len()]
                .iter()
                .all(|m| *m == n)
            && self.knots.iter().all(|v| v.is_finite())
            && self
                .knots
                .windows(2)
                .all(|k| k[0] < k[1] && (k[1] - k[0]).is_finite())
            && self
                .positions
                .iter()
                .chain(&self.first)
                .chain(&self.second)
                .flatten()
                .all(|v| v.is_finite())
            && self
                .current_limits
                .iter()
                .chain(&self.reference_limits)
                .all(|v| v.is_finite())
            && self.current_limits[0] < self.current_limits[1]
            && self.knots.contains(&self.current_limits[0])
            && self.knots.contains(&self.current_limits[1])
            && self.current_limits[2] == 0.
            && self.reference_limits[0] == 0.
            && r > 0.
            && (2. * r).is_finite()
            && self.current_limits[3] == 2. * r
            && self.positions.iter().all(|row| {
                let d = sub([row[3], row[4], row[5]], [row[0], row[1], row[2]]);
                let length = dot(d, d).sqrt();
                length.is_finite() && (length - r).abs() <= 1e-8 * r.max(1.)
            })
            && (0..n - 1).all(|i| {
                self.controls(i).iter().all(|c| {
                    c.iter().all(|v| v.is_finite())
                        && (0..3).all(|d| (2. * c[d + 3] - c[d]).is_finite())
                })
            })
    }
    fn controls(&self, i: usize) -> [[f64; 6]; 6] {
        let h = self.knots[i + 1] - self.knots[i];
        std::array::from_fn(|j| {
            std::array::from_fn(|d| match j {
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
    /// Exact tensor-product conversion over source-knot-aligned current limits.
    /// No interpolation solve, fitting, or normalization of intermediate jets.
    pub fn surface(&self) -> Option<NurbsSurface> {
        if !self.is_valid() {
            return None;
        }
        let mut knots = Vec::new();
        let mut points = Vec::new();
        for i in 0..self.knots.len() - 1 {
            if self.knots[i] < self.current_limits[0] || self.knots[i + 1] > self.current_limits[1]
            {
                continue;
            }
            knots.extend(std::iter::repeat_n(self.knots[i], 6));
            for c in self.controls(i) {
                points.push([c[0], c[1], c[2]]);
                points.push([2. * c[3] - c[0], 2. * c[4] - c[1], 2. * c[5] - c[2]]);
            }
        }
        knots.extend(std::iter::repeat_n(self.current_limits[1], 6));
        let s = NurbsSurface {
            degrees: [5, 1],
            control_counts: [points.len() / 2, 2],
            knots: [
                knots,
                vec![0., 0., self.current_limits[3], self.current_limits[3]],
            ],
            weights: vec![1.; points.len()],
            control_points: points,
        };
        s.is_valid().then_some(s)
    }
    pub fn evaluate(&self, uv: [f64; 2]) -> Result<Vec3, EvaluationError> {
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if uv.iter().any(|v| !v.is_finite()) {
            return Err(EvaluationError::InvalidParameter);
        }
        if uv[0] < self.current_limits[0]
            || uv[0] > self.current_limits[1]
            || uv[1] < 0.
            || uv[1] > self.current_limits[3]
        {
            return Err(EvaluationError::OutsideDomain);
        }
        let i = self
            .knots
            .partition_point(|k| *k <= uv[0])
            .saturating_sub(1)
            .min(self.knots.len() - 2);
        let q = (uv[0] - self.knots[i]) / (self.knots[i + 1] - self.knots[i]);
        let mut c = self.controls(i);
        for width in (1..6).rev() {
            for k in 0..width {
                let next = c[k + 1];
                for (v, z) in c[k].iter_mut().zip(next) {
                    *v = (1. - q) * *v + q * z
                }
            }
        }
        let t = uv[1] / self.reference_limits[1];
        let p = std::array::from_fn(|d| c[0][d] + t * (c[0][d + 3] - c[0][d]));
        if finite(p) {
            Ok(p)
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
}
