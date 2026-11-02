//! Closed parameter-representation relations. Allocation operands remain local
//! and unbound; accuracy words are not a license to widen spatial incidence.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeAllocationEncoding {
    BackwardDistance,
    OwnedChild,
    Selector2,
    WidthCoded,
    ExplicitU8,
    ExplicitU16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAllocationReference {
    pub value: u32,
    pub encoding: NativeAllocationEncoding,
    pub raw: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeParameterRelation {
    Unspecified,
    Identity {
        source_range: [f64; 2],
    },
    Affine {
        source_range: [f64; 2],
        scale: f64,
        offset: f64,
    },
    Sampled {
        source_range: [f64; 2],
        target_parameters: Vec<f64>,
        source_parameters: Vec<f64>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeParameterMapFields {
    pub source_reference: NativeAllocationReference,
    pub target_reference: NativeAllocationReference,
    pub selector: u16,
    pub selector_lead: u8,
    pub target_range: [f64; 2],
    /// Stored accuracy word. Its sign and physical-unit semantics are not a
    /// decoded permission to change the spatial incidence gate.
    pub signed_accuracy: f64,
    pub relation: NativeParameterRelation,
    pub target_sites: Vec<f64>,
    /// Retained fit word; it is not qualified as a global spatial gap bound.
    pub fit_tolerance: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeParameterMap {
    pub id: String,
    pub run: u32,
    pub class: u8,
    pub frame_flag: u8,
    pub frame_width: u8,
    pub header_token: Vec<u8>,
    pub fields: NativeParameterMapFields,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}

/// A query into a stored relation. Bracket endpoints describe adjacent saved
/// pairs; they do not define an interpolant or bound the intervening 3D gap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeParameterQuery {
    Exact {
        source_parameter: f64,
        target_parameter: f64,
        stored_sample: bool,
    },
    Bracket {
        source_parameter: f64,
        first_sample_index: u32,
        source_parameters: [f64; 2],
        target_parameters: [f64; 2],
    },
}

/// A query in the first operand's saved map gauge. Brackets retain the table
/// order, including decreasing second-operand coordinates, without defining an
/// interpolant. An affine result is a checked binary64 right inverse, not a
/// claim that the floating-point map has a unique preimage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeTargetParameterQuery {
    Exact {
        source_parameter: f64,
        target_parameter: f64,
        stored_sample: bool,
    },
    Bracket {
        target_parameter: f64,
        first_sample_index: u32,
        source_parameters: [f64; 2],
        target_parameters: [f64; 2],
    },
}

fn valid_range(r: [f64; 2]) -> bool {
    r.iter().all(|v| v.is_finite()) && r[0] < r[1] && (r[1] - r[0]).is_finite()
}

fn valid_samples(sp: &[f64], tp: &[f64], source: [f64; 2], target: [f64; 2]) -> bool {
    (2..=127).contains(&sp.len())
        && sp.len() == tp.len()
        && sp.iter().all(|p| p.is_finite() && inside(source, *p))
        && tp.iter().all(|p| p.is_finite() && inside(target, *p))
        && tp.windows(2).all(|w| w[0] < w[1])
        && (sp.windows(2).all(|w| w[0] < w[1]) || sp.windows(2).all(|w| w[0] > w[1]))
}

impl NativeParameterMapFields {
    /// Distinguish an exact saved/affine correspondence from a query strictly
    /// between saved samples. Incomplete table coverage remains unresolved.
    pub fn query_at_source(
        &self,
        source: f64,
    ) -> std::result::Result<NativeParameterQuery, EvaluationError> {
        if !valid_range(self.target_range) {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !source.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        let source_range = match &self.relation {
            NativeParameterRelation::Unspecified => {
                return Err(EvaluationError::UnresolvedGeometry);
            }
            NativeParameterRelation::Identity { source_range }
            | NativeParameterRelation::Affine { source_range, .. }
            | NativeParameterRelation::Sampled { source_range, .. } => *source_range,
        };
        if !valid_range(source_range) {
            return Err(EvaluationError::InvalidGeometry);
        }
        if source < source_range[0] || source > source_range[1] {
            return Err(EvaluationError::OutsideDomain);
        }
        if let NativeParameterRelation::Sampled {
            source_parameters: sp,
            target_parameters: tp,
            ..
        } = &self.relation
        {
            if !valid_samples(sp, tp, source_range, self.target_range) {
                return Err(EvaluationError::InvalidGeometry);
            }
            if let Some(i) = sp.iter().position(|p| p.to_bits() == source.to_bits()) {
                return Ok(NativeParameterQuery::Exact {
                    source_parameter: source,
                    target_parameter: tp[i],
                    stored_sample: true,
                });
            }
            for (i, w) in sp.windows(2).enumerate() {
                if w[0].min(w[1]) < source && source < w[0].max(w[1]) {
                    return Ok(NativeParameterQuery::Bracket {
                        source_parameter: source,
                        first_sample_index: i as u32,
                        source_parameters: [w[0], w[1]],
                        target_parameters: [tp[i], tp[i + 1]],
                    });
                }
            }
            return Err(EvaluationError::UnresolvedGeometry);
        }
        if let NativeParameterRelation::Affine { scale, offset, .. } = &self.relation
            && (!scale.is_finite() || *scale == 0. || !offset.is_finite())
        {
            return Err(EvaluationError::InvalidGeometry);
        }
        let target = self
            .image_at(source)
            .ok_or(EvaluationError::InvalidGeometry)?;
        if !inside(self.target_range, target) {
            return Err(EvaluationError::InvalidGeometry);
        }
        Ok(NativeParameterQuery::Exact {
            source_parameter: source,
            target_parameter: target,
            stored_sample: false,
        })
    }

    /// Query the saved first-operand coordinate without guessing an inverse
    /// interpolant or adding physical endpoint pairs to the sampled relation.
    /// An affine candidate must replay to the exact requested target bits.
    pub fn query_at_target(
        &self,
        target: f64,
    ) -> std::result::Result<NativeTargetParameterQuery, EvaluationError> {
        if !valid_range(self.target_range) {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !target.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        let source_range = match &self.relation {
            NativeParameterRelation::Unspecified => {
                return Err(EvaluationError::UnresolvedGeometry);
            }
            NativeParameterRelation::Identity { source_range }
            | NativeParameterRelation::Affine { source_range, .. }
            | NativeParameterRelation::Sampled { source_range, .. } => *source_range,
        };
        if !valid_range(source_range) {
            return Err(EvaluationError::InvalidGeometry);
        }
        if target < self.target_range[0] || target > self.target_range[1] {
            return Err(EvaluationError::OutsideDomain);
        }
        if let NativeParameterRelation::Sampled {
            source_parameters: sp,
            target_parameters: tp,
            ..
        } = &self.relation
        {
            if !valid_samples(sp, tp, source_range, self.target_range) {
                return Err(EvaluationError::InvalidGeometry);
            }
            if let Some(i) = tp.iter().position(|p| p.to_bits() == target.to_bits()) {
                // A retained near-domain sample is not permission to extend
                // the declared source range when evaluating its image.
                if sp[i] < source_range[0] || sp[i] > source_range[1] {
                    return Err(EvaluationError::OutsideDomain);
                }
                return Ok(NativeTargetParameterQuery::Exact {
                    source_parameter: sp[i],
                    target_parameter: target,
                    stored_sample: true,
                });
            }
            for (i, w) in tp.windows(2).enumerate() {
                if w[0] < target && target < w[1] {
                    return Ok(NativeTargetParameterQuery::Bracket {
                        target_parameter: target,
                        first_sample_index: i as u32,
                        source_parameters: [sp[i], sp[i + 1]],
                        target_parameters: [w[0], w[1]],
                    });
                }
            }
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let source = match self.relation {
            NativeParameterRelation::Identity { .. } => target,
            NativeParameterRelation::Affine { scale, offset, .. } => {
                if !scale.is_finite() || scale == 0. || !offset.is_finite() {
                    return Err(EvaluationError::InvalidGeometry);
                }
                (target - offset) / scale
            }
            _ => unreachable!(),
        };
        if !source.is_finite() {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        let NativeParameterQuery::Exact {
            target_parameter, ..
        } = self.query_at_source(source)?
        else {
            return Err(EvaluationError::UnresolvedGeometry);
        };
        if target_parameter.to_bits() != target.to_bits() {
            return Err(EvaluationError::UnresolvedGeometry);
        }
        Ok(NativeTargetParameterQuery::Exact {
            source_parameter: source,
            target_parameter: target,
            stored_sample: false,
        })
    }

    /// Exact relation where defined by the payload. Sampled relations return
    /// only a stored pair; no interpolation, extrapolation or fitting is implied.
    #[must_use]
    pub fn image_at(&self, source: f64) -> Option<f64> {
        if !source.is_finite() {
            return None;
        }
        match &self.relation {
            NativeParameterRelation::Unspecified => None,
            NativeParameterRelation::Identity { source_range } => {
                (source_range[0] <= source && source <= source_range[1]).then_some(source)
            }
            NativeParameterRelation::Affine {
                source_range,
                scale,
                offset,
            } => {
                let value = source * scale + offset;
                (source_range[0] <= source && source <= source_range[1] && value.is_finite())
                    .then_some(value)
            }
            NativeParameterRelation::Sampled {
                target_parameters,
                source_parameters,
                ..
            } => source_parameters
                .iter()
                .zip(target_parameters)
                .find_map(|(s, t)| (s.to_bits() == source.to_bits()).then_some(*t)),
        }
    }
    pub(super) fn records(&self) -> usize {
        self.target_sites.len()
            + match &self.relation {
                NativeParameterRelation::Sampled {
                    source_parameters,
                    target_parameters,
                    ..
                } => source_parameters.len() + target_parameters.len(),
                _ => 0,
            }
            + 1
    }
    fn words(&self) -> Vec<f64> {
        let mut out = vec![
            self.target_range[0],
            self.target_range[1],
            self.signed_accuracy,
        ];
        out.extend(&self.target_sites);
        out.extend(self.fit_tolerance);
        match &self.relation {
            NativeParameterRelation::Unspecified => (),
            NativeParameterRelation::Identity { source_range } => out.extend(source_range),
            NativeParameterRelation::Affine {
                source_range,
                scale,
                offset,
            } => {
                out.extend(source_range);
                out.extend([*scale, *offset]);
            }
            NativeParameterRelation::Sampled {
                source_range,
                target_parameters,
                source_parameters,
            } => {
                out.extend(source_range);
                out.extend(target_parameters);
                out.extend(source_parameters);
            }
        }
        out
    }
    pub(super) fn bit_equal(&self, other: &Self) -> bool {
        self == other
            && self
                .words()
                .iter()
                .zip(other.words())
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}
pub(super) fn reference(c: &mut Cursor<'_>) -> Option<NativeAllocationReference> {
    use NativeAllocationEncoding::*;
    let start = c.p;
    let byte = c.byte()?;
    let (value, encoding) = if byte % 4 == 1 {
        (u32::from(byte / 4), BackwardDistance)
    } else if [4, 8, 12, 16].contains(&byte) {
        let value = c
            .take(usize::from(byte / 4))?
            .iter()
            .enumerate()
            .fold(0, |v, (i, b)| v | (u32::from(*b) << (8 * i)));
        (value, WidthCoded)
    } else if byte == 6 {
        (u32::from(c.byte()?), ExplicitU8)
    } else if byte == 10 {
        (
            u32::from(u16::from_le_bytes(c.take(2)?.try_into().ok()?)),
            ExplicitU16,
        )
    } else if byte != 0 && byte % 4 == 2 {
        (u32::from(byte / 4), Selector2)
    } else if byte % 4 == 3 {
        (u32::from(byte / 4), OwnedChild)
    } else {
        return None;
    };
    Some(NativeAllocationReference {
        value,
        encoding,
        raw: c.bytes[start..c.p].to_vec(),
    })
}
fn range(c: &mut Cursor<'_>) -> Option<[f64; 2]> {
    let r = [c.number()?, c.number()?];
    (r[0] < r[1] && (r[1] - r[0]).is_finite()).then_some(r)
}
fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-7 + 1e-12 * a.abs().max(b.abs())
}
fn inside(r: [f64; 2], x: f64) -> bool {
    let rounding = 32. * f64::EPSILON * r[0].abs().max(r[1].abs()).max(x.abs()).max(1.);
    x >= r[0] - rounding && x <= r[1] + rounding
}
pub(super) fn parse(bytes: &[u8], class: u8) -> Option<NativeParameterMapFields> {
    if ![0x24, 0x25].contains(&class) {
        return None;
    }
    let mut c = Cursor::new(bytes);
    if c.byte()? != 0x82 {
        return None;
    }
    let source_reference = reference(&mut c)?;
    let target_reference = reference(&mut c)?;
    let selector_lead = c.byte()?;
    if ![10, 11].contains(&selector_lead) {
        return None;
    }
    let selector = u16::from_le_bytes(c.take(2)?.try_into().ok()?);
    let target_range = range(&mut c)?;
    let signed_accuracy = c.number()?;
    let (mode, sites) = match (class, selector) {
        (0x24, 0x0587) => (0, 0),
        (0x24, 0x0586) => (0, 1),
        (0x24, 0x0585) => (0, 1),
        (0x24, 0x0584 | 0x0001) => (0, 2),
        (0x24, 0x0987) => (1, 0),
        (0x24, 0x0985) => (1, 1),
        (0x24, 0x0984) => (1, 2),
        (_, 0x0d87) => (2, 0),
        (_, 0x0d86 | 0x0d85) => (2, 1),
        (_, 0x0d84) => (2, 2),
        (0x24, 0x0003) => (2, 2),
        (0x25, 0x0585) => (0, 1),
        (0x25, 0x1587) => (3, 0),
        (0x25, 0x1586 | 0x1585) => (3, 1),
        (0x25, 0x1584) => (3, 2),
        _ => return None,
    };
    let relation = match mode {
        0 => NativeParameterRelation::Unspecified,
        1 => NativeParameterRelation::Identity {
            source_range: range(&mut c)?,
        },
        2 => {
            let source_range = range(&mut c)?;
            let scale = c.number()?;
            let offset = c.number()?;
            if scale == 0. {
                return None;
            }
            NativeParameterRelation::Affine {
                source_range,
                scale,
                offset,
            }
        }
        _ => {
            if selector_lead != 10 {
                return None;
            }
            let source_range = range(&mut c)?;
            let n = c.byte()?.checked_sub(128)? as usize;
            if !(2..=127).contains(&n) || n > bytes.len().saturating_sub(c.p) / 16 {
                return None;
            }
            let mut target_parameters = Vec::with_capacity(n);
            let mut source_parameters = Vec::with_capacity(n);
            for _ in 0..n {
                target_parameters.push(c.number()?);
            }
            for _ in 0..n {
                source_parameters.push(c.number()?);
            }
            if !target_parameters.windows(2).all(|w| w[0] < w[1])
                || !(source_parameters.windows(2).all(|w| w[0] < w[1])
                    || source_parameters.windows(2).all(|w| w[0] > w[1]))
                || !target_parameters.iter().all(|v| inside(target_range, *v))
                || !source_parameters.iter().all(|v| inside(source_range, *v))
            {
                return None;
            }
            NativeParameterRelation::Sampled {
                source_range,
                target_parameters,
                source_parameters,
            }
        }
    };
    let mut target_sites = Vec::with_capacity(sites);
    for _ in 0..sites {
        target_sites.push(c.number()?);
    }
    if target_sites.windows(2).any(|w| w[0] >= w[1])
        || target_sites.iter().any(|v| {
            !inside(target_range, *v)
                && !(class == 0x24 && (close(*v, target_range[0]) || close(*v, target_range[1])))
        })
    {
        return None;
    }
    let fit_tolerance = if selector_lead == 10 {
        let fit = c.number()?;
        if fit < signed_accuracy.abs()
            || (class == 0x24 && fit.to_bits() != signed_accuracy.abs().to_bits())
        {
            return None;
        }
        Some(fit)
    } else {
        if signed_accuracy != 0. {
            return None;
        }
        None
    };
    if !c.done() {
        return None;
    }
    match &relation {
        NativeParameterRelation::Identity { source_range } => {
            if !(0..2).all(|i| close(source_range[i], target_range[i])) {
                return None;
            }
        }
        NativeParameterRelation::Affine {
            source_range,
            scale,
            offset,
        } => {
            if !(0..2).all(|i| {
                close(
                    source_range[if *scale > 0. { i } else { 1 - i }] * scale + offset,
                    target_range[i],
                )
            }) {
                return None;
            }
        }
        _ => (),
    }
    Some(NativeParameterMapFields {
        source_reference,
        target_reference,
        selector,
        selector_lead,
        target_range,
        signed_accuracy,
        relation,
        target_sites,
        fit_tolerance,
    })
}
