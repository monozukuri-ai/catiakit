//! Exact quintic Hermite spans from serialized value/derivative lanes.
use super::EvaluationError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UvJet {
    pub knots: Vec<f64>,
    pub positions: Vec<[f64; 2]>,
    pub first: Vec<[f64; 2]>,
    pub second: Vec<[f64; 2]>,
    pub parameter_range: [f64; 2],
}
impl UvJet {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let n = self.knots.len();
        (2..=8192).contains(&n)
            && [self.positions.len(), self.first.len(), self.second.len()]
                .into_iter()
                .all(|k| k == n)
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
            && (0..n - 1).all(|i| self.controls(i).iter().flatten().all(|x| x.is_finite()))
    }
    fn controls(&self, i: usize) -> [[f64; 2]; 6] {
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
    pub fn evaluate(&self, t: f64) -> Result<[f64; 2], EvaluationError> {
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        self.evaluate_validated(t)
    }
    /// Internal sampling after one complete structural validation.
    pub(crate) fn evaluate_validated(&self, t: f64) -> Result<[f64; 2], EvaluationError> {
        if !t.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        if t < self.knots[0] || t > self.knots[self.knots.len() - 1] {
            return Err(EvaluationError::OutsideDomain);
        }
        let i = self
            .knots
            .partition_point(|x| *x <= t)
            .saturating_sub(1)
            .min(self.knots.len() - 2);
        let h = self.knots[i + 1] - self.knots[i];
        let q = (t - self.knots[i]) / h;
        let mut controls = self.controls(i);
        for width in (1..6).rev() {
            for k in 0..width {
                let next = controls[k + 1];
                for (value, other) in controls[k].iter_mut().zip(next) {
                    *value = (1. - q) * *value + q * other;
                }
            }
        }
        if controls[0].iter().all(|v| v.is_finite()) {
            Ok(controls[0])
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
}
