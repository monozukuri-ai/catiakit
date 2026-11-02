//! Curve-point coordinates are serialized in reverse reference order. The
//! map coordinate uses the first operand's gauge, independently of its UV point.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeMapCoordinateEncoding {
    InheritedTarget,
    RangeStart,
    RangeEnd,
    Explicit,
}

/// A coordinate on the map's first operand, not a correspondence on its second
/// operand or a spatially qualified common curve. The original word is retained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeMapCoordinate {
    pub target_uv_tag: u32,
    pub parameter: f64,
    pub encoding: NativeMapCoordinateEncoding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_index: Option<u32>,
}

impl PartialEq for NativeMapCoordinate {
    fn eq(&self, other: &Self) -> bool {
        self.target_uv_tag == other.target_uv_tag
            && self.parameter.to_bits() == other.parameter.to_bits()
            && self.encoding == other.encoding
            && self.literal.map(f64::to_bits) == other.literal.map(f64::to_bits)
            && self.span_index == other.span_index
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeUvEndpoint {
    pub allocation_tag: u32,
    pub references: [u32; 3],
    pub control: u8,
    /// Coordinates of references 1 and 2, respectively, in reference order.
    pub uv_parameters: [f64; 2],
    pub opaque_map_coordinate_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_coordinate: Option<NativeMapCoordinate>,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeUvEdgeTrim {
    /// Parameters at the raw edge's start and end vertices; may be descending.
    pub parameters: [f64; 2],
    pub vertex_tags: [u32; 2],
    pub endpoints: [NativeUvEndpoint; 2],
}

impl NativeUvEdgeTrim {
    /// Map coordinates at the raw edge's start and end. These can differ from
    /// the UV endpoint coordinates; no sampled-map interpolation is implied.
    #[must_use]
    pub fn map_target_parameters(&self) -> Option<[f64; 2]> {
        let a = self.endpoints[0].map_coordinate.as_ref()?;
        let z = self.endpoints[1].map_coordinate.as_ref()?;
        (a.target_uv_tag == z.target_uv_tag && a.parameter.is_finite() && z.parameter.is_finite())
            .then_some([a.parameter, z.parameter])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePlaneSupport {
    pub control: u32,
    pub frame: Frame3,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}

pub(super) fn plane(bytes: &[u8], f: &Frame, source: &StreamSpan) -> Option<NativePlaneSupport> {
    if f.class != 0x27 || f.flag != 3 {
        return None;
    }
    let mut c = Cursor::new(&bytes[f.payload..f.end]);
    let control = c.compact()?;
    if control >= 0x2000 || control & 0x1800 != 0x1800 {
        return None;
    }
    let mut origin = [0.; 3];
    let mut x_axis = [0.; 3];
    let mut y_axis = [0.; 3];
    for (i, value) in origin.iter_mut().enumerate() {
        *value = if control & (1 << i) != 0 {
            0.
        } else {
            c.number()?
        };
    }
    for (i, value) in x_axis.iter_mut().enumerate() {
        *value = if control & (1 << (4 + i)) != 0 {
            0.
        } else if control & 8 != 0 {
            1.
        } else {
            c.number()?
        };
    }
    for (i, value) in y_axis.iter_mut().enumerate() {
        *value = if control & (1 << (8 + i)) != 0 {
            0.
        } else if control & 128 != 0 {
            1.
        } else {
            c.number()?
        };
    }
    let frame = Frame3 {
        origin,
        x_axis,
        y_axis,
        z_axis: cross(x_axis, y_axis),
    };
    if !c.done() || !frame.is_valid() {
        return None;
    }
    Some(NativePlaneSupport {
        control,
        frame,
        source: child(source, f.start, f.end)?,
        raw_frame_hex: hex(&bytes[f.start..f.end]),
    })
}

struct Program<'a> {
    defaults: Option<[f64; 2]>,
    knots: Option<&'a [f64]>,
}

pub(super) struct Decoder<'a, 'n> {
    pub bytes: &'a [u8],
    pub fs: &'a BTreeMap<u32, Frame>,
    pub source: &'a StreamSpan,
    pub jets: &'a BTreeMap<u64, &'n NativePcurve>,
    pub map_coordinates: bool,
    pub circles: bool,
}

impl<'n> Decoder<'_, 'n> {
    fn program(
        &self,
        tag: u32,
        map_range: Option<[f64; 2]>,
        work: &mut u64,
    ) -> Option<Program<'n>> {
        let f = self.fs.get(&tag)?;
        *work = work.saturating_add(((f.end - f.start) * 4 + 128) as u64);
        let (range, knots) = if f.class == 0x18 && f.flag == 3 {
            let fields = parameter_lines::parse(&self.bytes[f.payload..f.end])?;
            (fields.geometry.range(), None)
        } else if self.circles && f.class == 0x19 && f.flag == 3 {
            (
                parameter_circles::parse(&self.bytes[f.payload..f.end])?.parameter_range,
                None,
            )
        } else if f.class == 0x20 {
            let j = *self
                .jets
                .get(&self.source.logical_offset.checked_add(f.start as u64)?)?;
            if !matches_hex(&self.bytes[f.start..f.end], &j.raw_frame_hex) {
                return None;
            }
            (j.jet.parameter_range, Some(j.jet.knots.as_slice()))
        } else {
            return None;
        };
        // Omitted endpoint values belong to the referenced curve in this
        // representation, not an unrelated full historical interval.
        let defaults = if let Some(r) = map_range {
            let lo = range[0].max(r[0]);
            let hi = range[1].min(r[1]);
            (lo < hi).then_some([lo, hi])
        } else {
            Some(range)
        };
        Some(Program { defaults, knots })
    }

    fn endpoint(
        &self,
        tag: u32,
        representation: u32,
        operands: [u32; 2],
        ranges: [Option<[f64; 2]>; 2],
        work: &mut u64,
    ) -> Option<NativeUvEndpoint> {
        let f = self.fs.get(&tag)?;
        *work = work.saturating_add(
            ((f.end - f.start) * 4 + self.source.file_ranges.len() * 16 + 128) as u64,
        );
        if f.class != 6 || f.flag != 3 {
            return None;
        }
        let mut c = Cursor::new(&self.bytes[f.payload..f.end]);
        if c.byte()? != 0x83 {
            return None;
        }
        let references = [
            resolved_qualified(&mut c, tag, self.circles, |v| v == representation)?,
            resolved_qualified(&mut c, tag, self.circles, |v| operands.contains(&v))?,
            resolved_qualified(&mut c, tag, self.circles, |v| operands.contains(&v))?,
        ];
        if references[0] != representation
            || references[1] == references[2]
            || !((references[1..] == operands)
                || (references[1] == operands[1] && references[2] == operands[0]))
        {
            return None;
        }
        let first = self.program(
            references[1],
            ranges[usize::from(references[1] == operands[1])],
            work,
        )?;
        let last = self.program(
            references[2],
            ranges[usize::from(references[2] == operands[1])],
            work,
        )?;
        let control = c.byte()?;
        if control & 0x40 != 0 {
            return None;
        }
        let codes = [control & 3, (control >> 2) & 3, (control >> 4) & 3];
        let (v2, v1, suffix) = if control & 0x80 != 0 {
            if control & 0x30 != 0 {
                return None;
            }
            // The two code lanes are reversed relative to the UV references;
            // explicit values are still emitted from the highest lane first.
            let v1 = scalar(&mut c, codes[1], &first)?;
            let v2 = scalar(&mut c, codes[0], &last)?;
            (v2, v1, &c.bytes[c.p..c.p])
        } else {
            let v2 = scalar(&mut c, codes[2], &last)?;
            let v1 = scalar(&mut c, codes[1], &first)?;
            let start = c.p;
            match codes[0] {
                1 | 2 => (),
                0 => {
                    c.number()?;
                    if !c.done() && c.compact()? == 0 {
                        return None;
                    }
                }
                _ => return None,
            }
            (v2, v1, &c.bytes[start..c.p])
        };
        if !c.done() {
            return None;
        }
        let (target_program, inherited) = if references[1] == operands[0] {
            (&first, v1)
        } else {
            (&last, v2)
        };
        let map_coordinate = self
            .map_coordinates
            .then(|| {
                map_coordinate(
                    control,
                    suffix,
                    operands[0],
                    inherited,
                    target_program,
                    ranges[0]?,
                )
            })
            .flatten();
        Some(NativeUvEndpoint {
            allocation_tag: tag,
            references,
            control,
            uv_parameters: [v1, v2],
            opaque_map_coordinate_hex: hex(suffix),
            map_coordinate,
            source: child(self.source, f.start, f.end)?,
            raw_frame_hex: hex(&self.bytes[f.start..f.end]),
        })
    }

    pub(super) fn decode(
        &self,
        edge_tag: u32,
        refs: [u32; 3],
        operands: [u32; 2],
        fields: &NativeParameterMapFields,
        budget: &mut Budget<'_>,
    ) -> Result<Option<[NativeUvEndpoint; 2]>> {
        let mut work = 512u64;
        let ranges = [
            Some(fields.target_range),
            match &fields.relation {
                NativeParameterRelation::Identity { source_range }
                | NativeParameterRelation::Affine { source_range, .. }
                | NativeParameterRelation::Sampled { source_range, .. } => Some(*source_range),
                NativeParameterRelation::Unspecified => None,
            },
        ];
        let result = (|| {
            let f = self.fs.get(&edge_tag)?;
            work = work.saturating_add(((f.end - f.start) * 4) as u64);
            let mut c = Cursor::new(&self.bytes[f.payload..f.end]);
            let actual = [
                resolved(&mut c, edge_tag)?,
                resolved(&mut c, edge_tag)?,
                resolved(&mut c, edge_tag)?,
            ];
            if actual != refs {
                return None;
            }
            let points = [resolved(&mut c, edge_tag)?, resolved(&mut c, edge_tag)?];
            if !matches!(c.byte()?, 0x21 | 0x22) || !c.done() {
                return None;
            }
            Some([
                self.endpoint(points[0], refs[0], operands, ranges, &mut work)?,
                self.endpoint(points[1], refs[0], operands, ranges, &mut work)?,
            ])
        })();
        budget.work(work)?;
        Ok(result)
    }
}

fn map_coordinate(
    control: u8,
    bytes: &[u8],
    target_uv_tag: u32,
    inherited: f64,
    program: &Program<'_>,
    target_range: [f64; 2],
) -> Option<NativeMapCoordinate> {
    use NativeMapCoordinateEncoding::*;
    let mut c = Cursor::new(bytes);
    let mut literal = None;
    let mut span_index = None;
    let (encoding, parameter) = if control & 0x80 != 0 {
        (InheritedTarget, inherited)
    } else {
        match control & 3 {
            1 => (RangeEnd, target_range[1]),
            2 => (RangeStart, target_range[0]),
            0 => {
                let offset = c.number()?;
                literal = Some(offset);
                let parameter = if let Some(knots) = program.knots {
                    let index = c.compact()?;
                    span_index = Some(index);
                    let index = usize::try_from(index).ok()?;
                    let start = *knots.get(index.checked_sub(1)?)?;
                    let end = *knots.get(index)?;
                    let rounding =
                        64. * f64::EPSILON * offset.abs().max(start.abs()).max(end.abs()).max(1.);
                    if offset < -rounding || offset > end - start + rounding {
                        return None;
                    }
                    start + offset
                } else {
                    offset
                };
                (Explicit, parameter)
            }
            _ => return None,
        }
    };
    let rounding = 64.
        * f64::EPSILON
        * parameter
            .abs()
            .max(target_range[0].abs())
            .max(target_range[1].abs())
            .max(1.);
    if !c.done()
        || !parameter.is_finite()
        || parameter < target_range[0] - rounding
        || parameter > target_range[1] + rounding
    {
        return None;
    }
    Some(NativeMapCoordinate {
        target_uv_tag,
        parameter,
        encoding,
        literal,
        span_index,
    })
}

fn scalar(c: &mut Cursor<'_>, code: u8, program: &Program<'_>) -> Option<f64> {
    match code {
        1 => Some(program.defaults?[1]),
        2 => Some(program.defaults?[0]),
        0 => {
            let offset = c.number()?;
            let value = if let Some(knots) = program.knots {
                let index = usize::try_from(c.compact()?).ok()?;
                let start = *knots.get(index.checked_sub(1)?)?;
                let end = *knots.get(index)?;
                let rounding =
                    64. * f64::EPSILON * offset.abs().max(start.abs()).max(end.abs()).max(1.);
                if offset < -rounding || offset > end - start + rounding {
                    return None;
                }
                start + offset
            } else {
                offset
            };
            value.is_finite().then_some(value)
        }
        _ => None,
    }
}

pub(super) fn bind(
    endpoints: &[NativeUvEndpoint; 2],
    uv_tag: u32,
    vertices: [u32; 2],
    orientation: &NativeUvUseOrientation,
) -> Option<NativeUvEdgeTrim> {
    let mut parameters = [0.; 2];
    for i in 0..2 {
        let p = &endpoints[i];
        let index = p.references[1..].iter().position(|t| *t == uv_tag)?;
        parameters[i] = p.uv_parameters[index];
    }
    if parameters[0] == parameters[1]
        || !(parameters[1] - parameters[0]).is_finite()
        || orientation.uv_forward != ((parameters[0] < parameters[1]) == orientation.edge_forward)
    {
        return None;
    }
    Some(NativeUvEdgeTrim {
        parameters,
        vertex_tags: vertices,
        endpoints: endpoints.clone(),
    })
}
