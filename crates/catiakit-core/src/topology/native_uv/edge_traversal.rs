//! Edge terminal sense qualified against vertex-ordered first-map coordinates.
//! This is not CATIA Eval orientation or a decoded common spatial curve.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeEdgeMapTraversal {
    pub edge_tag: u32,
    pub representation_tag: u32,
    pub vertex_tags: [u32; 2],
    pub point_tags: [u32; 2],
    pub target_uv_tag: u32,
    pub control: u8,
    /// First-map coordinate increases along the raw edge's vertex order.
    pub map_forward: bool,
    pub map_target_parameters: [f64; 2],
    pub source: StreamSpan,
}

impl NativeEdgeMapTraversal {
    fn check(&self) -> std::result::Result<(), EvaluationError> {
        let [a, z] = self.map_target_parameters;
        if !a.is_finite()
            || !z.is_finite()
            || a == z
            || !(z - a).is_finite()
            || !matches!(
                (self.control, self.map_forward),
                (0x21, true) | (0x22, false)
            )
            || self.map_forward != (a < z)
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(())
    }
    /// A scalar in the saved first-operand gauge. The input fraction follows
    /// raw edge vertices, independently of a face's loop traversal.
    pub fn map_target_at_fraction(
        &self,
        fraction: f64,
    ) -> std::result::Result<f64, EvaluationError> {
        self.check()?;
        if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
            return Err(EvaluationError::OutsideDomain);
        }
        let result = interval_parameter(self.map_target_parameters, fraction);
        let [a, z] = self.map_target_parameters;
        if !result.is_finite() || result < a.min(z) || result > a.max(z) {
            return Err(EvaluationError::OutsideDomain);
        }
        Ok(result)
    }
    pub fn fraction_at_map_target(
        &self,
        parameter: f64,
    ) -> std::result::Result<f64, EvaluationError> {
        self.check()?;
        let [a, z] = self.map_target_parameters;
        if !parameter.is_finite() || parameter < a.min(z) || parameter > a.max(z) {
            return Err(EvaluationError::OutsideDomain);
        }
        let result = if parameter == a {
            0.
        } else if parameter == z {
            1.
        } else {
            (parameter - a) / (z - a)
        };
        if !result.is_finite() || !(0. ..=1.).contains(&result) {
            return Err(EvaluationError::OutsideDomain);
        }
        Ok(result)
    }
}

fn definition(binding: &NativeBoundaryUv, use_: &NativeBoundUvUse) -> Option<([u32; 5], u8)> {
    let source = &use_.edge_source;
    // A five-reference terminal production is small. Do not allocate or scan
    // an unbounded public hex field to answer a single-edge query.
    if source.length > 64
        || !inside(&binding.factory_source, source)
        || binding.factory_source.length.checked_mul(2)? != binding.raw_factory_hex.len() as u64
    {
        return None;
    }
    let a = usize::try_from(
        source
            .logical_offset
            .checked_sub(binding.factory_source.logical_offset)?,
    )
    .ok()?
    .checked_mul(2)?;
    let z = a.checked_add(usize::try_from(source.length).ok()?.checked_mul(2)?)?;
    let hex = binding.raw_factory_hex.get(a..z)?;
    let limits = ParseLimits::default();
    let mut budget = Budget {
        limits: &limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    let bytes = raw(hex, source, &mut budget).ok()?;
    let f = frame(&bytes, 0)?;
    if f.class != 0x5e || f.flag != 3 || f.token == 0 || f.end != bytes.len() {
        return None;
    }
    let mut cursor = Cursor::new(&bytes[f.payload..f.end]);
    let mut refs = [0; 5];
    for tag in &mut refs {
        let r = parameter_maps::reference(&mut cursor)?;
        *tag = if r.encoding == NativeAllocationEncoding::OwnedChild {
            use_.edge_tag.checked_add(r.value)?.checked_add(1)?
        } else {
            use_.edge_tag.checked_sub(r.value)?
        };
        if *tag == 0 {
            return None;
        }
    }
    let control = cursor.byte()?;
    (cursor.done() && matches!(control, 0x21 | 0x22)).then_some((refs, control))
}

impl NativeBoundaryUv {
    /// Query after validate_native_boundary_uv/validate_topology. Replay the
    /// edge's own bounded source frame and cross-check its first-map endpoint
    /// order. Unsupported or conflicting layouts remain unresolved. No stored
    /// binding revision is upgraded and no coedge is admitted to strict B-rep.
    pub fn edge_map_traversal(
        &self,
        edge_tag: u32,
    ) -> std::result::Result<NativeEdgeMapTraversal, EvaluationError> {
        let mut targets = self
            .uses
            .iter()
            .filter(|u| u.edge_tag == edge_tag && u.map_operand == 0);
        let use_ = targets.next().ok_or(EvaluationError::UnresolvedGeometry)?;
        if targets.next().is_some() {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let trim = use_
            .edge_trim
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let coordinates = trim
            .map_target_parameters()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if trim.vertex_tags[0] == trim.vertex_tags[1]
            || trim.endpoints.iter().any(|p| {
                p.references[0] != use_.representation_tag
                    || p.references[1] == p.references[2]
                    || !p.references[1..].contains(&use_.uv_tag)
                    || p.map_coordinate
                        .as_ref()
                        .is_none_or(|m| m.target_uv_tag != use_.uv_tag)
            })
            || self
                .uses
                .iter()
                .filter(|u| u.edge_tag == edge_tag)
                .any(|u| {
                    u.representation_tag != use_.representation_tag
                        || u.edge_source != use_.edge_source
                        || u.edge_trim.as_ref().is_some_and(|t| {
                            t.vertex_tags != trim.vertex_tags || t.endpoints != trim.endpoints
                        })
                })
        {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let (refs, control) = definition(self, use_).ok_or(EvaluationError::UnresolvedGeometry)?;
        let point_tags = trim.endpoints.each_ref().map(|p| p.allocation_tag);
        if refs
            != [
                use_.representation_tag,
                trim.vertex_tags[0],
                trim.vertex_tags[1],
                point_tags[0],
                point_tags[1],
            ]
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        let result = NativeEdgeMapTraversal {
            edge_tag,
            representation_tag: refs[0],
            vertex_tags: trim.vertex_tags,
            point_tags,
            target_uv_tag: use_.uv_tag,
            control,
            map_forward: control == 0x21,
            map_target_parameters: coordinates,
            source: use_.edge_source.clone(),
        };
        result.check()?;
        Ok(result)
    }
}
