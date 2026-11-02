//! Indexed standard B-rep recovery with explicit source and orientation gauges.
//!
//! Visualization coordinates bind handles to counted native vertices only when
//! binary32 identities are unique. Curves and surfaces come from native carrier
//! definitions and counted points. Mesh boundaries witness the trimmed paths;
//! they do not replace analytic carriers. A closed manifold graph is not a
//! certified solid. See the repository topology format notes for admission rules.
use crate::{
    Diagnostic, DiagnosticCode as Code, ParseLimits, Severity,
    container::{ContainerInfo, StreamInfo},
    geometry::*,
    metadata::StreamSpan,
};
use serde::{Deserialize, Serialize};
mod boundary;
mod charts;
mod intersections;
mod native_curve_binding;
mod native_support_binding;
mod native_uv;
mod nurbs_binding;
mod nurbs_limits;
mod nurbs_triangles;
mod rolling_binding;
mod rolling_sections;
mod ruled_binding;
pub use native_uv::{
    NativeAllocationEncoding, NativeAllocationReference, NativeBoundSupport, NativeBoundUvGeometry,
    NativeBoundUvUse, NativeBoundaryUv, NativeBoundaryUvSummary, NativeCarrier,
    NativeCountedReferenceFields, NativeCountedReferenceFrame, NativeEdgeGapSample,
    NativeEdgeMapTraversal, NativeEndpointImages, NativeJetPair, NativeMapCoordinate,
    NativeMapCoordinateEncoding, NativeNurbsCarrier, NativeNurbsCurveImage,
    NativeNurbsParameterObjects, NativePairedUvEdge, NativeParameterCircleFields,
    NativeParameterLine, NativeParameterLineFields, NativeParameterLineGeometry,
    NativeParameterMap, NativeParameterMapContext, NativeParameterMapFields, NativeParameterQuery,
    NativeParameterRelation, NativePcurve, NativePlaneSupport, NativeRecordRun,
    NativeRollingBallCarrier, NativeRollingBallTail, NativeRuledCarrier, NativeSphereMeridian,
    NativeTargetParameterQuery, NativeUvEdgeTrim, NativeUvEndpoint, NativeUvModel,
    NativeUvSupportChart, NativeUvUseOrientation, NativeUvValidationSummary,
    validate_native_boundary_uv, validate_native_uv,
};
mod partial;
mod singular;
mod sparse_arcs;
mod spherical_seams;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureExpectation {
    #[default]
    Unspecified,
    Closed,
    Open,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyOptions {
    pub expected_closure: ClosureExpectation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellClosure {
    Closed,
    Open,
    Unresolved,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    ClosedShell,
    OpenShell,
    Mixed,
    Unresolved,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopRole {
    Outer,
    Inner,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub id: String,
    pub population_id: u32,
    pub native_body_id: Option<String>,
    pub grouping_evidence: String,
    pub kind: BodyKind,
    pub regions: Vec<String>,
    pub export_eligible: bool,
    pub solid_validated: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub body: String,
    pub shells: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shell {
    pub id: String,
    pub region: String,
    pub faces: Vec<String>,
    pub closure: ShellClosure,
    pub euler_characteristic: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub id: String,
    pub shell: String,
    pub surface: String,
    pub native_ordinal: u32,
    pub native_sense_byte: i8,
    pub chart_sense: i8,
    pub orientation_evidence: String,
    pub loops: Vec<String>,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryLoop {
    pub id: String,
    pub face: String,
    pub role: LoopRole,
    pub coedges: Vec<String>,
    pub source: StreamSpan,
    pub signed_uv_area: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub singular_transitions: Vec<SingularTransition>,
}
/// A zero-dimensional native vertex can have a nontrivial connector in a
/// singular UV chart. This is not an additional physical edge or native pcurve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SingularTransition {
    pub after_coedge: String,
    pub vertex: String,
    pub uv_start: [f64; 2],
    pub uv_end: [f64; 2],
    pub source: StreamSpan,
    pub evidence: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coedge {
    pub id: String,
    pub owner_loop: String,
    pub edge: String,
    pub forward: bool,
    pub pcurve: String,
    pub radial_next: Option<String>,
    pub trim_handles: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub population_id: u32,
    pub native_ordinal: u32,
    pub native_allocation_tag: u32,
    pub vertices: [String; 2],
    pub curve: String,
    pub parameter_range: [f64; 2],
    pub direction_evidence: String,
    pub seam: bool,
    pub degenerate: bool,
    pub source: StreamSpan,
    pub support_source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub id: String,
    pub population_id: u32,
    pub point: String,
    pub native_allocation_id: Option<u32>,
    pub edges: Vec<String>,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PcurveGeometry {
    /// Exact analytic inversion of the 3D curve. Guides select periodic branches
    /// only; they do not interpolate the curve or supply its geometric locus.
    AnalyticProjection {
        surface: Box<SurfaceGeometry>,
        curve: Box<CurveGeometry>,
        parameter_range: [f64; 2],
        phase_guides: Vec<[f64; 3]>,
    },
    Line {
        start: [f64; 2],
        delta: [f64; 2],
    },
    PlaneCircle {
        center: [f64; 2],
        cosine: [f64; 2],
        sine: [f64; 2],
        angle_start: f64,
        angle_sweep: f64,
    },
}
impl PcurveGeometry {
    /// Normalized traversal parameter. A coedge can reverse its edge interval.
    pub fn evaluate(&self, q: f64) -> std::result::Result<[f64; 2], Code> {
        if !q.is_finite() || !(0. ..=1.).contains(&q) {
            return Err(Code::TopologyPcurveInvalid);
        }
        let value = match self {
            Self::AnalyticProjection {
                surface,
                curve,
                parameter_range,
                phase_guides,
            } => charts::projected(surface, curve, *parameter_range, phase_guides, q)
                .map_err(|e| e.code)?,
            Self::Line { start, delta } => [start[0] + q * delta[0], start[1] + q * delta[1]],
            Self::PlaneCircle {
                center,
                cosine,
                sine,
                angle_start,
                angle_sweep,
            } => {
                let t = angle_start + q * angle_sweep;
                std::array::from_fn(|i| center[i] + cosine[i] * t.cos() + sine[i] * t.sin())
            }
        };
        if value.iter().all(|v| v.is_finite()) {
            Ok(value)
        } else {
            Err(Code::TopologyPcurveInvalid)
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pcurve {
    pub id: String,
    pub coedge: String,
    pub surface: String,
    pub edge_parameter_range: [f64; 2],
    pub geometry: PcurveGeometry,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyIssue {
    pub population_id: u32,
    pub code: Code,
    pub source: Option<StreamSpan>,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyScope {
    pub population_id: u32,
    pub index_table_source: StreamSpan,
    pub index_entry_count: u32,
    pub uninterpreted_auxiliary_word: u32,
    pub triangle_count: u64,
    pub endpoint_binding: String,
}
/// Complete indexed boundary incidence retained before analytic UV recovery.
/// These records are evidence for further decoding, never exportable bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryPopulation {
    pub population_id: u32,
    pub vertex_points: Vec<String>,
    pub edges: Vec<BoundaryEdge>,
    pub loops: Vec<BoundaryWire>,
    pub index_scope: TopologyScope,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryEdge {
    pub native_ordinal: u32,
    pub curve: String,
    pub vertices: [u32; 2],
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryWire {
    pub face_ordinal: u32,
    pub occurrences: Vec<BoundaryOccurrence>,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryOccurrence {
    pub edge_ordinal: u32,
    pub forward: Option<bool>,
    pub trim_handles: Vec<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryValidationSummary {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    pub loops: usize,
    pub occurrences: usize,
    pub closed: bool,
    pub inferred_second_owners: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticPartial {
    pub population_id: u32,
    pub surfaces: Vec<SurfaceRecord>,
    pub curves: Vec<CurveRecord>,
    pub pcurves: Vec<AnalyticUse>,
    pub unresolved: Vec<AnalyticUseIssue>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticUse {
    pub face_ordinal: u32,
    pub boundary_loop: u32,
    pub occurrence: u32,
    pub edge_ordinal: u32,
    pub forward: bool,
    pub curve: String,
    pub surface: String,
    pub parameter_range: [f64; 2],
    pub pcurve: PcurveGeometry,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticUseIssue {
    pub face_ordinal: u32,
    pub edge_ordinal: u32,
    pub code: Code,
    /// The admission failure, retained independently of source UV availability.
    /// Older snapshots did not retain this diagnostic.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub source: StreamSpan,
}
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyModel {
    /// Source-bound UV programs retain their own gauge and may have a gap to
    /// another representation of the same edge. They do not certify a B-rep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_boundary_uv: Vec<NativeBoundaryUv>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_uv: Option<NativeUvModel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub analytic_partials: Vec<AnalyticPartial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<BoundaryPopulation>,
    pub bodies: Vec<Body>,
    pub regions: Vec<Region>,
    pub shells: Vec<Shell>,
    pub faces: Vec<Face>,
    pub loops: Vec<BoundaryLoop>,
    pub coedges: Vec<Coedge>,
    pub edges: Vec<Edge>,
    pub vertices: Vec<Vertex>,
    pub pcurves: Vec<Pcurve>,
    pub scopes: Vec<TopologyScope>,
    pub unresolved: Vec<TopologyIssue>,
}

pub(crate) struct RawEdge {
    pub handles: Vec<u32>,
    pub source: StreamSpan,
}
pub(crate) struct RawPacket {
    pub counts: [usize; 3],
    pub lengths: Vec<usize>,
    pub handles: Vec<u32>,
    pub normal: Option<(Vec3, StreamSpan)>,
    pub source: StreamSpan,
}
pub(crate) struct RawSource {
    pub main: Vec<u8>,
    pub stream: StreamInfo,
    pub edges: Vec<RawEdge>,
    pub packets: Vec<RawPacket>,
    pub prefix_end: usize,
    pub complete_edge_runs: bool,
    pub work: u64,
    pub elements: u64,
    pub witnesses: Vec<Option<(Vec3, StreamSpan)>>,
    pub additional_bytes: u64,
}
#[derive(Debug, Clone)]
struct Failure {
    code: Code,
    source: Option<StreamSpan>,
    detail: String,
}
type Result<T> = std::result::Result<T, Failure>;
fn fail(code: Code, detail: impl Into<String>) -> Failure {
    Failure {
        code,
        source: None,
        detail: detail.into(),
    }
}
struct Budget<'a> {
    limits: &'a ParseLimits,
    steps: u64,
    elements: u64,
    bytes: u64,
}
impl Budget<'_> {
    fn work(&mut self, n: u64) -> Result<()> {
        self.steps = self
            .steps
            .checked_add(n)
            .ok_or_else(|| fail(Code::TopologyStepLimitExceeded, "work counter overflow"))?;
        if self.steps > self.limits.max_topology_steps {
            return Err(fail(Code::TopologyStepLimitExceeded, "work budget"));
        }
        Ok(())
    }
    fn add(&mut self, n: usize) -> Result<()> {
        self.elements = self.elements.checked_add(n as u64).ok_or_else(|| {
            fail(
                Code::TopologyRecordLimitExceeded,
                "element counter overflow",
            )
        })?;
        if self.elements > self.limits.max_topology_records {
            return Err(fail(Code::TopologyRecordLimitExceeded, "element budget"));
        }
        Ok(())
    }
}
fn span(stream: &StreamInfo, start: usize, end: usize, b: &mut Budget<'_>) -> Result<StreamSpan> {
    if start > end || end as u64 > stream.logical_length {
        return Err(fail(Code::TopologySourceUnsupported, "source bounds"));
    }
    let mut file_ranges = Vec::new();
    for e in &stream.extents {
        b.work(1)?;
        let left = (start as u64).max(e.logical_offset);
        let right = (end as u64).min(e.logical_offset + e.logical_length);
        if left < right {
            file_ranges.push(crate::container::FileRange {
                offset: e.file_range.offset + left - e.logical_offset,
                length: right - left,
            });
        }
    }
    Ok(StreamSpan {
        stream_id: stream.id,
        logical_offset: start as u64,
        length: (end - start) as u64,
        file_ranges,
    })
}

fn word(bytes: &[u8], p: usize) -> Result<u32> {
    let a = bytes
        .get(p..p + 4)
        .ok_or_else(|| fail(Code::TopologySourceUnsupported, "truncated index table"))?;
    Ok(u32::from_le_bytes([a[0], a[1], a[2], a[3]]))
}
fn scalar(bytes: &[u8], p: &mut usize) -> Result<f64> {
    let x = f32::from_bits(word(bytes, *p)?) as f64;
    *p += 4;
    if !x.is_finite() {
        return Err(fail(
            Code::TopologySourceUnsupported,
            "non-finite index coordinate",
        ));
    }
    Ok(x)
}
struct IndexTable {
    coords: Vec<Vec3>,
    source: StreamSpan,
    auxiliary: u32,
}
fn index_candidate(raw: &RawSource, p: usize, b: &mut Budget<'_>) -> Result<IndexTable> {
    let bytes = &raw.main[..raw.prefix_end];
    let n = word(bytes, p + 6)? as usize;
    let auxiliary = word(bytes, p + 11)?;
    if n == 0 || bytes.get(p + 10) != Some(&255) || bytes.get(p + 15..p + 18) != Some(&[0, 0, 0]) {
        return Err(fail(Code::TopologySourceUnsupported, "index header"));
    }
    // At least one control bit per coordinate must fit before reserving memory.
    if n > bytes.len().saturating_sub(p + 19).saturating_mul(4) {
        return Err(fail(
            Code::TopologySourceUnsupported,
            "index count exceeds source",
        ));
    }
    b.add(n)?;
    let mode = *bytes
        .get(p + 18)
        .ok_or_else(|| fail(Code::TopologySourceUnsupported, "index mode"))?;
    let mut cursor = p + 19;
    let mut coords = Vec::new();
    if mode == 1 {
        if n > bytes.len().saturating_sub(cursor) / 12 {
            return Err(fail(Code::TopologySourceUnsupported, "index extent"));
        }
        for _ in 0..n {
            b.work(1)?;
            coords.push([
                scalar(bytes, &mut cursor)?,
                scalar(bytes, &mut cursor)?,
                scalar(bytes, &mut cursor)?,
            ]);
        }
    } else if mode == 0 {
        let count = n.div_ceil(4);
        let controls = bytes
            .get(cursor..cursor + count)
            .ok_or_else(|| fail(Code::TopologySourceUnsupported, "index controls"))?;
        cursor += count;
        if bytes.get(cursor) != Some(&255) {
            return Err(fail(Code::TopologySourceUnsupported, "index scalar header"));
        }
        cursor += 1;
        let scalars = word(bytes, cursor)? as usize;
        cursor += 4;
        let scalar_start = cursor;
        if scalars > bytes.len().saturating_sub(cursor) / 4 {
            return Err(fail(Code::TopologySourceUnsupported, "index scalar extent"));
        }
        let mut previous = [0.; 3];
        for i in 0..n {
            b.work(1)?;
            let code = (controls[i / 4] >> (2 * (i % 4))) & 3;
            if i == 0 && code != 0 {
                return Err(fail(
                    Code::TopologySourceUnsupported,
                    "first index row is not explicit",
                ));
            }
            let first = match code {
                0 => 0,
                1 => 3,
                2 => 2,
                _ => 1,
            };
            let mut point = previous;
            for x in &mut point[first..] {
                *x = scalar(bytes, &mut cursor)?;
            }
            coords.push(point);
            previous = point;
        }
        if cursor - scalar_start != scalars * 4 {
            return Err(fail(
                Code::TopologySourceUnsupported,
                "index scalar count mismatch",
            ));
        }
    } else {
        return Err(fail(Code::TopologySourceUnsupported, "index coding mode"));
    }
    Ok(IndexTable {
        coords,
        source: span(&raw.stream, p, cursor, b)?,
        auxiliary,
    })
}
fn key(p: Vec3) -> [u32; 3] {
    p.map(|v| (v as f32).to_bits())
}
type EndpointBinding = (IndexTable, Vec<[usize; 2]>, BTreeMap<[u32; 3], usize>);
fn choose_index(
    raw: &RawSource,
    points: &[PointRecord],
    b: &mut Budget<'_>,
) -> Result<EndpointBinding> {
    let mut map = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for (i, p) in points.iter().enumerate() {
        if map.insert(key(p.position), i).is_some() {
            duplicates.insert(key(p.position));
        }
    }
    let mut accepted = Vec::new();
    let mut failures = Vec::new();
    for p in 0..raw.prefix_end.saturating_sub(18) {
        b.work(1)?;
        if raw.main.get(p..p + 6) != Some(&[255, 255, 2, 0, 1, 255]) {
            continue;
        }
        let candidate = (|| {
            let table = index_candidate(raw, p, b)?;
            if raw
                .edges
                .iter()
                .flat_map(|e| &e.handles)
                .chain(raw.packets.iter().flat_map(|p| &p.handles))
                .any(|h| *h as usize >= table.coords.len())
            {
                return Err(fail(
                    Code::TopologyBoundaryInvalid,
                    "handle outside index table",
                ));
            }
            let mut endpoints = Vec::new();
            let mut covered = BTreeSet::new();
            for row in &raw.edges {
                b.work(1)?;
                let mut pair = [0; 2];
                for (i, h) in [row.handles.first(), row.handles.last()]
                    .into_iter()
                    .enumerate()
                {
                    let h = *h
                        .ok_or_else(|| fail(Code::TopologyEndpointAmbiguous, "empty edge row"))?
                        as usize;
                    let p = table.coords.get(h).ok_or_else(|| {
                        fail(
                            Code::TopologyEndpointAmbiguous,
                            "terminal handle outside index",
                        )
                    })?;
                    let k = key(*p);
                    if duplicates.contains(&k) {
                        return Err(fail(
                            Code::TopologyEndpointAmbiguous,
                            "coincident native identities",
                        ));
                    }
                    pair[i] = *map.get(&k).ok_or_else(|| {
                        fail(
                            Code::TopologyEndpointAmbiguous,
                            "terminal coordinate is not a counted vertex",
                        )
                    })?;
                    covered.insert(pair[i]);
                }
                endpoints.push(pair);
            }
            if covered.len() != points.len() {
                return Err(fail(
                    Code::TopologyEndpointAmbiguous,
                    "not every native vertex is selected",
                ));
            }
            Ok((table, endpoints))
        })();
        match candidate {
            Ok(value) => accepted.push(value),
            Err(e) if e.code.is_resource_limit() => return Err(e),
            Err(e) => failures.push(e),
        }
    }
    if accepted.len() != 1 {
        if accepted.is_empty()
            && failures.len() == 1
            && failures[0].code == Code::TopologyEndpointAmbiguous
        {
            return Err(failures.remove(0));
        }
        return Err(fail(
            Code::TopologySourceUnsupported,
            "no unique complete endpoint index table",
        ));
    }
    let (table, ends) = accepted.remove(0);
    Ok((table, ends, map))
}

fn mesh_boundaries(packet: &RawPacket, b: &mut Budget<'_>) -> Result<(Vec<Vec<u32>>, usize)> {
    let [a, strips, _] = packet.counts;
    let mut triangles = Vec::new();
    let h = &packet.handles;
    if a > h.len() / 3 {
        return Err(fail(
            Code::TopologyBoundaryInvalid,
            "independent triangle extent",
        ));
    }
    for t in h[..3 * a].chunks_exact(3) {
        triangles.push([t[0], t[1], t[2]]);
    }
    let mut cursor = 3 * a;
    for (i, &n) in packet.lengths.iter().enumerate() {
        let values = h
            .get(cursor..cursor + n)
            .ok_or_else(|| fail(Code::TopologyBoundaryInvalid, "primitive extent"))?;
        cursor += n;
        if n < 3 {
            return Err(fail(
                Code::TopologyBoundaryInvalid,
                "short triangle primitive",
            ));
        }
        for j in 0..n - 2 {
            b.work(1)?;
            triangles.push(if i < strips {
                if j % 2 == 0 {
                    [values[j], values[j + 1], values[j + 2]]
                } else {
                    [values[j + 1], values[j], values[j + 2]]
                }
            } else {
                [values[0], values[j + 1], values[j + 2]]
            });
        }
    }
    if cursor != h.len() {
        return Err(fail(Code::TopologyBoundaryInvalid, "primitive residue"));
    }
    b.add(triangles.len())?;
    let mut incidence: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
    let mut count = 0;
    for t in triangles {
        b.work(1)?;
        if t[0] == t[1] || t[1] == t[2] || t[2] == t[0] {
            continue;
        }
        count += 1;
        for (x, y) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = (x.min(y), x.max(y));
            let uses = incidence.entry(key).or_default();
            uses.push((x, y));
            if uses.len() > 2 {
                return Err(fail(
                    Code::TopologyNonManifold,
                    "triangle edge has more than two uses",
                ));
            }
        }
    }
    let mut next = BTreeMap::new();
    let mut incoming = BTreeSet::new();
    for uses in incidence.values() {
        b.work(1)?;
        if uses.len() == 2 {
            if uses[0].0 != uses[1].1 {
                return Err(fail(
                    Code::TopologyOrientationConflict,
                    "triangle interior is not oppositely oriented",
                ));
            }
        } else {
            let (a, c) = uses[0];
            if next.insert(a, c).is_some() || !incoming.insert(c) {
                return Err(fail(
                    Code::TopologyBoundaryInvalid,
                    "branched mesh boundary",
                ));
            }
        }
    }
    if next.keys().any(|h| !incoming.contains(h)) {
        return Err(fail(
            Code::TopologyBoundaryInvalid,
            "open mesh boundary chain",
        ));
    }
    let mut cycles = Vec::new();
    while let Some((&first, _)) = next.first_key_value() {
        let mut p = first;
        let mut cycle = Vec::new();
        loop {
            b.work(1)?;
            cycle.push(p);
            p = next
                .remove(&p)
                .ok_or_else(|| fail(Code::TopologyBoundaryInvalid, "boundary does not close"))?;
            if p == first {
                break;
            }
        }
        if cycle.len() < 3 {
            return Err(fail(Code::TopologyBoundaryInvalid, "short mesh boundary"));
        }
        cycles.push(cycle);
    }
    if cycles.is_empty() {
        return Err(fail(Code::TopologyBoundaryInvalid, "no trim boundary"));
    }
    Ok((cycles, count))
}

#[derive(Clone)]
struct Occurrence {
    edge: usize,
    path: Vec<u32>,
    start: usize,
    end: usize,
    forward: bool,
}
#[derive(Clone)]
struct Wire {
    face: usize,
    handles: Vec<u32>,
    occurrences: Vec<Occurrence>,
    uv: Vec<[f64; 2]>,
    role: LoopRole,
    area: f64,
}
fn same_path(a: &[u32], b: &[u32], table: &IndexTable) -> bool {
    let equal = |a: &u32, b: &u32| {
        a == b || key(table.coords[*a as usize]) == key(table.coords[*b as usize])
    };
    a.len() == b.len()
        && (a.iter().zip(b).all(|(a, b)| equal(a, b))
            || a.iter().zip(b.iter().rev()).all(|(a, b)| equal(a, b)))
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}
fn inverse(surface: &SurfaceGeometry, p: Vec3) -> Result<[f64; 2]> {
    charts::inverse(surface, p)
}

fn delta_angle(v: f64) -> f64 {
    (v + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
fn area(p: &[[f64; 2]]) -> f64 {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .take(p.len())
        .map(|(a, c)| a[0] * c[1] - a[1] * c[0])
        .sum::<f64>()
        / 2.
}
fn near2(a: [f64; 2], c: [f64; 2]) -> bool {
    (a[0] - c[0]).hypot(a[1] - c[1]) < 1e-10
}
fn orient(a: [f64; 2], c: [f64; 2], p: [f64; 2]) -> f64 {
    (c[0] - a[0]) * (p[1] - a[1]) - (c[1] - a[1]) * (p[0] - a[0])
}
fn on_segment(a: [f64; 2], c: [f64; 2], p: [f64; 2]) -> bool {
    orient(a, c, p).abs() < 1e-10
        && p[0] >= a[0].min(c[0]) - 1e-10
        && p[0] <= a[0].max(c[0]) + 1e-10
        && p[1] >= a[1].min(c[1]) - 1e-10
        && p[1] <= a[1].max(c[1]) + 1e-10
}
fn crosses(a: [f64; 2], c: [f64; 2], p: [f64; 2], q: [f64; 2]) -> bool {
    let x = orient(a, c, p);
    let y = orient(a, c, q);
    let z = orient(p, q, a);
    let w = orient(p, q, c);
    (x * y < 0. && z * w < 0.)
        || on_segment(a, c, p)
        || on_segment(a, c, q)
        || on_segment(p, q, a)
        || on_segment(p, q, c)
}
fn inside(p: [f64; 2], poly: &[[f64; 2]], b: &mut Budget<'_>) -> Result<bool> {
    let mut result = false;
    for (a, c) in poly
        .iter()
        .zip(poly.iter().cycle().skip(1))
        .take(poly.len())
    {
        b.work(1)?;
        if on_segment(*a, *c, p) {
            return Err(fail(Code::TopologyUvUnresolved, "touching loop boundaries"));
        }
        if (a[1] > p[1]) != (c[1] > p[1])
            && p[0] < (c[0] - a[0]) * (p[1] - a[1]) / (c[1] - a[1]) + a[0]
        {
            result = !result;
        }
    }
    Ok(result)
}
fn clean_polygon(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p = Vec::new();
    for x in points {
        if p.last().is_none_or(|previous| !near2(*previous, *x)) {
            p.push(*x);
        }
    }
    if p.len() > 1 && near2(p[0], p[p.len() - 1]) {
        p.pop();
    }
    p
}
fn prepare_uv(
    wires: &mut [Wire],
    surfaces: &[SurfaceRecord],
    curves: &[CurveRecord],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<()> {
    for wire in wires.iter_mut() {
        charts::prepare(wire, &surfaces[wire.face].geometry, curves, table, b)?;
    }
    classify_uv(wires, surfaces.len(), b)
}

fn classify_uv(wires: &mut [Wire], face_count: usize, b: &mut Budget<'_>) -> Result<()> {
    b.work((wires.len() as u64).saturating_mul(face_count as u64))?;
    let mut clean = Vec::new();
    for wire in wires.iter_mut() {
        let polygon = clean_polygon(&wire.uv);
        if polygon.len() < 3 || !area(&polygon).is_finite() || area(&polygon).abs() < 1e-12 {
            return Err(fail(Code::TopologyUvUnresolved, "zero-area UV loop"));
        }
        for i in 0..polygon.len() {
            for j in i + 1..polygon.len() {
                b.work(1)?;
                if j == i + 1 || (i == 0 && j + 1 == polygon.len()) {
                    continue;
                }
                if crosses(
                    polygon[i],
                    polygon[(i + 1) % polygon.len()],
                    polygon[j],
                    polygon[(j + 1) % polygon.len()],
                ) {
                    return Err(fail(
                        Code::TopologyUvUnresolved,
                        "self-intersecting UV polygon",
                    ));
                }
            }
        }
        wire.area = area(&polygon);
        clean.push(polygon);
    }
    for face in 0..face_count {
        let indices: Vec<_> = wires
            .iter()
            .enumerate()
            .filter(|(_, w)| w.face == face)
            .map(|(i, _)| i)
            .collect();
        let mut outer = None;
        for &i in &indices {
            let mut contains = true;
            for &j in &indices {
                if i != j && !inside(clean[j][0], &clean[i], b)? {
                    contains = false;
                }
            }
            if contains {
                if outer.is_some() {
                    return Err(fail(Code::TopologyUvUnresolved, "multiple outer loops"));
                }
                outer = Some(i);
            }
        }
        let outer =
            outer.ok_or_else(|| fail(Code::TopologyUvUnresolved, "no unique outer trim loop"))?;
        for &i in &indices {
            wires[i].role = if i == outer {
                LoopRole::Outer
            } else {
                LoopRole::Inner
            };
        }
        for (n, &i) in indices.iter().enumerate() {
            for &j in &indices[n + 1..] {
                for a in 0..clean[i].len() {
                    for c in 0..clean[j].len() {
                        b.work(1)?;
                        if crosses(
                            clean[i][a],
                            clean[i][(a + 1) % clean[i].len()],
                            clean[j][c],
                            clean[j][(c + 1) % clean[j].len()],
                        ) {
                            return Err(fail(
                                Code::TopologyUvUnresolved,
                                "intersecting face loops",
                            ));
                        }
                    }
                }
                if i != outer
                    && j != outer
                    && (inside(clean[i][0], &clean[j], b)? || inside(clean[j][0], &clean[i], b)?)
                {
                    return Err(fail(
                        Code::TopologyUvUnresolved,
                        "nested inner-loop islands",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn circle_path(
    curve: &CurveGeometry,
    path: &[u32],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<(f64, f64)> {
    let mut distinct = BTreeSet::new();
    let mut first = None;
    let mut previous = 0.;
    let mut total: f64 = 0.;
    for h in path {
        b.work(1)?;
        let p = table.coords[*h as usize];
        distinct.insert(key(p));
        let angle = curve.angular_parameter(p).ok_or_else(|| {
            fail(
                Code::TopologyGeometryUnsupported,
                "carrier has no angular parameter",
            )
        })?;
        let evaluated = curve.evaluate(angle).map_err(|_| {
            fail(
                Code::TopologyPcurveInvalid,
                "intersection branch outside real domain",
            )
        })?;
        if distance(evaluated, p) > 1e-3 {
            return Err(fail(
                Code::TopologyPcurveInvalid,
                "trim sample is off analytic carrier",
            ));
        }
        if first.is_some() {
            let step = delta_angle(angle - previous);
            if (step.abs() - std::f64::consts::PI).abs() < 1e-10 {
                return Err(fail(
                    Code::TopologyTrimUnresolved,
                    "antipodal samples do not select an arc",
                ));
            }
            if total.abs() > 1e-10 && step.abs() > 1e-10 && total * step < 0. {
                return Err(fail(
                    Code::TopologyTrimUnresolved,
                    "angular trim sample reverses direction",
                ));
            }
            total += step;
        } else {
            first = Some(angle);
        }
        previous = angle;
    }
    if distinct.len() < 3 || total.abs() < 1e-10 || total.abs() > std::f64::consts::TAU + 1e-6 {
        return Err(fail(
            Code::TopologyTrimUnresolved,
            "circle path does not select one finite arc",
        ));
    }
    Ok((first.unwrap(), total))
}
fn wires_polar_carriers(
    raw: &RawSource,
    edge: usize,
    point: Vec3,
    surfaces: &[SurfaceRecord],
    wires: &[Wire],
) -> bool {
    let owners: Vec<_> = wires
        .iter()
        .filter(|w| w.occurrences.iter().any(|o| o.edge == edge))
        .collect();
    !owners.is_empty()
        && owners.iter().all(|w| {
            matches!(
                surfaces[w.face].geometry,
                SurfaceGeometry::Cone { .. } | SurfaceGeometry::Sphere { .. }
            ) && charts::singular(&surfaces[w.face].geometry, point)
        })
        && raw.edges[edge].handles.len() == 2
}
fn resolve_planes(
    points: &[PointRecord],
    surfaces: &mut [SurfaceRecord],
    raw: &RawSource,
    binding: &EndpointBinding,
    wires: &[Wire],
    b: &mut Budget<'_>,
) -> Result<()> {
    let ends = &binding.1;
    for (face, surface) in surfaces.iter_mut().enumerate() {
        if matches!(
            surface.geometry,
            SurfaceGeometry::Unknown {
                reason: Code::GeometryPlaneLocationUnresolved
            }
        ) {
            let (normal, normal_source) = raw.packets[face]
                .normal
                .as_ref()
                .ok_or_else(|| fail(Code::TopologyGeometryUnsupported, "plane frame is missing"))?;
            let vertices: Vec<_> = wires
                .iter()
                .filter(|w| w.face == face)
                .flat_map(|w| w.occurrences.iter().flat_map(|o| ends[o.edge]))
                .collect();
            let first = *vertices.first().ok_or_else(|| {
                fail(
                    Code::TopologyGeometryUnsupported,
                    "plane has no native point witness",
                )
            })?;
            let frame = Frame3::from_axis(points[first].position, *normal)
                .ok_or_else(|| fail(Code::TopologyGeometryUnsupported, "invalid plane frame"))?;
            for v in vertices {
                b.work(1)?;
                if dot(sub(points[v].position, frame.origin), frame.z_axis).abs() > 1e-3 {
                    return Err(fail(
                        Code::TopologyPcurveInvalid,
                        "native vertices do not agree with the plane",
                    ));
                }
            }
            surface.geometry = SurfaceGeometry::Plane { frame };
            surface.domains = surface.geometry.domains();
            surface
                .evidence
                .extend([normal_source.clone(), points[first].source.clone()]);
        }
    }
    Ok(())
}
fn resolve_analytic_curves(
    points: &[PointRecord],
    surfaces: &[SurfaceRecord],
    curves: &mut [CurveRecord],
    binding: &EndpointBinding,
    wires: &[Wire],
    b: &mut Budget<'_>,
) -> Result<()> {
    let (table, ends, _) = binding;
    b.work((wires.len() as u64).saturating_mul(curves.len() as u64 + surfaces.len() as u64))?;
    for (e, curve) in curves.iter_mut().enumerate() {
        let faces: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
            .map(|w| w.face)
            .collect();
        charts::resolve_circle(curve, &faces, surfaces, ends[e].map(|v| points[v].position))?;
        let paths: Vec<_> = wires
            .iter()
            .flat_map(|w| w.occurrences.iter())
            .filter(|o| o.edge == e)
            .map(|o| o.path.as_slice())
            .collect();
        intersections::resolve(curve, &faces, surfaces, &paths, table, b)?;
    }
    Ok(())
}
fn resolve_geometry(
    points: &[PointRecord],
    surfaces: &mut [SurfaceRecord],
    curves: &mut [CurveRecord],
    raw: &RawSource,
    binding: &EndpointBinding,
    wires: &mut [Wire],
    b: &mut Budget<'_>,
) -> Result<Vec<[f64; 2]>> {
    let (table, ends, _) = binding;
    b.work((wires.len() as u64).saturating_mul(curves.len() as u64 + surfaces.len() as u64))?;
    resolve_planes(points, surfaces, raw, binding, wires, b)?;
    let sparse = sparse_arcs::recover(points, curves, surfaces, wires, ends, &raw.witnesses, b)?;
    let mut intervals = Vec::new();
    let mut first_error = None;
    for (e, curve) in curves.iter_mut().enumerate() {
        let [a, c] = ends[e];
        let p = points[a].position;
        let q = points[c].position;
        let polar_carriers = wires_polar_carriers(raw, e, p, surfaces, wires);
        let mut all: Vec<_> = wires
            .iter_mut()
            .flat_map(|w| w.occurrences.iter_mut())
            .filter(|o| o.edge == e)
            .collect();
        curve.endpoint_point_ids = Some([points[a].id.clone(), points[c].id.clone()]);
        let resolved = (|| -> Result<[f64; 2]> {
            let interval = if curve.raw_parameters_hex.starts_with("0002003336") {
                let length = distance(p, q);
                if length == 0. {
                    let valid = raw.edges[e].handles.len() == 2
                        && raw.edges[e].handles[0] != raw.edges[e].handles[1]
                        && !all.is_empty()
                        && polar_carriers;
                    if !valid {
                        return Err(Failure {
                            code: Code::TopologyDegenerateUnresolved,
                            source: Some(raw.edges[e].source.clone()),
                            detail: format!("edge {e}: no explicit polar UV incidence"),
                        });
                    }
                    curve.geometry = CurveGeometry::Point { position: p };
                    [0., 1.]
                } else {
                    let direction = unit(sub(q, p))
                        .ok_or_else(|| fail(Code::TopologyGeometryUnsupported, "line direction"))?;
                    curve.geometry = CurveGeometry::Line {
                        origin: p,
                        direction,
                    };
                    for occurrence in &all {
                        for h in &occurrence.path {
                            b.work(1)?;
                            let d = sub(table.coords[*h as usize], p);
                            let t = dot(d, direction);
                            if dot(sub(d, scale(direction, t)), sub(d, scale(direction, t))).sqrt()
                                > 1e-3
                                || t < -1e-3
                                || t > length + 1e-3
                            {
                                return Err(fail(
                                    Code::TopologyPcurveInvalid,
                                    "line trim sample is off its edge interval",
                                ));
                            }
                        }
                    }
                    [0., length]
                }
            } else if matches!(
                curve.geometry,
                CurveGeometry::Nurbs { .. }
                    | CurveGeometry::SurfaceJet { .. }
                    | CurveGeometry::RollingIntersection { .. }
            ) {
                curve
                    .parameter_interval
                    .ok_or_else(|| fail(Code::TopologyTrimUnresolved, "native NURBS interval"))?
            } else if matches!(
                curve.geometry,
                CurveGeometry::Circle { .. }
                    | CurveGeometry::Ellipse { .. }
                    | CurveGeometry::CylinderIntersection { .. }
            ) {
                let mut selected = None;
                for occurrence in &mut all {
                    let mut path = occurrence.path.clone();
                    if a != c && !occurrence.forward {
                        path.reverse();
                    }
                    let sparse_path = path
                        .iter()
                        .map(|h| key(table.coords[*h as usize]))
                        .collect::<BTreeSet<_>>()
                        .len()
                        < 3;
                    let (start, sweep) = if sparse_path && let Some(w) = sparse.get(&e) {
                        let range = w.range;
                        curve.evidence.extend(w.evidence.clone());
                        (range[0], range[1] - range[0])
                    } else {
                        circle_path(&curve.geometry, &path, table, b)?
                    };
                    let interval = if a == c {
                        if (sweep.abs() - std::f64::consts::TAU).abs() > 1e-6 {
                            return Err(fail(
                                Code::TopologyTrimUnresolved,
                                "closed circle does not make one turn",
                            ));
                        }
                        occurrence.forward = sweep > 0.;
                        [start, start + std::f64::consts::TAU]
                    } else {
                        [start, start + sweep]
                    };
                    if let Some(old) = selected {
                        let old: [f64; 2] = old;
                        if delta_angle(interval[0] - old[0]).abs() > 1e-6
                            || ((interval[1] - interval[0]) - (old[1] - old[0])).abs() > 1e-6
                        {
                            return Err(fail(
                                Code::TopologyTrimUnresolved,
                                "incident faces select different arcs",
                            ));
                        }
                    } else {
                        selected = Some(interval);
                    }
                }
                selected
                    .ok_or_else(|| fail(Code::TopologyIncidenceInvalid, "unused physical edge"))?
            } else {
                return Err(fail(
                    Code::TopologyGeometryUnsupported,
                    format!("edge {e}: unsupported 3D edge carrier"),
                ));
            };
            Ok(interval)
        })();
        let interval = match resolved {
            Ok(v) => v,
            Err(mut e) => {
                if e.code.is_resource_limit() {
                    return Err(e);
                }
                if e.source.is_none() {
                    e.source = Some(curve.source.clone());
                }
                if first_error.is_none() {
                    first_error = Some(e);
                }
                continue;
            }
        };
        curve.parameter_interval = Some(interval);
        curve.endpoint_point_ids = Some([points[a].id.clone(), points[c].id.clone()]);
        if !curve
            .parameterization
            .starts_with("derived_circle_plane_from_source_closed")
            && !curve.parameterization.starts_with("derived_native_")
        {
            curve.parameterization = "indexed_native_endpoint_and_trim_path_gauge".into();
        }
        curve
            .evidence
            .extend([points[a].source.clone(), points[c].source.clone()]);
        intervals.push(interval);
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(intervals)
}

fn orient_wires(
    wires: &[Wire],
    edge_count: usize,
    uses: &[CurveUse],
    b: &mut Budget<'_>,
) -> Result<Vec<bool>> {
    let mut edge_uses = vec![Vec::new(); edge_count];
    let mut links = vec![Vec::new(); wires.len()];
    for (i, w) in wires.iter().enumerate() {
        for o in &w.occurrences {
            edge_uses[o.edge].push((i, o.forward));
        }
    }
    for (e, occurrences) in edge_uses.iter().enumerate() {
        b.work(1)?;
        let faces: BTreeSet<_> = occurrences
            .iter()
            .map(|(w, _)| wires[*w].face as u32)
            .collect();
        let expected: BTreeSet<_> = uses[e].serialized_face_slots.into_iter().collect();
        if occurrences.is_empty()
            || occurrences.len() > 2
            || (if uses[e].serialized_face_slots[0] == uses[e].serialized_face_slots[1] {
                !faces.contains(&uses[e].serialized_face_slots[0])
            } else {
                faces != expected
            })
        {
            return Err(fail(
                Code::TopologyIncidenceInvalid,
                format!("edge {e}: incomplete or nonmanifold incidence"),
            ));
        }
        if occurrences.len() == 2 {
            let (a, x) = occurrences[0];
            let (c, y) = occurrences[1];
            let parity = x == y;
            links[a].push((c, parity));
            links[c].push((a, parity));
        }
    }
    for (i, a) in wires.iter().enumerate() {
        for (j, c) in wires.iter().enumerate().skip(i + 1) {
            b.work(1)?;
            if a.face != c.face {
                continue;
            }
            let expected_opposite = a.role != c.role;
            let currently_opposite = (a.area > 0.) != (c.area > 0.);
            let parity = expected_opposite != currently_opposite;
            links[i].push((j, parity));
            links[j].push((i, parity));
        }
    }
    let mut values = vec![None; wires.len()];
    for i in 0..wires.len() {
        if values[i].is_some() {
            continue;
        }
        values[i] = Some(false);
        let mut queue = VecDeque::from([i]);
        while let Some(a) = queue.pop_front() {
            for &(c, p) in &links[a] {
                b.work(1)?;
                let wanted = values[a].unwrap() ^ p;
                if let Some(v) = values[c] {
                    if v != wanted {
                        return Err(fail(
                            Code::TopologyOrientationConflict,
                            "inconsistent boundary parity",
                        ));
                    }
                } else {
                    values[c] = Some(wanted);
                    queue.push_back(c);
                }
            }
        }
    }
    Ok(values.into_iter().map(Option::unwrap).collect())
}

fn pcurve(
    surface: &SurfaceGeometry,
    curve: &CurveGeometry,
    interval: [f64; 2],
    uv_start: [f64; 2],
    phase_guides: Vec<[f64; 3]>,
) -> Result<PcurveGeometry> {
    match (surface, curve) {
        (SurfaceGeometry::Plane { frame }, CurveGeometry::Line { .. }) => {
            let p = curve
                .evaluate(interval[0])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "line start"))?;
            let q = curve
                .evaluate(interval[1])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "line end"))?;
            let a = inverse(surface, p)?;
            let c = inverse(surface, q)?;
            let _ = frame;
            Ok(PcurveGeometry::Line {
                start: a,
                delta: [c[0] - a[0], c[1] - a[1]],
            })
        }
        (
            SurfaceGeometry::Plane { frame },
            CurveGeometry::Circle {
                frame: circle,
                radius,
            },
        ) => {
            let center = inverse(surface, circle.origin)?;
            if dot(
                cross(frame.z_axis, circle.z_axis),
                cross(frame.z_axis, circle.z_axis),
            ) > 1e-10
            {
                return Err(fail(Code::TopologyPcurveInvalid, "circle is not in plane"));
            }
            Ok(PcurveGeometry::PlaneCircle {
                center,
                cosine: [
                    radius * dot(circle.x_axis, frame.x_axis),
                    radius * dot(circle.x_axis, frame.y_axis),
                ],
                sine: [
                    radius * dot(circle.y_axis, frame.x_axis),
                    radius * dot(circle.y_axis, frame.y_axis),
                ],
                angle_start: interval[0],
                angle_sweep: interval[1] - interval[0],
            })
        }
        (SurfaceGeometry::Cylinder { frame, .. }, CurveGeometry::Line { direction, .. }) => {
            if dot(
                cross(*direction, frame.z_axis),
                cross(*direction, frame.z_axis),
            ) > 1e-10
            {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "line is not a cylinder generator",
                ));
            }
            let p = curve
                .evaluate(interval[0])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "generator start"))?;
            let q = curve
                .evaluate(interval[1])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "generator end"))?;
            let a = inverse(surface, p)?;
            let c = inverse(surface, q)?;
            Ok(PcurveGeometry::Line {
                start: [uv_start[0], a[1]],
                delta: [0., c[1] - a[1]],
            })
        }
        (
            SurfaceGeometry::Cylinder { frame, radius: r },
            CurveGeometry::Circle {
                frame: circle,
                radius,
            },
        ) => {
            if (radius - r).abs() > 1e-3
                || dot(
                    cross(circle.z_axis, frame.z_axis),
                    cross(circle.z_axis, frame.z_axis),
                ) > 1e-10
            {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "circle is not a cylinder section",
                ));
            }
            let d = sub(circle.origin, frame.origin);
            if dot(
                sub(d, scale(frame.z_axis, dot(d, frame.z_axis))),
                sub(d, scale(frame.z_axis, dot(d, frame.z_axis))),
            )
            .sqrt()
                > 1e-3
            {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "section center is off axis",
                ));
            }
            let p = curve
                .evaluate(interval[0])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "section start"))?;
            let a = inverse(surface, p)?;
            Ok(PcurveGeometry::Line {
                start: [uv_start[0], a[1]],
                delta: [
                    (interval[1] - interval[0]) * dot(circle.z_axis, frame.z_axis).signum(),
                    0.,
                ],
            })
        }
        _ => Ok(PcurveGeometry::AnalyticProjection {
            surface: Box::new(surface.clone()),
            curve: Box::new(curve.clone()),
            parameter_range: interval,
            phase_guides,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_population(
    data: &[u8],
    geometry: &mut GeometryModel,
    pop: &GeometryPopulation,
    raw: &RawSource,
    native_uv: Option<&NativeUvModel>,
    b: &mut Budget<'_>,
    boundary_evidence: &mut Option<BoundaryPopulation>,
    analytic_evidence: &mut Option<AnalyticPartial>,
) -> Result<TopologyModel> {
    let pid = pop.id;
    let prefix = format!("p{pid}:brep");
    let body_id = format!("{prefix}:body");
    let points: Vec<_> = geometry
        .points
        .iter()
        .filter(|p| p.population_id == pid)
        .cloned()
        .collect();
    let mut surfaces: Vec<_> = geometry
        .surfaces
        .iter()
        .filter(|p| p.population_id == pid)
        .cloned()
        .collect();
    let mut curves: Vec<_> = geometry
        .curves
        .iter()
        .filter(|p| p.population_id == pid)
        .cloned()
        .collect();
    let uses: Vec<_> = geometry
        .curve_uses
        .iter()
        .filter(|p| p.population_id == pid)
        .cloned()
        .collect();
    let surface_uses: Vec<_> = geometry
        .surface_uses
        .iter()
        .filter(|p| p.population_id == pid)
        .cloned()
        .collect();
    if points.len() != pop.point_count as usize
        || surfaces.len() != pop.face_count as usize
        || curves.len() != raw.edges.len()
        || raw.packets.len() != surfaces.len()
        || uses.len() != curves.len()
    {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "population cardinality",
        ));
    }
    let ids: BTreeSet<_> = points
        .iter()
        .map(|p| p.id.clone())
        .chain(surfaces.iter().map(|p| p.id.clone()))
        .chain(curves.iter().map(|p| p.id.clone()))
        .collect();
    if geometry.unresolved.iter().any(|u| {
        u.code == Code::GeometryNativeIdentityAmbiguous
            && u.candidates.iter().any(|id| ids.contains(id))
    }) {
        return Err(fail(
            Code::TopologyEndpointAmbiguous,
            "native identity has multiple candidates",
        ));
    }
    let binding = choose_index(raw, &points, b)?;
    for (i, c) in curves.iter().enumerate() {
        if c.raw_parameters_hex.starts_with("0002003336")
            && binding.1[i][0] == binding.1[i][1]
            && !uses[i].serialized_face_slots.iter().any(|f| {
                matches!(
                    surfaces[*f as usize].geometry,
                    SurfaceGeometry::Cone { .. } | SurfaceGeometry::Sphere { .. }
                )
            })
        {
            return Err(Failure {
                code: Code::TopologyDegenerateUnresolved,
                source: Some(raw.edges[i].source.clone()),
                detail: format!("edge {i}: collapsed line has no polar carrier"),
            });
        }
    }
    let (mut wires, triangle_count) = boundary::recover(raw, &binding, &uses, &curves, &points, b)?;
    b.add(
        points.len()
            + curves.len()
            + wires.len()
            + wires
                .iter()
                .map(|w| {
                    w.occurrences
                        .iter()
                        .map(|o| o.path.len() + 1)
                        .sum::<usize>()
                })
                .sum::<usize>(),
    )?;
    *boundary_evidence = Some(BoundaryPopulation {
        population_id: pid,
        vertex_points: points.iter().map(|p| p.id.clone()).collect(),
        edges: curves
            .iter()
            .enumerate()
            .map(|(i, c)| BoundaryEdge {
                native_ordinal: i as u32,
                curve: c.id.clone(),
                vertices: binding.1[i].map(|v| v as u32),
                source: raw.edges[i].source.clone(),
            })
            .collect(),
        loops: wires
            .iter()
            .map(|w| BoundaryWire {
                face_ordinal: w.face as u32,
                occurrences: w
                    .occurrences
                    .iter()
                    .map(|o| BoundaryOccurrence {
                        edge_ordinal: o.edge as u32,
                        forward: if binding.1[o.edge][0] == binding.1[o.edge][1] {
                            None
                        } else {
                            Some(o.forward)
                        },
                        trim_handles: o.path.clone(),
                    })
                    .collect(),
                source: raw.packets[w.face].source.clone(),
            })
            .collect(),
        index_scope: TopologyScope {
            population_id: pid,
            index_table_source: binding.0.source.clone(),
            index_entry_count: binding.0.coords.len() as u32,
            uninterpreted_auxiliary_word: binding.0.auxiliary,
            triangle_count: triangle_count as u64,
            endpoint_binding: "unique_bit_exact_counted_vertex_coordinates".into(),
        },
    });
    // Charge scans used to construct ownership and connected components before
    // performing them, including collections whose iterator predicates scan wires.
    b.work(
        (wires.len() as u64)
            .saturating_mul(curves.len() as u64 + surfaces.len() as u64)
            .saturating_mul(4),
    )?;
    nurbs_binding::recover(
        &mut surfaces,
        &mut curves,
        &points,
        &wires,
        &binding,
        native_uv,
        b,
    )?;
    resolve_planes(&points, &mut surfaces, raw, &binding, &wires, b)?;
    rolling_binding::recover(
        &mut surfaces,
        &mut curves,
        &points,
        &wires,
        raw,
        &binding,
        native_uv,
        b,
    )?;
    nurbs_triangles::recover(&mut surfaces, &points, &wires, raw, &binding, native_uv, b)?;
    ruled_binding::recover(
        &mut surfaces,
        &mut curves,
        &points,
        &wires,
        raw,
        &binding,
        native_uv,
        b,
    )?;
    rolling_sections::recover(&mut curves, &surfaces, &points, &wires, &binding, b)?;
    resolve_analytic_curves(&points, &surfaces, &mut curves, &binding, &wires, b)?;
    native_curve_binding::recover(
        &mut curves,
        &surfaces,
        &points,
        &wires,
        &binding,
        native_uv,
        b,
    )?;
    nurbs_limits::recover(
        &mut curves,
        &surfaces,
        &points,
        &wires,
        &binding,
        native_uv,
        b,
    )?;
    if let Some(native) = native_uv {
        for (e, curve) in curves.iter_mut().enumerate() {
            let owners: Vec<_> = wires
                .iter()
                .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
                .map(|w| w.face)
                .collect();
            let paths: Vec<_> = wires
                .iter()
                .flat_map(|w| w.occurrences.iter())
                .filter(|o| o.edge == e)
                .map(|o| o.path.as_slice())
                .collect();
            spherical_seams::resolve(curve, &owners, &surfaces, &paths, &binding.0, native, b)?;
        }
    }
    let intervals = match resolve_geometry(
        &points,
        &mut surfaces,
        &mut curves,
        raw,
        &binding,
        &mut wires,
        b,
    ) {
        Ok(v) => v,
        Err(error) => {
            if !error.code.is_resource_limit() {
                native_support_binding::refine(
                    data,
                    geometry,
                    &mut surfaces,
                    boundary_evidence
                        .as_ref()
                        .ok_or_else(|| fail(Code::TopologyUnavailable, "partial boundary"))?,
                    native_uv,
                    b,
                )?;
                *analytic_evidence = Some(partial::recover(
                    &surfaces, &curves, &wires, raw, &binding, b,
                )?);
            }
            return Err(error);
        }
    };
    if let Some(s) = surfaces
        .iter()
        .find(|s| matches!(s.geometry, SurfaceGeometry::Unknown { .. }))
    {
        let issue_source = s.source.clone();
        let issue_ordinal = s.record_ordinal;
        native_support_binding::refine(
            data,
            geometry,
            &mut surfaces,
            boundary_evidence
                .as_ref()
                .ok_or_else(|| fail(Code::TopologyUnavailable, "partial boundary"))?,
            native_uv,
            b,
        )?;
        *analytic_evidence = Some(partial::recover(
            &surfaces, &curves, &wires, raw, &binding, b,
        )?);
        return Err(Failure {
            code: Code::TopologyGeometryUnsupported,
            source: Some(issue_source),
            detail: format!("face {issue_ordinal}: unresolved strict carrier/trim integration"),
        });
    }
    let (table, ends, _) = binding;
    intersections::rechart_spheres(&mut surfaces, &curves, &wires, raw, &table, b)?;
    prepare_uv(&mut wires, &surfaces, &curves, &table, b)?;
    let flips = orient_wires(&wires, curves.len(), &uses, b)?;
    let mut out = TopologyModel::default();
    out.scopes.push(TopologyScope {
        population_id: pid,
        index_table_source: table.source.clone(),
        index_entry_count: table.coords.len() as u32,
        uninterpreted_auxiliary_word: table.auxiliary,
        triangle_count: triangle_count as u64,
        endpoint_binding: "unique_bit_exact_counted_vertex_coordinates".into(),
    });
    let vertex_id = |i: usize| format!("{prefix}:v{i}");
    let edge_id = |i: usize| format!("{prefix}:e{i}");
    let face_id = |i: usize| format!("{prefix}:f{i}");
    for (i, p) in points.iter().enumerate() {
        b.add(1)?;
        out.vertices.push(Vertex {
            id: vertex_id(i),
            population_id: pid,
            point: p.id.clone(),
            native_allocation_id: p.native_allocation_id,
            edges: Vec::new(),
            source: p.source.clone(),
        });
    }
    for (e, c) in curves.iter().enumerate() {
        b.add(1)?;
        let [a, c0] = ends[e];
        out.edges.push(Edge {
            id: edge_id(e),
            population_id: pid,
            native_ordinal: e as u32,
            native_allocation_tag: c.native_allocation_tag,
            vertices: [vertex_id(a), vertex_id(c0)],
            curve: c.id.clone(),
            parameter_range: intervals[e],
            direction_evidence: if a == c0 {
                "canonical_circle_chart_with_indexed_seam_vertex"
            } else {
                "indexed_handle_port_order_gauge"
            }
            .into(),
            seam: false,
            degenerate: matches!(c.geometry, CurveGeometry::Point { .. }),
            source: raw.edges[e].source.clone(),
            support_source: c.source.clone(),
        });
        out.vertices[a].edges.push(edge_id(e));
        if a != c0 {
            out.vertices[c0].edges.push(edge_id(e));
        }
    }
    for (w, wire) in wires.iter().enumerate() {
        b.add(1)?;
        let loop_id = format!("{prefix}:l{w}");
        let mut coedges = Vec::new();
        let flip = flips[w];
        let ordered: Vec<_> = if flip {
            wire.occurrences.iter().rev().collect()
        } else {
            wire.occurrences.iter().collect()
        };
        for (i, o) in ordered.into_iter().enumerate() {
            b.add(2)?;
            let forward = o.forward ^ flip;
            let interval = if forward {
                intervals[o.edge]
            } else {
                [intervals[o.edge][1], intervals[o.edge][0]]
            };
            let mut path = o.path.clone();
            if flip {
                path.reverse();
            }
            let uv_path: Vec<_> = (0..path.len())
                .map(|j| {
                    wire.uv[if flip {
                        (o.end + wire.uv.len() - j % wire.uv.len()) % wire.uv.len()
                    } else {
                        (o.start + j) % wire.uv.len()
                    }]
                })
                .collect();
            b.work(
                (path.len() as u64).saturating_mul(charts::curve_cost(&curves[o.edge].geometry)),
            )?;
            b.add(charts::carrier_records(
                &surfaces[wire.face].geometry,
                &curves[o.edge].geometry,
            ))?;
            let guides =
                charts::guides(&curves[o.edge].geometry, interval, &path, &uv_path, &table)?;
            let pc = pcurve(
                &surfaces[wire.face].geometry,
                &curves[o.edge].geometry,
                interval,
                uv_path[0],
                guides,
            )?;
            let coedge_id = format!("{prefix}:c{w}_{i}");
            let pcurve_id = format!("{prefix}:uv{w}_{i}");
            out.pcurves.push(Pcurve {
                id: pcurve_id.clone(),
                coedge: coedge_id.clone(),
                surface: surfaces[wire.face].id.clone(),
                edge_parameter_range: interval,
                geometry: pc,
                source: raw.packets[wire.face].source.clone(),
            });
            out.coedges.push(Coedge {
                id: coedge_id.clone(),
                owner_loop: loop_id.clone(),
                edge: edge_id(o.edge),
                forward,
                pcurve: pcurve_id,
                radial_next: None,
                trim_handles: path,
            });
            coedges.push(coedge_id);
        }
        out.loops.push(BoundaryLoop {
            id: loop_id,
            face: face_id(wire.face),
            role: wire.role,
            coedges,
            source: raw.packets[wire.face].source.clone(),
            signed_uv_area: if flip { -wire.area } else { wire.area },
            singular_transitions: Vec::new(),
        });
    }
    singular::capture(&mut out, &surfaces, &points, b)?;
    let mut by_edge: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, c) in out.coedges.iter().enumerate() {
        by_edge.entry(c.edge.clone()).or_default().push(i);
    }
    for (e, edge) in out.edges.iter_mut().enumerate() {
        let members = &by_edge[&edge.id];
        if members.len() == 2 {
            let a = members[0];
            let c = members[1];
            out.coedges[a].radial_next = Some(out.coedges[c].id.clone());
            out.coedges[c].radial_next = Some(out.coedges[a].id.clone());
            let owners: Vec<_> = wires
                .iter()
                .enumerate()
                .flat_map(|(i, w)| {
                    w.occurrences
                        .iter()
                        .filter(move |o| o.edge == e)
                        .map(move |_| i)
                })
                .collect();
            edge.seam = owners.len() == 2 && wires[owners[0]].face == wires[owners[1]].face;
        }
    }
    // Connected face components form regions/shells. Their grouping into one
    // population body is inferred and does not invent a native body identity.
    let mut neighbors = vec![BTreeSet::new(); surfaces.len()];
    for e in 0..curves.len() {
        let owners: Vec<_> = wires
            .iter()
            .filter(|w| w.occurrences.iter().any(|o| o.edge == e))
            .map(|w| w.face)
            .collect();
        for &a in &owners {
            for &c in &owners {
                neighbors[a].insert(c);
            }
        }
    }
    let mut components = Vec::new();
    let mut visited = BTreeSet::new();
    for first in 0..surfaces.len() {
        if !visited.insert(first) {
            continue;
        }
        let mut queue = VecDeque::from([first]);
        let mut faces = Vec::new();
        while let Some(f) = queue.pop_front() {
            b.work(1)?;
            faces.push(f);
            for &next in &neighbors[f] {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        faces.sort_unstable();
        components.push(faces);
    }
    let mut regions = Vec::new();
    let mut closures = Vec::new();
    for (component, faces) in components.iter().enumerate() {
        b.add(2 + faces.len())?;
        let region_id = format!("{prefix}:r{component}");
        let shell_id = format!("{prefix}:s{component}");
        let face_set: BTreeSet<_> = faces.iter().copied().collect();
        let mut shell_edges = BTreeSet::new();
        let mut shell_vertices = BTreeSet::new();
        for w in &wires {
            if face_set.contains(&w.face) {
                for o in &w.occurrences {
                    shell_edges.insert(o.edge);
                    shell_vertices.extend(ends[o.edge]);
                }
            }
        }
        let closed = shell_edges.iter().all(|e| by_edge[&edge_id(*e)].len() == 2);
        let closure = if closed {
            ShellClosure::Closed
        } else {
            ShellClosure::Open
        };
        closures.push(closure);
        let characteristic = shell_vertices.len() as i64
            - shell_edges
                .iter()
                .filter(|e| !matches!(curves[**e].geometry, CurveGeometry::Point { .. }))
                .count() as i64
            + faces
                .iter()
                .map(|f| 2 - wires.iter().filter(|w| w.face == *f).count() as i64)
                .sum::<i64>();
        out.regions.push(Region {
            id: region_id.clone(),
            body: body_id.clone(),
            shells: vec![shell_id.clone()],
        });
        regions.push(region_id.clone());
        out.shells.push(Shell {
            id: shell_id.clone(),
            region: region_id,
            faces: faces.iter().map(|f| face_id(*f)).collect(),
            closure,
            euler_characteristic: characteristic,
        });
        for &f in faces {
            let loops: Vec<_> = out
                .loops
                .iter()
                .filter(|l| l.face == face_id(f))
                .map(|l| l.id.clone())
                .collect();
            let outer = out
                .loops
                .iter()
                .find(|l| l.face == face_id(f) && l.role == LoopRole::Outer)
                .unwrap();
            out.faces.push(Face {
                id: face_id(f),
                shell: shell_id.clone(),
                surface: surfaces[f].id.clone(),
                native_ordinal: f as u32,
                native_sense_byte: surface_uses[f].orientation_sign,
                chart_sense: if outer.signed_uv_area > 0. { 1 } else { -1 },
                orientation_evidence: "mesh_boundary_and_radial_parity_in_derived_chart".into(),
                loops,
                source: raw.packets[f].source.clone(),
            });
        }
    }
    let kind = if closures.iter().all(|c| *c == ShellClosure::Closed) {
        BodyKind::ClosedShell
    } else if closures.iter().all(|c| *c == ShellClosure::Open) {
        BodyKind::OpenShell
    } else {
        BodyKind::Mixed
    };
    b.add(1)?;
    out.bodies.push(Body {
        id: body_id.clone(),
        population_id: pid,
        native_body_id: None,
        grouping_evidence: "one_admitted_FBB_population_with_face_connectivity".into(),
        kind,
        regions,
        export_eligible: false,
        solid_validated: false,
    });
    let resolved: BTreeSet<_> = surfaces
        .iter()
        .filter(|s| !matches!(s.geometry, SurfaceGeometry::Unknown { .. }))
        .map(|s| s.id.clone())
        .chain(
            curves
                .iter()
                .filter(|s| !matches!(s.geometry, CurveGeometry::Unknown { .. }))
                .map(|s| s.id.clone()),
        )
        .collect();
    geometry.unresolved.retain(|u| {
        u.subject_id
            .as_ref()
            .is_none_or(|id| !resolved.contains(id))
    });
    for s in &surfaces {
        let target = geometry.surfaces.iter_mut().find(|x| x.id == s.id).unwrap();
        *target = s.clone();
    }
    for c in &curves {
        let target = geometry.curves.iter_mut().find(|x| x.id == c.id).unwrap();
        *target = c.clone();
    }
    validate_impl(geometry, &out, b)?;
    out.bodies[0].export_eligible = true;
    for p in &mut geometry.populations {
        if p.id == pid {
            p.body_id = Some(body_id.clone());
        }
    }
    for u in &mut geometry.surface_uses {
        if u.population_id == pid {
            u.body_id = Some(body_id.clone());
        }
    }
    for u in &mut geometry.curve_uses {
        if u.population_id == pid {
            u.body_id = Some(body_id.clone());
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationSummary {
    pub bodies: usize,
    pub faces: usize,
    pub edges: usize,
    pub coedges: usize,
    pub pcurves: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyValidationError {
    pub code: Code,
    pub detail: String,
}
impl std::fmt::Display for TopologyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.message(), self.detail)
    }
}
impl std::error::Error for TopologyValidationError {}

fn unique<T>(items: &[T], id: impl Fn(&T) -> &str) -> Result<BTreeMap<String, &T>> {
    let mut map = BTreeMap::new();
    for item in items {
        if map.insert(id(item).to_owned(), item).is_some() {
            return Err(fail(Code::TopologyValidationInvalid, "duplicate identity"));
        }
    }
    Ok(map)
}
fn member<'a, T>(map: &'a BTreeMap<String, T>, id: &str) -> Result<&'a T> {
    map.get(id).ok_or_else(|| {
        fail(
            Code::TopologyValidationInvalid,
            format!("missing identity {id}"),
        )
    })
}
fn uv_distance(surface: &SurfaceGeometry, a: [f64; 2], c: [f64; 2]) -> f64 {
    let scale = match surface {
        SurfaceGeometry::SphereStereographic { radius, .. } => [2. * radius; 2],
        SurfaceGeometry::Cylinder { radius, .. } => [*radius, 1.],
        SurfaceGeometry::Cone { half_angle, .. } => {
            [a[1].max(c[1]).abs().max(1.) * half_angle.sin(), 1.]
        }
        SurfaceGeometry::Sphere { radius, .. } => [*radius; 2],
        SurfaceGeometry::Torus {
            major_radius,
            minor_radius,
            ..
        } => [major_radius + minor_radius, *minor_radius],
        _ => [1.; 2],
    };
    ((a[0] - c[0]) * scale[0]).hypot((a[1] - c[1]) * scale[1])
}

fn validate_impl(
    geometry: &GeometryModel,
    t: &TopologyModel,
    b: &mut Budget<'_>,
) -> Result<ValidationSummary> {
    let mut bound_populations = BTreeSet::new();
    for binding in &t.native_boundary_uv {
        if !bound_populations.insert(binding.population_id) {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "duplicate native UV population",
            ));
        }
        let boundary = t
            .boundaries
            .iter()
            .find(|p| p.population_id == binding.population_id)
            .ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "native UV boundary missing",
                )
            })?;
        let native = t.native_uv.as_ref().ok_or_else(|| {
            fail(
                Code::TopologyValidationInvalid,
                "native UV source model missing",
            )
        })?;
        native_uv::allocations::validate(geometry, boundary, binding, native, b)?;
    }
    native_support_binding::validate(t, b)?;
    b.add(
        t.bodies.len()
            + t.regions.len()
            + t.shells.len()
            + t.faces.len()
            + t.loops.len()
            + t.coedges.len()
            + t.edges.len()
            + t.vertices.len()
            + t.pcurves.len(),
    )?;
    b.add(geometry.points.len() + geometry.curves.len() + geometry.surfaces.len())?;
    b.work((t.vertices.len() as u64).saturating_mul(t.edges.len() as u64))?;
    let points = unique(&geometry.points, |x| &x.id)?;
    let curves = unique(&geometry.curves, |x| &x.id)?;
    let surfaces = unique(&geometry.surfaces, |x| &x.id)?;
    let bodies = unique(&t.bodies, |x| &x.id)?;
    let regions = unique(&t.regions, |x| &x.id)?;
    let shells = unique(&t.shells, |x| &x.id)?;
    let faces = unique(&t.faces, |x| &x.id)?;
    let loops = unique(&t.loops, |x| &x.id)?;
    let coedges = unique(&t.coedges, |x| &x.id)?;
    let edges = unique(&t.edges, |x| &x.id)?;
    let vertices = unique(&t.vertices, |x| &x.id)?;
    let pcurves = unique(&t.pcurves, |x| &x.id)?;
    let mut owned = BTreeSet::new();
    for body in &t.bodies {
        if body.kind == BodyKind::Unresolved {
            if !body.regions.is_empty() || body.export_eligible {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "unresolved body has shape ownership",
                ));
            }
            continue;
        }
        if body.regions.is_empty() || body.solid_validated {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "missing body region or unqualified solid claim",
            ));
        }
        for id in &body.regions {
            let region = member(&regions, id)?;
            if region.body != body.id || !owned.insert(id.clone()) {
                return Err(fail(Code::TopologyValidationInvalid, "region ownership"));
            }
        }
    }
    if owned.len() != regions.len() {
        return Err(fail(Code::TopologyValidationInvalid, "orphan region"));
    }
    owned.clear();
    for region in &t.regions {
        member(&bodies, &region.body)?;
        if region.shells.is_empty() {
            return Err(fail(Code::TopologyValidationInvalid, "empty region"));
        }
        for id in &region.shells {
            let shell = member(&shells, id)?;
            if shell.region != region.id || !owned.insert(id.clone()) {
                return Err(fail(Code::TopologyValidationInvalid, "shell ownership"));
            }
        }
    }
    if owned.len() != shells.len() {
        return Err(fail(Code::TopologyValidationInvalid, "orphan shell"));
    }
    owned.clear();
    for shell in &t.shells {
        member(&regions, &shell.region)?;
        if shell.faces.is_empty() {
            return Err(fail(Code::TopologyValidationInvalid, "empty shell"));
        }
        for id in &shell.faces {
            let face = member(&faces, id)?;
            if face.shell != shell.id || !owned.insert(id.clone()) {
                return Err(fail(Code::TopologyValidationInvalid, "face ownership"));
            }
        }
    }
    if owned.len() != faces.len() {
        return Err(fail(Code::TopologyValidationInvalid, "orphan face"));
    }
    owned.clear();
    for face in &t.faces {
        member(&shells, &face.shell)?;
        let surface = member(&surfaces, &face.surface)?;
        if face.loops.is_empty()
            || !matches!(face.chart_sense, 1 | -1)
            || !matches!(face.native_sense_byte, 1 | -1)
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "face senses or loops",
            ));
        }
        let mut outer = 0;
        for id in &face.loops {
            let boundary = member(&loops, id)?;
            if boundary.face != face.id || !owned.insert(id.clone()) {
                return Err(fail(Code::TopologyValidationInvalid, "loop ownership"));
            }
            if boundary.role == LoopRole::Outer {
                outer += 1;
            }
            let desired = if boundary.role == LoopRole::Outer {
                face.chart_sense
            } else {
                -face.chart_sense
            };
            if !boundary.signed_uv_area.is_finite()
                || boundary.signed_uv_area * desired as f64 <= 0.
            {
                return Err(fail(
                    Code::TopologyOrientationConflict,
                    "outer/inner chart sense",
                ));
            }
            let _ = surface;
        }
        if outer != 1 {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "one outer loop is required",
            ));
        }
    }
    if owned.len() != loops.len() {
        return Err(fail(Code::TopologyValidationInvalid, "orphan loop"));
    }
    owned.clear();
    let face_indices: BTreeMap<_, _> = t
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id.clone(), i))
        .collect();
    let mut uv_wires = Vec::new();
    let mut edge_uses: BTreeMap<String, Vec<&Coedge>> = BTreeMap::new();
    type VertexLink = BTreeMap<(String, usize), Vec<(String, usize)>>;
    let mut vertex_links: BTreeMap<String, VertexLink> = BTreeMap::new();
    for boundary in &t.loops {
        if boundary.coedges.is_empty() {
            return Err(fail(Code::TopologyBoundaryInvalid, "empty ordered loop"));
        }
        let face = member(&faces, &boundary.face)?;
        let surface = &member(&surfaces, &face.surface)?.geometry;
        let mut uv_polygon = Vec::new();
        b.add(boundary.singular_transitions.len())?;
        let mut transition_ids = BTreeSet::new();
        for transition in &boundary.singular_transitions {
            if !boundary.coedges.contains(&transition.after_coedge)
                || !transition_ids.insert(&transition.after_coedge)
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "polar connector ownership",
                ));
            }
        }
        for (i, id) in boundary.coedges.iter().enumerate() {
            b.work(1)?;
            let c = *member(&coedges, id)?;
            if c.owner_loop != boundary.id || !owned.insert(id.clone()) {
                return Err(fail(Code::TopologyValidationInvalid, "coedge ownership"));
            }
            let edge = *member(&edges, &c.edge)?;
            let next = *member(
                &coedges,
                &boundary.coedges[(i + 1) % boundary.coedges.len()],
            )?;
            let next_edge = *member(&edges, &next.edge)?;
            let start = if c.forward { 0 } else { 1 };
            let end = 1 - start;
            let next_start = if next.forward { 0 } else { 1 };
            if edge.vertices[end] != next_edge.vertices[next_start] {
                return Err(fail(
                    Code::TopologyBoundaryInvalid,
                    "coedges do not close head-to-tail",
                ));
            }
            if !edge.degenerate {
                let mut following = None;
                for offset in 1..=boundary.coedges.len() {
                    b.work(1)?;
                    let candidate = *member(
                        &coedges,
                        &boundary.coedges[(i + offset) % boundary.coedges.len()],
                    )?;
                    let candidate_edge = *member(&edges, &candidate.edge)?;
                    if !candidate_edge.degenerate {
                        following = Some((candidate, candidate_edge));
                        break;
                    }
                }
                let (following, following_edge) = following.ok_or_else(|| {
                    fail(
                        Code::TopologyDegenerateUnresolved,
                        "boundary contains only collapsed edges",
                    )
                })?;
                let next_start = if following.forward { 0 } else { 1 };
                if edge.vertices[end] != following_edge.vertices[next_start] {
                    return Err(fail(
                        Code::TopologyBoundaryInvalid,
                        "collapsed chain endpoints disagree",
                    ));
                }
                let links = vertex_links.entry(edge.vertices[end].clone()).or_default();
                let a = (edge.id.clone(), end);
                let d = (following_edge.id.clone(), next_start);
                links.entry(a.clone()).or_default().push(d.clone());
                links.entry(d).or_default().push(a);
            }
            let pc = *member(&pcurves, &c.pcurve)?;
            if pc.coedge != c.id || pc.surface != face.surface {
                return Err(fail(Code::TopologyValidationInvalid, "pcurve ownership"));
            }
            let expected = if c.forward {
                edge.parameter_range
            } else {
                [edge.parameter_range[1], edge.parameter_range[0]]
            };
            if pc.edge_parameter_range != expected {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "edge parameter direction mismatch",
                ));
            }
            let subdivisions = match &pc.geometry {
                PcurveGeometry::Line { .. } => 1,
                PcurveGeometry::AnalyticProjection {
                    phase_guides,
                    surface,
                    curve,
                    ..
                } => {
                    b.add(
                        phase_guides
                            .len()
                            .saturating_add(charts::carrier_records(surface, curve)),
                    )?;
                    b.work(charts::projection_cost(surface, curve).saturating_mul(42))?;
                    b.work(phase_guides.len() as u64 * 40)?;
                    32
                }
                PcurveGeometry::PlaneCircle { angle_sweep, .. } => {
                    if !angle_sweep.is_finite() || angle_sweep.abs() > std::f64::consts::TAU + 1e-6
                    {
                        return Err(fail(Code::TopologyPcurveInvalid, "pcurve sweep"));
                    }
                    (angle_sweep.abs() / (std::f64::consts::PI / 16.))
                        .ceil()
                        .max(1.) as usize
                }
            };
            b.add(subdivisions)?;
            for j in 0..subdivisions {
                uv_polygon.push(
                    pc.geometry
                        .evaluate(j as f64 / subdivisions as f64)
                        .map_err(|_| fail(Code::TopologyPcurveInvalid, "UV boundary sample"))?,
                );
            }
            let curve = &member(&curves, &edge.curve)?.geometry;
            if edge.degenerate != matches!(curve, CurveGeometry::Point { .. })
                || (edge.degenerate
                    && (edge.vertices[0] != edge.vertices[1] || edge.parameter_range != [0., 1.]))
            {
                return Err(fail(
                    Code::TopologyDegenerateUnresolved,
                    "collapsed edge locus or endpoints",
                ));
            }

            for q in [0., 0.125, 0.25, 0.5, 0.75, 0.875, 1.] {
                b.work(1)?;
                let uv = pc
                    .geometry
                    .evaluate(q)
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "invalid UV evaluation"))?;
                let p = surface
                    .evaluate(uv)
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "invalid support evaluation"))?;
                let station = interval_parameter(expected, q);
                let c3 = curve
                    .evaluate(station)
                    .map_err(|_| fail(Code::TopologyPcurveInvalid, "invalid edge evaluation"))?;
                if distance(p, c3) > 1e-3 {
                    return Err(fail(Code::TopologyPcurveInvalid, "3D/2D boundary mismatch"));
                }
            }
            let uv0 = pc
                .geometry
                .evaluate(0.)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "pcurve start"))?;
            let uv1 = pc
                .geometry
                .evaluate(1.)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "pcurve end"))?;
            if edge.degenerate {
                let CurveGeometry::Point { position } = curve else {
                    return Err(fail(Code::TopologyDegenerateUnresolved, "collapsed locus"));
                };
                if !matches!(
                    surface,
                    SurfaceGeometry::Cone { .. } | SurfaceGeometry::Sphere { .. }
                ) || !charts::singular(surface, *position)
                    || (uv0[0] - uv1[0]).abs() < 1e-8
                    || (uv0[1] - uv1[1]).abs() > 1e-8
                {
                    return Err(fail(
                        Code::TopologyDegenerateUnresolved,
                        "collapsed UV interval is not a polar interval",
                    ));
                }
            }
            let next_pc = *member(&pcurves, &next.pcurve)?;
            let next_uv = next_pc
                .geometry
                .evaluate(0.)
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "next UV incidence"))?;
            let transition = boundary
                .singular_transitions
                .iter()
                .find(|tr| tr.after_coedge == c.id);
            if uv_distance(surface, uv1, next_uv) > 1e-3 {
                let tr = transition.ok_or_else(|| {
                    fail(
                        Code::TopologyPcurveInvalid,
                        "UV chain has no polar connector",
                    )
                })?;
                let vertex = *member(&vertices, &edge.vertices[end])?;
                let point = *member(&points, &vertex.point)?;
                if tr.vertex != vertex.id
                    || tr.source != point.source
                    || uv_distance(surface, tr.uv_start, uv1) > 1e-8
                    || uv_distance(surface, tr.uv_end, next_uv) > 1e-8
                {
                    return Err(fail(
                        Code::TopologyValidationInvalid,
                        "polar connector endpoint evidence",
                    ));
                }
                singular::connector(surface, point.position, tr.uv_start, tr.uv_end, b)?;
                uv_polygon.push(uv1);
            } else if transition.is_some() {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "unnecessary polar connector",
                ));
            }
            edge_uses.entry(edge.id.clone()).or_default().push(c);
        }
        uv_wires.push(Wire {
            face: face_indices[&face.id],
            handles: Vec::new(),
            occurrences: Vec::new(),
            uv: uv_polygon,
            role: boundary.role,
            area: 0.,
        });
    }
    classify_uv(&mut uv_wires, t.faces.len(), b)?;
    for (wire, boundary) in uv_wires.iter().zip(&t.loops) {
        if wire.role != boundary.role || wire.area * boundary.signed_uv_area <= 0. {
            return Err(fail(
                Code::TopologyOrientationConflict,
                "pcurve loop classification differs",
            ));
        }
    }
    if owned.len() != coedges.len() || pcurves.len() != coedges.len() {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "orphan coedge or pcurve",
        ));
    }
    let mut used_vertices = BTreeSet::new();
    for edge in &t.edges {
        b.work(1)?;
        if edge.parameter_range.iter().any(|x| !x.is_finite())
            || edge.parameter_range[0] == edge.parameter_range[1]
        {
            return Err(fail(
                Code::TopologyDegenerateUnresolved,
                "unqualified degenerate edge",
            ));
        }
        let curve = &member(&curves, &edge.curve)?.geometry;
        if matches!(
            curve,
            CurveGeometry::Circle { .. }
                | CurveGeometry::Ellipse { .. }
                | CurveGeometry::CylinderIntersection { .. }
        ) && (edge.parameter_range[1] - edge.parameter_range[0]).abs()
            > std::f64::consts::TAU + 1e-6
        {
            return Err(fail(
                Code::TopologyTrimUnresolved,
                "edge circle wraps more than once",
            ));
        }
        for i in 0..2 {
            let vertex = member(&vertices, &edge.vertices[i])?;
            let p = member(&points, &vertex.point)?;
            if !finite(p.position)
                || vertex.population_id != edge.population_id
                || p.population_id != edge.population_id
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "vertex population or coordinate",
                ));
            }
            let evaluated = curve
                .evaluate(edge.parameter_range[i])
                .map_err(|_| fail(Code::TopologyPcurveInvalid, "edge endpoint evaluation"))?;
            if distance(evaluated, p.position) > 1e-3 {
                return Err(fail(
                    Code::TopologyPcurveInvalid,
                    "edge endpoint is off its native vertex",
                ));
            }
            if !vertex.edges.contains(&edge.id) {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "vertex edge ownership",
                ));
            }
            used_vertices.insert(vertex.id.clone());
        }
        let uses = edge_uses
            .get(&edge.id)
            .ok_or_else(|| fail(Code::TopologyIncidenceInvalid, "unused edge"))?;
        if uses.len() > 2 {
            return Err(fail(
                Code::TopologyNonManifold,
                "edge has more than two coedges",
            ));
        }
        if uses.len() == 2 {
            if uses[0].forward == uses[1].forward
                || uses[0].radial_next.as_ref() != Some(&uses[1].id)
                || uses[1].radial_next.as_ref() != Some(&uses[0].id)
            {
                return Err(fail(
                    Code::TopologyOrientationConflict,
                    "radial pair is not opposite",
                ));
            }
        } else if uses[0].radial_next.is_some() {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "open edge has a radial mate",
            ));
        }
        let owner_face =
            |c: &Coedge| -> Result<String> { Ok(member(&loops, &c.owner_loop)?.face.clone()) };
        let seam = uses.len() == 2 && owner_face(uses[0])? == owner_face(uses[1])?;
        if seam != edge.seam {
            return Err(fail(Code::TopologyValidationInvalid, "seam classification"));
        }
        let owner_shells: BTreeSet<_> = uses
            .iter()
            .map(|c| owner_face(c).and_then(|f| Ok(member(&faces, &f)?.shell.clone())))
            .collect::<Result<_>>()?;
        if owner_shells.len() != 1 {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "edge crosses shell ownership",
            ));
        }
        let mut actual = Vec::new();
        for c in uses {
            let face = member(&faces, &owner_face(c)?)?;
            let shell = member(&shells, &face.shell)?;
            let region = member(&regions, &shell.region)?;
            if bodies[&region.body].population_id != edge.population_id
                || surfaces[&face.surface].population_id != edge.population_id
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "cross-population ownership",
                ));
            }
            actual.push(face.native_ordinal);
        }
        let source = geometry
            .curve_uses
            .iter()
            .find(|u| {
                u.population_id == edge.population_id && u.edge_ordinal == edge.native_ordinal
            })
            .ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "serialized incidence missing",
                )
            })?;
        if (source.serialized_face_slots[0] != source.serialized_face_slots[1]
            && actual
                .iter()
                .any(|f| !source.serialized_face_slots.contains(f)))
            || source
                .serialized_face_slots
                .iter()
                .any(|f| !actual.contains(f))
        {
            return Err(fail(
                Code::TopologyIncidenceInvalid,
                "serialized face incidence differs",
            ));
        }
    }
    if used_vertices.len() != vertices.len() {
        return Err(fail(Code::TopologyValidationInvalid, "unused vertex"));
    }
    for vertex in &t.vertices {
        let links = vertex_links
            .get(&vertex.id)
            .ok_or_else(|| fail(Code::TopologyNonManifold, "vertex has no link"))?;
        let mut ends = 0;
        for adjacent in links.values() {
            if adjacent.len() == 1 {
                ends += 1;
            } else if adjacent.len() != 2 {
                return Err(fail(Code::TopologyNonManifold, "branched vertex link"));
            }
        }
        if ends != 0 && ends != 2 {
            return Err(fail(
                Code::TopologyNonManifold,
                "vertex link is neither a disk nor half disk",
            ));
        }
        let first = links.first_key_value().unwrap().0.clone();
        let mut seen = BTreeSet::from([first.clone()]);
        let mut queue = VecDeque::from([first]);
        while let Some(p) = queue.pop_front() {
            for q in &links[&p] {
                b.work(1)?;
                if seen.insert(q.clone()) {
                    queue.push_back(q.clone());
                }
            }
        }
        if seen.len() != links.len() {
            return Err(fail(Code::TopologyNonManifold, "disconnected vertex fan"));
        }
        let expected: BTreeSet<_> = t
            .edges
            .iter()
            .filter(|e| e.vertices.contains(&vertex.id))
            .map(|e| e.id.clone())
            .collect();
        if vertex.edges.iter().cloned().collect::<BTreeSet<_>>() != expected
            || vertex.edges.len() != expected.len()
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "vertex adjacency differs",
            ));
        }
    }
    for shell in &t.shells {
        let mut es = BTreeSet::new();
        let mut vs = BTreeSet::new();
        let mut characteristic = 0;
        for face in &shell.faces {
            let f = member(&faces, face)?;
            characteristic += 2 - f.loops.len() as i64;
            for lid in &f.loops {
                for cid in &member(&loops, lid)?.coedges {
                    let c = member(&coedges, cid)?;
                    let e = member(&edges, &c.edge)?;
                    es.insert(e.id.clone());
                    vs.extend(e.vertices.iter().cloned());
                }
            }
        }
        b.work((shell.faces.len() as u64).saturating_mul(es.len() as u64))?;
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([shell.faces[0].clone()]);
        while let Some(face) = queue.pop_front() {
            if !visited.insert(face.clone()) {
                continue;
            }
            for e in &es {
                for c in &edge_uses[e] {
                    let owner = member(&loops, &c.owner_loop)?.face.clone();
                    if owner == face {
                        for neighbor in &edge_uses[e] {
                            queue.push_back(member(&loops, &neighbor.owner_loop)?.face.clone());
                        }
                    }
                }
            }
        }
        if visited.len() != shell.faces.len() {
            return Err(fail(Code::TopologyNonManifold, "disconnected shell"));
        }
        characteristic +=
            vs.len() as i64 - es.iter().filter(|e| !edges[*e].degenerate).count() as i64;
        let closed = es.iter().all(|e| edge_uses[e].len() == 2);
        if characteristic != shell.euler_characteristic
            || (closed != (shell.closure == ShellClosure::Closed))
            || (closed && (characteristic > 2 || characteristic % 2 != 0))
        {
            return Err(fail(
                Code::TopologyNonManifold,
                "shell closure or Euler characteristic",
            ));
        }
    }
    for body in &t.bodies {
        if body.kind == BodyKind::Unresolved {
            continue;
        }
        let closures: Vec<_> = body
            .regions
            .iter()
            .flat_map(|r| regions[r].shells.iter().map(|s| shells[s].closure))
            .collect();
        let kind = if closures.iter().all(|c| *c == ShellClosure::Closed) {
            BodyKind::ClosedShell
        } else if closures.iter().all(|c| *c == ShellClosure::Open) {
            BodyKind::OpenShell
        } else {
            BodyKind::Mixed
        };
        if kind != body.kind {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "body closure classification",
            ));
        }
    }
    Ok(ValidationSummary {
        bodies: t
            .bodies
            .iter()
            .filter(|b| b.kind != BodyKind::Unresolved)
            .count(),
        faces: t.faces.len(),
        edges: t.edges.len(),
        coedges: t.coedges.len(),
        pcurves: t.pcurves.len(),
    })
}

/// Rechecks public/deserialized references and boundary embeddings. This does
/// not authenticate native provenance or replace external solid validation.
pub fn validate_topology(
    geometry: &GeometryModel,
    topology: &TopologyModel,
    limits: &ParseLimits,
) -> std::result::Result<ValidationSummary, TopologyValidationError> {
    let mut b = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    validate_impl(geometry, topology, &mut b).map_err(|e| TopologyValidationError {
        code: e.code,
        detail: e.detail,
    })
}

/// Validate retained boundary incidence without asserting analytic UV recovery.
pub fn validate_boundaries(
    geometry: &GeometryModel,
    population: &BoundaryPopulation,
    limits: &ParseLimits,
) -> std::result::Result<BoundaryValidationSummary, TopologyValidationError> {
    let mut b = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    boundary::validate(geometry, population, &mut b).map_err(|e| TopologyValidationError {
        code: e.code,
        detail: e.detail,
    })
}

pub use partial::AnalyticValidationSummary;
/// Revalidate independently recovered analytic occurrences; this does not
/// certify loop classification, a shell, or a complete body.
pub fn validate_analytic_partial(
    geometry: &GeometryModel,
    boundary: &BoundaryPopulation,
    partial: &AnalyticPartial,
    limits: &ParseLimits,
) -> std::result::Result<AnalyticValidationSummary, TopologyValidationError> {
    let mut b = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    boundary::validate(geometry, boundary, &mut b)
        .and_then(|_| self::partial::validate(geometry, boundary, partial, &mut b))
        .map_err(|e| TopologyValidationError {
            code: e.code,
            detail: e.detail,
        })
}

fn append(to: &mut TopologyModel, mut from: TopologyModel) {
    to.boundaries.append(&mut from.boundaries);
    to.analytic_partials.append(&mut from.analytic_partials);
    to.bodies.append(&mut from.bodies);
    to.regions.append(&mut from.regions);
    to.shells.append(&mut from.shells);
    to.faces.append(&mut from.faces);
    to.loops.append(&mut from.loops);
    to.coedges.append(&mut from.coedges);
    to.edges.append(&mut from.edges);
    to.vertices.append(&mut from.vertices);
    to.pcurves.append(&mut from.pcurves);
    to.scopes.append(&mut from.scopes);
    to.unresolved.append(&mut from.unresolved);
}
pub(crate) fn recover(
    data: &[u8],
    container: &ContainerInfo,
    geometry: &mut GeometryModel,
    limits: &ParseLimits,
    options: &TopologyOptions,
    model: &mut TopologyModel,
    diagnostics: &mut Vec<Diagnostic>,
) -> std::result::Result<(), Box<Diagnostic>> {
    let mut b = Budget {
        limits,
        steps: 0,
        elements: 0,
        bytes: 0,
    };
    let populations = geometry.populations.clone();
    let native_uv = native_uv::decode(data, container, &mut b)
        .map_err(|e| Box::new(Diagnostic::new(e.code, Severity::Error, None)))?;
    if !native_uv.parameter_maps.is_empty()
        || !native_uv.pcurves.is_empty()
        || !native_uv.carriers.is_empty()
        || !native_uv.nurbs_carriers.is_empty()
        || !native_uv.rolling_ball_carriers.is_empty()
        || !native_uv.ruled_carriers.is_empty()
    {
        model.native_uv = Some(native_uv);
    }
    for pop in populations {
        let mut staged = geometry.clone();
        let mut boundary_evidence = None;
        let mut analytic_evidence = None;
        let result =
            (|| {
                b.work(1)?;
                let remaining = ParseLimits {
                    max_topology_bytes: limits.max_topology_bytes.saturating_sub(b.bytes),
                    max_topology_records: limits.max_topology_records.saturating_sub(b.elements),
                    max_topology_steps: limits.max_topology_steps.saturating_sub(b.steps),
                    ..*limits
                };
                let raw = crate::native::topology_source(data, container, &pop, &remaining)
                    .map_err(|d| Failure {
                        code: d.code,
                        source: None,
                        detail: d.message,
                    })?;
                b.bytes = b
                    .bytes
                    .saturating_add(raw.main.len() as u64)
                    .saturating_add(raw.additional_bytes);
                if b.bytes > limits.max_topology_bytes {
                    return Err(fail(Code::TopologyByteLimitExceeded, "source bytes"));
                }
                b.work(raw.work)?;
                b.add(raw.elements as usize)?;
                build_population(
                    data,
                    &mut staged,
                    &pop,
                    &raw,
                    model.native_uv.as_ref(),
                    &mut b,
                    &mut boundary_evidence,
                    &mut analytic_evidence,
                )
            })();
        match result {
            Ok(mut out) => {
                let mismatch = out
                    .bodies
                    .iter()
                    .any(|body| match options.expected_closure {
                        ClosureExpectation::Unspecified => false,
                        ClosureExpectation::Closed => body.kind != BodyKind::ClosedShell,
                        ClosureExpectation::Open => body.kind != BodyKind::OpenShell,
                    });
                if mismatch {
                    for body in &mut out.bodies {
                        body.export_eligible = false;
                    }
                    out.unresolved.push(TopologyIssue {
                        population_id: pop.id,
                        code: Code::TopologyClosureMismatch,
                        source: Some(pop.face_rows.clone()),
                        detail:
                            "caller-supplied closure expectation; native intent remains undeclared"
                                .into(),
                    });
                }
                *geometry = staged;
                append(model, out);
            }
            Err(e) => {
                if e.code.is_resource_limit() {
                    return Err(Box::new(Diagnostic::new(e.code, Severity::Error, None)));
                }
                if let Some(analytic) = analytic_evidence {
                    model.analytic_partials.push(analytic);
                }
                if let Some(boundary) = boundary_evidence {
                    model.boundaries.push(boundary);
                }
                model.unresolved.push(TopologyIssue {
                    population_id: pop.id,
                    code: e.code,
                    source: e.source.or(Some(pop.face_rows.clone())),
                    detail: e.detail,
                });
                model.bodies.push(Body {
                    id: format!("p{}:brep:body", pop.id),
                    population_id: pop.id,
                    native_body_id: None,
                    grouping_evidence: "unresolved_standard_population".into(),
                    kind: BodyKind::Unresolved,
                    regions: Vec::new(),
                    export_eligible: false,
                    solid_validated: false,
                });
            }
        }
    }
    if let Some(native) = &model.native_uv {
        model.native_boundary_uv = native_uv::allocations::recover(
            data,
            geometry,
            &model.boundaries,
            &model.analytic_partials,
            native,
            &mut b,
        )
        .map_err(|e| Box::new(Diagnostic::new(e.code, Severity::Error, None)))?;
    }
    if model.bodies.is_empty() {
        model.unresolved.push(TopologyIssue {
            population_id: 0,
            code: Code::TopologyUnavailable,
            source: None,
            detail: "no admitted geometry population".into(),
        });
    }
    let mut codes = Vec::new();
    for issue in &model.unresolved {
        if !codes.contains(&issue.code) {
            codes.push(issue.code);
            diagnostics.push(Diagnostic::new(issue.code, Severity::Warning, None));
        }
    }
    Ok(())
}
