//! Development-only comparison with the frozen SHA-256 corpus inventory.
use catiakit_core::{ParseOptions, ParseStatus, container::reconstruct_stream, inspect};
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
    let outcome = inspect(&data, &options);
    if outcome.report.status == ParseStatus::Rejected {
        return Err("container inspection rejected the input".into());
    }
    let container = outcome
        .value
        .as_ref()
        .and_then(|v| v.container.as_ref())
        .ok_or("no container")?;
    let mut hashes = Vec::new();
    for stream in &container.streams {
        let bytes = reconstruct_stream(&data, stream, &options.limits)?;
        let hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        hashes.push(json!({"stream_id":stream.id,"sha256":hash}));
    }
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(
        &mut output,
        &json!({"inspection":outcome,"stream_hashes":hashes}),
    )?;
    writeln!(output)?;
    Ok(())
}
