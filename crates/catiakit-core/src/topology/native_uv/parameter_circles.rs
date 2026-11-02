//! Closed class-19 parameter circles. The final scalar is retained without
//! assigning it a trim, phase, tolerance or native evaluation meaning.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeParameterCircleFields {
    pub allocation_operand: NativeAllocationReference,
    pub center: [f64; 2],
    pub radius: f64,
    pub parameter_range: [f64; 2],
    pub control: u8,
    pub retained_parameter: f64,
}

impl PartialEq for NativeParameterCircleFields {
    fn eq(&self, other: &Self) -> bool {
        self.allocation_operand == other.allocation_operand
            && self.control == other.control
            && self
                .center
                .into_iter()
                .chain([self.radius])
                .chain(self.parameter_range)
                .chain([self.retained_parameter])
                .map(f64::to_bits)
                .eq(other
                    .center
                    .into_iter()
                    .chain([other.radius])
                    .chain(other.parameter_range)
                    .chain([other.retained_parameter])
                    .map(f64::to_bits))
    }
}

impl NativeParameterCircleFields {
    /// Evaluate the declared arc in its own length-scaled parameter gauge.
    /// No correspondence, carrier lift or endpoint correction is implied.
    pub fn evaluate(&self, t: f64) -> std::result::Result<[f64; 2], EvaluationError> {
        self.evaluate_on_interval(t, self.parameter_range)
    }

    // A qualified curve-point trim can select another arc of the same
    // analytic circle. This does not extend a NURBS support's knot domain.
    pub(super) fn evaluate_on_interval(
        &self,
        t: f64,
        range: [f64; 2],
    ) -> std::result::Result<[f64; 2], EvaluationError> {
        if !self.valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !t.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        if !range
            .iter()
            .all(|v| v.is_finite() && (v / self.radius).is_finite())
            || range[0] >= range[1]
            || !(range[1] - range[0]).is_finite()
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        if t < range[0] || t > range[1] {
            return Err(EvaluationError::OutsideDomain);
        }
        let angle = t / self.radius;
        let p = [
            self.center[0] + self.radius * angle.cos(),
            self.center[1] + self.radius * angle.sin(),
        ];
        p.iter()
            .all(|v| v.is_finite())
            .then_some(p)
            .ok_or(EvaluationError::InvalidGeometry)
    }

    fn valid(&self) -> bool {
        self.control == 1
            && self.radius.is_finite()
            && self.radius > 0.
            && self.center.iter().all(|v| v.is_finite())
            && self.retained_parameter.is_finite()
            && self
                .parameter_range
                .iter()
                .all(|v| v.is_finite() && (v / self.radius).is_finite())
            && self.parameter_range[0] < self.parameter_range[1]
            && (self.parameter_range[1] - self.parameter_range[0]).is_finite()
    }
}

pub(super) fn parse(bytes: &[u8]) -> Option<NativeParameterCircleFields> {
    let mut c = Cursor::new(bytes);
    let value = NativeParameterCircleFields {
        allocation_operand: parameter_maps::reference(&mut c)?,
        center: [c.number()?, c.number()?],
        radius: c.number()?,
        parameter_range: [c.number()?, c.number()?],
        control: c.byte()?,
        retained_parameter: c.number()?,
    };
    (c.done() && value.valid()).then_some(value)
}
