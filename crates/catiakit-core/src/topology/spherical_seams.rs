//! Native full-sphere meridian productions supply candidate circle planes.
//! Complete matched trim paths select a unique serialized plane; no plane fit.
use super::*;
pub(super) fn resolve(
    curve: &mut CurveRecord,
    faces: &[usize],
    surfaces: &[SurfaceRecord],
    paths: &[&[u32]],
    table: &IndexTable,
    native: &NativeUvModel,
    b: &mut Budget<'_>,
) -> Result<()> {
    if !matches!(curve.geometry, CurveGeometry::Unknown { .. })
        || !curve.raw_parameters_hex.starts_with("0012003337")
        || paths.len() != 2
    {
        return Ok(());
    }
    let mut fields = [0.; 4];
    let hex = &curve.raw_parameters_hex[10..];
    if hex.len() != 32 {
        return Ok(());
    }
    for (i, v) in fields.iter_mut().enumerate() {
        *v = f32::from_bits(
            u32::from_str_radix(&hex[8 * i..8 * i + 8], 16)
                .map_err(|_| fail(Code::TopologyGeometryUnsupported, "circle parameter bits"))?,
        ) as f64;
    }
    let center = [fields[0], fields[1], fields[2]];
    let radius = fields[3];
    if radius <= 0. || !radius.is_finite() || !finite(center) {
        return Ok(());
    }
    let owners: BTreeSet<_> = faces.iter().copied().collect();
    if owners.len()!=2||owners.iter().any(|f|!matches!(&surfaces[*f].geometry,SurfaceGeometry::Sphere{frame,radius:r} if frame.origin==center&&r.to_bits()==radius.to_bits())){return Ok(());}
    if paths.iter().any(|p| {
        p.iter()
            .map(|h| key(table.coords[*h as usize]))
            .collect::<BTreeSet<_>>()
            .len()
            < 3
    }) {
        return Ok(());
    }
    let first = paths[0];
    let p = sub(table.coords[first[0] as usize], center);
    let q = sub(table.coords[*first.last().unwrap() as usize], center);
    if dot(p, q) > -radius * radius * (1. - 1e-5) {
        return Ok(());
    }
    let mut accepted = Vec::new();
    for meridian in &native.sphere_meridians {
        let Some(carrier) = native.carriers.iter().find(|c| c.id == meridian.carrier) else {
            continue;
        };
        let SurfaceGeometry::Sphere { frame, radius: r } = &carrier.geometry else {
            continue;
        };
        if (*r as f32).to_bits() != (radius as f32).to_bits()
            || (0..3).any(|d| (frame.origin[d] as f32).to_bits() != (center[d] as f32).to_bits())
        {
            continue;
        }
        if distance(p, scale(frame.z_axis, dot(p, frame.z_axis))) > 1e-3
            || distance(q, scale(frame.z_axis, dot(q, frame.z_axis))) > 1e-3
        {
            continue;
        }
        let mut valid = true;
        for path in paths {
            for h in *path {
                b.work(1)?;
                let d = sub(table.coords[*h as usize], center);
                if dot(d, meridian.normal).abs() > 1e-3 || (dot(d, d).sqrt() - radius).abs() > 1e-3
                {
                    valid = false;
                    break;
                }
            }
            if !valid {
                break;
            }
        }
        if valid {
            accepted.push(meridian);
        }
    }
    if !accepted.is_empty()
        && accepted.iter().all(|a| {
            dot(
                cross(a.normal, accepted[0].normal),
                cross(a.normal, accepted[0].normal),
            ) < 1e-20
        })
    {
        curve.geometry = CurveGeometry::Circle {
            frame: Frame3::from_axis(center, accepted[0].normal)
                .ok_or_else(|| fail(Code::TopologyGeometryUnsupported, "native meridian plane"))?,
            radius,
        };
        curve.domain = curve.geometry.domain();
        curve.parameterization =
            "derived_circle_plane_from_source_closed_native_sphere_meridian".into();
        curve
            .evidence
            .extend(accepted.iter().map(|m| m.source.clone()));
        curve.evidence.push(table.source.clone());
    }
    Ok(())
}
