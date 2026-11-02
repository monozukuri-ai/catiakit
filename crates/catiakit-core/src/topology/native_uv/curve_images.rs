//! A 3D image of one source UV use. It need not coincide with the other side
//! of the topological edge and is never promoted to a healed common curve.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeNurbsCurveImage {
    pub face_tag: u32,
    pub edge_tag: u32,
    pub uv_tag: u32,
    pub loop_tag: u32,
    pub loop_slot: u32,
    pub carrier: String,
    pub fixed_axis: usize,
    pub fixed_parameter: f64,
    pub parameter_offset: f64,
    pub parameter_scale: f64,
    pub parameter_range: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_parameters: Option<[f64; 2]>,
    pub curve: NurbsCurve,
    pub surface_source: StreamSpan,
    pub uv_source: StreamSpan,
}

impl NativeNurbsCurveImage {
    /// Replay the contraction against a UV use whose boundary binding has
    /// already been source-validated. Cached image fields are not proof by
    /// themselves; this does not check another face's spatial incidence.
    pub fn validate_against(
        &self,
        use_: &NativeBoundUvUse,
        native: &NativeUvModel,
    ) -> std::result::Result<(), EvaluationError> {
        if use_.nurbs_curve_image(native)? == *self {
            Ok(())
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    fn mapped_parameter(&self, t: f64) -> std::result::Result<f64, EvaluationError> {
        let [lo, hi] = self.parameter_range;
        if !lo.is_finite()
            || !hi.is_finite()
            || lo >= hi
            || !(hi - lo).is_finite()
            || !self.parameter_scale.is_finite()
            || self.parameter_scale == 0.
            || !self.parameter_offset.is_finite()
            || self.fixed_axis > 1
            || !self.fixed_parameter.is_finite()
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !t.is_finite() || t < lo || t > hi {
            return Err(EvaluationError::OutsideDomain);
        }
        let s = self.parameter_offset + self.parameter_scale * t;
        s.is_finite()
            .then_some(s)
            .ok_or(EvaluationError::InvalidParameter)
    }
    pub fn evaluate(&self, t: f64) -> std::result::Result<Vec3, EvaluationError> {
        self.curve.evaluate(self.mapped_parameter(t)?)
    }
    pub fn derivative(&self, t: f64) -> std::result::Result<Vec3, EvaluationError> {
        let d = self.curve.derivative(self.mapped_parameter(t)?)?;
        let d = d.map(|v| v * self.parameter_scale);
        finite(d)
            .then_some(d)
            .ok_or(EvaluationError::InvalidGeometry)
    }
    /// Follow the raw edge's start-to-end direction, independently of face loop
    /// direction. Endpoints retain strict carrier-domain failures.
    pub fn evaluate_on_edge(&self, fraction: f64) -> std::result::Result<Vec3, EvaluationError> {
        let [a, z] = self
            .edge_parameters
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if !a.is_finite() || !z.is_finite() || a == z || !(z - a).is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
            return Err(EvaluationError::OutsideDomain);
        }
        self.evaluate(if fraction == 0. {
            a
        } else if fraction == 1. {
            z
        } else {
            a + (z - a) * fraction
        })
    }
}

impl NativeBoundUvUse {
    /// Derive an isoparametric 3D image from a validated saved NURBS/line use.
    /// This contracts source control rows, never fits a sampled correspondence.
    pub fn nurbs_curve_image(
        &self,
        native: &NativeUvModel,
    ) -> std::result::Result<NativeNurbsCurveImage, EvaluationError> {
        let NativeBoundSupport::Nurbs { carrier } = &self.support else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        let NativeBoundUvGeometry::Line { fields } = &self.geometry else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        use NativeParameterLineGeometry::*;
        let (fixed_axis, fixed, offset, scale) = match &fields.geometry {
            ConstantU { u, .. } => (0, *u, 0., 1.),
            ConstantV { v, .. } => (1, *v, 0., 1.),
            UndirectedAxis { origin, axis, .. } if *axis < 2 => {
                (1 - *axis, origin[1 - *axis], origin[*axis], -1.)
            }
            Directed {
                origin, direction, ..
            } if direction[0] == 0. && direction[1] != 0. => {
                (0, origin[0], origin[1], direction[1])
            }
            Directed {
                origin, direction, ..
            } if direction[1] == 0. && direction[0] != 0. => {
                (1, origin[1], origin[0], direction[0])
            }
            _ => return Err(EvaluationError::UnresolvedGeometry),
        };
        if !offset.is_finite() || !scale.is_finite() || scale == 0. {
            return Err(EvaluationError::InvalidGeometry);
        }
        let surface = native
            .nurbs_carriers
            .iter()
            .find(|c| c.id == *carrier)
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let curve = surface.geometry.isocurve(fixed_axis, fixed)?;
        let image = NativeNurbsCurveImage {
            face_tag: self.face_tag,
            edge_tag: self.edge_tag,
            uv_tag: self.uv_tag,
            loop_tag: self.loop_tag,
            loop_slot: self.loop_slot,
            carrier: carrier.clone(),
            fixed_axis,
            fixed_parameter: fixed,
            parameter_offset: offset,
            parameter_scale: scale,
            parameter_range: self.parameter_range,
            edge_parameters: self.edge_trim.as_ref().map(|t| t.parameters),
            curve,
            surface_source: surface.source.clone(),
            uv_source: self.uv_source.clone(),
        };
        // Check the public scalar domains even if a caller constructs a forged
        // use; extraction itself does not silently repair its endpoints.
        image.mapped_parameter(image.parameter_range[0])?;
        Ok(image)
    }
}
