//! Admitted standard CFV2 population grammar. Layout reference: cadmpeg §5
//! at 6289a0d (CC BY 4.0); independently implemented, no runtime dependency.
use crate::{
    Diagnostic, DiagnosticCode as Code, ParseLimits, Severity, SourceLocation,
    binary::ByteReader,
    container::{ContainerInfo, DirectoryKind, FileRange, StreamInfo, reconstruct_stream},
    geometry::*,
    metadata::StreamSpan,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
struct Issue {
    code: Code,
    offset: usize,
}
type Result<T> = std::result::Result<T, Issue>;
fn bad(offset: usize) -> Issue {
    Issue {
        code: Code::GeometrySpineInvalid,
        offset,
    }
}
fn failure(code: Code, offset: usize) -> Issue {
    Issue { code, offset }
}
fn diagnostic(e: Issue, id: Option<u32>, severity: Severity) -> Diagnostic {
    let mut d = Diagnostic::new(e.code, severity, None);
    d.location = id.map(|id| SourceLocation::LogicalStream {
        stream_id: id.to_string(),
        offset: e.offset as u64,
    });
    d
}
struct Budget<'a> {
    limits: &'a ParseLimits,
    steps: u64,
    bytes: u64,
    records: u64,
}
impl Budget<'_> {
    fn work(&mut self, n: u64, p: usize) -> Result<()> {
        self.steps = self
            .steps
            .checked_add(n)
            .ok_or_else(|| failure(Code::GeometryStepLimitExceeded, p))?;
        if self.steps > self.limits.max_geometry_steps {
            return Err(failure(Code::GeometryStepLimitExceeded, p));
        }
        Ok(())
    }
    fn count(&self, n: usize, p: usize) -> Result<()> {
        if n as u64 > self.limits.max_geometry_records {
            Err(failure(Code::GeometryRecordLimitExceeded, p))
        } else {
            Ok(())
        }
    }
}
struct View<'a> {
    bytes: &'a [u8],
    limits: &'a ParseLimits,
}
impl<'a> View<'a> {
    fn take(&self, p: usize, n: usize) -> Result<&'a [u8]> {
        ByteReader::new(self.bytes, 0, self.limits)
            .and_then(|r| r.subreader(p as u64, n as u64))
            .and_then(|mut r| r.read_bytes(n as u64))
            .map_err(|e| {
                failure(
                    if e.code == Code::UnexpectedEof {
                        Code::GeometrySpineInvalid
                    } else {
                        e.code
                    },
                    p,
                )
            })
    }
    fn byte(&self, p: usize) -> Result<u8> {
        Ok(self.take(p, 1)?[0])
    }
    fn count(&self, p: &mut usize) -> Result<usize> {
        let value = self.byte(*p)?;
        *p += 1;
        if value != 255 {
            return Ok(value as usize);
        }
        let b = self.take(*p, 4)?;
        *p += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }
    fn f32s(&self, p: usize, n: usize, big: bool) -> Result<Vec<f64>> {
        Ok(self
            .take(p, n * 4)?
            .chunks_exact(4)
            .map(|b| {
                f64::from(if big {
                    f32::from_be_bytes([b[0], b[1], b[2], b[3]])
                } else {
                    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
                })
            })
            .collect())
    }
    fn tag(&self, p: usize) -> Result<u32> {
        let b = self.take(p, 3)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], 0]))
    }
}
const DELIMITER: &[u8] = &[0x10, 0x24, 4, 255, 255, 0, 0, 0];
fn fbb(bytes: &[u8], p: usize) -> bool {
    bytes
        .get(p..p + 4)
        .is_some_and(|b| matches!(b[0], 0x30 | 0xb0) && b[1..] == [4, 4, 255])
}

#[derive(Clone, PartialEq)]
struct Spine {
    face_start: usize,
    face_end: usize,
    faces: usize,
    width: u8,
    tables: Vec<(usize, usize)>,
    edge_rows: Vec<(usize, usize)>,
    vertices: Vec<(usize, Vec3)>,
    vertex_start: usize,
    vertex_end: usize,
}

fn walk_spine(
    v: &View<'_>,
    start: usize,
    end: usize,
    width: u8,
    budget: &mut Budget<'_>,
) -> Result<Spine> {
    let mut p = end;
    let mut tables = Vec::new();
    let mut edges = Vec::new();
    loop {
        budget.work(1, p)?;
        if v.byte(p)? != 1 || !matches!(v.byte(p + 1)?, 1 | 2) {
            return Err(bad(p));
        }
        let table = p;
        let kind = v.byte(p + 1)?;
        p += 2;
        let count = v.count(&mut p)?;
        if count == 0 || count > v.bytes.len().saturating_sub(p) / (2 + 2 * width as usize) {
            return Err(bad(p));
        }
        budget.count(count + edges.len(), p)?;
        for _ in 0..count {
            let row = p;
            budget.work(1, p)?;
            if v.byte(p)? != 2 {
                return Err(bad(p));
            }
            p += 1;
            let arity = v.count(&mut p)?;
            if arity < 2 {
                return Err(bad(p));
            }
            budget.work(arity as u64, p)?;
            v.take(p, arity * width as usize)?;
            p += arity * width as usize;
            edges.push((row, p));
        }
        if v.take(p, 8)? != DELIMITER {
            return Err(bad(p));
        }
        p += 8;
        tables.push((table, p));
        if width != 2 && (tables.len() != 1 || kind != 1) {
            return Err(bad(p));
        }
        if v.take(p, 2)? == [1, 6] {
            break;
        }
        if width != 2 {
            return Err(bad(p));
        }
    }
    let vertex_start = p;
    p += 2;
    let count = v.count(&mut p)?;
    if count == 0 || count > v.bytes.len().saturating_sub(p) / 15 {
        return Err(bad(p));
    }
    budget.count(count, p)?;
    let mut vertices = Vec::new();
    for _ in 0..count {
        budget.work(1, p)?;
        if v.take(p, 3)? != [5, 8, 1] {
            return Err(bad(p));
        }
        let xyz = v.f32s(p + 3, 3, false)?;
        let xyz = [xyz[0], xyz[1], xyz[2]];
        if !finite(xyz) {
            return Err(failure(Code::GeometryParameterInvalid, p + 3));
        }
        vertices.push((p, xyz));
        p += 15;
    }
    if v.bytes.get(p..p + 8) == Some(DELIMITER) {
        p += 8;
    }
    Ok(Spine {
        face_start: start,
        face_end: end,
        faces: (end - start) / 8,
        width,
        tables,
        edge_rows: edges,
        vertices,
        vertex_start,
        vertex_end: p,
    })
}

fn select_spine(v: &View<'_>, budget: &mut Budget<'_>) -> Result<Spine> {
    let mut runs = Vec::new();
    let mut p = 0;
    while p + 8 <= v.bytes.len() {
        budget.work(1, p)?;
        if !fbb(v.bytes, p) {
            p += 1;
            continue;
        }
        let start = p;
        let payload = v.take(p + 4, 4)?;
        let mut consistent = true;
        while p + 8 <= v.bytes.len() && fbb(v.bytes, p) {
            budget.work(1, p)?;
            if v.take(p + 4, 4)? != payload {
                consistent = false;
            }
            p += 8;
        }
        budget.count(runs.len() + 1, start)?;
        runs.push((start, p, consistent));
    }
    let maximum = runs
        .iter()
        .map(|(a, b, _)| b - a)
        .max()
        .ok_or_else(|| failure(Code::GeometryLayoutUnsupported, 0))?;
    let largest: Vec<_> = runs
        .into_iter()
        .filter(|(a, b, _)| b - a == maximum)
        .collect();
    if largest.len() != 1 {
        return Err(failure(Code::GeometryPopulationAmbiguous, largest[0].0));
    }
    let (start, end, consistent) = largest[0];
    if !consistent {
        return Err(bad(start));
    }
    budget.count((end - start) / 8, start)?;
    let mut candidates = Vec::new();
    let mut saved_error = None;
    for width in 1..=3 {
        match walk_spine(v, start, end, width, budget) {
            Ok(spine) => candidates.push(spine),
            Err(e)
                if e.code == Code::GeometryStepLimitExceeded
                    || e.code == Code::ReadLimitExceeded =>
            {
                return Err(e);
            }
            Err(e) => saved_error = Some(e),
        }
    }
    if candidates.len() != 1 {
        return Err(if candidates.len() > 1 {
            failure(Code::GeometryPopulationAmbiguous, start)
        } else {
            saved_error.unwrap_or_else(|| bad(end))
        });
    }
    Ok(candidates.remove(0))
}

#[derive(Clone, PartialEq)]
struct Trim {
    start: usize,
    end: usize,
    normal: Option<(usize, Vec3)>,
    primitive_counts: [usize; 3],
    lengths: Vec<usize>,
    handle_start: usize,
    handle_count: usize,
}
fn trim_packet(
    v: &View<'_>,
    start: usize,
    stop: usize,
    width: u8,
    lane: u8,
    budget: &mut Budget<'_>,
) -> Result<Trim> {
    if v.take(start, 2)?[0] != 1 {
        return Err(bad(start));
    }
    let kind = v.byte(start + 1)?;
    if !(0x41..=0x4f).contains(&kind) {
        return Err(bad(start));
    }
    let mut p = start + 2;
    let mut abc = [0; 3];
    for (i, bit) in [1, 2, 4].into_iter().enumerate() {
        if kind & bit != 0 {
            abc[i] = v.count(&mut p)?;
        }
    }
    if v.byte(p)? != 255 {
        return Err(bad(p));
    }
    let n = v.count(&mut p)?;
    let normal = if kind & 8 != 0 {
        let values = v.f32s(p, 3, false)?;
        let a = [values[0], values[1], values[2]];
        if !finite(a) || (dot(a, a) - 1.).abs() >= 1e-6 {
            return Err(bad(p));
        }
        let result = Some((p, a));
        p += 12;
        result
    } else {
        None
    };
    let lengths = abc[1].checked_add(abc[2]).ok_or_else(|| bad(p))?;
    if n == 0 || lengths > stop.saturating_sub(p) || n > stop.saturating_sub(p) / width as usize {
        return Err(bad(p));
    }
    budget.count(lengths, p)?;
    budget.work(n as u64 + lengths as u64, p)?;
    let mut total = abc[0].checked_mul(3).ok_or_else(|| bad(p))?;
    let mut primitive_lengths = Vec::new();
    for _ in 0..lengths {
        let k = if kind == 0x42 && abc[1] == 2 {
            let k = v.byte(p)? as usize;
            p += 1;
            k
        } else if lane == 2 {
            let b = v.take(p, 2)?;
            p += 2;
            u16::from_be_bytes([b[0], b[1]]) as usize
        } else {
            v.count(&mut p)?
        };
        total = total.checked_add(k).ok_or_else(|| bad(p))?;
        primitive_lengths.push(k);
    }
    let end = p
        .checked_add(n * width as usize)
        .filter(|e| *e <= stop)
        .ok_or_else(|| bad(p))?;
    if total != n {
        return Err(bad(p));
    }
    v.take(p, end - p)?;
    Ok(Trim {
        start,
        end,
        normal,
        primitive_counts: abc,
        lengths: primitive_lengths,
        handle_start: p,
        handle_count: n,
    })
}
fn select_trims(v: &View<'_>, spine: &Spine, budget: &mut Budget<'_>) -> Result<Vec<Trim>> {
    let mut candidates = Vec::new();
    for lane in 1..=2 {
        if lane == 2 && spine.width != 2 {
            continue;
        }
        let mut nodes = BTreeMap::new();
        for p in 0..spine.face_start.saturating_sub(1) {
            budget.work(1, p)?;
            if v.bytes[p] != 1 || !(0x41..=0x4f).contains(&v.bytes[p + 1]) {
                continue;
            }
            match trim_packet(v, p, spine.face_start, spine.width, lane, budget) {
                Ok(node) => {
                    budget.count(nodes.len() + 1, p)?;
                    nodes.insert(p, node);
                }
                Err(e) if e.code.is_resource_limit() => return Err(e),
                Err(_) => {}
            }
        }
        let mut lengths = BTreeMap::new();
        for (&p, node) in nodes.iter().rev() {
            budget.work(1, p)?;
            if node.end == spine.face_start {
                lengths.insert(p, 1);
            } else if let Some(length) = lengths.get(&node.end) {
                lengths.insert(p, length + 1);
            }
        }
        for (&start, _) in lengths.iter().filter(|(_, n)| **n == spine.faces) {
            let mut p = start;
            let mut chain = Vec::new();
            while p != spine.face_start {
                budget.work(1, p)?;
                let node = nodes.get(&p).ok_or_else(|| bad(p))?;
                p = node.end;
                chain.push(node.clone());
            }
            if !candidates.contains(&chain) {
                budget.count(candidates.len() + 1, start)?;
                candidates.push(chain);
            }
        }
    }
    if candidates.len() != 1 {
        return Err(failure(
            if candidates.is_empty() {
                Code::GeometryTrimUnresolved
            } else {
                Code::GeometryPopulationAmbiguous
            },
            spine.face_start,
        ));
    }
    Ok(candidates.remove(0))
}

#[derive(Clone)]
struct Surface {
    start: usize,
    end: usize,
    tag: u32,
    kind: u8,
    parameters: Vec<f64>,
    sense: i8,
    parameter_end: usize,
}
#[derive(Clone, Copy)]
enum SupportKind {
    Line,
    Circle([f64; 4]),
    Spline,
}
#[derive(Clone)]
struct Support {
    start: usize,
    end: usize,
    tag: u32,
    kind: SupportKind,
    faces: [u32; 2],
    parameter_end: usize,
}
fn surface(v: &View<'_>, p: usize) -> Result<Surface> {
    let tag = v.tag(p)?;
    let header = v.take(p + 3, 5)?;
    let length = u16::from_be_bytes([header[0], header[1]]) as usize;
    let kind = header[4];
    let (size, n) = if header[2..4] == [0, 0x33] {
        match (kind, length) {
            (0x32, 2) => (49, 0),
            (0x33 | 0x34, 26) => (73, 6),
            (0x35, 18) => (65, 4),
            (0x38, 30) => (77, 7),
            _ => return Err(bad(p)),
        }
    } else if v.take(p + 3, 3)? == [0, 0, 0] {
        (47, 0)
    } else {
        return Err(bad(p));
    };
    let parameter_end = if size == 47 { p + 6 } else { p + 8 + 4 * n };
    let bounds = v.f32s(parameter_end, 10, false)?;
    if !bounds.iter().all(|v| v.is_finite())
        || bounds[3..6].iter().any(|v| *v < 0.)
        || bounds[9] < 0.
    {
        return Err(bad(parameter_end));
    }
    let sense = match v.byte(p + size - 1)? {
        1 => 1,
        255 => -1,
        _ => return Err(bad(p + size - 1)),
    };
    Ok(Surface {
        start: p,
        end: p + size,
        tag,
        kind: if size == 47 { 0 } else { kind },
        parameters: v.f32s(p + 8, n, true)?,
        sense,
        parameter_end,
    })
}
fn support(v: &View<'_>, start: usize, faces: usize) -> Result<Support> {
    if v.byte(start)? != 0x60 {
        return Err(bad(start));
    }
    let tag = v.tag(start + 1)?;
    let p = start + 4;
    let (kind, mut end) = if v.take(p, 3)? == [0, 0, 0] {
        (SupportKind::Spline, p + 3)
    } else if v.take(p, 5)? == [0, 2, 0, 0x33, 0x36] {
        (SupportKind::Line, p + 5)
    } else if v.take(p, 5)? == [0, 18, 0, 0x33, 0x37] {
        let values = v.f32s(p + 5, 4, true)?;
        (
            SupportKind::Circle([values[0], values[1], values[2], values[3]]),
            p + 21,
        )
    } else {
        return Err(bad(p));
    };
    let parameter_end = end;
    let a = v.count(&mut end)?;
    let b = v.count(&mut end)?;
    if a >= faces || b >= faces {
        return Err(bad(parameter_end));
    }
    Ok(Support {
        start,
        end,
        tag,
        kind,
        faces: [a as u32, b as u32],
        parameter_end,
    })
}
struct Roster {
    surfaces: Vec<Surface>,
    supports: Vec<Support>,
    surface_start: usize,
    support_start: usize,
    end: usize,
}
fn select_roster(v: &View<'_>, spine: &Spine, budget: &mut Budget<'_>) -> Result<Roster> {
    let mut candidates = Vec::new();
    for start in 0..v.bytes.len().saturating_sub(46) {
        budget.work(1, start)?;
        if surface(v, start).is_err() {
            continue;
        }
        let attempt = (|| {
            let mut p = start;
            let mut surfaces = Vec::new();
            let mut supports = Vec::new();
            for _ in 0..spine.faces {
                budget.work(1, p)?;
                let s = surface(v, p)?;
                p = s.end;
                surfaces.push(s);
            }
            let support_start = p;
            for _ in 0..spine.edge_rows.len() {
                budget.work(1, p)?;
                let s = support(v, p, spine.faces)?;
                p = s.end;
                supports.push(s);
            }
            if support(v, p, spine.faces).is_ok() {
                return Err(bad(p));
            }
            Ok(Roster {
                surfaces,
                supports,
                surface_start: start,
                support_start,
                end: p,
            })
        })();
        match attempt {
            Ok(roster) => {
                budget.count(candidates.len() + 1, start)?;
                candidates.push(roster);
            }
            Err(e) if e.code.is_resource_limit() => return Err(e),
            Err(_) => {}
        }
    }
    if candidates.len() != 1 {
        return Err(failure(
            if candidates.is_empty() {
                Code::GeometrySurfaceRosterUnresolved
            } else {
                Code::GeometryPopulationAmbiguous
            },
            0,
        ));
    }
    Ok(candidates.remove(0))
}

fn source(
    stream: &StreamInfo,
    start: usize,
    end: usize,
    budget: &mut Budget<'_>,
) -> Result<StreamSpan> {
    if end < start || end as u64 > stream.logical_length {
        return Err(bad(start));
    }
    let mut ranges = Vec::new();
    for e in &stream.extents {
        budget.work(1, start)?;
        let a = (start as u64).max(e.logical_offset);
        let b = (end as u64).min(e.logical_offset + e.logical_length);
        if a < b {
            ranges.push(FileRange {
                offset: e.file_range.offset + a - e.logical_offset,
                length: b - a,
            });
        }
    }
    Ok(StreamSpan {
        stream_id: stream.id,
        logical_offset: start as u64,
        length: (end - start) as u64,
        file_ranges: ranges,
    })
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn axis(x: f64, y: f64, hemisphere: f64) -> Option<Vec3> {
    if !x.is_finite() || !y.is_finite() || !hemisphere.is_finite() {
        return None;
    }
    let delta = 1. - x * x - y * y;
    if delta < -4. * f64::from(f32::EPSILON) {
        return None;
    }
    unit([
        x,
        y,
        if delta <= 4. * f64::from(f32::EPSILON) {
            0.
        } else {
            delta.sqrt().copysign(hemisphere)
        },
    ])
}
fn analytic(s: &Surface) -> SurfaceGeometry {
    let p = &s.parameters;
    let decoded = (|| {
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        match s.kind {
            0x33 | 0x34 => {
                let frame = Frame3::from_axis([p[0], p[1], p[2]], axis(p[3], p[4], p[5])?)?;
                if p[5].abs() == 0. {
                    return None;
                }
                if s.kind == 0x33 {
                    Some(SurfaceGeometry::Cylinder {
                        frame,
                        radius: p[5].abs(),
                    })
                } else if p[5].abs() < std::f64::consts::FRAC_PI_2 {
                    Some(SurfaceGeometry::Cone {
                        frame,
                        half_angle: p[5].abs(),
                    })
                } else {
                    None
                }
            }
            0x35 if p[3] > 0. => Some(SurfaceGeometry::Sphere {
                frame: Frame3::from_axis([p[0], p[1], p[2]], [0., 0., 1.])?,
                radius: p[3],
            }),
            0x38 if p[5].abs() > 0. && p[6] > 0. => Some(SurfaceGeometry::Torus {
                frame: Frame3::from_axis([p[0], p[1], p[2]], axis(p[3], p[4], p[5])?)?,
                major_radius: p[5].abs(),
                minor_radius: p[6],
            }),
            _ => None,
        }
    })();
    decoded.unwrap_or(SurfaceGeometry::Unknown {
        reason: match s.kind {
            0 => Code::GeometryCarrierUnsupported,
            0x32 => Code::GeometryPlaneLocationUnresolved,
            _ => Code::GeometryParameterInvalid,
        },
    })
}

fn circle_axis(s: &Support, surfaces: &[SurfaceGeometry]) -> Option<Vec3> {
    let SupportKind::Circle(values) = s.kind else {
        return None;
    };
    let center = [values[0], values[1], values[2]];
    let radius = values[3];
    if !finite(center) || !radius.is_finite() || radius <= 0. {
        return None;
    }
    let mut result: Option<Vec3> = None;
    for face in s.faces {
        let candidate = match &surfaces[face as usize] {
            SurfaceGeometry::Cylinder { frame, radius: r } => {
                let v = sub(center, frame.origin);
                let radial = sub(v, scale(frame.z_axis, dot(v, frame.z_axis)));
                if dot(radial, radial).sqrt() <= 1e-3 && (radius - r).abs() <= 1e-3 {
                    Some(frame.z_axis)
                } else {
                    None
                }
            }
            SurfaceGeometry::Plane { frame }
                if dot(sub(center, frame.origin), frame.z_axis).abs() <= 1e-3 =>
            {
                Some(frame.z_axis)
            }
            _ => None,
        };
        if let Some(a) = candidate {
            if result.is_some_and(|b| cross(a, b).iter().map(|v| v * v).sum::<f64>() > 1e-10) {
                return None;
            }
            if result.is_none() {
                result = Some(a);
            }
        }
    }
    result
}

struct PopulationInput<'a, 'b> {
    surf: &'a View<'b>,
    main_stream: &'a StreamInfo,
    surf_stream: &'a StreamInfo,
    directory_id: u32,
    population_id: u32,
    spine: Spine,
    trims: Vec<Trim>,
    roster: Roster,
}
fn emit(
    input: PopulationInput<'_, '_>,
    budget: &mut Budget<'_>,
    model: &mut GeometryModel,
) -> Result<()> {
    let PopulationInput {
        surf,
        main_stream,
        surf_stream,
        directory_id,
        population_id,
        spine,
        trims,
        roster,
    } = input;
    let additional = spine.vertices.len() + 2 * spine.faces + 2 * spine.edge_rows.len();
    budget.records = budget
        .records
        .checked_add(additional as u64)
        .ok_or_else(|| failure(Code::GeometryRecordLimitExceeded, 0))?;
    if budget.records > budget.limits.max_geometry_records {
        return Err(failure(Code::GeometryRecordLimitExceeded, 0));
    }
    let mut geometries: Vec<_> = roster.surfaces.iter().map(analytic).collect();
    let mut evidence: Vec<Vec<StreamSpan>> = vec![Vec::new(); geometries.len()];
    // An exact rim-circle/cylinder relation supplies a plane point. Bounds are
    // retained but never used for plane location or an analytic definition.
    for (i, s) in roster.surfaces.iter().enumerate() {
        if s.kind != 0x32 {
            continue;
        }
        let Some((normal_offset, normal)) = trims[i].normal else {
            continue;
        };
        let normal = unit(normal).ok_or_else(|| bad(normal_offset))?;
        let mut centers = Vec::new();
        let mut witness = None;
        let mut consistent = true;
        for support in &roster.supports {
            budget.work(1, support.start)?;
            if !support.faces.contains(&(i as u32)) {
                continue;
            }
            if let SupportKind::Circle(v) = support.kind {
                let center = [v[0], v[1], v[2]];
                if !finite(center) || !v[3].is_finite() || v[3] <= 0. {
                    continue;
                }
                centers.push(center);
                if let Some(a) = circle_axis(support, &geometries) {
                    if dot(cross(a, normal), cross(a, normal)) > 1e-10 {
                        consistent = false;
                    } else if witness.is_none() {
                        witness = Some((center, support));
                    }
                }
            }
        }
        if let Some((origin, support)) = witness {
            if centers
                .iter()
                .any(|c| dot(sub(*c, origin), normal).abs() > 1e-3)
            {
                consistent = false;
            }
            if consistent {
                geometries[i] = SurfaceGeometry::Plane {
                    frame: Frame3::from_axis(origin, normal).ok_or_else(|| bad(normal_offset))?,
                };
                evidence[i] = vec![
                    source(main_stream, normal_offset, normal_offset + 12, budget)?,
                    source(surf_stream, support.start, support.end, budget)?,
                ];
            }
        }
    }
    let mut known_main = Vec::new();
    let mut known_surf = Vec::new();
    // A complete trailing roster with matching cardinality and strictly
    // increasing identities supplies positional allocation identity, not edges.
    let identities = (|| {
        if surf.bytes.len().checked_sub(roster.end)? != spine.vertices.len() * 7 {
            return None;
        }
        let mut ids = Vec::new();
        for i in 0..spine.vertices.len() {
            let p = roster.end + i * 7;
            if surf.bytes.get(p) != Some(&0x54) || surf.bytes.get(p + 4..p + 7) != Some(&[0, 0, 0])
            {
                return None;
            }
            let tag = surf.tag(p + 1).ok()?;
            if ids.last().is_some_and(|previous| *previous >= tag) {
                return None;
            }
            ids.push(tag);
        }
        Some(ids)
    })();
    for (i, (p, position)) in spine.vertices.iter().enumerate() {
        budget.work(1, *p)?;
        known_main.push((*p, *p + 15));
        let identity_source = if identities.is_some() {
            let p = roster.end + i * 7;
            known_surf.push((p, p + 7));
            Some(source(surf_stream, p, p + 7, budget)?)
        } else {
            None
        };
        model.points.push(PointRecord {
            id: format!("p{population_id}:point:{i}"),
            population_id,
            table_ordinal: i as u32,
            native_allocation_id: identities.as_ref().map(|ids| ids[i]),
            identity_source,
            position: *position,
            source: source(main_stream, *p, *p + 15, budget)?,
        });
    }
    for (i, s) in roster.surfaces.iter().enumerate() {
        let id = format!("p{population_id}:surface:{i}");
        let geometry = geometries[i].clone();
        let span = source(surf_stream, s.start, s.end, budget)?;
        if let SurfaceGeometry::Unknown { reason } = geometry {
            model.unresolved.push(UnresolvedGeometry {
                code: reason,
                subject_id: Some(id.clone()),
                source: Some(span.clone()),
                native_tag: Some(s.tag),
                candidates: Vec::new(),
            });
        }
        known_surf.extend([(s.start, s.parameter_end), (s.end - 1, s.end)]);
        if let Some((p, _)) = trims[i].normal {
            known_main.push((p, p + 12));
        }
        model.surfaces.push(SurfaceRecord {
            id: id.clone(),
            population_id,
            record_ordinal: i as u32,
            native_carrier_tag: s.tag,
            domains: geometry.domains(),
            geometry,
            source: span,
            evidence: evidence[i].clone(),
            raw_parameters_hex: hex(
                surf.take(s.start + 8, s.parameter_end.saturating_sub(s.start + 8))?
            ),
            parameterization: "derived_canonical_carrier_chart".into(),
        });
        model.surface_uses.push(SurfaceUse {
            population_id,
            face_ordinal: i as u32,
            definition_id: id,
            orientation_sign: s.sense,
            body_id: None,
        });
    }
    for (i, s) in roster.supports.iter().enumerate() {
        budget.work(1, s.start)?;
        let mut evidence = Vec::new();
        let geometry = match s.kind {
            SupportKind::Circle(v) => {
                if v.iter().any(|v| !v.is_finite()) || v[3] <= 0. {
                    CurveGeometry::Unknown {
                        reason: Code::GeometryParameterInvalid,
                    }
                } else if let Some(a) = circle_axis(s, &geometries) {
                    CurveGeometry::Circle {
                        frame: Frame3::from_axis([v[0], v[1], v[2]], a)
                            .ok_or_else(|| bad(s.start))?,
                        radius: v[3],
                    }
                } else {
                    CurveGeometry::Unknown {
                        reason: Code::GeometryCurveFrameUnresolved,
                    }
                }
            }
            SupportKind::Line => {
                match (
                    &geometries[s.faces[0] as usize],
                    &geometries[s.faces[1] as usize],
                ) {
                    (SurfaceGeometry::Plane { frame: a }, SurfaceGeometry::Plane { frame: b }) => {
                        let d = cross(a.z_axis, b.z_axis);
                        let n = dot(d, d);
                        if n > 1e-12 {
                            let origin = scale(
                                add(
                                    scale(cross(b.z_axis, d), dot(a.z_axis, a.origin)),
                                    scale(cross(d, a.z_axis), dot(b.z_axis, b.origin)),
                                ),
                                1. / n,
                            );
                            if finite(origin) {
                                CurveGeometry::Line {
                                    origin,
                                    direction: unit(d).ok_or_else(|| bad(s.start))?,
                                }
                            } else {
                                CurveGeometry::Unknown {
                                    reason: Code::GeometryParameterInvalid,
                                }
                            }
                        } else {
                            CurveGeometry::Unknown {
                                reason: Code::GeometryCurveFrameUnresolved,
                            }
                        }
                    }
                    _ => CurveGeometry::Unknown {
                        reason: Code::GeometryCurveFrameUnresolved,
                    },
                }
            }
            SupportKind::Spline => CurveGeometry::Unknown {
                reason: Code::GeometryCarrierUnsupported,
            },
        };
        for face in s.faces {
            let index = face as usize;
            let surface = &roster.surfaces[index];
            evidence.push(source(surf_stream, surface.start, surface.end, budget)?);
            evidence.extend_from_slice(
                &model.surfaces[model.surfaces.len() - spine.faces + index].evidence,
            );
        }
        let id = format!("p{population_id}:curve:{i}");
        let span = source(surf_stream, s.start, s.end, budget)?;
        if let CurveGeometry::Unknown { reason } = geometry {
            model.unresolved.push(UnresolvedGeometry {
                code: reason,
                subject_id: Some(id.clone()),
                source: Some(span.clone()),
                native_tag: Some(s.tag),
                candidates: Vec::new(),
            });
        }
        known_surf.push((s.start, s.end));
        model.curves.push(CurveRecord {
            id: id.clone(),
            population_id,
            record_ordinal: i as u32,
            native_allocation_tag: s.tag,
            domain: geometry.domain(),
            geometry,
            source: span,
            evidence,
            raw_parameters_hex: hex(surf.take(s.start + 4, s.parameter_end - s.start - 4)?),
            parameter_interval: None,
            endpoint_point_ids: None,
            parameterization: "derived_canonical_carrier_chart".into(),
        });
        model.curve_uses.push(CurveUse {
            population_id,
            edge_ordinal: i as u32,
            definition_id: id,
            serialized_face_slots: s.faces,
            body_id: None,
        });
    }
    let population = GeometryPopulation {
        id: population_id,
        directory_id,
        main_stream_id: main_stream.id,
        surface_stream_id: surf_stream.id,
        face_count: spine.faces as u32,
        edge_count: spine.edge_rows.len() as u32,
        point_count: spine.vertices.len() as u32,
        handle_width: spine.width,
        face_rows: source(main_stream, spine.face_start, spine.face_end, budget)?,
        edge_tables: spine
            .tables
            .iter()
            .map(|&(a, b)| source(main_stream, a, b, budget))
            .collect::<Result<_>>()?,
        vertex_table: source(main_stream, spine.vertex_start, spine.vertex_end, budget)?,
        trim_chain: source(main_stream, trims[0].start, spine.face_start, budget)?,
        surface_roster: source(
            surf_stream,
            roster.surface_start,
            roster.support_start,
            budget,
        )?,
        support_table: source(surf_stream, roster.support_start, roster.end, budget)?,
        body_id: None,
    };
    // All remaining bytes (including face bounds, mesh handles and unknown
    // headers) retain recoverable ranges instead of acquiring geometry meaning.
    opaque(main_stream, known_main, budget, model)?;
    opaque(surf_stream, known_surf, budget, model)?;
    model.populations.push(population);
    Ok(())
}
fn opaque(
    stream: &StreamInfo,
    mut known: Vec<(usize, usize)>,
    budget: &mut Budget<'_>,
    model: &mut GeometryModel,
) -> Result<()> {
    known.sort_unstable();
    let mut p = 0;
    for (a, b) in known {
        if a > p {
            model.opaque_ranges.push(source(stream, p, a, budget)?);
        }
        p = p.max(b);
    }
    if p < stream.logical_length as usize {
        model
            .opaque_ranges
            .push(source(stream, p, stream.logical_length as usize, budget)?);
    }
    Ok(())
}

pub(crate) fn decode(
    data: &[u8],
    container: &ContainerInfo,
    limits: &ParseLimits,
    model: &mut GeometryModel,
    diagnostics: &mut Vec<Diagnostic>,
) -> std::result::Result<(), Box<Diagnostic>> {
    let mut budget = Budget {
        limits,
        steps: 0,
        bytes: 0,
        records: 0,
    };
    for directory in &container.directories {
        if directory.kind != DirectoryKind::Nested {
            continue;
        }
        let select = |name: &str| {
            let streams: Vec<_> = container
                .streams
                .iter()
                .filter(|s| s.directory_id == directory.id && s.name.as_deref() == Some(name))
                .collect();
            let maximum = streams.iter().map(|s| s.logical_length).max()?;
            let best: Vec<_> = streams
                .into_iter()
                .filter(|s| s.logical_length == maximum)
                .collect();
            if best.len() == 1 { Some(best[0]) } else { None }
        };
        let (Some(main), Some(surf)) = (select("MainDataStream"), select("SurfacicReps")) else {
            model.unresolved.push(UnresolvedGeometry {
                code: Code::GeometryPopulationAmbiguous,
                subject_id: None,
                source: None,
                native_tag: None,
                candidates: Vec::new(),
            });
            continue;
        };
        budget.bytes = budget
            .bytes
            .saturating_add(main.logical_length)
            .saturating_add(surf.logical_length);
        if budget.bytes > limits.max_geometry_bytes {
            return Err(Box::new(diagnostic(
                failure(Code::GeometryLimitExceeded, 0),
                Some(main.id),
                Severity::Error,
            )));
        }
        let main_bytes = reconstruct_stream(data, main, limits)?;
        let surf_bytes = reconstruct_stream(data, surf, limits)?;
        let mv = View {
            bytes: &main_bytes,
            limits,
        };
        let sv = View {
            bytes: &surf_bytes,
            limits,
        };
        let mut staged = GeometryModel::default();
        let mut error_stream = Some(main.id);
        let attempt = (|| {
            let spine = select_spine(&mv, &mut budget)?;
            let trims = select_trims(&mv, &spine, &mut budget)?;
            error_stream = Some(surf.id);
            let roster = select_roster(&sv, &spine, &mut budget)?;
            error_stream = None;
            emit(
                PopulationInput {
                    surf: &sv,
                    main_stream: main,
                    surf_stream: surf,
                    directory_id: directory.id,
                    population_id: model.populations.len() as u32,
                    spine,
                    trims,
                    roster,
                },
                &mut budget,
                &mut staged,
            )
        })();
        if let Err(e) = attempt {
            if e.code.is_resource_limit() {
                return Err(Box::new(diagnostic(e, error_stream, Severity::Error)));
            }
            model.unresolved.push(UnresolvedGeometry {
                code: e.code,
                subject_id: None,
                source: None,
                native_tag: None,
                candidates: Vec::new(),
            });
            for stream in [main, surf] {
                model.opaque_ranges.push(
                    source(stream, 0, stream.logical_length as usize, &mut budget)
                        .map_err(|e| Box::new(diagnostic(e, Some(stream.id), Severity::Error)))?,
                );
            }
        } else {
            model.populations.append(&mut staged.populations);
            model.points.append(&mut staged.points);
            model.surfaces.append(&mut staged.surfaces);
            model.curves.append(&mut staged.curves);
            model.surface_uses.append(&mut staged.surface_uses);
            model.curve_uses.append(&mut staged.curve_uses);
            model.unresolved.append(&mut staged.unresolved);
            model.opaque_ranges.append(&mut staged.opaque_ranges);
            model.layout = GeometryLayout::StandardNested;
            model.units = UnitContext {
                length: "millimeter".into(),
                angle: "radian".into(),
                evidence: "selected_standard_grammar_convention".into(),
                native_unit_declaration: None,
            };
        }
    }
    let mut tags: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for p in &model.points {
        if let Some(tag) = p.native_allocation_id {
            tags.entry(tag).or_default().push(p.id.clone());
        }
    }
    for s in &model.surfaces {
        tags.entry(s.native_carrier_tag)
            .or_default()
            .push(s.id.clone());
    }
    for s in &model.curves {
        tags.entry(s.native_allocation_tag)
            .or_default()
            .push(s.id.clone());
    }
    for (tag, ids) in tags {
        if ids.len() > 1 {
            model.unresolved.push(UnresolvedGeometry {
                code: Code::GeometryNativeIdentityAmbiguous,
                subject_id: None,
                source: None,
                native_tag: Some(tag),
                candidates: ids,
            });
        }
    }
    if !model.populations.is_empty() {
        model.layout = GeometryLayout::StandardNested;
        model.units = UnitContext {
            length: "millimeter".into(),
            angle: "radian".into(),
            evidence: "selected_standard_grammar_convention".into(),
            native_unit_declaration: None,
        };
    } else {
        model.unresolved.push(UnresolvedGeometry {
            code: Code::GeometryLayoutUnsupported,
            subject_id: None,
            source: None,
            native_tag: None,
            candidates: Vec::new(),
        });
    }
    let mut codes = Vec::new();
    for unresolved in &model.unresolved {
        if !codes.contains(&unresolved.code) {
            codes.push(unresolved.code);
            diagnostics.push(Diagnostic::new(unresolved.code, Severity::Warning, None));
        }
    }
    diagnostics.push(Diagnostic::new(
        Code::GeometryTopologyPending,
        Severity::Info,
        None,
    ));
    Ok(())
}

/// Revisit exactly the population already admitted by the geometry request.
/// Mesh coordinates will only bind handles to existing counted vertices.
pub(crate) fn topology_source(
    data: &[u8],
    container: &ContainerInfo,
    pop: &GeometryPopulation,
    limits: &ParseLimits,
) -> std::result::Result<crate::topology::RawSource, Box<Diagnostic>> {
    let stream = container.stream(pop.main_stream_id).ok_or_else(|| {
        Box::new(Diagnostic::new(
            Code::TopologySourceUnsupported,
            Severity::Warning,
            None,
        ))
    })?;
    if stream.logical_length > limits.max_topology_bytes {
        return Err(Box::new(Diagnostic::new(
            Code::TopologyByteLimitExceeded,
            Severity::Error,
            None,
        )));
    }
    let bytes = reconstruct_stream(data, stream, limits)?;
    let surface_stream = container.stream(pop.surface_stream_id).ok_or_else(|| {
        Box::new(Diagnostic::new(
            Code::TopologySourceUnsupported,
            Severity::Warning,
            None,
        ))
    })?;
    if stream
        .logical_length
        .saturating_add(surface_stream.logical_length)
        > limits.max_topology_bytes
    {
        return Err(Box::new(Diagnostic::new(
            Code::TopologyByteLimitExceeded,
            Severity::Error,
            None,
        )));
    }
    let surface_bytes = reconstruct_stream(data, surface_stream, limits)?;
    let adapted = ParseLimits {
        max_geometry_steps: limits.max_topology_steps,
        max_geometry_records: limits.max_topology_records,
        ..*limits
    };
    let mut budget = Budget {
        limits: &adapted,
        steps: 0,
        bytes: 0,
        records: 0,
    };
    let view = View {
        bytes: &bytes,
        limits: &adapted,
    };
    let result = (|| {
        let sv = View {
            bytes: &surface_bytes,
            limits: &adapted,
        };
        let mut at = pop.surface_roster.logical_offset as usize;
        let mut witnesses = Vec::new();
        for _ in 0..pop.face_count {
            budget.work(1, at)?;
            let s = surface(&sv, at)?;
            witnesses.push(if matches!(s.kind, 0x33 | 0x38) {
                let v = sv.f32s(s.parameter_end, 3, false)?;
                Some((
                    [v[0], v[1], v[2]],
                    source(
                        surface_stream,
                        s.parameter_end,
                        s.parameter_end + 12,
                        &mut budget,
                    )?,
                ))
            } else {
                None
            });
            at = s.end;
        }
        if at != (pop.surface_roster.logical_offset + pop.surface_roster.length) as usize {
            return Err(bad(at));
        }
        let spine = walk_spine(
            &view,
            pop.face_rows.logical_offset as usize,
            (pop.face_rows.logical_offset + pop.face_rows.length) as usize,
            pop.handle_width,
            &mut budget,
        )?;
        if spine.faces != pop.face_count as usize
            || spine.edge_rows.len() != pop.edge_count as usize
            || spine.vertices.len() != pop.point_count as usize
        {
            return Err(bad(0));
        }
        let trims = select_trims(&view, &spine, &mut budget)?;
        if trims.first().map(|t| t.start) != Some(pop.trim_chain.logical_offset as usize) {
            return Err(bad(0));
        }
        let mut elements = 0u64;
        let mut read_handles = |p: usize, n: usize| -> Result<Vec<u32>> {
            elements = elements
                .checked_add(n as u64)
                .ok_or_else(|| failure(Code::TopologyRecordLimitExceeded, p))?;
            if elements > limits.max_topology_records {
                return Err(failure(Code::TopologyRecordLimitExceeded, p));
            }
            Ok(view
                .take(p, n * pop.handle_width as usize)?
                .chunks_exact(pop.handle_width as usize)
                .map(|row| row.iter().fold(0u32, |a, b| (a << 8) | u32::from(*b)))
                .collect())
        };
        let mut edges = Vec::new();
        for &(start, end) in &spine.edge_rows {
            let mut p = start + 1;
            let n = view.count(&mut p)?;
            let handles = read_handles(p, n)?;
            edges.push(crate::topology::RawEdge {
                handles,
                source: source(stream, start, end, &mut budget)?,
            });
        }
        let mut packets = Vec::new();
        for trim in &trims {
            packets.push(crate::topology::RawPacket {
                counts: trim.primitive_counts,
                lengths: trim.lengths.clone(),
                handles: read_handles(trim.handle_start, trim.handle_count)?,
                normal: trim
                    .normal
                    .map(|(p, n)| source(stream, p, p + 12, &mut budget).map(|s| (n, s)))
                    .transpose()?,
                source: source(stream, trim.start, trim.end, &mut budget)?,
            });
        }
        Ok((
            edges,
            packets,
            trims[0].start,
            spine.tables.len() == 1,
            budget.steps,
            elements,
            witnesses,
        ))
    })();
    match result {
        Ok((edges, packets, prefix_end, complete_edge_runs, work, elements, witnesses)) => {
            Ok(crate::topology::RawSource {
                main: bytes,
                stream: stream.clone(),
                edges,
                packets,
                prefix_end,
                complete_edge_runs,
                work,
                elements,
                witnesses,
                additional_bytes: surface_bytes.len() as u64,
            })
        }
        Err(mut e) => {
            e.code = match e.code {
                Code::GeometryStepLimitExceeded => Code::TopologyStepLimitExceeded,
                Code::GeometryRecordLimitExceeded => Code::TopologyRecordLimitExceeded,
                _ => e.code,
            };
            Err(Box::new(diagnostic(e, Some(stream.id), Severity::Warning)))
        }
    }
}
