//! Nonperiodic rational tensor-product carrier in serialized knot coordinates.
use super::{EvaluationError, Vec3, dot, finite, sub};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NurbsSurface {
    pub degrees: [usize; 2],
    pub knots: [Vec<f64>; 2],
    pub control_counts: [usize; 2],
    /// U-major, V-minor source order.
    pub control_points: Vec<Vec3>,
    pub weights: Vec<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NurbsCurve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub control_points: Vec<Vec3>,
    pub weights: Vec<f64>,
}
impl NurbsCurve {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let n = self.control_points.len();
        let p = self.degree;
        (1..=16).contains(&p)
            && n > p
            && n <= 65536
            && self.knots.len() == n + p + 1
            && self.weights.len() == n
            && self.control_points.iter().all(|p| finite(*p))
            && self.weights.iter().all(|w| w.is_finite() && *w > 0.)
            && self.knots.iter().all(|k| k.is_finite())
            && self
                .knots
                .windows(2)
                .all(|w| w[0] <= w[1] && (w[1] - w[0]).is_finite())
            && self.knots[p] < self.knots[n]
            && (self.knots[n] - self.knots[p]).is_finite()
            && self.knots[..=p].iter().all(|x| *x == self.knots[p])
            && self.knots[n..].iter().all(|x| *x == self.knots[n])
            && self.knots.windows(p + 2).all(|w| w[0] != w[p + 1])
    }
    pub fn range(&self) -> Option<[f64; 2]> {
        self.is_valid().then(|| {
            [
                self.knots[self.degree],
                self.knots[self.control_points.len()],
            ]
        })
    }
    pub fn evaluate(&self, t: f64) -> Result<Vec3, EvaluationError> {
        Ok(self.jet(t)?.0)
    }
    pub fn derivative(&self, t: f64) -> Result<Vec3, EvaluationError> {
        Ok(self.jet(t)?.1)
    }
    fn jet(&self, t: f64) -> Result<(Vec3, Vec3), EvaluationError> {
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !t.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        let n = self.control_points.len();
        let p = self.degree;
        let basis = local_basis(&self.knots, p, n, t)?;
        let lower = local_basis(&self.knots, p - 1, n, t)?;
        let low = |i: usize| lower.iter().find(|(j, _)| *j == i).map_or(0., |(_, v)| *v);
        let maximum = self.weights.iter().copied().fold(0., f64::max);
        let mut numerator = [0.; 3];
        let mut tangent = [0.; 3];
        let mut w = 0.;
        let mut dw = 0.;
        for (i, value) in basis {
            let weight = self.weights[i] / maximum;
            let a = self.knots[i + p] - self.knots[i];
            let z = self.knots[i + p + 1] - self.knots[i + 1];
            let derivative = if a == 0. { 0. } else { p as f64 * low(i) / a }
                - if z == 0. {
                    0.
                } else {
                    p as f64 * low(i + 1) / z
                };
            w += value * weight;
            dw += derivative * weight;
            for d in 0..3 {
                numerator[d] += value * weight * self.control_points[i][d];
                tangent[d] += derivative * weight * self.control_points[i][d];
            }
        }
        if w <= 0. || !w.is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let position = numerator.map(|x| x / w);
        let derivative = std::array::from_fn(|d| (tangent[d] - position[d] * dw) / w);
        if finite(position) && finite(derivative) {
            Ok((position, derivative))
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    #[must_use]
    pub fn inversion_work(&self) -> u64 {
        128u64
            .saturating_mul(
                (self.control_points.len() as u64)
                    .saturating_add((self.degree as u64).saturating_mul(self.degree as u64)),
            )
            .saturating_add(self.knots.len() as u64)
    }
    /// Bounded numerical inversion of the exact native carrier. A returned
    /// parameter is a checked 3D membership witness, not a fitted curve.
    pub fn parameter_at_point(&self, p: Vec3, tolerance: f64) -> Option<f64> {
        let [lo, hi] = self.range()?;
        if !finite(p) || !tolerance.is_finite() || tolerance <= 0. {
            return None;
        }
        let evaluations = std::cell::Cell::new(0usize);
        let evaluate = |t: f64| {
            if evaluations.get() >= 128 {
                return None;
            }
            evaluations.set(evaluations.get() + 1);
            self.jet(t).ok()
        };
        let distance = |t: f64| evaluate(t).map(|(q, _)| dot(sub(q, p), sub(q, p)));
        let mut seeds = vec![lo, hi];
        for pair in self.knots.windows(2) {
            if pair[0] < pair[1] {
                for j in 0..=2 {
                    seeds.push(if j == 2 {
                        pair[1]
                    } else {
                        pair[0] + (pair[1] - pair[0]) * j as f64 / 2.
                    });
                }
            }
        }
        let mut ranked: Vec<_> = seeds
            .into_iter()
            .filter_map(|t| Some((t, distance(t)?)))
            .collect();
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut best = *ranked.first()?;
        'seeds: for &(seed, _) in ranked.iter().take(4) {
            let mut t = seed;
            for _ in 0..24 {
                let Some((q, tangent)) = evaluate(t) else {
                    break 'seeds;
                };
                let residual = sub(q, p);
                let rr = dot(residual, residual);
                if rr < best.1 {
                    best = (t, rr);
                }
                let length = dot(tangent, tangent);
                if length < 1e-28 || rr < 1e-26 {
                    break;
                }
                let step = -dot(residual, tangent) / length;
                let mut amount = 1.;
                let mut next = t;
                for _ in 0..10 {
                    let trial = (t + amount * step).clamp(lo, hi);
                    let Some(error) = distance(trial) else {
                        break 'seeds;
                    };
                    if error < rr {
                        next = trial;
                        break;
                    }
                    amount *= 0.5;
                }
                if (next - t).abs() < 1e-13 * (hi - lo) {
                    break;
                }
                t = next;
            }
        }
        (best.1 <= tolerance * tolerance).then_some(best.0)
    }
    pub fn transformed(&self, t: &super::RigidTransform) -> Result<Self, EvaluationError> {
        if !self.is_valid() || !t.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let mut out = self.clone();
        out.control_points = out.control_points.iter().map(|p| t.point(*p)).collect();
        Ok(out)
    }
}
impl NurbsSurface {
    /// Contract one tensor axis at a fixed parameter. This is an isocurve of
    /// the saved rational surface, not a spatial fit or an intersection curve.
    pub fn isocurve(
        &self,
        fixed_axis: usize,
        parameter: f64,
    ) -> Result<NurbsCurve, EvaluationError> {
        if fixed_axis > 1 || !parameter.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let moving = 1 - fixed_axis;
        let basis = local_basis(
            &self.knots[fixed_axis],
            self.degrees[fixed_axis],
            self.control_counts[fixed_axis],
            parameter,
        )?;
        let active: Vec<_> = basis.into_iter().filter(|(_, b)| *b != 0.).collect();
        let count = self.control_counts[moving];
        let nv = self.control_counts[1];
        let index = |i: usize, j: usize| {
            if fixed_axis == 0 {
                i * nv + j
            } else {
                j * nv + i
            }
        };
        let mut control_points = Vec::with_capacity(count);
        let mut weights = Vec::with_capacity(count);
        if active.len() == 1 && active[0].1 == 1. {
            // Preserve an exact boundary control row without a weighted
            // multiply/divide roundtrip.
            for j in 0..count {
                let k = index(active[0].0, j);
                control_points.push(self.control_points[k]);
                weights.push(self.weights[k]);
            }
        } else {
            let maximum = self.weights.iter().copied().fold(0., f64::max);
            for j in 0..count {
                let mut numerator = [0.; 3];
                let mut weight = 0.;
                for (i, b) in &active {
                    let k = index(*i, j);
                    let w = b * (self.weights[k] / maximum);
                    weight += w;
                    for (value, p) in numerator.iter_mut().zip(self.control_points[k]) {
                        *value += w * p;
                    }
                }
                if weight <= 0. || !weight.is_finite() {
                    return Err(EvaluationError::InvalidGeometry);
                }
                control_points.push(numerator.map(|v| v / weight));
                weights.push(weight);
            }
        }
        let curve = NurbsCurve {
            degree: self.degrees[moving],
            knots: self.knots[moving].clone(),
            control_points,
            weights,
        };
        curve
            .is_valid()
            .then_some(curve)
            .ok_or(EvaluationError::InvalidGeometry)
    }

    pub fn boundary_curves(&self) -> Option<Vec<(usize, f64, NurbsCurve)>> {
        if !self.is_valid() {
            return None;
        }
        let [nu, nv] = self.control_counts;
        let mut out = Vec::new();
        for axis in 0..2 {
            for end in 0..2 {
                let fixed = self.knots[axis][if end == 0 {
                    self.degrees[axis]
                } else {
                    self.control_counts[axis]
                }];
                let indices: Vec<_> = if axis == 0 {
                    let row = if end == 0 { 0 } else { nu - 1 };
                    (0..nv).map(|j| row * nv + j).collect()
                } else {
                    let col = if end == 0 { 0 } else { nv - 1 };
                    (0..nu).map(|i| i * nv + col).collect()
                };
                out.push((
                    axis,
                    fixed,
                    NurbsCurve {
                        degree: self.degrees[1 - axis],
                        knots: self.knots[1 - axis].clone(),
                        control_points: indices.iter().map(|i| self.control_points[*i]).collect(),
                        weights: indices.iter().map(|i| self.weights[*i]).collect(),
                    },
                ));
            }
        }
        Some(out)
    }
    pub fn boundary_parameters(&self, p: Vec3, tolerance: f64) -> Option<[f64; 2]> {
        let mut accepted = Vec::new();
        for (axis, fixed, c) in self.boundary_curves()? {
            if let Some(t) = c.parameter_at_point(p, tolerance) {
                let q = c.evaluate(t).ok()?;
                let mut uv = [0.; 2];
                uv[axis] = fixed;
                uv[1 - axis] = t;
                accepted.push((dot(sub(q, p), sub(q, p)), uv));
            }
        }
        accepted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let first = accepted.first()?;
        if accepted.iter().skip(1).any(|a| {
            (a.0 - first.0).abs() < 1e-18 && (0..2).any(|d| (a.1[d] - first.1[d]).abs() > 1e-6)
        }) {
            return None;
        }
        Some(first.1)
    }
    pub fn transformed(&self, t: &super::RigidTransform) -> Result<Self, EvaluationError> {
        if !self.is_valid() || !t.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let mut out = self.clone();
        out.control_points = out.control_points.iter().map(|p| t.point(*p)).collect();
        Ok(out)
    }
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let Some(n) = self.control_counts[0].checked_mul(self.control_counts[1]) else {
            return false;
        };
        n > 0
            && n <= 65536
            && self.control_points.len() == n
            && self.weights.len() == n
            && self.control_points.iter().all(|p| finite(*p))
            && self.weights.iter().all(|w| w.is_finite() && *w > 0.)
            && (0..2).all(|d| {
                let p = self.degrees[d];
                let k = &self.knots[d];
                let count = self.control_counts[d];
                (1..=16).contains(&p)
                    && count > p
                    && k.len() == count + p + 1
                    && k.iter().all(|x| x.is_finite())
                    && k.windows(2)
                        .all(|w| w[0] <= w[1] && (w[1] - w[0]).is_finite())
                    && k[p] < k[count]
                    && (k[count] - k[p]).is_finite()
                    && k.iter().take(p + 1).all(|x| *x == k[p])
                    && k.iter().rev().take(p + 1).all(|x| *x == k[count])
                    && k.windows(p + 2).all(|w| w[0] != w[p + 1])
            })
    }
    pub fn evaluate(&self, uv: [f64; 2]) -> Result<Vec3, EvaluationError> {
        Ok(self.partials(uv)?[0])
    }
    /// Exact rational tensor-product position and first partial derivatives.
    pub fn partials(&self, uv: [f64; 2]) -> Result<[Vec3; 3], EvaluationError> {
        if !self.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !uv.iter().all(|v| v.is_finite()) {
            return Err(EvaluationError::InvalidParameter);
        }
        let mut basis = Vec::with_capacity(2);
        let mut derivatives = Vec::with_capacity(2);
        for (axis, t) in uv.into_iter().enumerate() {
            let p = self.degrees[axis];
            let n = self.control_counts[axis];
            let ks = &self.knots[axis];
            let bs = local_basis(ks, p, n, t)?;
            let lower = local_basis(ks, p - 1, n, t)?;
            let low = |i: usize| lower.iter().find(|(j, _)| *j == i).map_or(0., |(_, v)| *v);
            let ds = bs
                .iter()
                .map(|(i, _)| {
                    let a = ks[i + p] - ks[*i];
                    let z = ks[i + p + 1] - ks[i + 1];
                    (if a == 0. { 0. } else { p as f64 * low(*i) / a })
                        - (if z == 0. {
                            0.
                        } else {
                            p as f64 * low(i + 1) / z
                        })
                })
                .collect::<Vec<_>>();
            basis.push(bs);
            derivatives.push(ds);
        }
        let maximum = self.weights.iter().copied().fold(0., f64::max);
        let mut sums = [[0.; 3]; 3];
        let mut weights = [0.; 3];
        for (a, (i, u)) in basis[0].iter().enumerate() {
            for (b, (j, v)) in basis[1].iter().enumerate() {
                let k = i * self.control_counts[1] + j;
                let w = self.weights[k] / maximum;
                let ws = [
                    u * v * w,
                    derivatives[0][a] * v * w,
                    u * derivatives[1][b] * w,
                ];
                for (order, value) in ws.into_iter().enumerate() {
                    weights[order] += value;
                    for (sum, coordinate) in sums[order].iter_mut().zip(self.control_points[k]) {
                        *sum += value * coordinate;
                    }
                }
            }
        }
        if weights[0] <= 0. || !weights.iter().all(|w| w.is_finite()) {
            return Err(EvaluationError::InvalidGeometry);
        }
        let p = sums[0].map(|v| v / weights[0]);
        let du = std::array::from_fn(|d| (sums[1][d] - p[d] * weights[1]) / weights[0]);
        let dv = std::array::from_fn(|d| (sums[2][d] - p[d] * weights[2]) / weights[0]);
        if [p, du, dv].into_iter().all(finite) {
            Ok([p, du, dv])
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    #[must_use]
    pub fn inversion_work(&self) -> u64 {
        if self.separable_ruled_axis().is_some() {
            return 128u64
                .saturating_mul((self.control_counts[0] as u64).saturating_add(
                    (self.degrees[0] as u64).saturating_mul(self.degrees[0] as u64),
                ))
                .saturating_add(self.knots[0].len() as u64)
                .saturating_add(self.control_points.len() as u64 * 8);
        }
        384u64.saturating_mul(
            (self.control_points.len() as u64)
                .saturating_add(
                    self.degrees
                        .iter()
                        .map(|p| (*p as u64).saturating_mul(*p as u64))
                        .sum::<u64>(),
                )
                .saturating_add(self.knots.iter().map(|k| k.len() as u64).sum::<u64>()),
        )
    }
    // A degree-one ruling and a constant affine model coordinate determine V
    // independently of U. This is an exact structural test, not a fitted axis.
    pub(crate) fn has_separable_ruled_chart(&self) -> bool {
        self.separable_ruled_axis().is_some()
    }
    fn separable_ruled_axis(&self) -> Option<(usize, f64, f64)> {
        if self.degrees[1] != 1
            || self.control_counts[1] != 2
            || self.control_counts[0].checked_mul(2)? != self.control_points.len()
            || self.weights.len() != self.control_points.len()
        {
            return None;
        }
        let w = *self.weights.first()?;
        if !w.is_finite() || w <= 0. || self.weights.iter().any(|v| *v != w) {
            return None;
        }
        let mut best = None;
        for d in 0..3 {
            let a = self.control_points[0][d];
            let z = self.control_points[1][d];
            if !(z - a).is_finite() || (z - a).abs() < 1e-12 {
                continue;
            }
            if self
                .control_points
                .chunks_exact(2)
                .all(|p| p[0][d] == a && p[1][d] == z)
                && best.is_none_or(|(_, x, y): (usize, f64, f64)| (z - a).abs() > (y - x).abs())
            {
                best = Some((d, a, z))
            }
        }
        best
    }
    /// Bounded membership witness in the full native knot domain. Failure to
    /// find a witness is not a negative-membership or injectivity proof.
    pub fn parameters_at_point(&self, p: Vec3, tolerance: f64) -> Option<[f64; 2]> {
        if !self.is_valid() || !finite(p) || !tolerance.is_finite() || tolerance <= 0. {
            return None;
        }
        let domain: [[f64; 2]; 2] = std::array::from_fn(|a| {
            [
                self.knots[a][self.degrees[a]],
                self.knots[a][self.control_counts[a]],
            ]
        });
        if (0..3).any(|d| {
            p[d] < self
                .control_points
                .iter()
                .map(|q| q[d])
                .fold(f64::INFINITY, f64::min)
                - tolerance
                || p[d]
                    > self
                        .control_points
                        .iter()
                        .map(|q| q[d])
                        .fold(f64::NEG_INFINITY, f64::max)
                        + tolerance
        }) {
            return None;
        }
        if let Some((d, a, z)) = self.separable_ruled_axis() {
            let q = ((p[d] - a) / (z - a)).clamp(0., 1.);
            let curve = NurbsCurve {
                degree: self.degrees[0],
                knots: self.knots[0].clone(),
                control_points: self
                    .control_points
                    .chunks_exact(2)
                    .map(|c| std::array::from_fn(|d| (1. - q) * c[0][d] + q * c[1][d]))
                    .collect(),
                weights: vec![1.; self.control_counts[0]],
            };
            let u = curve.parameter_at_point(p, tolerance)?;
            let uv = [u, domain[1][0] + q * (domain[1][1] - domain[1][0])];
            let [point, du, dv] = self.partials(uv).ok()?;
            let normal = super::cross(du, dv);
            return (dot(sub(point, p), sub(point, p)) <= tolerance * tolerance
                && dot(normal, normal) > 1e-24 * dot(du, du) * dot(dv, dv))
            .then_some(uv);
        }
        let calls = std::cell::Cell::new(0usize);
        let jet = |uv| {
            if calls.get() >= 384 {
                return None;
            }
            calls.set(calls.get() + 1);
            self.partials(uv).ok()
        };
        let mut ranked = Vec::new();
        for i in 0..=8 {
            for j in 0..=8 {
                let uv = std::array::from_fn(|a| {
                    let step = if a == 0 { i } else { j };
                    if step == 8 {
                        domain[a][1]
                    } else {
                        domain[a][0] + (domain[a][1] - domain[a][0]) * step as f64 / 8.
                    }
                });
                let state = jet(uv)?;
                let normal = super::cross(state[1], state[2]);
                if dot(normal, normal) <= 1e-24 * dot(state[1], state[1]) * dot(state[2], state[2])
                {
                    continue;
                }
                let v = sub(state[0], p);
                ranked.push((dot(v, v), uv));
            }
        }
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best = *ranked.first()?;
        'seeds: for &(_, seed) in ranked.iter().take(4) {
            let mut uv = seed;
            for _ in 0..32 {
                let Some([q, u, v]) = jet(uv) else {
                    break 'seeds;
                };
                let r = sub(q, p);
                let rr = dot(r, r);
                let a = dot(u, u);
                let b = dot(u, v);
                let c = dot(v, v);
                let det = a * c - b * b;
                if det <= 1e-24 * (a * c).max(1e-300) {
                    break;
                }
                if rr < best.0 {
                    best = (rr, uv)
                }
                if rr < 1e-26 {
                    break;
                }
                let gu = dot(r, u);
                let gv = dot(r, v);
                let step = [(-c * gu + b * gv) / det, (b * gu - a * gv) / det];
                let mut amount = 1.;
                let mut next = uv;
                for _ in 0..10 {
                    let trial = std::array::from_fn(|d| {
                        (uv[d] + amount * step[d]).clamp(domain[d][0], domain[d][1])
                    });
                    let Some(state) = jet(trial) else {
                        break 'seeds;
                    };
                    let d = sub(state[0], p);
                    let error = dot(d, d);
                    let normal = super::cross(state[1], state[2]);
                    if error < best.0
                        && dot(normal, normal)
                            > 1e-24 * dot(state[1], state[1]) * dot(state[2], state[2])
                    {
                        best = (error, trial)
                    }
                    if error < rr {
                        next = trial;
                        break;
                    }
                    amount *= 0.5;
                }
                if (0..2).all(|a| (next[a] - uv[a]).abs() <= 1e-12 * (domain[a][1] - domain[a][0]))
                {
                    break;
                }
                uv = next;
            }
        }
        (best.0 <= tolerance * tolerance).then_some(best.1)
    }
}
fn local_basis(
    knots: &[f64],
    degree: usize,
    count: usize,
    t: f64,
) -> Result<Vec<(usize, f64)>, EvaluationError> {
    if t < knots[degree] || t > knots[count] {
        return Err(EvaluationError::OutsideDomain);
    }
    let span = if t == knots[count] {
        count - 1
    } else {
        knots
            .partition_point(|x| *x <= t)
            .saturating_sub(1)
            .min(count - 1)
    };
    let mut values = vec![0.; degree + 1];
    let mut left = vec![0.; degree + 1];
    let mut right = vec![0.; degree + 1];
    values[0] = 1.;
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let term = if denominator == 0. {
                0.
            } else {
                values[r] / denominator
            };
            values[r] = saved + right[r + 1] * term;
            saved = left[j - r] * term;
        }
        values[j] = saved;
    }
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(i, v)| (span - degree + i, v))
        .collect())
}
