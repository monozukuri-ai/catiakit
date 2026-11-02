//! Local metadata validation helper; no reference implementation dependency.
use catiakit_core::{ParseOptions, ParseStatus, extract_preview, inspect};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("expected input path")?;
    let options = ParseOptions::default();
    let mut data = Vec::new();
    File::open(path)?
        .take(options.limits.max_input_bytes + 1)
        .read_to_end(&mut data)?;
    let inspection = inspect(&data, &options);
    if inspection.report.status == ParseStatus::Rejected {
        return Err("metadata inspection rejected input".into());
    }
    let info = inspection
        .value
        .as_ref()
        .and_then(|v| v.metadata.as_ref())
        .ok_or("no metadata")?;
    let mut hashes = Vec::new();
    for preview in &info.previews.items {
        let image = extract_preview(&data, Some(preview.id), &options)
            .value
            .ok_or("preview extraction failed")?;
        let hash: String = Sha256::digest(&image.bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        hashes.push(json!({"preview_id":preview.id,"sha256":hash,"size_bytes":image.bytes.len()}));
    }
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(
        &mut out,
        &json!({"inspection":inspection,"preview_hashes":hashes}),
    )?;
    writeln!(out)?;
    Ok(())
}
