//! Current UV programs reached through a qualified allocation registry. Geometry
//! is evaluated in its own stored chart; no shared-edge gap bound is inferred.
use super::*;
#[path = "edge_trims.rs"]
mod edge_trims;
pub use edge_trims::{
    NativeMapCoordinate, NativeMapCoordinateEncoding, NativePlaneSupport, NativeUvEdgeTrim,
    NativeUvEndpoint,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeBoundSupport {
    Analytic { carrier: String },
    Nurbs { carrier: String },
    Plane { plane: NativePlaneSupport },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeBoundUvGeometry {
    Line { fields: NativeParameterLineFields },
    Circle { fields: NativeParameterCircleFields },
    Jet { pcurve: String },
}

/// Source-qualified native UV to neutral analytic chart conversion. This is
/// separate from the legacy unbound carrier's retained chart interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeUvSupportChart {
    pub scale: [f64; 2],
    pub offset: [f64; 2],
}

/// Source loop sense, admitted only when the entire loop closes by native
/// vertex identity. Bit 1 of `flags` is retained without a semantic claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeUvUseOrientation {
    pub control: u8,
    pub flags: u8,
    pub face_control: u8,
    pub edge_forward: bool,
    pub uv_forward: bool,
    pub start_vertex_tag: u32,
    pub end_vertex_tag: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeBoundUvUse {
    pub face_ordinal: u32,
    pub edge_ordinal: u32,
    pub face_tag: u32,
    pub edge_tag: u32,
    pub loop_tag: u32,
    pub loop_slot: u32,
    pub uv_tag: u32,
    pub support_tag: u32,
    pub representation_tag: u32,
    pub parameter_map: String,
    /// Zero selects the map's first operand/target range; one selects its
    /// second operand/source range. This is not a traversal orientation.
    pub map_operand: u8,
    pub support: NativeBoundSupport,
    /// Conversion qualified in this allocation context. Revision 0/1 JSON
    /// retains its earlier unbound chart interpretation when this is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_chart: Option<NativeUvSupportChart>,
    pub geometry: NativeBoundUvGeometry,
    /// Ascending interval from qualified endpoint coordinates when present,
    /// otherwise the referenced map. This is not a spatial error bound.
    pub parameter_range: [f64; 2],
    pub program_range: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<NativeUvUseOrientation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_trim: Option<NativeUvEdgeTrim>,
    pub uv_source: StreamSpan,
    pub face_source: StreamSpan,
    pub loop_source: StreamSpan,
    pub edge_source: StreamSpan,
}

impl NativeBoundUvUse {
    fn map_target_coordinates(&self) -> std::result::Result<[f64; 2], EvaluationError> {
        let trim = self
            .edge_trim
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if self.map_operand != 0
            || trim.endpoints.iter().any(|p| {
                p.map_coordinate
                    .as_ref()
                    .is_none_or(|m| m.target_uv_tag != self.uv_tag)
            })
        {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let parameters = trim
            .map_target_parameters()
            .ok_or(EvaluationError::InvalidGeometry)?;
        if parameters[0] == parameters[1] || !(parameters[1] - parameters[0]).is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(parameters)
    }

    /// Follow the map target coordinate from the raw edge's start to end.
    /// This does not interpolate a correspondence to the second operand.
    pub fn map_target_parameter_on_edge(
        &self,
        fraction: f64,
    ) -> std::result::Result<f64, EvaluationError> {
        let [a, z] = self.map_target_coordinates()?;
        if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
            return Err(EvaluationError::OutsideDomain);
        }
        Ok(if fraction == 0. {
            a
        } else if fraction == 1. {
            z
        } else {
            a + (z - a) * fraction
        })
    }

    /// Evaluate the first operand on the separately saved map interval. An analytic line or circle
    /// can use these qualified coordinates; a jet keeps its own domain.
    /// The UV point interval and the second operand remain independent.
    pub fn evaluate_map_target(
        &self,
        t: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<[f64; 2], EvaluationError> {
        let [a, z] = self.map_target_coordinates()?;
        self.evaluate_interval(
            t,
            native,
            [a.min(z), a.max(z)],
            matches!(
                self.geometry,
                NativeBoundUvGeometry::Line { .. } | NativeBoundUvGeometry::Circle { .. }
            ),
        )
    }

    /// One source surface image, with the support's strict domain preserved.
    pub fn lift_map_target(
        &self,
        t: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<Vec3, EvaluationError> {
        self.lift_uv(self.evaluate_map_target(t, native)?, native)
    }

    /// Native UV parameter at a normalized position along the loop. Qualified
    /// source endpoints select the interval when available. This does not
    /// assert incidence within a spatial tolerance or a healed edge trim.
    pub fn parameter_in_loop(&self, fraction: f64) -> std::result::Result<f64, EvaluationError> {
        let o = self
            .orientation
            .as_ref()
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        if !fraction.is_finite() || !(0. ..=1.).contains(&fraction) {
            return Err(EvaluationError::OutsideDomain);
        }
        let [lo, hi] = self.parameter_range;
        if !lo.is_finite() || !hi.is_finite() || lo >= hi || !(hi - lo).is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let q = if o.uv_forward {
            fraction
        } else {
            1. - fraction
        };
        Ok(if q == 0. {
            lo
        } else if q == 1. {
            hi
        } else {
            lo + (hi - lo) * q
        })
    }

    pub fn evaluate_in_loop(
        &self,
        fraction: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<[f64; 2], EvaluationError> {
        self.evaluate(self.parameter_in_loop(fraction)?, native)
    }

    pub fn lift_in_loop(
        &self,
        fraction: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<Vec3, EvaluationError> {
        self.lift(self.parameter_in_loop(fraction)?, native)
    }
    /// Evaluate the saved program. Implicit positive-axis omission controls in
    /// the admitted class-18 production encode a negative unit direction.
    /// This does not orient the program along a coedge or interpolate a map.
    pub fn evaluate(
        &self,
        t: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<[f64; 2], EvaluationError> {
        let extend_program = matches!(
            self.geometry,
            NativeBoundUvGeometry::Line { .. } | NativeBoundUvGeometry::Circle { .. }
        ) && self.edge_trim.as_ref().is_some_and(|trim| {
            let [a, z] = trim.parameters;
            a.is_finite() && z.is_finite() && a != z && self.parameter_range == [a.min(z), a.max(z)]
        });
        self.evaluate_interval(t, native, self.parameter_range, extend_program)
    }

    fn evaluate_interval(
        &self,
        t: f64,
        native: &NativeUvModel,
        range: [f64; 2],
        extend_program: bool,
    ) -> std::result::Result<[f64; 2], EvaluationError> {
        if [range, self.program_range]
            .iter()
            .any(|r| !r.iter().all(|v| v.is_finite()) || r[0] >= r[1])
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !t.is_finite() || t < range[0] || t > range[1] {
            return Err(EvaluationError::OutsideDomain);
        }
        let rounding = 64.
            * f64::EPSILON
            * t.abs()
                .max(self.program_range[0].abs())
                .max(self.program_range[1].abs())
                .max(1.);
        if matches!(self.geometry, NativeBoundUvGeometry::Circle { .. })
            && !extend_program
            && (t < self.program_range[0] || t > self.program_range[1])
        {
            return Err(EvaluationError::OutsideDomain);
        }
        if !extend_program
            && (t < self.program_range[0] - rounding || t > self.program_range[1] + rounding)
        {
            return Err(EvaluationError::OutsideDomain);
        }
        let t = if extend_program {
            t
        } else {
            t.clamp(self.program_range[0], self.program_range[1])
        };
        match &self.geometry {
            NativeBoundUvGeometry::Line { fields } if extend_program => {
                use NativeParameterLineGeometry::*;
                let p = match &fields.geometry {
                    ConstantU { u, .. } => [*u, t],
                    ConstantV { v, .. } => [t, *v],
                    Directed {
                        origin, direction, ..
                    } => std::array::from_fn(|i| origin[i] + t * direction[i]),
                    UndirectedAxis { origin, axis, .. } if *axis < 2 => {
                        let mut p = *origin;
                        p[*axis] -= t;
                        p
                    }
                    _ => return Err(EvaluationError::InvalidGeometry),
                };
                p.iter()
                    .all(|v| v.is_finite())
                    .then_some(p)
                    .ok_or(EvaluationError::InvalidGeometry)
            }
            NativeBoundUvGeometry::Line { fields } => fields
                .geometry
                .images_at(t)
                .into_iter()
                .next()
                .ok_or(EvaluationError::InvalidParameter),
            NativeBoundUvGeometry::Jet { pcurve } => native
                .pcurves
                .iter()
                .find(|p| p.id == *pcurve)
                .ok_or(EvaluationError::UnresolvedGeometry)?
                .jet
                .evaluate(t),
            NativeBoundUvGeometry::Circle { fields } if extend_program => {
                fields.evaluate_on_interval(t, range)
            }
            NativeBoundUvGeometry::Circle { fields } => fields.evaluate(t),
        }
    }

    /// Lift through the explicitly referenced native support, in native length
    /// units. The result can differ from another representation of this edge.
    pub fn lift(
        &self,
        t: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<Vec3, EvaluationError> {
        self.lift_uv(self.evaluate(t, native)?, native)
    }

    fn lift_uv(
        &self,
        uv: [f64; 2],
        native: &NativeUvModel,
    ) -> std::result::Result<Vec3, EvaluationError> {
        match &self.support {
            NativeBoundSupport::Analytic { carrier } => {
                let c = native
                    .carriers
                    .iter()
                    .find(|c| c.id == *carrier)
                    .ok_or(EvaluationError::UnresolvedGeometry)?;
                let (scale, offset) = self
                    .support_chart
                    .as_ref()
                    .map_or((c.uv_scale, c.uv_offset), |chart| {
                        (chart.scale, chart.offset)
                    });
                if scale.iter().any(|v| !v.is_finite() || *v == 0.)
                    || offset.iter().any(|v| !v.is_finite())
                {
                    return Err(EvaluationError::InvalidGeometry);
                }
                c.geometry
                    .evaluate(std::array::from_fn(|i| uv[i] * scale[i] + offset[i]))
            }
            NativeBoundSupport::Nurbs { carrier } => native
                .nurbs_carriers
                .iter()
                .find(|c| c.id == *carrier)
                .ok_or(EvaluationError::UnresolvedGeometry)?
                .geometry
                .evaluate(uv),
            NativeBoundSupport::Plane { plane } => SurfaceGeometry::Plane {
                frame: plane.frame.clone(),
            }
            .evaluate(uv),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeBoundaryUv {
    /// 0 replays the original map-interval binding grammar; 1 also decodes
    /// source curve-point coordinates and qualified plane frames.
    #[serde(default)]
    pub revision: u8,
    pub population_id: u32,
    pub run: u32,
    pub factory_source: StreamSpan,
    pub raw_factory_hex: String,
    pub qualified_faces: u32,
    pub qualified_edges: u32,
    pub qualified_vertices: u32,
    pub uses: Vec<NativeBoundUvUse>,
}

/// A pointwise distance between two saved surface representations of one edge.
/// Neither this observation nor a saved fit word certifies a global gap bound
/// or defines a replacement common 3D curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeEdgeGapSample {
    pub edge_tag: u32,
    pub representation_tag: u32,
    /// First/target and second/source operands, respectively.
    pub uv_tags: [u32; 2],
    pub parameters: [f64; 2],
    pub points: [Vec3; 2],
    pub distance: f64,
    pub stored_sample: bool,
}

impl NativeBoundaryUv {
    /// Use an exact identity/affine relation or an explicitly stored sample.
    /// Sampled maps are never interpolated. Both programs and supports must
    /// admit the parameters; ambiguous/seam incidences remain unresolved.
    /// A representation interval can exceed the current edge's trim. This
    /// method observes that representation, and does not decode its endpoint
    /// trim or certify proximity to an independently defined 3D edge curve.
    pub fn edge_gap_at(
        &self,
        edge_tag: u32,
        source_parameter: f64,
        native: &NativeUvModel,
    ) -> std::result::Result<NativeEdgeGapSample, EvaluationError> {
        let uses: Vec<_> = self
            .uses
            .iter()
            .filter(|u| u.edge_tag == edge_tag)
            .collect();
        if uses.len() != 2
            || uses[0].map_operand == uses[1].map_operand
            || uses[0].representation_tag != uses[1].representation_tag
            || uses[0].parameter_map != uses[1].parameter_map
        {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let target = uses
            .iter()
            .find(|u| u.map_operand == 0)
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let source = uses
            .iter()
            .find(|u| u.map_operand == 1)
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let map = native
            .parameter_maps
            .iter()
            .find(|m| m.id == source.parameter_map && m.run == self.run)
            .ok_or(EvaluationError::UnresolvedGeometry)?;
        let target_parameter = map
            .fields
            .image_at(source_parameter)
            .ok_or(EvaluationError::OutsideDomain)?;
        let points = [
            target.lift(target_parameter, native)?,
            source.lift(source_parameter, native)?,
        ];
        let distance = points[0]
            .iter()
            .zip(points[1])
            .fold(0f64, |d, (a, b)| d.hypot(a - b));
        if !distance.is_finite() {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(NativeEdgeGapSample {
            edge_tag,
            representation_tag: source.representation_tag,
            uv_tags: [target.uv_tag, source.uv_tag],
            parameters: [target_parameter, source_parameter],
            points,
            distance,
            stored_sample: matches!(map.fields.relation, NativeParameterRelation::Sampled { .. }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBoundaryUvSummary {
    pub uses: usize,
    pub lines: usize,
    #[serde(default)]
    pub circles: usize,
    pub jets: usize,
    pub qualified_faces: u32,
    pub qualified_edges: u32,
    pub qualified_vertices: u32,
    pub evaluated_sites: usize,
    pub evaluated_lifts: usize,
    pub unresolved_lifts: usize,
    pub oriented_uses: usize,
    pub oriented_loops: usize,
    pub endpoint_trimmed_uses: usize,
    pub plane_uses: usize,
    pub extended_line_uses: usize,
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}

fn matches_hex(bytes: &[u8], stored: &str) -> bool {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes.len().checked_mul(2) == Some(stored.len())
        && bytes
            .iter()
            .zip(stored.as_bytes().chunks_exact(2))
            .all(|(b, pair)| pair == [DIGITS[(b >> 4) as usize], DIGITS[(b & 15) as usize]])
}

fn registry(bytes: &[u8], b: &mut Budget<'_>) -> Result<Option<BTreeMap<u32, Frame>>> {
    b.work(bytes.len() as u64)?;
    let mut frames = BTreeMap::new();
    let (mut offset, mut tag) = (0, 0u32);
    while offset < bytes.len() {
        let Some(f) = frame(bytes, offset) else {
            return Ok(None);
        };
        if f.token == 0 || frames.is_empty() && f.token != 1 {
            return Ok(None);
        }
        let Some(next) = tag.checked_add(f.token) else {
            return Ok(None);
        };
        tag = next;
        offset = f.end;
        b.add(1)?;
        b.work(32)?;
        frames.insert(tag, f);
    }
    Ok((frames.len() >= 2).then_some(frames))
}

fn resolved(c: &mut Cursor<'_>, tag: u32) -> Option<u32> {
    let r = parameter_maps::reference(c)?;
    match r.encoding {
        NativeAllocationEncoding::OwnedChild => tag.checked_add(r.value)?.checked_add(1),
        _ => tag.checked_sub(r.value),
    }
    .filter(|v| *v != 0)
}

fn resolved_qualified(
    c: &mut Cursor<'_>,
    tag: u32,
    extended: bool,
    accepts: impl Fn(u32) -> bool,
) -> Option<u32> {
    let r = parameter_maps::reference(c)?;
    if extended
        && matches!(
            r.encoding,
            NativeAllocationEncoding::ExplicitU8 | NativeAllocationEncoding::ExplicitU16
        )
    {
        let mut choices = [
            tag.checked_sub(r.value),
            tag.checked_add(r.value).and_then(|v| v.checked_add(1)),
        ]
        .into_iter()
        .flatten()
        .filter(|v| *v != 0 && accepts(*v));
        let value = choices.next()?;
        return choices.next().is_none().then_some(value);
    }
    (if r.encoding == NativeAllocationEncoding::OwnedChild {
        tag.checked_add(r.value)?.checked_add(1)
    } else {
        tag.checked_sub(r.value)
    })
    .filter(|v| *v != 0)
}

fn loop_counted<'a>(
    payload: &'a [u8],
    tag: u32,
    support: u32,
    fs: &BTreeMap<u32, Frame>,
    bytes: &[u8],
    extended: bool,
) -> Option<(Vec<u32>, &'a [u8])> {
    if !extended {
        return counted(payload, tag);
    }
    let mut c = Cursor::new(payload);
    let n = c.byte()?.checked_sub(128)? as usize;
    if n < 3 || n % 2 != 1 || n >= payload.len() {
        return None;
    }
    let mut refs = Vec::with_capacity(n);
    for i in 0..n {
        refs.push(resolved_qualified(&mut c, tag, true, |v| {
            if i == n - 1 {
                v == support
            } else {
                fs.get(&v).is_some_and(|f| {
                    f.flag == 3
                        && if i % 2 == 1 {
                            f.class == 0x5e
                        } else {
                            matches!(f.class, 0x18 | 0x19 | 0x20)
                                && resolved_qualified(
                                    &mut Cursor::new(&bytes[f.payload..f.end]),
                                    v,
                                    true,
                                    |target| target == support,
                                ) == Some(support)
                        }
                })
            }
        })?);
    }
    let tail = payload.get(c.p..)?;
    (!tail.is_empty()).then_some((refs, tail))
}

fn counted(payload: &[u8], tag: u32) -> Option<(Vec<u32>, &[u8])> {
    let mut c = Cursor::new(payload);
    let count = c.byte()?.checked_sub(128)? as usize;
    if count == 0 || count >= payload.len() {
        return None;
    }
    let refs = (0..count)
        .map(|_| resolved(&mut c, tag))
        .collect::<Option<Vec<_>>>()?;
    let tail = payload.get(c.p..)?;
    (!tail.is_empty()).then_some((refs, tail))
}

// Extended controls keep their encoding. Support and child-loop positions
// require one typed target. Complete incidence is checked by build.
fn face_counted<'a>(
    payload: &'a [u8],
    tag: u32,
    fs: &BTreeMap<u32, Frame>,
    extended: bool,
) -> Option<(Vec<u32>, &'a [u8], bool)> {
    if !extended {
        let (r, t) = counted(payload, tag)?;
        return Some((r, t, false));
    }
    let mut c = Cursor::new(payload);
    let n = c.byte()?.checked_sub(128)? as usize;
    if n == 0 || n >= payload.len() {
        return None;
    }
    let mut refs = Vec::with_capacity(n);
    let mut used_forward = false;
    for i in 0..n {
        let r = parameter_maps::reference(&mut c)?;
        let target = if matches!(
            r.encoding,
            NativeAllocationEncoding::ExplicitU8 | NativeAllocationEncoding::ExplicitU16
        ) {
            let choices = [
                tag.checked_sub(r.value),
                tag.checked_add(r.value).and_then(|v| v.checked_add(1)),
            ]
            .into_iter()
            .flatten()
            .filter(|t| {
                fs.get(t).is_some_and(|f| {
                    f.flag == 3
                        && if i > 0 {
                            f.class == 0x62
                        } else {
                            matches!(f.class, 0x27 | 0x28 | 0x29 | 0x2a | 0x2b | 0x34)
                        }
                })
            })
            .collect::<Vec<_>>();
            if choices.len() != 1 {
                return None;
            }
            used_forward |= choices[0] > tag;
            choices[0]
        } else if r.encoding == NativeAllocationEncoding::OwnedChild {
            tag.checked_add(r.value)?.checked_add(1)?
        } else {
            tag.checked_sub(r.value)?
        };
        if target == 0 {
            return None;
        }
        refs.push(target);
    }
    let tail = payload.get(c.p..)?;
    (!tail.is_empty()).then_some((refs, tail, used_forward))
}

fn edge(payload: &[u8], tag: u32) -> Option<[u32; 3]> {
    let mut c = Cursor::new(payload);
    let refs = [
        resolved(&mut c, tag)?,
        resolved(&mut c, tag)?,
        resolved(&mut c, tag)?,
    ];
    // Remaining endpoint-representation data and flags have multiple forms;
    // retain them in source rather than interpreting an unqualified layout.
    payload
        .get(c.p..)
        .filter(|tail| !tail.is_empty())
        .map(|_| refs)
}

fn loop_orientations(
    refs: &[u32],
    tail: &[u8],
    face_control: u8,
    edges: &BTreeMap<u32, (u32, [u32; 3])>,
) -> Option<Vec<NativeUvUseOrientation>> {
    let n = refs.len().checked_sub(1)? / 2;
    if n == 0
        || refs.len() != n * 2 + 1
        || tail.first().copied()? != u8::try_from(n).ok()?.checked_add(128)?
        || !matches!(face_control, 3 | 5)
        || !matches!(tail.get(1), Some(0x41 | 0x50))
    {
        return None;
    }
    let length = (3 * n).div_ceil(8);
    let packed = tail.get(2..2 + length)?;
    // Both productions retain a trailing source payload. Reject truncation and
    // nonzero unused bits rather than borrowing bits from that payload.
    if tail.len() <= 2 + length || 3 * n % 8 != 0 && packed[length - 1] >> (3 * n % 8) != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let bit = 3 * i;
        let word =
            u16::from(packed[bit / 8]) | (u16::from(*packed.get(bit / 8 + 1).unwrap_or(&0)) << 8);
        let flags = ((word >> (bit % 8)) & 7) as u8;
        let edge_forward = (flags & 1 != 0) ^ (face_control == 3);
        let e = edges.get(&refs[2 * i + 1])?.1;
        result.push(NativeUvUseOrientation {
            control: tail[1],
            flags,
            face_control,
            edge_forward,
            uv_forward: flags & 4 != 0,
            start_vertex_tag: if edge_forward { e[1] } else { e[2] },
            end_vertex_tag: if edge_forward { e[2] } else { e[1] },
        });
    }
    result
        .iter()
        .zip(result.iter().cycle().skip(1))
        .take(n)
        .all(|(a, b)| a.end_vertex_tag == b.start_vertex_tag)
        .then_some(result)
}

fn child(source: &StreamSpan, start: usize, end: usize) -> Option<StreamSpan> {
    if start >= end || end as u64 > source.length {
        return None;
    }
    let mut cursor = 0u64;
    let mut file_ranges = Vec::new();
    for r in &source.file_ranges {
        let next = cursor.checked_add(r.length)?;
        let a = cursor.max(start as u64);
        let z = next.min(end as u64);
        if a < z {
            file_ranges.push(crate::container::FileRange {
                offset: r.offset.checked_add(a - cursor)?,
                length: z - a,
            });
        }
        cursor = next;
    }
    (cursor == source.length).then_some(StreamSpan {
        stream_id: source.stream_id,
        logical_offset: source.logical_offset.checked_add(start as u64)?,
        length: (end - start) as u64,
        file_ranges,
    })
}

struct AllocationInput<'a> {
    bytes: &'a [u8],
    source: &'a StreamSpan,
    run: u32,
    revision: u8,
}

fn build(
    input: AllocationInput<'_>,
    g: &GeometryModel,
    boundary: &BoundaryPopulation,
    native: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<Option<NativeBoundaryUv>> {
    let AllocationInput {
        bytes,
        source,
        run,
        revision,
    } = input;
    if revision > 3 {
        return Ok(None);
    }
    let Some(fs) = registry(bytes, b)? else {
        return Ok(None);
    };
    let faces: BTreeMap<_, _> = g
        .surfaces
        .iter()
        .filter(|s| s.population_id == boundary.population_id)
        .map(|s| (s.record_ordinal, s))
        .collect();
    let edges: BTreeMap<_, _> = g
        .curves
        .iter()
        .filter(|c| c.population_id == boundary.population_id)
        .map(|c| (c.record_ordinal, c))
        .collect();
    let points: BTreeMap<_, _> = g.points.iter().map(|p| (p.id.as_str(), p)).collect();
    let vertices = boundary
        .vertex_points
        .iter()
        .map(|id| points.get(id.as_str()).and_then(|p| p.native_allocation_id))
        .collect::<Option<Vec<_>>>();
    let Some(vertices) = vertices else {
        return Ok(None);
    };
    let face_ordinals: BTreeSet<_> = boundary.loops.iter().map(|l| l.face_ordinal).collect();
    if faces.keys().copied().collect::<BTreeSet<_>>() != face_ordinals
        || faces.len()
            != g.surfaces
                .iter()
                .filter(|s| s.population_id == boundary.population_id)
                .count()
        || edges.len()
            != g.curves
                .iter()
                .filter(|c| c.population_id == boundary.population_id)
                .count()
        || vertices.iter().collect::<BTreeSet<_>>().len() != vertices.len()
        || edges
            .values()
            .map(|c| c.native_allocation_tag)
            .collect::<BTreeSet<_>>()
            .len()
            != edges.len()
        || faces
            .values()
            .map(|s| s.native_carrier_tag)
            .collect::<BTreeSet<_>>()
            .len()
            != faces.len()
    {
        return Ok(None);
    }
    if faces.is_empty()
        || edges.len() != boundary.edges.len()
        || faces.values().any(|s| {
            fs.get(&s.native_carrier_tag)
                .is_none_or(|f| f.class != 0x5f)
        })
        || vertices
            .iter()
            .any(|v| fs.get(v).is_none_or(|f| f.class != 0x5d))
    {
        return Ok(None);
    }
    let mut edge_refs = BTreeMap::new();
    for e in &boundary.edges {
        let Some(c) = edges.get(&e.native_ordinal) else {
            return Ok(None);
        };
        let tag = c.native_allocation_tag;
        let Some(f) = fs.get(&tag).filter(|f| f.class == 0x5e) else {
            return Ok(None);
        };
        let Some(refs) = edge(&bytes[f.payload..f.end], tag) else {
            return Ok(None);
        };
        let Some(a) = vertices.get(e.vertices[0] as usize) else {
            return Ok(None);
        };
        let Some(z) = vertices.get(e.vertices[1] as usize) else {
            return Ok(None);
        };
        if !((refs[1] == *a && refs[2] == *z) || (refs[1] == *z && refs[2] == *a)) {
            return Ok(None);
        }
        edge_refs.insert(tag, (e.native_ordinal, refs));
    }
    let mut incidence: BTreeMap<u32, BTreeMap<u32, usize>> = BTreeMap::new();
    for loop_ in &boundary.loops {
        for o in &loop_.occurrences {
            let Some(c) = edges.get(&o.edge_ordinal) else {
                return Ok(None);
            };
            *incidence
                .entry(loop_.face_ordinal)
                .or_default()
                .entry(c.native_allocation_tag)
                .or_default() += 1;
        }
    }
    let carriers: BTreeMap<_, _> = native
        .carriers
        .iter()
        .filter(|c| c.run == run)
        .map(|c| (c.source.logical_offset, c))
        .collect();
    let nurbs: BTreeMap<_, _> = native
        .nurbs_carriers
        .iter()
        .filter(|c| c.run == run)
        .map(|c| (c.source.logical_offset, c))
        .collect();
    let jets: BTreeMap<_, _> = native
        .pcurves
        .iter()
        .filter(|c| c.run == run)
        .map(|c| (c.source.logical_offset, c))
        .collect();
    let maps: BTreeMap<_, _> = native
        .parameter_maps
        .iter()
        .filter(|m| m.run == run)
        .map(|m| (m.source.logical_offset, m))
        .collect();
    let trim_decoder = edge_trims::Decoder {
        bytes,
        fs: &fs,
        source,
        jets: &jets,
        map_coordinates: revision >= 2,
        circles: revision >= 3,
    };
    let mut uses = Vec::new();
    let mut trim_cache: BTreeMap<u32, Option<[NativeUvEndpoint; 2]>> = BTreeMap::new();
    let mut support_cache: BTreeMap<u32, Option<NativeBoundSupport>> = BTreeMap::new();
    let mut chart_cache = BTreeMap::new();
    for (ordinal, surface) in &faces {
        let face_tag = surface.native_carrier_tag;
        let face = &fs[&face_tag];
        let Some((refs, tail, _forward_children)) =
            face_counted(&bytes[face.payload..face.end], face_tag, &fs, revision >= 3)
        else {
            continue;
        };
        b.add(refs.len())?;
        if refs.len() < 2 || !matches!(tail, [3] | [5]) {
            continue;
        }
        let support_tag = refs[0];
        let Some(sf) = fs.get(&support_tag) else {
            continue;
        };
        let address = source.logical_offset + sf.start as u64;
        if let std::collections::btree_map::Entry::Vacant(entry) = support_cache.entry(support_tag)
        {
            b.work(((sf.end - sf.start) * 2) as u64)?;
            let record = if let Some(c) = carriers.get(&address) {
                if revision >= 2 {
                    let scale = if let SurfaceGeometry::Sphere { radius, .. } = &c.geometry {
                        [1. / radius, 1. / radius]
                    } else {
                        c.uv_scale
                    };
                    chart_cache.insert(
                        support_tag,
                        NativeUvSupportChart {
                            scale,
                            offset: c.uv_offset,
                        },
                    );
                }
                Some((
                    NativeBoundSupport::Analytic {
                        carrier: c.id.clone(),
                    },
                    &c.raw_frame_hex,
                ))
            } else {
                nurbs.get(&address).map(|c| {
                    (
                        NativeBoundSupport::Nurbs {
                            carrier: c.id.clone(),
                        },
                        &c.raw_frame_hex,
                    )
                })
            };
            let mut verified = record.and_then(|(support, stored)| {
                matches_hex(&bytes[sf.start..sf.end], stored).then_some(support)
            });
            if verified.is_none() && revision >= 1 {
                verified = edge_trims::plane(bytes, sf, source)
                    .map(|plane| NativeBoundSupport::Plane { plane });
            }
            entry.insert(verified);
        }
        let Some(support) = support_cache[&support_tag].clone() else {
            continue;
        };
        let mut pairs = Vec::new();
        let mut observed = BTreeMap::<u32, usize>::new();
        let mut valid = true;
        for loop_tag in &refs[1..] {
            let Some(lf) = fs.get(loop_tag).filter(|f| f.class == 0x62) else {
                valid = false;
                break;
            };
            let Some((rr, loop_tail)) = loop_counted(
                &bytes[lf.payload..lf.end],
                *loop_tag,
                support_tag,
                &fs,
                bytes,
                revision >= 3,
            ) else {
                valid = false;
                break;
            };
            b.add(rr.len())?;
            if rr.len() < 3 || rr.len() % 2 != 1 || rr.last() != Some(&support_tag) {
                valid = false;
                break;
            }
            let orientations = loop_orientations(&rr, loop_tail, tail[0], &edge_refs);
            for (slot, pair) in rr[..rr.len() - 1].chunks_exact(2).enumerate() {
                if !edge_refs.contains_key(&pair[1]) || !fs.contains_key(&pair[0]) {
                    valid = false;
                    break;
                }
                *observed.entry(pair[1]).or_default() += 1;
                pairs.push((
                    *loop_tag,
                    slot as u32,
                    pair[0],
                    pair[1],
                    orientations.as_ref().map(|v| v[slot].clone()),
                ));
            }
            if !valid {
                break;
            }
        }
        if !valid || incidence.get(ordinal) != Some(&observed) {
            continue;
        }
        for (loop_tag, loop_slot, uv_tag, edge_tag, orientation) in pairs {
            let uv = &fs[&uv_tag];
            b.work(((uv.end - uv.start) * 2 + source.file_ranges.len() * 4 + 32) as u64)?;
            let mut c = Cursor::new(&bytes[uv.payload..uv.end]);
            if resolved_qualified(&mut c, uv_tag, revision >= 3, |v| v == support_tag)
                != Some(support_tag)
            {
                continue;
            }
            let (geometry, program_range) = if uv.class == 0x18 {
                let Some(fields) = parameter_lines::parse(&bytes[uv.payload..uv.end]) else {
                    continue;
                };
                let range = fields.geometry.range();
                (NativeBoundUvGeometry::Line { fields }, range)
            } else if uv.class == 0x19 && revision >= 3 && uv.flag == 3 {
                let Some(fields) = parameter_circles::parse(&bytes[uv.payload..uv.end]) else {
                    continue;
                };
                let range = fields.parameter_range;
                (NativeBoundUvGeometry::Circle { fields }, range)
            } else if uv.class == 0x20 {
                let Some(j) = jets.get(&(source.logical_offset + uv.start as u64)) else {
                    continue;
                };
                if !matches_hex(&bytes[uv.start..uv.end], &j.raw_frame_hex) {
                    continue;
                }
                (
                    NativeBoundUvGeometry::Jet {
                        pcurve: j.id.clone(),
                    },
                    j.jet.parameter_range,
                )
            } else {
                continue;
            };
            let representation_tag = edge_refs[&edge_tag].1[0];
            let Some(mf) = fs
                .get(&representation_tag)
                .filter(|f| [0x24, 0x25].contains(&f.class))
            else {
                continue;
            };
            b.work(((mf.end - mf.start) * 2) as u64)?;
            let Some(map) = maps.get(&(source.logical_offset + mf.start as u64)) else {
                continue;
            };
            if !matches_hex(&bytes[mf.start..mf.end], &map.raw_frame_hex) {
                continue;
            }
            let mut operands = Cursor::new(&bytes[mf.payload..mf.end]);
            if operands.byte() != Some(0x82) {
                continue;
            }
            let mut operand = || {
                let at = operands.p;
                if let Some(v) =
                    resolved_qualified(&mut operands, representation_tag, revision >= 3, |v| {
                        v == uv_tag
                    })
                {
                    return Some(v);
                }
                operands.p = at;
                resolved_qualified(&mut operands, representation_tag, revision >= 3, |v| {
                    fs.get(&v)
                        .is_some_and(|f| f.flag == 3 && matches!(f.class, 0x18 | 0x19 | 0x20))
                })
            };
            let refs = [operand(), operand()];
            let map_operand = if refs[0] == Some(uv_tag) && refs[1] != Some(uv_tag) {
                0
            } else if refs[1] == Some(uv_tag) && refs[0] != Some(uv_tag) {
                1
            } else {
                continue;
            };
            let mut edge_trim = if revision >= 1 {
                if let (Some(a), Some(z), Some(o)) = (refs[0], refs[1], orientation.as_ref()) {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        trim_cache.entry(edge_tag)
                    {
                        // Each endpoint and program payload is parsed at most
                        // once per current edge, independently of face order.
                        let decoded = trim_decoder.decode(
                            edge_tag,
                            edge_refs[&edge_tag].1,
                            [a, z],
                            &map.fields,
                            b,
                        )?;
                        if let Some(points) = &decoded {
                            b.add(
                                2 + points.iter().filter(|p| p.map_coordinate.is_some()).count(),
                            )?;
                            b.work(points.iter().map(|p| p.source.length * 4).sum())?;
                        }
                        entry.insert(decoded);
                    }
                    trim_cache[&edge_tag].as_ref().and_then(|points| {
                        edge_trims::bind(
                            points,
                            uv_tag,
                            [edge_refs[&edge_tag].1[1], edge_refs[&edge_tag].1[2]],
                            o,
                        )
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(trim) = &edge_trim {
                let rounding = 64.
                    * f64::EPSILON
                    * program_range
                        .into_iter()
                        .chain(trim.parameters)
                        .map(f64::abs)
                        .fold(1., f64::max);
                if matches!(geometry, NativeBoundUvGeometry::Jet { .. })
                    && trim.parameters.iter().any(|p| {
                        *p < program_range[0] - rounding || *p > program_range[1] + rounding
                    })
                {
                    edge_trim = None;
                }
            }
            let map_range = if map_operand == 0 {
                Some(map.fields.target_range)
            } else {
                match &map.fields.relation {
                    NativeParameterRelation::Identity { source_range }
                    | NativeParameterRelation::Affine { source_range, .. }
                    | NativeParameterRelation::Sampled { source_range, .. } => Some(*source_range),
                    NativeParameterRelation::Unspecified => None,
                }
            };
            if let (Some(trim), Some(range)) = (&edge_trim, map_range) {
                let rounding = 64.
                    * f64::EPSILON
                    * range
                        .into_iter()
                        .chain(trim.parameters)
                        .map(f64::abs)
                        .fold(1., f64::max);
                if trim
                    .parameters
                    .iter()
                    .any(|p| *p < range[0] - rounding || *p > range[1] + rounding)
                {
                    edge_trim = None;
                }
            }
            let range = if let Some(trim) = &edge_trim {
                [
                    trim.parameters[0].min(trim.parameters[1]),
                    trim.parameters[0].max(trim.parameters[1]),
                ]
            } else if map_operand == 0 {
                map.fields.target_range
            } else {
                match &map.fields.relation {
                    NativeParameterRelation::Identity { source_range }
                    | NativeParameterRelation::Affine { source_range, .. }
                    | NativeParameterRelation::Sampled { source_range, .. } => *source_range,
                    NativeParameterRelation::Unspecified => continue,
                }
            };
            let rounding = 64.
                * f64::EPSILON
                * range
                    .into_iter()
                    .chain(program_range)
                    .map(f64::abs)
                    .fold(1., f64::max);
            if matches!(geometry, NativeBoundUvGeometry::Circle { .. })
                && edge_trim.is_none()
                && (range[0] < program_range[0] || range[1] > program_range[1])
            {
                continue;
            }
            if (range[0] < program_range[0] - rounding || range[1] > program_range[1] + rounding)
                && !(edge_trim.is_some()
                    && matches!(
                        geometry,
                        NativeBoundUvGeometry::Line { .. } | NativeBoundUvGeometry::Circle { .. }
                    ))
            {
                // Some saved maps cover extended historical representations.
                // Do not extrapolate a program to make that interval fit.
                continue;
            }
            let lf = &fs[&loop_tag];
            let ef = &fs[&edge_tag];
            let Some(uv_source) = child(source, uv.start, uv.end) else {
                return Ok(None);
            };
            let Some(face_source) = child(source, face.start, face.end) else {
                return Ok(None);
            };
            let Some(loop_source) = child(source, lf.start, lf.end) else {
                return Ok(None);
            };
            let Some(edge_source) = child(source, ef.start, ef.end) else {
                return Ok(None);
            };
            b.add(1)?;
            if chart_cache.contains_key(&support_tag) {
                b.add(1)?;
            }
            uses.push(NativeBoundUvUse {
                face_ordinal: *ordinal,
                edge_ordinal: edge_refs[&edge_tag].0,
                face_tag,
                edge_tag,
                loop_tag,
                loop_slot,
                uv_tag,
                support_tag,
                representation_tag,
                parameter_map: map.id.clone(),
                map_operand,
                support: support.clone(),
                support_chart: chart_cache.get(&support_tag).cloned(),
                geometry,
                parameter_range: range,
                program_range,
                orientation,
                edge_trim,
                uv_source,
                face_source,
                loop_source,
                edge_source,
            });
        }
    }
    if uses.is_empty() {
        return Ok(None);
    }
    b.work(bytes.len() as u64 * 2)?;
    Ok(Some(NativeBoundaryUv {
        revision,
        population_id: boundary.population_id,
        run,
        factory_source: source.clone(),
        raw_factory_hex: hex(bytes),
        qualified_faces: faces.len() as u32,
        qualified_edges: edges.len() as u32,
        qualified_vertices: vertices.len() as u32,
        uses,
    }))
}

pub(in crate::topology) fn recover(
    data: &[u8],
    geometry: &GeometryModel,
    boundaries: &[BoundaryPopulation],
    partials: &[AnalyticPartial],
    native: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<Vec<NativeBoundaryUv>> {
    let mut result = Vec::new();
    for boundary in boundaries {
        if !partials
            .iter()
            .any(|p| p.population_id == boundary.population_id)
        {
            continue;
        }
        if let Some(binding) = recover_boundary(data, geometry, boundary, native, b)? {
            result.push(binding);
        }
    }
    Ok(result)
}

/// Recover one population's source links without assuming trimmed geometry is
/// already complete. The registry, all native tags and factory uniqueness are
/// qualified by the same production used for the retained public binding.
pub(in crate::topology) fn recover_boundary(
    data: &[u8],
    geometry: &GeometryModel,
    boundary: &BoundaryPopulation,
    native: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<Option<NativeBoundaryUv>> {
    let mut candidates = Vec::new();
    for (run, record) in native.runs.iter().enumerate() {
        b.bytes = b.bytes.saturating_add(record.source.length);
        if b.bytes > b.limits.max_topology_bytes {
            return Err(fail(
                Code::TopologyByteLimitExceeded,
                "allocation source bytes",
            ));
        }
        let mut bytes = Vec::new();
        for range in &record.source.file_ranges {
            let Some(end) = range.offset.checked_add(range.length) else {
                continue;
            };
            let Some(raw) = data.get(range.offset as usize..end as usize) else {
                continue;
            };
            bytes.extend_from_slice(raw);
        }
        if bytes.len() as u64 != record.source.length {
            continue;
        }
        if let Some(binding) = build(
            AllocationInput {
                bytes: &bytes,
                source: &record.source,
                run: run as u32,
                revision: 3,
            },
            geometry,
            boundary,
            native,
            b,
        )? {
            candidates.push(binding);
        }
    }
    // An independent factory with colliding IDs must not select a result.
    if candidates.len() == 1 {
        return Ok(Some(candidates.remove(0)));
    }
    Ok(None)
}

pub fn validate_native_boundary_uv(
    geometry: &GeometryModel,
    boundary: &BoundaryPopulation,
    binding: &NativeBoundaryUv,
    native: &NativeUvModel,
    limits: &crate::ParseLimits,
) -> std::result::Result<NativeBoundaryUvSummary, TopologyValidationError> {
    let mut budget = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    validate(geometry, boundary, binding, native, &mut budget).map_err(|e| {
        TopologyValidationError {
            code: e.code,
            detail: e.detail,
        }
    })
}

pub(in crate::topology) fn validate(
    geometry: &GeometryModel,
    boundary: &BoundaryPopulation,
    binding: &NativeBoundaryUv,
    native: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<NativeBoundaryUvSummary> {
    validate_internal(native, b)?;
    let invalid = || {
        fail(
            Code::TopologyValidationInvalid,
            "native allocation UV binding",
        )
    };
    let run = native.runs.get(binding.run as usize).ok_or_else(invalid)?;
    if binding.population_id != boundary.population_id || binding.factory_source != run.source {
        return Err(invalid());
    }
    let bytes = raw(&binding.raw_factory_hex, &binding.factory_source, b)?;
    let mut expected = build(
        AllocationInput {
            bytes: &bytes,
            source: &binding.factory_source,
            run: binding.run,
            revision: binding.revision,
        },
        geometry,
        boundary,
        native,
        b,
    )?
    .ok_or_else(invalid)?;
    // Earlier serialized bindings did not claim loop sense. Their original
    // source/geometry replay remains valid without upgrading that claim.
    for (actual, replayed) in binding.uses.iter().zip(&mut expected.uses) {
        if actual.orientation.is_none() {
            replayed.orientation = None;
        }
    }
    if expected != *binding {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "native allocation binding fields differ",
        ));
    }
    let mut lines = 0;
    let mut circles = 0;
    let mut sites = 0;
    let mut lifts = 0;
    for u in &binding.uses {
        if matches!(u.geometry, NativeBoundUvGeometry::Line { .. }) {
            lines += 1;
        }
        if matches!(u.geometry, NativeBoundUvGeometry::Circle { .. }) {
            circles += 1;
        }
        for i in 0..=8 {
            let controls = native
                .nurbs_carriers
                .iter()
                .map(|c| c.geometry.control_points.len())
                .max()
                .unwrap_or(0);
            let knots = native
                .pcurves
                .iter()
                .map(|c| c.jet.knots.len())
                .max()
                .unwrap_or(0);
            b.work(
                (native.pcurves.len()
                    + native.carriers.len()
                    + native.nurbs_carriers.len()
                    + controls * 4
                    + knots * 32
                    + 1024) as u64,
            )?;
            let t = if i == 8 {
                u.parameter_range[1]
            } else {
                u.parameter_range[0] + (u.parameter_range[1] - u.parameter_range[0]) * i as f64 / 8.
            };
            if !u
                .evaluate(t, native)
                .is_ok_and(|p| p.iter().all(|v| v.is_finite()))
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    format!(
                        "native UV program domain face {} edge {}",
                        u.face_tag, u.edge_tag
                    ),
                ));
            }
            // Exact source identity does not authorize carrier extrapolation.
            if let Ok(point) = u.lift(t, native) {
                if !finite(point) {
                    return Err(invalid());
                }
                lifts += 1;
            }
            sites += 1;
        }
    }
    Ok(NativeBoundaryUvSummary {
        uses: binding.uses.len(),
        lines,
        circles,
        jets: binding.uses.len() - lines - circles,
        qualified_faces: binding.qualified_faces,
        qualified_edges: binding.qualified_edges,
        qualified_vertices: binding.qualified_vertices,
        evaluated_sites: sites,
        evaluated_lifts: lifts,
        unresolved_lifts: sites - lifts,
        oriented_uses: binding
            .uses
            .iter()
            .filter(|u| u.orientation.is_some())
            .count(),
        oriented_loops: binding
            .uses
            .iter()
            .filter(|u| u.orientation.is_some())
            .map(|u| (u.face_tag, u.loop_tag))
            .collect::<BTreeSet<_>>()
            .len(),
        endpoint_trimmed_uses: binding
            .uses
            .iter()
            .filter(|u| u.edge_trim.is_some())
            .count(),
        plane_uses: binding
            .uses
            .iter()
            .filter(|u| matches!(u.support, NativeBoundSupport::Plane { .. }))
            .count(),
        extended_line_uses: binding
            .uses
            .iter()
            .filter(|u| {
                matches!(u.geometry, NativeBoundUvGeometry::Line { .. })
                    && (u.parameter_range[0] < u.program_range[0]
                        || u.parameter_range[1] > u.program_range[1])
            })
            .count(),
    })
}
