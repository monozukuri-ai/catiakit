//! Sample the edge curves and procedural surfaces of a decoded document.
//!
//! Reads a `decode --topology --json` outcome from stdin and writes JSON with
//! one point list per recovered edge, evaluated over the edge's own parameter
//! interval, one UV list per pcurve over its normalized traversal parameter,
//! and one point grid per surface that has no exact counterpart in common
//! geometry kernels. External adapters use exact kernel primitives for
//! analytic and NURBS carriers and interpolate these samples for the rest.
//!
//! Arguments: `[samples_per_edge] [grid_u] [grid_v]` (defaults 33, 25, 13).
use catiakit_core::{
    GeometryDocument, ParseOutcome,
    geometry::{CurveGeometry, SurfaceGeometry},
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn argument(index: usize, default: usize) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(std::env::args()
        .nth(index)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .map_or(default, |v| v.max(2)))
}

fn kind(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let per_edge = argument(1, 33)?;
    let grid_u = argument(2, 25)?;
    let grid_v = argument(3, 13)?;
    let outcome: ParseOutcome<GeometryDocument> = serde_json::from_reader(std::io::stdin().lock())?;
    let document = outcome.value.ok_or("no document")?;
    let topology = document
        .topology
        .as_ref()
        .ok_or("topology was not requested")?;
    let curves: HashMap<&str, &CurveGeometry> = document
        .geometry
        .curves
        .iter()
        .map(|c| (c.id.as_str(), &c.geometry))
        .collect();
    let mut edges = Vec::new();
    for edge in &topology.edges {
        let curve = curves
            .get(edge.curve.as_str())
            .ok_or_else(|| format!("edge {} references unknown curve {}", edge.id, edge.curve))?;
        let [a, b] = edge.parameter_range;
        let mut points = Vec::with_capacity(per_edge);
        let mut error = None;
        for i in 0..per_edge {
            let t = match i {
                0 => a,
                i if i + 1 == per_edge => b,
                i => a + (b - a) * i as f64 / (per_edge - 1) as f64,
            };
            match curve.evaluate(t) {
                Ok(p) => points.push(p),
                Err(e) => {
                    error = Some(format!("{e:?} at parameter {t}"));
                    break;
                }
            }
        }
        edges.push(json!({
            "id": edge.id,
            "curve": edge.curve,
            "kind": kind(curve),
            "parameter_range": edge.parameter_range,
            "vertices": edge.vertices,
            "points": if error.is_none() { points } else { Vec::new() },
            "error": error,
        }));
    }
    let mut pcurves = Vec::new();
    for pcurve in &topology.pcurves {
        let mut uv = Vec::with_capacity(per_edge);
        let mut error = None;
        for i in 0..per_edge {
            let q = i as f64 / (per_edge - 1) as f64;
            match pcurve.geometry.evaluate(q) {
                Ok(p) => uv.push(p),
                Err(e) => {
                    error = Some(format!("{e:?} at normalized parameter {q}"));
                    break;
                }
            }
        }
        pcurves.push(json!({
            "id": pcurve.id,
            "coedge": pcurve.coedge,
            "surface": pcurve.surface,
            "kind": kind(&pcurve.geometry),
            "edge_parameter_range": pcurve.edge_parameter_range,
            "uv": if error.is_none() { uv } else { Vec::new() },
            "error": error,
        }));
    }
    let mut grids = Vec::new();
    for surface in &document.geometry.surfaces {
        let SurfaceGeometry::RollingBall { surface: jet } = &surface.geometry else {
            continue;
        };
        let [u0, u1] = jet.parameter_range;
        let mut rows = Vec::with_capacity(grid_u);
        let mut error = None;
        'outer: for i in 0..grid_u {
            // The end stations use the declared bounds exactly; the linear
            // form can round one ULP past the domain.
            let u = match i {
                0 => u0,
                i if i + 1 == grid_u => u1,
                i => u0 + (u1 - u0) * i as f64 / (grid_u - 1) as f64,
            };
            let mut row = Vec::with_capacity(grid_v);
            for j in 0..grid_v {
                let v = match j {
                    0 => 0.0,
                    j if j + 1 == grid_v => 1.0,
                    j => j as f64 / (grid_v - 1) as f64,
                };
                match surface.geometry.evaluate([u, v]) {
                    Ok(p) => row.push(p),
                    Err(e) => {
                        error = Some(format!("{e:?} at uv ({u}, {v})"));
                        break 'outer;
                    }
                }
            }
            rows.push(row);
        }
        grids.push(json!({
            "id": surface.id,
            "kind": "rolling_ball",
            "u_range": jet.parameter_range,
            "v_range": [0.0, 1.0],
            "points": if error.is_none() { rows } else { Vec::new() },
            "error": error,
        }));
    }
    serde_json::to_writer(
        std::io::stdout().lock(),
        &json!({
            "samples_per_edge": per_edge,
            "grid": [grid_u, grid_v],
            "edges": edges,
            "pcurves": pcurves,
            "surface_grids": grids,
        }),
    )?;
    Ok(())
}
