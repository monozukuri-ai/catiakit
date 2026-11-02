//! A view of two source-linked UV images. No single healed curve is inferred.
use super::*;

/// Two images at a source-qualified physical endpoint. The first image uses
/// the map coordinate; the second uses its own saved UV endpoint coordinate.
/// They need not be a saved map pair or the same spatial point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeEndpointImages {
    pub edge_tag: u32,
    pub vertex_tag: u32,
    pub allocation_tag: u32,
    pub uv_tags: [u32; 2],
    pub map_target_parameter: f64,
    /// UV point coordinates in operand order, independently of reference order.
    pub uv_point_parameters: [f64; 2],
    pub points: [[f64; 3]; 2],
    pub distance: f64,
}

#[derive(Debug)]
pub struct NativePairedUvEdge<'a> {
    target: &'a NativeBoundUvUse,
    source: &'a NativeBoundUvUse,
    map: &'a NativeParameterMap,
    native: &'a NativeUvModel,
}

impl<'a> NativePairedUvEdge<'a> {
    /// The first operand defines the saved map coordinate gauge. This is not
    /// a claim about CATIA's choice of a unique common spatial curve.
    #[must_use]
    pub fn coordinate_reference(&self) -> &'a NativeBoundUvUse {
        self.target
    }
    #[must_use]
    pub fn second_representation(&self) -> &'a NativeBoundUvUse {
        self.source
    }
    #[must_use]
    pub fn parameter_map(&self) -> &'a NativeParameterMap {
        self.map
    }
    /// Evaluate a saved endpoint without substituting a sampled-map pair or
    /// snapping to a shared point. Replay the complete binding before use.
    pub fn endpoint_images(
        &self,
        vertex_tag: u32,
    ) -> std::result::Result<NativeEndpointImages, EvaluationError> {
        let a = self
            .target
            .edge_trim
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let z = self
            .source
            .edge_trim
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if a.vertex_tags != z.vertex_tags || a.vertex_tags[0] == a.vertex_tags[1] {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let i = a
            .vertex_tags
            .iter()
            .position(|&v| v == vertex_tag)
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let p = &a.endpoints[i];
        if p != &z.endpoints[i] || p.references[0] != self.target.representation_tag {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let uv_tags = [self.target.uv_tag, self.source.uv_tag];
        let uv_point_parameters = if p.references[1..] == uv_tags {
            p.uv_parameters
        } else if p.references[1..] == [uv_tags[1], uv_tags[0]] {
            [p.uv_parameters[1], p.uv_parameters[0]]
        } else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        if !uv_point_parameters.iter().all(|t| t.is_finite())
            || uv_point_parameters[0].to_bits() != a.parameters[i].to_bits()
            || uv_point_parameters[1].to_bits() != z.parameters[i].to_bits()
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        let coordinate = p
            .map_coordinate
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if coordinate.target_uv_tag != self.target.uv_tag {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let points = [
            self.target
                .lift_map_target(coordinate.parameter, self.native)?,
            self.source.lift(uv_point_parameters[1], self.native)?,
        ];
        let distance = points[0]
            .iter()
            .zip(points[1])
            .map(|(a, z)| (a - z) * (a - z))
            .sum::<f64>()
            .sqrt();
        if !distance.is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(NativeEndpointImages {
            edge_tag: self.target.edge_tag,
            vertex_tag,
            allocation_tag: p.allocation_tag,
            uv_tags,
            map_target_parameter: coordinate.parameter,
            uv_point_parameters,
            points,
            distance,
        })
    }
    pub fn query_at_source(
        &self,
        source_parameter: f64,
    ) -> std::result::Result<NativeParameterQuery, EvaluationError> {
        self.map.fields.query_at_source(source_parameter)
    }
    /// Query in the first map gauge. This is not CATIA's global parameter.
    pub fn query_at_target(
        &self,
        target_parameter: f64,
    ) -> std::result::Result<NativeTargetParameterQuery, EvaluationError> {
        self.map.fields.query_at_target(target_parameter)
    }
    /// Observe a defined correspondence on the saved map interval, including
    /// qualified target line extensions beyond the UV point interval.
    pub fn gap_at_source(
        &self,
        source_parameter: f64,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        let NativeParameterQuery::Exact {
            target_parameter,
            stored_sample,
            ..
        } = self.query_at_source(source_parameter)?
        else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        self.gap_at_pair(target_parameter, source_parameter, stored_sample)
    }

    /// Observe only an exact saved or checked affine correspondence. A target
    /// bracket or uncovered tail does not choose a second-operand parameter.
    pub fn gap_at_target(
        &self,
        target_parameter: f64,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        let NativeTargetParameterQuery::Exact {
            source_parameter,
            stored_sample,
            ..
        } = self.query_at_target(target_parameter)?
        else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        self.gap_at_pair(target_parameter, source_parameter, stored_sample)
    }

    fn gap_at_pair(
        &self,
        target_parameter: f64,
        source_parameter: f64,
        stored_sample: bool,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        let points = [
            self.target.lift_map_target(target_parameter, self.native)?,
            self.source.lift(source_parameter, self.native)?,
        ];
        let squared = points[0]
            .iter()
            .zip(points[1])
            .map(|(a, z)| (a - z) * (a - z))
            .sum::<f64>();
        let distance = squared.sqrt();
        if !distance.is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(NativeEdgeGapSample {
            edge_tag: self.target.edge_tag,
            representation_tag: self.target.representation_tag,
            uv_tags: [self.target.uv_tag, self.source.uv_tag],
            parameters: [target_parameter, source_parameter],
            points,
            distance,
            stored_sample,
        })
    }
}

impl NativeBoundaryUv {
    /// Obtain a paired representation after source validation of the binding.
    /// Missing, duplicate, or ambiguous operands never choose a side silently.
    pub fn paired_edge<'a>(
        &'a self,
        edge_tag: u32,
        native: &'a NativeUvModel,
    ) -> std::result::Result<NativePairedUvEdge<'a>, EvaluationError> {
        let mut uses = self.uses.iter().filter(|u| u.edge_tag == edge_tag);
        let a = uses.next().ok_or(EvaluationError::UnresolvedGeometry)?;
        let z = uses.next().ok_or(EvaluationError::UnresolvedGeometry)?;
        if uses.next().is_some()
            || a.representation_tag != z.representation_tag
            || a.parameter_map != z.parameter_map
            || a.uv_tag == z.uv_tag
            || !matches!((a.map_operand, z.map_operand), (0, 1) | (1, 0))
        {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let (target, source) = if a.map_operand == 0 { (a, z) } else { (z, a) };
        let mut maps = native
            .parameter_maps
            .iter()
            .filter(|m| m.id == target.parameter_map && m.run == self.run);
        let map = maps.next().ok_or(EvaluationError::UnresolvedGeometry)?;
        if maps.next().is_some() {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        Ok(NativePairedUvEdge {
            target,
            source,
            map,
            native,
        })
    }

    /// A pointwise map observation, distinct from the UV-point-interval API.
    pub fn map_gap_at_source(
        &self,
        edge_tag: u32,
        source: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        self.paired_edge(edge_tag, native)?.gap_at_source(source)
    }

    /// A first-map-coordinate observation, with the same source validation
    /// requirement and unresolved interpolation as the source-side API.
    pub fn map_gap_at_target(
        &self,
        edge_tag: u32,
        target: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        self.paired_edge(edge_tag, native)?.gap_at_target(target)
    }
}
