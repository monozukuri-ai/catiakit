//! Exact indexed aliases and complete physical-edge coverage of mesh cycles.
use super::*;

#[derive(Clone)]
struct Placement {
    occurrence: Occurrence,
    segments: usize,
}

fn line_path(
    curve: &CurveRecord,
    ends: [usize; 2],
    points: &[PointRecord],
    path: &[u32],
    table: &IndexTable,
    b: &mut Budget<'_>,
) -> Result<bool> {
    if !curve.raw_parameters_hex.starts_with("0002003336") {
        return Ok(true);
    }
    let a = points[ends[0]].position;
    let d = sub(points[ends[1]].position, a);
    let length = dot(d, d).sqrt();
    for h in path {
        b.work(1)?;
        let p = table.coords[*h as usize];
        if length == 0. {
            if key(a) != key(p) {
                return Ok(false);
            }
        } else {
            let axis = scale(d, 1. / length);
            let delta = sub(p, a);
            let t = dot(delta, axis);
            if distance(delta, scale(axis, t)) > 1e-3 || t < -1e-3 || t > length + 1e-3 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

struct Context<'a> {
    raw: &'a RawSource,
    binding: &'a EndpointBinding,
    uses: &'a [CurveUse],
    curves: &'a [CurveRecord],
    points: &'a [PointRecord],
}

fn placements(
    face: usize,
    cycle: &[u32],
    cx: &Context<'_>,
    b: &mut Budget<'_>,
) -> Result<Vec<Placement>> {
    let (table, ends, map) = cx.binding;
    let n = cycle.len();
    let mut at: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, h) in cycle.iter().enumerate() {
        b.work(1)?;
        if let Some(v) = map.get(&key(table.coords[*h as usize])) {
            at.entry(*v).or_default().push(i);
        }
    }
    let mut found = Vec::new();
    for (e, row) in cx.raw.edges.iter().enumerate() {
        b.work(1)?;
        let slots = cx.uses[e].serialized_face_slots;
        // Repeated slots identify one known owner, not necessarily both owners.
        // A second owner needs a complete indexed occurrence and global closure.
        if slots[0] != slots[1] && !slots.contains(&(face as u32)) {
            continue;
        }
        let [a, c] = ends[e];
        let mut starts = at.get(&a).cloned().unwrap_or_default();
        if a != c {
            starts.extend(at.get(&c).into_iter().flatten());
        }
        for start in starts {
            let start_vertex = map[&key(table.coords[cycle[start] as usize])];
            let other = if start_vertex == a { c } else { a };
            for &end in at.get(&other).into_iter().flatten() {
                b.work(1)?;
                let segments = (end + n - start) % n;
                let segments = if segments == 0 { n } else { segments };
                if (cx.raw.complete_edge_runs || row.handles.len() > 2)
                    && segments + 1 != row.handles.len()
                {
                    continue;
                }
                b.work(segments as u64 + 1)?;
                let path: Vec<_> = (0..=segments).map(|i| cycle[(start + i) % n]).collect();
                let matched = if cx.raw.complete_edge_runs {
                    same_path(&row.handles, &path, table)
                } else {
                    row.handles.len() == 2
                        || same_path(
                            &row.handles[1..row.handles.len() - 1],
                            &path[1..path.len() - 1],
                            table,
                        )
                };
                if !matched || !line_path(&cx.curves[e], ends[e], cx.points, &path, table, b)? {
                    continue;
                }
                b.add(path.len() + 1)?;
                found.push(Placement {
                    occurrence: Occurrence {
                        edge: e,
                        path,
                        start,
                        end,
                        forward: start_vertex == a,
                    },
                    segments,
                });
            }
        }
    }
    Ok(found)
}

type Signature = Vec<(usize, usize, usize)>;
fn signature(path: &[Occurrence], cycle: &[u32], binding: &EndpointBinding) -> Signature {
    let (table, _, map) = binding;
    let tokens: Vec<_> = path
        .iter()
        .map(|o| {
            (
                o.edge,
                map[&key(table.coords[cycle[o.start] as usize])],
                map[&key(table.coords[cycle[o.end] as usize])],
            )
        })
        .collect();
    (0..tokens.len())
        .map(|i| tokens[i..].iter().chain(&tokens[..i]).copied().collect())
        .min()
        .unwrap_or_default()
}

fn cycle_solutions(
    face: usize,
    cycle: &[u32],
    cx: &Context<'_>,
    b: &mut Budget<'_>,
) -> Result<Vec<Vec<Occurrence>>> {
    let candidates = placements(face, cycle, cx, b)?;
    let n = cycle.len();
    let mut next: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, c) in candidates.iter().enumerate() {
        next.entry(c.occurrence.start).or_default().push(i);
    }
    let mut found = BTreeMap::new();
    for (first, c) in candidates.iter().enumerate() {
        if c.occurrence.start != 0 && c.occurrence.start + c.segments < n {
            continue;
        }
        let root = c.occurrence.start;
        let mut stack = vec![(vec![first], c.segments)];
        while let Some((path, length)) = stack.pop() {
            b.work(1)?;
            if length == n {
                let result: Vec<_> = path
                    .iter()
                    .map(|i| candidates[*i].occurrence.clone())
                    .collect();
                let key = signature(&result, cycle, cx.binding);
                if let std::collections::btree_map::Entry::Vacant(entry) = found.entry(key) {
                    b.add(result.len())?;
                    entry.insert(result);
                }
                continue;
            }
            for &i in next.get(&((root + length) % n)).into_iter().flatten() {
                b.work(path.len() as u64 + 1)?;
                let candidate = &candidates[i];
                let edge = candidate.occurrence.edge;
                let cap = if cx.uses[edge].serialized_face_slots == [face as u32; 2] {
                    2
                } else {
                    1
                };
                if length + candidate.segments > n
                    || path
                        .iter()
                        .filter(|i| candidates[**i].occurrence.edge == edge)
                        .count()
                        >= cap
                {
                    continue;
                }
                b.add(path.len() + 1)?;
                let mut extended = path.clone();
                extended.push(i);
                stack.push((extended, length + candidate.segments));
            }
        }
    }
    Ok(found.into_values().collect())
}

fn match_face(
    face: usize,
    cycles: Vec<Vec<u32>>,
    cx: &Context<'_>,
    b: &mut Budget<'_>,
) -> Result<Vec<Wire>> {
    let mut domains = Vec::new();
    for cycle in &cycles {
        let solutions = cycle_solutions(face, cycle, cx, b)?;
        if solutions.is_empty() {
            return Err(fail(
                Code::TopologyTrimUnresolved,
                format!("face {face}: no complete indexed boundary cover"),
            ));
        }
        domains.push(solutions);
    }
    let required: BTreeSet<_> = cx
        .uses
        .iter()
        .enumerate()
        .filter(|(_, u)| u.serialized_face_slots.contains(&(face as u32)))
        .map(|(i, _)| i)
        .collect();
    let mut states = vec![Vec::<usize>::new()];
    for (i, domain) in domains.iter().enumerate() {
        let mut next = Vec::new();
        for state in states {
            for choice in 0..domain.len() {
                b.work(1)?;
                let selected = state.iter().copied().chain([choice]);
                let mut used = BTreeMap::<usize, usize>::new();
                for (j, which) in selected.enumerate() {
                    for o in &domains[j][which] {
                        b.work(1)?;
                        *used.entry(o.edge).or_default() += 1;
                    }
                }
                if used.iter().any(|(e, n)| {
                    *n > if cx.uses[*e].serialized_face_slots == [face as u32; 2] {
                        2
                    } else {
                        1
                    }
                }) {
                    continue;
                }
                if i + 1 == domains.len() && !required.iter().all(|e| used.contains_key(e)) {
                    continue;
                }
                b.add(state.len() + 1)?;
                let mut extended = state.clone();
                extended.push(choice);
                next.push(extended);
            }
        }
        states = next;
    }
    if states.len() != 1 {
        return Err(fail(
            Code::TopologyTrimUnresolved,
            format!(
                "face {face}: {} complete boundary assignments",
                states.len()
            ),
        ));
    }
    let chosen = states.remove(0);
    Ok(cycles
        .into_iter()
        .enumerate()
        .map(|(i, handles)| Wire {
            face,
            handles,
            occurrences: domains[i][chosen[i]].clone(),
            uv: Vec::new(),
            role: LoopRole::Outer,
            area: 0.,
        })
        .collect())
}

pub(super) fn recover(
    raw: &RawSource,
    binding: &EndpointBinding,
    uses: &[CurveUse],
    curves: &[CurveRecord],
    points: &[PointRecord],
    b: &mut Budget<'_>,
) -> Result<(Vec<Wire>, usize)> {
    let cx = Context {
        raw,
        binding,
        uses,
        curves,
        points,
    };
    let mut wires = Vec::new();
    let mut triangles = 0;
    for (face, packet) in raw.packets.iter().enumerate() {
        let locate = |mut e: Failure| {
            if e.source.is_none() {
                e.source = Some(packet.source.clone());
            }
            e
        };
        let (cycles, n) = mesh_boundaries(packet, b).map_err(locate)?;
        triangles += n;
        wires.extend(match_face(face, cycles, &cx, b).map_err(locate)?);
    }
    let mut incidence = vec![Vec::new(); raw.edges.len()];
    for w in &wires {
        for o in &w.occurrences {
            incidence[o.edge].push(w.face as u32);
        }
    }
    for (e, owners) in incidence.iter().enumerate() {
        b.work(1)?;
        let slots = uses[e].serialized_face_slots;
        if owners.is_empty()
            || owners.len() > 2
            || !slots.iter().all(|f| owners.contains(f))
            || (slots[0] != slots[1] && owners.iter().any(|f| !slots.contains(f)))
        {
            return Err(fail(
                Code::TopologyIncidenceInvalid,
                format!("edge {e}: incomplete indexed incidence"),
            ));
        }
    }
    Ok((wires, triangles))
}

pub(super) fn validate(
    g: &GeometryModel,
    t: &BoundaryPopulation,
    b: &mut Budget<'_>,
) -> Result<BoundaryValidationSummary> {
    b.add(t.vertex_points.len() + t.edges.len() + t.loops.len())?;
    b.add(g.points.len() + g.curves.len())?;
    let pop = g
        .populations
        .iter()
        .find(|p| p.id == t.population_id)
        .ok_or_else(|| {
            fail(
                Code::TopologyValidationInvalid,
                "boundary population missing",
            )
        })?;
    if t.vertex_points.len() != pop.point_count as usize
        || t.edges.len() != pop.edge_count as usize
        || t.index_scope.population_id != pop.id
    {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "boundary population cardinality",
        ));
    }
    let points: BTreeMap<_, _> = g.points.iter().map(|p| (&p.id, p)).collect();
    let curves: BTreeMap<_, _> = g.curves.iter().map(|c| (&c.id, c)).collect();
    let mut ids = BTreeSet::new();
    for (i, id) in t.vertex_points.iter().enumerate() {
        let p = points
            .get(id)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "boundary point missing"))?;
        if !ids.insert(id)
            || p.population_id != pop.id
            || p.table_ordinal as usize != i
            || !finite(p.position)
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "boundary native vertex identity",
            ));
        }
    }
    for (i, e) in t.edges.iter().enumerate() {
        let c = curves
            .get(&e.curve)
            .ok_or_else(|| fail(Code::TopologyValidationInvalid, "boundary curve missing"))?;
        if e.native_ordinal as usize != i
            || c.population_id != pop.id
            || c.record_ordinal as usize != i
            || e.vertices
                .iter()
                .any(|v| *v as usize >= t.vertex_points.len())
        {
            return Err(fail(
                Code::TopologyValidationInvalid,
                "boundary edge identity",
            ));
        }
    }
    let used_vertices: BTreeSet<_> = t.edges.iter().flat_map(|e| e.vertices).collect();
    if used_vertices.len() != t.vertex_points.len() {
        return Err(fail(
            Code::TopologyValidationInvalid,
            "unused boundary vertex",
        ));
    }
    let mut uses = vec![Vec::new(); t.edges.len()];
    let mut faces = BTreeSet::new();
    let mut occurrences = 0;
    for wire in &t.loops {
        if wire.face_ordinal >= pop.face_count || wire.occurrences.is_empty() {
            return Err(fail(
                Code::TopologyBoundaryInvalid,
                "boundary face or empty cycle",
            ));
        }
        faces.insert(wire.face_ordinal);
        b.add(wire.occurrences.len())?;
        for (i, o) in wire.occurrences.iter().enumerate() {
            b.work(1)?;
            b.add(o.trim_handles.len())?;
            let e = t
                .edges
                .get(o.edge_ordinal as usize)
                .ok_or_else(|| fail(Code::TopologyValidationInvalid, "boundary edge reference"))?;
            let next = &wire.occurrences[(i + 1) % wire.occurrences.len()];
            let ne = t
                .edges
                .get(next.edge_ordinal as usize)
                .ok_or_else(|| fail(Code::TopologyValidationInvalid, "boundary next edge"))?;
            if o.trim_handles.len() < 2
                || o.trim_handles
                    .iter()
                    .any(|h| *h >= t.index_scope.index_entry_count)
                || next.trim_handles.first() != o.trim_handles.last()
            {
                return Err(fail(Code::TopologyBoundaryInvalid, "boundary handle chain"));
            }
            if (o.forward.is_none() && e.vertices[0] != e.vertices[1])
                || (next.forward.is_none() && ne.vertices[0] != ne.vertices[1])
            {
                return Err(fail(
                    Code::TopologyValidationInvalid,
                    "unresolved nonclosed boundary direction",
                ));
            }
            let end = e.vertices[usize::from(o.forward.unwrap_or(true))];
            let start = ne.vertices[usize::from(!next.forward.unwrap_or(true))];
            if end != start {
                return Err(fail(
                    Code::TopologyBoundaryInvalid,
                    "boundary native endpoints do not close",
                ));
            }
            uses[o.edge_ordinal as usize].push(wire.face_ordinal);
            occurrences += 1;
        }
    }
    if faces.len() != pop.face_count as usize {
        return Err(fail(
            Code::TopologyIncidenceInvalid,
            "boundary misses face population",
        ));
    }
    let mut inferred = 0;
    for (i, owners) in uses.iter().enumerate() {
        let source = g
            .curve_uses
            .iter()
            .find(|u| u.population_id == pop.id && u.edge_ordinal as usize == i)
            .ok_or_else(|| {
                fail(
                    Code::TopologyValidationInvalid,
                    "boundary incidence missing",
                )
            })?;
        let slots = source.serialized_face_slots;
        b.work(g.curve_uses.len() as u64)?;
        if owners.is_empty()
            || owners.len() > 2
            || !slots.iter().all(|f| owners.contains(f))
            || (slots[0] != slots[1] && owners.iter().any(|f| !slots.contains(f)))
        {
            return Err(fail(
                Code::TopologyIncidenceInvalid,
                "boundary incidence conflicts with serialized slots",
            ));
        }
        if owners.iter().any(|f| !slots.contains(f)) {
            inferred += 1;
        }
    }
    Ok(BoundaryValidationSummary {
        vertices: t.vertex_points.len(),
        edges: t.edges.len(),
        faces: faces.len(),
        loops: t.loops.len(),
        occurrences,
        closed: uses.iter().all(|u| u.len() == 2),
        inferred_second_owners: inferred,
    })
}
