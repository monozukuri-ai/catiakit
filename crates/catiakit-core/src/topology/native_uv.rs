//! Length-closed consolidated native pcurves. Unbound historical records are
//! evidence, not current face/coedge assignments.
use super::*;
pub(super) mod allocations;
mod curve_images;
mod edge_traversal;
pub use curve_images::NativeNurbsCurveImage;
pub use edge_traversal::NativeEdgeMapTraversal;
mod paired_edges;
mod parameter_circles;
mod parameter_lines;
mod parameter_maps;
pub use paired_edges::{NativeEndpointImages, NativePairedUvEdge};
mod reference_contexts;
pub use allocations::{
    NativeBoundSupport, NativeBoundUvGeometry, NativeBoundUvUse, NativeBoundaryUv,
    NativeBoundaryUvSummary, NativeEdgeGapSample, NativeMapCoordinate, NativeMapCoordinateEncoding,
    NativePlaneSupport, NativeUvEdgeTrim, NativeUvEndpoint, NativeUvSupportChart,
    NativeUvUseOrientation, validate_native_boundary_uv,
};
pub use parameter_circles::NativeParameterCircleFields;
pub use parameter_lines::{
    NativeNurbsParameterObjects, NativeParameterLine, NativeParameterLineFields,
    NativeParameterLineGeometry,
};
pub use parameter_maps::{
    NativeAllocationEncoding, NativeAllocationReference, NativeParameterMap,
    NativeParameterMapFields, NativeParameterQuery, NativeParameterRelation,
    NativeTargetParameterQuery,
};
pub use reference_contexts::{
    NativeCountedReferenceFields, NativeCountedReferenceFrame, NativeParameterMapContext,
};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeUvModel {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_map_contexts: Vec<NativeParameterMapContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nurbs_parameter_objects: Vec<NativeNurbsParameterObjects>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_maps: Vec<NativeParameterMap>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ruled_carriers: Vec<NativeRuledCarrier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rolling_ball_carriers: Vec<NativeRollingBallCarrier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sphere_meridians: Vec<NativeSphereMeridian>,
    pub runs: Vec<NativeRecordRun>,
    pub pcurves: Vec<NativePcurve>,
    pub carriers: Vec<NativeCarrier>,
    pub nurbs_carriers: Vec<NativeNurbsCarrier>,
    pub jet_pairs: Vec<NativeJetPair>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRuledCarrier {
    pub id: String,
    pub run: u32,
    pub geometry: crate::geometry::RuledJet,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRollingBallCarrier {
    pub id: String,
    pub run: u32,
    pub geometry: crate::geometry::RollingBallJet,
    pub tail: NativeRollingBallTail,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRollingBallTail {
    pub current_limits: [f64; 4],
    pub maximal_limits: [f64; 4],
    pub reference: f64,
    pub control: u8,
    pub scalar_jet: Option<[Vec<f64>; 3]>,
    pub terminal: Option<u32>,
    pub reference_copy: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeSphereMeridian {
    pub carrier: String,
    pub normal: Vec3,
    pub source: StreamSpan,
    pub raw_frames_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeRecordRun {
    pub source: StreamSpan,
    pub frame_count: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePcurve {
    pub id: String,
    pub run: u32,
    pub support_tag: u32,
    pub leading_extrapolation_sites: u32,
    pub array_control: u8,
    pub frame_flag: u8,
    pub frame_width: u8,
    pub header_token: Vec<u8>,
    pub jet: UvJet,
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeCarrier {
    pub id: String,
    pub run: u32,
    pub class: u8,
    pub header_operand: u32,
    pub geometry: SurfaceGeometry,
    /// Retained unbound chart scale. Current allocation-bound uses carry a
    /// separately source-replayed `support_chart`, including sphere UV units.
    pub uv_scale: [f64; 2],
    /// Translation after scaling the native pcurve chart.
    #[serde(default)]
    pub uv_offset: [f64; 2],
    pub native_bounds: [f64; 4],
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeNurbsCarrier {
    pub id: String,
    pub run: u32,
    pub geometry: NurbsSurface,
    /// Serialized span-local scalars; these are not global knot coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_span_indices: Option<[u32; 4]>,
    /// Resolved original-knot bounds, before the retained affine parameter map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knot_limits: Option<[f64; 4]>,
    pub current_limits: [f64; 4],
    /// Retained original-to-current parameter pairs (scale, offset).
    pub parameter_map: [[f64; 2]; 2],
    pub source: StreamSpan,
    pub raw_frame_hex: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeJetPair {
    pub run: u32,
    pub pcurves: [String; 2],
    pub parameter_range: [f64; 2],
    pub tolerance: f64,
    pub source: StreamSpan,
    pub local_slots: [u32; 2],
    pub selector_raw: Vec<u8>,
    pub raw_packet_hex: String,
}

#[derive(Clone)]
struct Frame {
    start: usize,
    payload: usize,
    end: usize,
    class: u8,
    flag: u8,
    width: u8,
    family: u8,
    token: u32,
    token_bytes: Vec<u8>,
}
struct Cursor<'a> {
    bytes: &'a [u8],
    p: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, p: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let q = self.p.checked_add(n)?;
        let out = self.bytes.get(self.p..q)?;
        self.p = q;
        Some(out)
    }
    fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn compact(&mut self) -> Option<u32> {
        let x = self.byte()?;
        if x % 4 == 1 {
            return Some(u32::from(x / 4));
        }
        let width = match x {
            4 => 1,
            8 => 2,
            12 => 3,
            16 => 4,
            _ => return None,
        };
        Some(
            self.take(width)?
                .iter()
                .enumerate()
                .fold(0, |a, (i, b)| a | (u32::from(*b) << (8 * i))),
        )
    }
    fn number(&mut self) -> Option<f64> {
        let x = f64::from_le_bytes(self.take(8)?.try_into().ok()?);
        x.is_finite().then_some(x)
    }
    fn vector(&mut self) -> Option<Vec3> {
        Some([self.number()?, self.number()?, self.number()?])
    }
    fn allocation(&mut self) -> Option<Vec<u8>> {
        let start = self.p;
        let x = *self.bytes.get(start)?;
        if x % 4 == 1 || [4, 8, 12, 16].contains(&x) {
            self.compact()?;
        } else if x == 6 {
            self.take(2)?;
        } else if x == 10 {
            self.take(3)?;
        } else if x != 0 && matches!(x % 4, 2 | 3) {
            self.take(1)?;
        } else {
            return None;
        }
        Some(self.bytes[start..self.p].to_vec())
    }
    fn done(&self) -> bool {
        self.p == self.bytes.len()
    }
}
fn frame(bytes: &[u8], p: usize) -> Option<Frame> {
    let lead = *bytes.get(p)?;
    let flag = *bytes.get(p + 1)?;
    if ![3, 19, 131].contains(&flag) {
        return None;
    }
    let (width, h, n, family) = match lead {
        165..=169 => (
            lead - 164,
            7,
            u32::from_le_bytes(bytes.get(p + 3..p + 7)?.try_into().ok()?) as usize,
            b'A',
        ),
        178..=182 => (lead - 177, 4, usize::from(*bytes.get(p + 3)?), b'B'),
        _ => return None,
    };
    let payload = p.checked_add(h + usize::from(width))?;
    let end = payload.checked_add(n)?;
    if n == 0 || end > bytes.len() || bytes[p + 2] > 0x65 {
        return None;
    }
    let mut c = Cursor::new(bytes.get(p + h..payload)?);
    let token = c.compact()?;
    if !c.done() {
        return None;
    }
    Some(Frame {
        start: p,
        payload,
        end,
        class: bytes[p + 2],
        flag,
        width,
        family,
        token,
        token_bytes: bytes[p + h..payload].to_vec(),
    })
}
fn jet(bytes: &[u8]) -> Option<(u32, u32, u8, UvJet)> {
    let mut c = Cursor::new(bytes);
    let support = c.compact()?;
    if c.compact()? != 5 {
        return None;
    }
    let n = c.compact()? as usize;
    if !(2..=8192).contains(&n) || n > bytes.len() / 56 {
        return None;
    }
    let extrapolation = match c.byte()? {
        12 => 0,
        8 => {
            let x = c.byte()?;
            if x % 4 != 1 {
                return None;
            }
            u32::from(x / 4)
        }
        _ => return None,
    };
    let mut knots = Vec::with_capacity(n);
    for _ in 0..n {
        knots.push(c.number()?);
    }
    if c.compact()? as usize != n {
        return None;
    }
    let control = c.byte()?;
    if control % 4 != 1 {
        return None;
    }
    let mut lanes = Vec::with_capacity(6);
    for j in 0..6 {
        if j == 4 && c.byte()? != 5 {
            return None;
        }
        let mut a = Vec::with_capacity(n);
        for _ in 0..n {
            a.push(c.number()?);
        }
        lanes.push(a);
    }
    let parameter_range = [c.number()?, c.number()?];
    if !matches!(bytes.get(c.p..), Some([7]) | Some([7, 0])) {
        return None;
    }
    let pair = |i: usize| (0..n).map(|j| [lanes[i][j], lanes[i + 1][j]]).collect();
    let out = UvJet {
        knots,
        positions: pair(0),
        first: pair(2),
        second: pair(4),
        parameter_range,
    };
    out.is_valid()
        .then_some((support, extrapolation, control, out))
}
type DecodedCarrier = (SurfaceGeometry, [f64; 2], [f64; 2], [f64; 4]);
fn masked_cylinder(bytes: &[u8]) -> Option<DecodedCarrier> {
    let mut c = Cursor::new(bytes);
    let origin = c.vector()?;
    let mask = c.byte()?;
    if mask > 0x3f {
        return None;
    }
    let explicit = 122usize.checked_sub(8 * mask.count_ones() as usize)?;
    if bytes.len() != explicit && bytes.len() + 8 != explicit {
        return None;
    }
    let mut axes = [0.; 6];
    for (i, value) in axes.iter_mut().enumerate() {
        if mask & (1 << i) == 0 {
            *value = c.number()?;
        }
    }
    let x = [axes[0], axes[1], axes[2]];
    let y = [axes[3], axes[4], axes[5]];
    let frame = Frame3 {
        origin,
        x_axis: x,
        y_axis: y,
        z_axis: cross(x, y),
    };
    let radius = c.number()?;
    let bounds = [c.number()?, c.number()?, c.number()?, c.number()?];
    let span = bounds[1] - bounds[0];
    let full = std::f64::consts::TAU * radius;
    if !frame.is_valid()
        || radius <= 0.
        || !full.is_finite()
        || !span.is_finite()
        || span <= 0.
        || span > full + 1e-6
        || bounds[2] >= bounds[3]
    {
        return None;
    }
    match c.byte()? {
        3 if bytes.len() == explicit => {
            let shift = c.number()?;
            let expected = (bounds[0] + bounds[1]) / 2. - std::f64::consts::PI * radius;
            let rounding = 32. * f64::EPSILON * (bounds[0].abs() + bounds[1].abs() + full).max(1.);
            if !expected.is_finite() || (shift - expected).abs() > rounding {
                return None;
            }
        }
        7 if bytes.len() + 8 == explicit && (span - full).abs() <= 1e-6 => (),
        _ => return None,
    }
    if !c.done() || !(1. / radius).is_finite() {
        return None;
    }
    Some((
        SurfaceGeometry::Cylinder { frame, radius },
        [1. / radius, 1.],
        [0.; 2],
        bounds,
    ))
}
fn carrier(bytes: &[u8], class: u8) -> Option<DecodedCarrier> {
    if class == 40
        && let Some(value) = masked_cylinder(bytes)
    {
        return Some(value);
    }
    let mut c = Cursor::new(bytes);
    let origin = c.vector()?;
    let mut offset = [0.; 2];
    let (geometry, scale, bounds) = match class {
        42 if bytes.len() == 152 => {
            let x = c.vector()?;
            let y = c.vector()?;
            let z = c.vector()?;
            let r = c.number()?;
            let bounds = [c.number()?, c.number()?, c.number()?, c.number()?];
            if r <= 0. || c.number()?.to_bits() != r.to_bits() {
                return None;
            }
            let shift = c.number()?;
            if bounds[1] - bounds[0] > std::f64::consts::TAU + 1e-12
                || bounds[2] < -std::f64::consts::FRAC_PI_2
                || bounds[3] > std::f64::consts::FRAC_PI_2
            {
                return None;
            }
            let expected = r * ((bounds[0] + bounds[1]) / 2. - std::f64::consts::PI);
            let rounding = 64.
                * f64::EPSILON
                * r.abs().max(1.)
                * (bounds[0].abs() + bounds[1].abs() + std::f64::consts::PI);
            if !expected.is_finite() || (shift - expected).abs() > rounding {
                return None;
            }
            (
                SurfaceGeometry::Sphere {
                    frame: Frame3 {
                        origin,
                        x_axis: super::scale(x, 1. / r),
                        y_axis: super::scale(y, 1. / r),
                        z_axis: super::scale(z, 1. / r),
                    },
                    radius: r,
                },
                [1., 1.],
                bounds,
            )
        }
        41 if bytes.len() == 184 => {
            let mut frame = Frame3 {
                origin,
                x_axis: c.vector()?,
                y_axis: c.vector()?,
                z_axis: c.vector()?,
            };
            let angle = c.number()?;
            let r = c.number()?;
            let bounds = [c.number()?, c.number()?, c.number()?, c.number()?];
            let scale = c.number()?;
            if r < 0. || scale <= 0. || c.number()? != 1. || c.number()? != 0. {
                return None;
            }
            let domain = [c.number()?, c.number()?];
            if !angular_domain([bounds[0], bounds[1]], domain) {
                return None;
            }
            if !(0. ..std::f64::consts::FRAC_PI_2).contains(&angle)
                || angle == 0.
                || !frame.is_valid()
            {
                return None;
            }
            let shift = r / angle.cos();
            if !shift.is_finite() || bounds[2] + shift < 0. {
                return None;
            }
            frame.origin = sub(origin, super::scale(frame.z_axis, shift * angle.sin()));
            offset = [0., shift];
            (
                SurfaceGeometry::Cone {
                    frame,
                    half_angle: std::f64::consts::FRAC_PI_2 - angle,
                },
                [1. / scale, 1.],
                bounds,
            )
        }
        43 if bytes.len() == 200 => {
            let frame = Frame3 {
                origin,
                x_axis: c.vector()?,
                y_axis: c.vector()?,
                z_axis: c.vector()?,
            };
            let major_radius = c.number()?;
            let minor_radius = c.number()?;
            let u = [c.number()?, c.number()?];
            let ud = [c.number()?, c.number()?];
            let v = [c.number()?, c.number()?];
            let vd = [c.number()?, c.number()?];
            let su = c.number()?;
            let sv = c.number()?;
            if su <= 0.
                || sv <= 0.
                || c.number()? != 0.
                || !angular_domain(u, ud)
                || !angular_domain(v, vd)
            {
                return None;
            }
            (
                SurfaceGeometry::Torus {
                    frame,
                    major_radius,
                    minor_radius,
                },
                [1. / su, 1. / sv],
                [u[0], u[1], v[0], v[1]],
            )
        }
        40 if [82, 90, 98].contains(&bytes.len()) => {
            let token = c.byte()?;
            let (x, z) = match (bytes.len(), token) {
                (82, 29) => {
                    if c.number()? != 1. || c.number()? != 1. {
                        return None;
                    }
                    ([0., 1., 0.], [1., 0., 0.])
                }
                (90, 25 | 28) | (98, 14) => {
                    let a = c.number()?;
                    let b = c.number()?;
                    if c.number()? != 1. {
                        return None;
                    }
                    match token {
                        25 => ([-b, a, 0.], [a, b, 0.]),
                        28 => ([a, b, 0.], [b, -a, 0.]),
                        _ => ([a, 0., b], [0., 1., 0.]),
                    }
                }
                _ => return None,
            };
            let r = c.number()?;
            let bounds = [c.number()?, c.number()?, c.number()?, c.number()?];
            let full = std::f64::consts::TAU * r;
            let span = bounds[1] - bounds[0];
            if !full.is_finite()
                || !span.is_finite()
                || (bytes.len() != 98 && (span - full).abs() > 1e-6)
                || span > full + 1e-6
            {
                return None;
            }
            let terminal = c.byte()?;
            if bytes.len() == 98 {
                if terminal != 3 {
                    return None;
                }
                let shift = c.number()?;
                if shift.to_bits()
                    != ((bounds[0] + bounds[1]) / 2. - std::f64::consts::PI * r).to_bits()
                {
                    return None;
                }
            } else if terminal != 7 {
                return None;
            }
            (
                SurfaceGeometry::Cylinder {
                    frame: Frame3 {
                        origin,
                        x_axis: x,
                        y_axis: cross(z, x),
                        z_axis: z,
                    },
                    radius: r,
                },
                [1. / r, 1.],
                bounds,
            )
        }
        _ => return None,
    };
    if !c.done()
        || bounds[0] >= bounds[1]
        || bounds[2] >= bounds[3]
        || !scale.iter().all(|x| x.is_finite())
    {
        return None;
    }
    geometry
        .evaluate([
            bounds[0] * scale[0] + offset[0],
            bounds[2] * scale[1] + offset[1],
        ])
        .ok()?;
    Some((geometry, scale, offset, bounds))
}
fn angular_domain(range: [f64; 2], domain: [f64; 2]) -> bool {
    (domain[1] - domain[0] - std::f64::consts::TAU).abs() <= 1e-10
        && range[0] >= domain[0] - 1e-12
        && range[1] <= domain[1] + 1e-12
        && ((range[0] / 2. + range[1] / 2.) - (domain[0] / 2. + domain[1] / 2.)).abs() <= 1e-12
}
fn ruled(bytes: &[u8]) -> Option<crate::geometry::RuledJet> {
    let mut c = Cursor::new(bytes);
    let n = c.compact()? as usize;
    if !(2..=8192).contains(&n)
        || n > bytes.len() / 152
        || c.compact()? != 5
        || c.compact()? as usize != n
    {
        return None;
    }
    match c.byte()? {
        12 => (),
        8 if c.byte()? % 4 == 1 => (),
        _ => return None,
    }
    let mut knots = Vec::with_capacity(n);
    for _ in 0..n {
        knots.push(c.number()?)
    }
    let mut blocks = Vec::new();
    for _ in 0..3 {
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            rows.push([
                c.number()?,
                c.number()?,
                c.number()?,
                c.number()?,
                c.number()?,
                c.number()?,
            ])
        }
        blocks.push(rows);
    }
    let geometry = crate::geometry::RuledJet {
        knots,
        positions: blocks.remove(0),
        first: blocks.remove(0),
        second: blocks.remove(0),
        current_limits: [c.number()?, c.number()?, c.number()?, c.number()?],
        reference_limits: [c.number()?, c.number()?],
    };
    (c.done() && geometry.is_valid()).then_some(geometry)
}
type NurbsDecoded = (
    NurbsSurface,
    [f64; 4],
    [[f64; 2]; 2],
    [u32; 4],
    Option<[f64; 4]>,
);
fn nurbs(bytes: &[u8]) -> Option<NurbsDecoded> {
    let mut c = Cursor::new(bytes);
    let mut degrees = [0; 2];
    let mut counts = [0; 2];
    let mut knots = [Vec::new(), Vec::new()];
    for d in 0..2 {
        let degree = c.compact()? as usize;
        let n = c.compact()? as usize;
        if ![1, 3, 5].contains(&degree) || !(2..=8192).contains(&n) || n > bytes.len() / 8 {
            return None;
        }
        match c.byte()? {
            12 => {}
            8 => {
                if c.byte()? % 4 != 1 {
                    return None;
                }
            }
            _ => return None,
        };
        let mut distinct = Vec::with_capacity(n);
        for _ in 0..n {
            distinct.push(c.number()?);
        }
        if !distinct.windows(2).all(|w| w[0] < w[1]) {
            return None;
        }
        for (i, k) in distinct.into_iter().enumerate() {
            let m = if i == 0 || i + 1 == n {
                degree + 1
            } else if degree == 5 {
                3
            } else {
                1
            };
            knots[d].extend(std::iter::repeat_n(k, m));
        }
        degrees[d] = degree;
        counts[d] = knots[d].len() - degree - 1;
    }
    let n = counts[0].checked_mul(counts[1])?;
    if n == 0 || n > 65536 || n > bytes.len().saturating_sub(c.p) / 24 {
        return None;
    }
    let mode = c.byte()?;
    if ![1, 5].contains(&mode) {
        return None;
    }
    let mut control_points = Vec::with_capacity(n);
    for _ in 0..n {
        control_points.push(c.vector()?);
    }
    let mut weights = vec![1.; n];
    if mode == 5 {
        if *bytes.get(c.p)? == 0 {
            c.byte()?;
            for w in &mut weights {
                *w = c.number()?;
            }
        } else {
            for row in 0..counts[0] {
                match c.byte()? {
                    2 if row > 0 => {
                        for j in 0..counts[1] {
                            weights[row * counts[1] + j] = weights[(row - 1) * counts[1] + j];
                        }
                    }
                    1 => {
                        if ![3, 7].contains(&c.byte()?) || c.byte()? != 0 {
                            return None;
                        }
                        for j in 0..counts[1].div_ceil(2) {
                            let w = c.number()?;
                            weights[row * counts[1] + j] = w;
                            weights[row * counts[1] + counts[1] - j - 1] = w;
                        }
                    }
                    _ => return None,
                }
            }
        }
    }
    let tail_start = c.p;
    let remaining = bytes.len() - tail_start;
    if ![133, 141, 142].contains(&remaining) {
        return None;
    }
    let markers = [c.byte()?, c.byte()?, c.byte()?, c.byte()?];
    if markers[0] != 5 || markers[1] % 4 != 1 || markers[2] != 5 || markers[3] % 4 != 1 {
        return None;
    }
    let spans = markers.map(|m| u32::from(m / 4));
    let limits = [c.number()?, c.number()?, c.number()?, c.number()?];
    let map = [[c.number()?, c.number()?], [c.number()?, c.number()?]];
    if limits[0] >= limits[1] || limits[2] >= limits[3] || map.iter().any(|p| p[0] == 0.) {
        return None;
    }
    let flags = c.take(3)?;
    if flags != [1, 1, 1] && (remaining == 133 || flags != [5, 5, 1]) {
        return None;
    }
    for _ in 0..if remaining == 133 { 7 } else { 8 } {
        let v = c.number()?;
        if remaining == 133 && v != 0. {
            return None;
        }
    }
    let suffix = c.take(if remaining == 142 { 7 } else { 6 })?;
    let closed = if remaining == 142 {
        suffix[0] % 4 == 1
            && suffix[1..4] == [0, 9, 1]
            && suffix[4] % 4 == 1
            && suffix[5..] == [7, 7]
    } else {
        suffix == [1, 0, 1, 0, 7, 7] || (remaining == 141 && suffix == [9, 0, 9, 0, 7, 7])
    };
    if !closed || !c.done() {
        return None;
    }
    let geometry = NurbsSurface {
        degrees,
        knots,
        control_counts: counts,
        control_points,
        weights,
    };
    if !geometry.is_valid() {
        return None;
    }
    let knot_limits = (|| {
        let mut out = [0.; 4];
        for axis in 0..2 {
            let mut distinct = geometry.knots[axis].clone();
            distinct.dedup();
            for end in 0..2 {
                let index = spans[2 * axis + end].checked_sub(1)? as usize;
                let (a, z) = (*distinct.get(index)?, *distinct.get(index + 1)?);
                let local = limits[2 * axis + end];
                let eps = 64. * f64::EPSILON * a.abs().max(z.abs()).max(1.);
                if local < -eps || local > z - a + eps {
                    return None;
                }
                out[2 * axis + end] = a + local;
            }
        }
        Some(out)
    })();
    Some((geometry, limits, map, spans, knot_limits))
}
fn rolling(bytes: &[u8]) -> Option<(crate::geometry::RollingBallJet, NativeRollingBallTail)> {
    let mut c = Cursor::new(bytes);
    let n = c.compact()? as usize;
    if !(2..=8192).contains(&n) || c.compact()? != 5 || c.compact()? as usize != n {
        return None;
    }
    match c.byte()? {
        12 => {}
        8 => {
            if c.byte()? % 4 != 1 {
                return None;
            }
        }
        _ => return None,
    }
    if n > bytes.len().saturating_sub(c.p) / 248 {
        return None;
    }
    let mut knots = Vec::with_capacity(n);
    for _ in 0..n {
        knots.push(c.number()?)
    }
    let mut lanes: [Vec<[f64; 10]>; 3] = std::array::from_fn(|_| Vec::with_capacity(n));
    for lane in &mut lanes {
        for _ in 0..n {
            let mut row = [0.; 10];
            for x in &mut row {
                *x = c.number()?
            }
            lane.push(row)
        }
    }
    let tail_length = bytes.len() - c.p;
    let mut current_limits = [0.; 4];
    for x in &mut current_limits {
        *x = c.number()?
    }
    let reference = c.number()?;
    let control = c.byte()?;
    if reference <= 0.
        || reference.to_bits() != current_limits[3].to_bits()
        || ![3, 5].contains(&control)
    {
        return None;
    }
    let maximal_v = [c.number()?, c.number()?];
    let (scalar_jet, terminal, reference_copy) = if tail_length == 79 || tail_length == 87 {
        if c.take(6)? != [5, 0, 0, 0, 0, 0] {
            return None;
        }
        (None, None, tail_length == 87)
    } else if tail_length == 87 + 24 * n {
        if c.take(2)? != [1, 1] {
            return None;
        }
        let mut lanes: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::with_capacity(n));
        for row in &mut lanes {
            for _ in 0..n {
                row.push(c.number()?)
            }
        }
        let terminal = u32::from_le_bytes(c.take(4)?.try_into().ok()?);
        if terminal > 1 {
            return None;
        }
        (Some(lanes), Some(terminal), true)
    } else {
        return None;
    };
    let maximal_u = [c.number()?, c.number()?];
    if reference_copy && c.number()?.to_bits() != reference.to_bits() {
        return None;
    }
    if !c.done()
        || current_limits[0] >= current_limits[1]
        || current_limits[2] >= current_limits[3]
        || maximal_u[0] > current_limits[0]
        || maximal_u[1] < current_limits[1]
        || maximal_v[0] > current_limits[2]
        || maximal_v[1] < current_limits[3]
        || maximal_u[0] < knots[0]
        || maximal_u[1] > knots[n - 1]
    {
        return None;
    }
    let [positions, first, second] = lanes;
    let geometry = crate::geometry::RollingBallJet {
        knots,
        positions,
        first,
        second,
        parameter_range: [current_limits[0], current_limits[1]],
    };
    geometry.is_valid().then_some((
        geometry,
        NativeRollingBallTail {
            current_limits,
            maximal_limits: [maximal_u[0], maximal_u[1], maximal_v[0], maximal_v[1]],
            reference,
            control,
            scalar_jet,
            terminal,
            reference_copy,
        },
    ))
}
type ParameterPacket = ([f64; 2], f64, [u32; 2], Vec<u8>);
fn sphere_meridian(bytes: &[u8], frames: &[Frame]) -> Option<Vec3> {
    if frames.len() != 4
        || !frames.iter().all(|f| f.family == b'B')
        || !frames.iter().map(|f| f.class).eq([42, 24, 24, 24])
    {
        return None;
    }
    let (geometry, _, _, bounds) = carrier(&bytes[frames[0].payload..frames[0].end], 42)?;
    let SurfaceGeometry::Sphere { frame, radius } = geometry else {
        return None;
    };
    if (bounds[1] - bounds[0] - std::f64::consts::TAU).abs() > 1e-12
        || (bounds[2] + std::f64::consts::FRAC_PI_2).abs() > 1e-12
        || (bounds[3] - std::f64::consts::FRAC_PI_2).abs() > 1e-12
    {
        return None;
    }
    for (i, f) in frames[1..].iter().enumerate() {
        let mut c = Cursor::new(&bytes[f.payload..f.end]);
        if c.compact()? != i as u32 + 1 {
            return None;
        }
        let control = c.byte()?;
        let t0 = c.number()?;
        let u = c.number()?;
        let v = c.number()?;
        if v != 0.
            || t0 >= 0.
            || (t0 + std::f64::consts::FRAC_PI_2 * radius).abs() > 1e-10 * radius.max(1.)
        {
            return None;
        }
        if control == 8 {
            if c.number()?.abs() > 1e-12 || (c.number()?.abs() - 1.).abs() > 1e-12 {
                return None;
            }
        } else if control != 0x48 {
            return None;
        }
        let expected = radius
            * if i == 0 {
                bounds[0]
            } else if i == 1 {
                bounds[1]
            } else {
                bounds[0] / 2. + bounds[1] / 2.
            };
        if !c.done() || (u - expected).abs() > 1e-10 * radius.max(1.) {
            return None;
        }
    }
    unit(cross(
        frame.z_axis,
        add(
            scale(frame.x_axis, bounds[0].cos()),
            scale(frame.y_axis, bounds[0].sin()),
        ),
    ))
}
fn packet(bytes: &[u8]) -> Option<ParameterPacket> {
    let mut c = Cursor::new(bytes);
    if c.byte()? != 130 {
        return None;
    }
    let slots = [c.compact()?, c.compact()?];
    let selector = c.allocation()?;
    let mut values = Vec::new();
    while !c.done() {
        values.push(c.number()?);
    }
    if values.len() != 9
        || values[0] >= values[1]
        || values[2] <= 0.
        || values[5] != 1.
        || values[0] != values[3]
        || values[0] != values[6]
        || values[1] != values[4]
        || values[1] != values[7]
        || values[2].to_bits() != values[8].to_bits()
        || !matches!(slots, [1, 2] | [2, 1])
    {
        return None;
    }
    Some(([values[0], values[1]], values[2], slots, selector))
}

pub(super) fn decode(
    data: &[u8],
    container: &ContainerInfo,
    b: &mut Budget<'_>,
) -> Result<NativeUvModel> {
    let mut out = NativeUvModel::default();
    let owners: BTreeSet<_> = container
        .directories
        .iter()
        .flat_map(|d| d.owner_stream_ids.iter().copied())
        .collect();
    for stream in &container.streams {
        if stream.logical_length < 12 || owners.contains(&stream.id) {
            continue;
        }
        b.bytes = b.bytes.saturating_add(stream.logical_length);
        if b.bytes > b.limits.max_topology_bytes {
            return Err(fail(
                Code::TopologyByteLimitExceeded,
                "native UV source bytes",
            ));
        }
        let bytes = crate::container::reconstruct_stream(data, stream, b.limits)
            .map_err(|d| fail(d.code, d.message))?;
        b.work(bytes.len() as u64)?;
        let mut p = 0;
        while p < bytes.len() {
            let Some(first) = frame(&bytes, p) else {
                p += 1;
                continue;
            };
            let mut fs = vec![first];
            let mut end = fs[0].end;
            while let Some(next) = frame(&bytes, end) {
                b.add(1)?;
                end = next.end;
                fs.push(next);
            }
            if fs.len() < 2 {
                // Do not resynchronize into a complete frame's opaque payload.
                p = end;
                continue;
            }
            p = end;
            let run = out.runs.len() as u32;
            let mut local = BTreeMap::new();
            let mut local_nurbs = BTreeMap::new();
            let before = (
                out.parameter_maps.len(),
                out.ruled_carriers.len(),
                out.pcurves.len(),
                out.carriers.len(),
                out.nurbs_carriers.len(),
                out.rolling_ball_carriers.len(),
            );
            for (i, f) in fs.iter().enumerate() {
                b.work((f.end - f.payload) as u64)?;
                let payload = &bytes[f.payload..f.end];
                if f.flag == 3
                    && (f.family == b'B' || f.class == 0x25)
                    && let Some(fields) = parameter_maps::parse(payload, f.class)
                {
                    b.add(fields.records())?;
                    out.parameter_maps.push(NativeParameterMap {
                        id: format!("native:s{}:map{}", stream.id, f.start),
                        run,
                        class: f.class,
                        frame_flag: f.flag,
                        frame_width: f.width,
                        header_token: f.token_bytes.clone(),
                        fields,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect(),
                    });
                    if let Some(window) = fs.get(i..i + 3)
                        && let Some(context) = reference_contexts::decode(
                            &bytes,
                            window,
                            stream,
                            out.parameter_maps.last().unwrap(),
                            b,
                        )?
                    {
                        out.parameter_map_contexts.push(context);
                    }
                }
                if f.family == b'A'
                    && f.class == 0x39
                    && f.flag == 3
                    && let Some(geometry) = ruled(payload)
                {
                    b.add(geometry.knots.len().saturating_mul(19).saturating_add(1))?;
                    out.ruled_carriers.push(NativeRuledCarrier {
                        id: format!("native:s{}:ruled{}", stream.id, f.start),
                        run,
                        geometry,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect(),
                    });
                }
                if f.family == b'A'
                    && f.class == 50
                    && let Some((geometry, tail)) = rolling(payload)
                {
                    b.add(geometry.knots.len().saturating_mul(31).saturating_add(1))?;
                    out.rolling_ball_carriers.push(NativeRollingBallCarrier {
                        id: format!("native:s{}:rolling{}", stream.id, f.start),
                        run,
                        geometry,
                        tail,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|v| format!("{v:02x}"))
                            .collect(),
                    });
                }
                if f.class == 32
                    && let Some((tag, extra, control, jet)) = jet(payload)
                {
                    b.add(jet.knots.len() * 7 + 1)?;
                    let id = format!("native:s{}:pc{}", stream.id, f.start);
                    local.insert(i, id.clone());
                    out.pcurves.push(NativePcurve {
                        id,
                        run,
                        support_tag: tag,
                        leading_extrapolation_sites: extra,
                        array_control: control,
                        frame_flag: f.flag,
                        frame_width: f.width,
                        header_token: f.token_bytes.clone(),
                        jet,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|x| format!("{x:02x}"))
                            .collect(),
                    });
                }
                if f.family == b'B'
                    && let Some((geometry, uv_scale, uv_offset, native_bounds)) =
                        carrier(payload, f.class)
                {
                    b.add(1)?;
                    out.carriers.push(NativeCarrier {
                        id: format!("native:s{}:surface{}", stream.id, f.start),
                        run,
                        class: f.class,
                        header_operand: f.token,
                        geometry,
                        uv_scale,
                        uv_offset,
                        native_bounds,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|x| format!("{x:02x}"))
                            .collect(),
                    });
                }
                if f.class == 52
                    && f.family == b'A'
                    && let Some((
                        geometry,
                        current_limits,
                        parameter_map,
                        limit_span_indices,
                        knot_limits,
                    )) = nurbs(payload)
                {
                    b.add(
                        geometry.control_points.len() * 2
                            + geometry.knots.iter().map(Vec::len).sum::<usize>()
                            + 2,
                    )?;
                    let id = format!("native:s{}:nurbs{}", stream.id, f.start);
                    local_nurbs.insert(f.start, id.clone());
                    out.nurbs_carriers.push(NativeNurbsCarrier {
                        id,
                        run,
                        geometry,
                        current_limits,
                        parameter_map,
                        limit_span_indices: Some(limit_span_indices),
                        knot_limits,
                        source: span(stream, f.start, f.end, b)?,
                        raw_frame_hex: bytes[f.start..f.end]
                            .iter()
                            .map(|x| format!("{x:02x}"))
                            .collect(),
                    });
                }
            }
            if before
                == (
                    out.parameter_maps.len(),
                    out.ruled_carriers.len(),
                    out.pcurves.len(),
                    out.carriers.len(),
                    out.nurbs_carriers.len(),
                    out.rolling_ball_carriers.len(),
                )
            {
                continue;
            }
            out.runs.push(NativeRecordRun {
                source: span(stream, fs[0].start, end, b)?,
                frame_count: fs.len() as u32,
            });
            for window in fs.windows(5) {
                if window[0].family != b'A'
                    || window[0].class != 52
                    || !window[1..]
                        .iter()
                        .all(|f| f.family == b'B' && f.class == 0x18 && f.flag == 3)
                {
                    continue;
                }
                let Some(carrier) = local_nurbs.get(&window[0].start) else {
                    continue;
                };
                let mut lines = Vec::new();
                for (ordinal, f) in window[1..].iter().enumerate() {
                    if let Some(fields) = parameter_lines::parse(&bytes[f.payload..f.end]) {
                        b.add(1)?;
                        lines.push(NativeParameterLine {
                            ordinal,
                            fields,
                            source: span(stream, f.start, f.end, b)?,
                        });
                    }
                }
                if !lines.is_empty() {
                    b.add(1)?;
                    out.nurbs_parameter_objects
                        .push(NativeNurbsParameterObjects {
                            carrier: carrier.clone(),
                            run,
                            lines,
                            source: span(stream, window[0].start, window[4].end, b)?,
                            raw_frames_hex: bytes[window[0].start..window[4].end]
                                .iter()
                                .map(|v| format!("{v:02x}"))
                                .collect(),
                        });
                }
            }
            for (i, window) in fs.windows(3).enumerate() {
                if window.iter().map(|f| f.class).eq([32, 32, 35])
                    && window[0].family == window[1].family
                    && window[2].family == b'B'
                    && let (Some(a), Some(c)) = (local.get(&i), local.get(&(i + 1)))
                    && let Some((parameter_range, tolerance, local_slots, selector_raw)) =
                        packet(&bytes[window[2].payload..window[2].end])
                {
                    let aa = out.pcurves.iter().find(|p| &p.id == a).unwrap();
                    let cc = out.pcurves.iter().find(|p| &p.id == c).unwrap();
                    if aa.jet.knots != cc.jet.knots
                        || aa.jet.parameter_range != parameter_range
                        || cc.jet.parameter_range != parameter_range
                    {
                        continue;
                    }
                    b.add(1)?;
                    out.jet_pairs.push(NativeJetPair {
                        run,
                        pcurves: [a.clone(), c.clone()],
                        parameter_range,
                        tolerance,
                        local_slots,
                        selector_raw,
                        source: span(stream, window[0].start, window[2].end, b)?,
                        raw_packet_hex: bytes[window[2].start..window[2].end]
                            .iter()
                            .map(|x| format!("{x:02x}"))
                            .collect(),
                    });
                }
            }
            for window in fs.windows(4) {
                if let Some(normal) = sphere_meridian(&bytes, window)
                    && let Some(carrier) = out.carriers.iter().find(|c| {
                        c.source.stream_id == stream.id
                            && c.source.logical_offset == window[0].start as u64
                    })
                {
                    b.add(1)?;
                    out.sphere_meridians.push(NativeSphereMeridian {
                        carrier: carrier.id.clone(),
                        normal,
                        source: span(stream, window[0].start, window[3].end, b)?,
                        raw_frames_hex: bytes[window[0].start..window[3].end]
                            .iter()
                            .map(|x| format!("{x:02x}"))
                            .collect(),
                    });
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeUvValidationSummary {
    #[serde(default)]
    pub parameter_map_contexts: usize,
    #[serde(default)]
    pub nurbs_parameter_lines: usize,
    #[serde(default)]
    pub parameter_maps: usize,
    #[serde(default)]
    pub ruled_carriers: usize,
    pub rolling_ball_carriers: usize,
    pub sphere_meridians: usize,
    pub runs: usize,
    pub pcurves: usize,
    pub analytic_carriers: usize,
    pub nurbs_carriers: usize,
    pub jet_pairs: usize,
    pub evaluated_uv_sites: usize,
    pub evaluated_surface_sites: usize,
}
fn raw(hex: &str, source: &StreamSpan, b: &mut Budget<'_>) -> Result<Vec<u8>> {
    if source.length.checked_mul(2) != Some(hex.len() as u64) {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "native frame byte census",
        ));
    }
    b.bytes = b.bytes.saturating_add(source.length);
    if b.bytes > b.limits.max_topology_bytes {
        return Err(fail(
            Code::TopologyByteLimitExceeded,
            "native validation bytes",
        ));
    }
    b.work(hex.len() as u64)?;
    hex.as_bytes()
        .chunks_exact(2)
        .map(|q| {
            let digit = |x: u8| match x {
                b'0'..=b'9' => Some(x - b'0'),
                b'a'..=b'f' => Some(x - b'a' + 10),
                _ => None,
            };
            Ok(
                (digit(q[0]).ok_or_else(|| fail(Code::TopologyValidationInvalid, "native hex"))?
                    << 4)
                    | digit(q[1])
                        .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native hex"))?,
            )
        })
        .collect()
}
fn inside(parent: &StreamSpan, child: &StreamSpan) -> bool {
    let Some(pe) = parent.logical_offset.checked_add(parent.length) else {
        return false;
    };
    let Some(ce) = child.logical_offset.checked_add(child.length) else {
        return false;
    };
    if parent.stream_id != child.stream_id
        || child.logical_offset < parent.logical_offset
        || ce > pe
    {
        return false;
    }
    let mut p = parent.logical_offset;
    let mut ranges = Vec::new();
    for r in &parent.file_ranges {
        let Some(end) = p.checked_add(r.length) else {
            return false;
        };
        let a = p.max(child.logical_offset);
        let z = end.min(ce);
        if a < z {
            let Some(offset) = r.offset.checked_add(a - p) else {
                return false;
            };
            ranges.push(crate::container::FileRange {
                offset,
                length: z - a,
            });
        }
        p = end;
    }
    p == pe && ranges == child.file_ranges
}
pub(super) fn validate_internal(
    model: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<NativeUvValidationSummary> {
    b.add(
        model.runs.len()
            + model.parameter_map_contexts.len()
            + model.nurbs_parameter_objects.len()
            + model.parameter_maps.len()
            + model.ruled_carriers.len()
            + model.pcurves.len()
            + model.carriers.len()
            + model.nurbs_carriers.len()
            + model.rolling_ball_carriers.len()
            + model.jet_pairs.len(),
    )?;
    let pcs = unique(&model.pcurves, |p| &p.id)?;
    unique(&model.carriers, |p| &p.id)?;
    let nurbs_by_id = unique(&model.nurbs_carriers, |p| &p.id)?;
    unique(&model.rolling_ball_carriers, |p| &p.id)?;
    unique(&model.ruled_carriers, |p| &p.id)?;
    let maps_by_id = unique(&model.parameter_maps, |p| &p.id)?;
    let mut ids = BTreeSet::new();
    let mut uv_sites = 0;
    let mut surface_sites = 0;
    let check = |run: u32, source: &StreamSpan| -> Result<()> {
        let r = model
            .runs
            .get(run as usize)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native run reference"))?;
        if r.frame_count < 2 || !inside(&r.source, source) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native source ownership",
            ));
        }
        Ok(())
    };
    for p in &model.parameter_maps {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        b.add(p.fields.records())?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| {
                f.end == bytes.len()
                    && f.class == p.class
                    && f.flag == 3
                    && (f.family == b'B' || f.class == 0x25)
            })
            .ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "native parameter map frame",
                )
            })?;
        let fields = parameter_maps::parse(&bytes[f.payload..f.end], f.class).ok_or_else(|| {
            fail(
                Code::TopologyValidationInvalid,
                "native parameter map payload",
            )
        })?;
        if !fields.bit_equal(&p.fields)
            || f.width != p.frame_width
            || f.flag != p.frame_flag
            || f.token_bytes != p.header_token
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native parameter map fields differ",
            ));
        }
    }
    let mut map_contexts = BTreeSet::new();
    for context in &model.parameter_map_contexts {
        check(context.run, &context.source)?;
        let map = member(&maps_by_id, &context.map)?;
        if !map_contexts.insert(&context.map)
            || context.run != map.run
            || context.source.logical_offset != map.source.logical_offset
            || !inside(&context.source, &map.source)
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native map context owner",
            ));
        }
        let mut end = map
            .source
            .logical_offset
            .checked_add(map.source.length)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native map context offset"))?;
        for vector in &context.following {
            if vector.source.logical_offset != end || !inside(&context.source, &vector.source) {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "native map context adjacency",
                ));
            }
            let bytes = raw(&vector.raw_frame_hex, &vector.source, b)?;
            let f = frame(&bytes, 0)
                .filter(|f| f.family == b'B' && f.class == 6 && f.flag == 3 && f.end == bytes.len())
                .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native reference frame"))?;
            let fields = reference_contexts::parse(&bytes[f.payload..f.end])
                .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native reference lane"))?;
            b.add(fields.references.len() + 1)?;
            if vector.frame_width != f.width
                || vector.header_token != f.token_bytes
                || vector.fields != fields
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "native reference fields",
                ));
            }
            end = end.checked_add(vector.source.length).ok_or_else(|| {
                fail(Code::TopologyValidationInvalid, "native map context offset")
            })?;
        }
        if end.checked_sub(context.source.logical_offset) != Some(context.source.length) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native map context extent",
            ));
        }
    }
    let mut parameter_contexts = BTreeSet::new();
    let mut nurbs_parameter_lines = 0;
    for group in &model.nurbs_parameter_objects {
        check(group.run, &group.source)?;
        if !parameter_contexts.insert(&group.carrier) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "duplicate native parameter context",
            ));
        }
        let carrier = member(&nurbs_by_id, &group.carrier)?;
        let bytes = raw(&group.raw_frames_hex, &group.source, b)?;
        let mut fs = Vec::new();
        let mut p = 0;
        while p < bytes.len() && fs.len() <= 5 {
            let f = frame(&bytes, p).ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "native parameter context frame",
                )
            })?;
            p = f.end;
            fs.push(f);
        }
        if fs.len() != 5
            || p != bytes.len()
            || fs[0].family != b'A'
            || fs[0].class != 52
            || !fs[1..]
                .iter()
                .all(|f| f.family == b'B' && f.class == 0x18 && f.flag == 3)
            || group.run != carrier.run
            || carrier.source.logical_offset != group.source.logical_offset
            || !inside(&group.source, &carrier.source)
            || bytes[..fs[0].end]
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
                != carrier.raw_frame_hex
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native NURBS parameter context",
            ));
        }
        let parsed: Vec<_> = fs[1..]
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                parameter_lines::parse(&bytes[f.payload..f.end]).map(|fields| (i, f, fields))
            })
            .collect();
        if parsed.is_empty()
            || group.lines.is_empty()
            || group.lines.windows(2).any(|w| w[0].ordinal >= w[1].ordinal)
            || group
                .lines
                .iter()
                .any(|line| !parsed.iter().any(|(i, _, _)| *i == line.ordinal))
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native parameter line population",
            ));
        }
        b.add(group.lines.len())?;
        for (ordinal, f, fields) in parsed {
            let Some(line) = group.lines.iter().find(|line| line.ordinal == ordinal) else {
                // Older JSON retained the raw context while these three
                // explicit-range controls were still opaque. The omission
                // does not alter any retained line or bind a current face.
                if matches!(fields.control, 0 | 0x40 | 0x80) {
                    continue;
                }
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "native parameter line population",
                ));
            };
            if ordinal != line.ordinal
                || !parameter_lines::bit_equal(&fields, &line.fields)
                || line.source.logical_offset != group.source.logical_offset + f.start as u64
                || line.source.length != (f.end - f.start) as u64
                || !inside(&group.source, &line.source)
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "native parameter line fields",
                ));
            }
            nurbs_parameter_lines += 1;
        }
    }
    for p in &model.pcurves {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        b.add(p.jet.knots.len() * 7)?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| f.end == bytes.len() && f.class == 32)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native pcurve frame"))?;
        let value = jet(&bytes[f.payload..f.end])
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native pcurve payload"))?;
        if value
            != (
                p.support_tag,
                p.leading_extrapolation_sites,
                p.array_control,
                p.jet.clone(),
            )
            || f.width != p.frame_width
            || f.flag != p.frame_flag
            || f.token_bytes != p.header_token
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native UV fields differ from raw frame",
            ));
        }
        for pair in p.jet.knots.windows(2) {
            for j in 0..=4 {
                b.work(128)?;
                p.jet
                    .evaluate_validated(if j == 4 {
                        pair[1]
                    } else {
                        pair[0] + (pair[1] - pair[0]) * j as f64 / 4.
                    })
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "native UV evaluation"))?;
                uv_sites += 1;
            }
        }
    }
    for p in &model.carriers {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| f.end == bytes.len() && f.class == p.class && f.family == b'B')
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native carrier frame"))?;
        let value = carrier(&bytes[f.payload..f.end], f.class)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native carrier payload"))?;
        if value != (p.geometry.clone(), p.uv_scale, p.uv_offset, p.native_bounds)
            || f.token != p.header_operand
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native carrier fields differ",
            ));
        }
    }
    for p in &model.nurbs_carriers {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        b.add(
            p.geometry.control_points.len() * 2
                + p.geometry.knots.iter().map(Vec::len).sum::<usize>(),
        )?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| f.end == bytes.len() && f.class == 52 && f.family == b'A')
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native NURBS frame"))?;
        let value = nurbs(&bytes[f.payload..f.end])
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native NURBS payload"))?;
        if value.0 != p.geometry
            || value.1 != p.current_limits
            || value.2 != p.parameter_map
            || p.limit_span_indices.is_some_and(|v| v != value.3)
            || p.knot_limits.is_some_and(|v| Some(v) != value.4)
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native NURBS fields differ",
            ));
        }
        for i in 0..=4 {
            for j in 0..=4 {
                b.work(p.geometry.control_points.len() as u64 * 2 + 128)?;
                let uv = std::array::from_fn(|d| {
                    let k = &p.geometry.knots[d];
                    let a = k[p.geometry.degrees[d]];
                    let z = k[p.geometry.control_counts[d]];
                    let step = if d == 0 { i } else { j };
                    if step == 4 {
                        z
                    } else {
                        a + (z - a) * step as f64 / 4.
                    }
                });
                p.geometry
                    .evaluate(uv)
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "native NURBS evaluation"))?;
                surface_sites += 1;
            }
        }
    }
    for p in &model.ruled_carriers {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        b.add(p.geometry.knots.len().saturating_mul(19))?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| f.family == b'A' && f.class == 0x39 && f.flag == 3 && f.end == bytes.len())
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "ruled frame"))?;
        if ruled(&bytes[f.payload..f.end]) != Some(p.geometry.clone()) {
            return Err(fail(Code::TopologyValidationInvalid, "ruled fields"));
        }
        for i in 0..=8 {
            for j in 0..=4 {
                b.work(p.geometry.knots.len() as u64 * 32)?;
                let l = p.geometry.current_limits;
                let u = if i == 8 {
                    l[1]
                } else {
                    l[0] + (l[1] - l[0]) * i as f64 / 8.
                };
                p.geometry
                    .evaluate([u, l[2] + (l[3] - l[2]) * j as f64 / 4.])
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "ruled evaluation"))?;
                surface_sites += 1;
            }
        }
    }
    for p in &model.rolling_ball_carriers {
        if !ids.insert(&p.id) {
            return Err(fail(Code::TopologyValidationInvalid, "native identity"));
        }
        check(p.run, &p.source)?;
        b.add(p.geometry.knots.len().saturating_mul(31))?;
        let bytes = raw(&p.raw_frame_hex, &p.source, b)?;
        let f = frame(&bytes, 0)
            .filter(|f| f.family == b'A' && f.class == 50 && f.end == bytes.len())
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "rolling frame"))?;
        if rolling(&bytes[f.payload..f.end]) != Some((p.geometry.clone(), p.tail.clone())) {
            return Err(fail(Code::TopologyValidationInvalid, "rolling fields"));
        }
        b.work(p.geometry.knots.len() as u64 * 64)?;
        for i in 0..=4 {
            for j in 0..=4 {
                let [a, z] = p.geometry.parameter_range;
                let t = if i == 4 {
                    z
                } else {
                    a + (z - a) * i as f64 / 4.
                };
                p.geometry
                    .evaluate_validated([t, j as f64 / 4.])
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "rolling evaluation"))?;
                surface_sites += 1;
            }
        }
    }
    for pair in &model.jet_pairs {
        check(pair.run, &pair.source)?;
        let a = *member(&pcs, &pair.pcurves[0])?;
        let c = *member(&pcs, &pair.pcurves[1])?;
        let start = pair.source.logical_offset;
        let end = start
            .checked_add(pair.source.length)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native pair range"))?;
        let packet_start = c.source.logical_offset + c.source.length;
        if a.run != pair.run
            || c.run != pair.run
            || a.id == c.id
            || a.source.logical_offset != start
            || a.source.logical_offset + a.source.length != c.source.logical_offset
            || packet_start > end
            || a.jet.knots != c.jet.knots
            || a.jet.parameter_range != pair.parameter_range
            || c.jet.parameter_range != pair.parameter_range
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native pair adjacency",
            ));
        }
        let bytes = raw(
            &pair.raw_packet_hex,
            &StreamSpan {
                stream_id: pair.source.stream_id,
                logical_offset: packet_start,
                length: end - packet_start,
                file_ranges: Vec::new(),
            },
            b,
        )?;
        let f = frame(&bytes, 0)
            .filter(|f| f.end == bytes.len() && f.class == 35 && f.family == b'B')
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native parameter packet"))?;
        let value = packet(&bytes[f.payload..f.end]).ok_or_else(|| {
            fail(
                Code::TopologyValidationInvalid,
                "native parameter packet payload",
            )
        })?;
        if value
            != (
                pair.parameter_range,
                pair.tolerance,
                pair.local_slots,
                pair.selector_raw.clone(),
            )
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native parameter packet fields differ",
            ));
        }
    }
    for meridian in &model.sphere_meridians {
        b.add(1)?;
        let carrier = model
            .carriers
            .iter()
            .find(|c| c.id == meridian.carrier)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native meridian carrier"))?;
        check(carrier.run, &meridian.source)?;
        if carrier.source.logical_offset != meridian.source.logical_offset {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native meridian adjacency",
            ));
        }
        let bytes = raw(&meridian.raw_frames_hex, &meridian.source, b)?;
        let mut fs = Vec::new();
        let mut p = 0;
        while p < bytes.len() {
            let f = frame(&bytes, p)
                .ok_or_else(|| fail(Code::TopologyValidationInvalid, "native meridian frame"))?;
            p = f.end;
            fs.push(f);
        }
        if fs.first().is_none_or(|f| {
            bytes[..f.end]
                .iter()
                .map(|x| format!("{x:02x}"))
                .collect::<String>()
                != carrier.raw_frame_hex
        }) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native meridian carrier bytes",
            ));
        }
        if sphere_meridian(&bytes, &fs) != Some(meridian.normal) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "native meridian fields",
            ));
        }
    }
    Ok(NativeUvValidationSummary {
        parameter_map_contexts: model.parameter_map_contexts.len(),
        nurbs_parameter_lines,
        parameter_maps: model.parameter_maps.len(),
        ruled_carriers: model.ruled_carriers.len(),
        sphere_meridians: model.sphere_meridians.len(),
        rolling_ball_carriers: model.rolling_ball_carriers.len(),
        runs: model.runs.len(),
        pcurves: model.pcurves.len(),
        analytic_carriers: model.carriers.len(),
        nurbs_carriers: model.nurbs_carriers.len(),
        jet_pairs: model.jet_pairs.len(),
        evaluated_uv_sites: uv_sites,
        evaluated_surface_sites: surface_sites,
    })
}

/// Recheck decoded fields against retained native frame bytes and source scopes.
/// This checks internal consistency; authentication requires the original file.
pub fn validate_native_uv(
    model: &NativeUvModel,
    limits: &ParseLimits,
) -> std::result::Result<NativeUvValidationSummary, TopologyValidationError> {
    let mut b = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    validate_internal(model, &mut b).map_err(|e| TopologyValidationError {
        code: e.code,
        detail: e.detail,
    })
}
