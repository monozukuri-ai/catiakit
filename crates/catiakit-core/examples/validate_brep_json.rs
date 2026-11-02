//! Revalidate a CLI outcome from stdin, independently of native decoding.
use catiakit_core::{
    GeometryDocument, ParseLimits, ParseOutcome, topology::validate_boundaries, validate_topology,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let max_topology_records = std::env::args()
        .nth(1)
        .map(|x| x.parse())
        .transpose()?
        .unwrap_or(ParseLimits::default().max_topology_records);
    let limits = ParseLimits {
        max_topology_records,
        max_topology_steps: std::env::args()
            .nth(2)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(ParseLimits::default().max_topology_steps),
        ..ParseLimits::default()
    };
    let outcome: ParseOutcome<GeometryDocument> = serde_json::from_reader(std::io::stdin().lock())?;
    let d = outcome.value.ok_or("no document")?;
    let t = d.topology.ok_or("topology was not requested")?;
    let graph = validate_topology(&d.geometry, &t, &limits)?;
    let native_uv = t
        .native_uv
        .as_ref()
        .map(|n| catiakit_core::topology::validate_native_uv(n, &limits))
        .transpose()?;
    let native_boundary_uv = t
        .native_boundary_uv
        .iter()
        .map(|binding| {
            let boundary = t
                .boundaries
                .iter()
                .find(|b| b.population_id == binding.population_id)
                .ok_or("missing native boundary")?;
            Ok::<_, Box<dyn std::error::Error>>(
                catiakit_core::topology::validate_native_boundary_uv(
                    &d.geometry,
                    boundary,
                    binding,
                    t.native_uv.as_ref().ok_or("no native UV")?,
                    &limits,
                )?,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let boundaries = t
        .boundaries
        .iter()
        .map(|p| validate_boundaries(&d.geometry, p, &limits))
        .collect::<Result<Vec<_>, _>>()?;
    let analytic = t
        .analytic_partials
        .iter()
        .map(|p| {
            let boundary = t
                .boundaries
                .iter()
                .find(|b| b.population_id == p.population_id)
                .ok_or("missing analytic boundary")?;
            Ok::<_, Box<dyn std::error::Error>>(catiakit_core::topology::validate_analytic_partial(
                &d.geometry,
                boundary,
                p,
                &limits,
            )?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_writer(
        std::io::stdout().lock(),
        &serde_json::json!({"topology":graph,"boundary_populations":boundaries,"analytic_partials":analytic,"native_uv":native_uv,"native_boundary_uv":native_boundary_uv}),
    )?;
    Ok(())
}
