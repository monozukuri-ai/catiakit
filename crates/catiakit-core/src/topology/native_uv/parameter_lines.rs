//! Parameter objects adjacent to a stored NURBS. Adjacency retains context;
//! the allocation operand does not assign a persistent support identity.
use super::*;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeParameterLineGeometry {
    ConstantU {
        u: f64,
        range: [f64; 2],
    },
    ConstantV {
        v: f64,
        range: [f64; 2],
    },
    Directed {
        origin: [f64; 2],
        direction: [f64; 2],
        range: [f64; 2],
    },
    UndirectedAxis {
        origin: [f64; 2],
        axis: usize,
        range: [f64; 2],
    },
}
impl NativeParameterLineGeometry {
    #[must_use]
    pub fn range(&self) -> [f64; 2] {
        match self {
            Self::ConstantU { range, .. }
            | Self::ConstantV { range, .. }
            | Self::Directed { range, .. }
            | Self::UndirectedAxis { range, .. } => *range,
        }
    }
    /// Stored UV point choices. An implicit-axis program retains both senses.
    #[must_use]
    pub fn images_at(&self, t: f64) -> Vec<[f64; 2]> {
        let r = self.range();
        if !t.is_finite() || t < r[0] || t > r[1] {
            return Vec::new();
        }
        let values = match self {
            Self::ConstantU { u, .. } => vec![[*u, t]],
            Self::ConstantV { v, .. } => vec![[t, *v]],
            Self::Directed {
                origin, direction, ..
            } => vec![[origin[0] + t * direction[0], origin[1] + t * direction[1]]],
            Self::UndirectedAxis { origin, axis, .. } if *axis < 2 => [-1., 1.]
                .into_iter()
                .map(|sense| {
                    let mut p = *origin;
                    p[*axis] += sense * t;
                    p
                })
                .collect(),
            _ => Vec::new(),
        };
        values
            .into_iter()
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .collect()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeParameterLineFields {
    pub allocation_operand: NativeAllocationReference,
    pub control: u8,
    pub geometry: NativeParameterLineGeometry,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeParameterLine {
    pub ordinal: usize,
    pub fields: NativeParameterLineFields,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeNurbsParameterObjects {
    pub carrier: String,
    pub run: u32,
    pub lines: Vec<NativeParameterLine>,
    pub source: StreamSpan,
    pub raw_frames_hex: String,
}
pub(super) fn parse(bytes: &[u8]) -> Option<NativeParameterLineFields> {
    let mut c = Cursor::new(bytes);
    let allocation_operand = parameter_maps::reference(&mut c)?;
    let control = c.byte()?;
    let geometry = if control < 0x20 && matches!(control % 4, 1 | 2) {
        let bits = [0x10, 0x04, 0x08];
        let mut tuple = [0.; 3];
        let mut stored = 0;
        for (v, bit) in tuple.iter_mut().zip(bits) {
            if control & bit == 0 {
                *v = c.number()?;
                stored += 1;
            }
        }
        if stored == 0 || tuple[1] >= tuple[2] {
            return None;
        }
        if control % 4 == 1 {
            NativeParameterLineGeometry::ConstantU {
                u: tuple[0],
                range: [tuple[1], tuple[2]],
            }
        } else {
            NativeParameterLineGeometry::ConstantV {
                v: tuple[0],
                range: [tuple[1], tuple[2]],
            }
        }
    } else if [0x00, 0x08, 0x40, 0x48, 0x80, 0x88].contains(&control) {
        let t = c.number()?;
        let upper = if control & 8 != 0 {
            if t >= 0. {
                return None;
            }
            -t
        } else {
            c.number()?
        };
        if t >= upper {
            return None;
        }
        let origin = [c.number()?, c.number()?];
        let range = [t, upper];
        if control & 0xc0 == 0 {
            let direction = [c.number()?, c.number()?];
            if (direction[0].hypot(direction[1]) - 1.).abs() > 1e-12 {
                return None;
            }
            NativeParameterLineGeometry::Directed {
                origin,
                direction,
                range,
            }
        } else {
            NativeParameterLineGeometry::UndirectedAxis {
                origin,
                axis: usize::from(control & 0xc0 == 0x40),
                range,
            }
        }
    } else {
        return None;
    };
    let range = geometry.range();
    let choices = if matches!(geometry, NativeParameterLineGeometry::UndirectedAxis { .. }) {
        2
    } else {
        1
    };
    if !(range[1] - range[0]).is_finite()
        || range
            .iter()
            .any(|t| geometry.images_at(*t).len() != choices)
    {
        return None;
    }
    c.done().then_some(NativeParameterLineFields {
        allocation_operand,
        control,
        geometry,
    })
}
pub(super) fn bit_equal(a: &NativeParameterLineFields, b: &NativeParameterLineFields) -> bool {
    fn words(g: &NativeParameterLineGeometry) -> Vec<f64> {
        let mut out = g.range().to_vec();
        match g {
            NativeParameterLineGeometry::ConstantU { u, .. } => out.push(*u),
            NativeParameterLineGeometry::ConstantV { v, .. } => out.push(*v),
            NativeParameterLineGeometry::Directed {
                origin, direction, ..
            } => {
                out.extend(origin);
                out.extend(direction);
            }
            NativeParameterLineGeometry::UndirectedAxis { origin, .. } => out.extend(origin),
        }
        out
    }
    a == b
        && words(&a.geometry)
            .iter()
            .zip(words(&b.geometry))
            .all(|(a, b)| a.to_bits() == b.to_bits())
}
