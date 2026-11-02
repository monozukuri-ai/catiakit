use catiakit_core::{
    ClosureExpectation, TopologyOptions, decode_brep_file, decode_brep_reader,
    metadata::StorageRole,
};
use catiakit_core::{
    Diagnostic, DiagnosticCode, DocumentInfo, FeatureStatus, ParseLimits, ParseOptions,
    ParseOutcome, ParseReport, ParseStatus, PreviewInfo, Severity, decode_geometry_file,
    decode_geometry_reader, extract_preview_file, extract_preview_reader, extract_stream_file,
    extract_stream_reader, inspect_file, inspect_reader,
};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    name = "catiakit",
    version,
    about = "Inspect CATIA V5 containers and logical streams"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect directories, stream extents, and available decoding coverage.
    Inspect(InspectArgs),
    /// Reconstruct one stream into a new output file.
    DumpStream(DumpArgs),
    /// Extract one bounded stored JPEG into a new output file.
    ExtractPreview(PreviewArgs),
    /// Decode supported analytic carriers and retain unresolved geometry.
    Decode(DecodeArgs),
}

#[derive(Args)]
struct SourceArgs {
    /// Input file, or - for standard input. Extension is not used for detection.
    input: PathBuf,
    #[command(flatten)]
    limits: LimitsArgs,
}

#[derive(Args)]
struct InspectArgs {
    #[command(flatten)]
    source: SourceArgs,
    /// Emit one versioned JSON report, including rejected inputs.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DumpArgs {
    #[command(flatten)]
    source: SourceArgs,
    /// Stream id from inspect on the same input image.
    #[arg(long)]
    stream_id: u32,
    /// New output file. Existing files are preserved.
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct PreviewArgs {
    #[command(flatten)]
    source: SourceArgs,
    /// Required when several candidates are available.
    #[arg(long)]
    preview_id: Option<u32>,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct DecodeArgs {
    #[command(flatten)]
    source: SourceArgs,
    #[arg(long)]
    json: bool,
    /// Write the full outcome to a new JSON file, including partial reports.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Require complete geometry and topology; partial carriers do not satisfy it.
    #[arg(long,value_parser=["geometry", "topology"])]
    require: Option<String>,
    /// Recover and validate the supported indexed B-rep subset.
    #[arg(long)]
    topology: bool,
    /// Caller-supplied closure expectation, independent of native author intent.
    #[arg(long, value_parser=["unspecified", "closed", "open"], requires="topology")]
    expect_closure: Option<String>,
}

#[derive(Args)]
struct LimitsArgs {
    #[arg(long, default_value_t = ParseLimits::default().max_input_bytes)]
    max_input_bytes: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_read_bytes)]
    max_read_bytes: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_directory_bytes)]
    max_directory_bytes: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_streams)]
    max_streams: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_extents)]
    max_extents: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_stream_bytes)]
    max_stream_bytes: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_total_stream_bytes)]
    max_total_stream_bytes: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_nesting_depth)]
    max_nesting_depth: u32,
    #[arg(long, default_value_t = ParseLimits::default().max_scan_steps)]
    max_scan_steps: u64,
    #[arg(long, default_value_t = ParseLimits::default().max_name_bytes)]
    max_name_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_metadata_bytes)]
    max_metadata_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_metadata_records)]
    max_metadata_records: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_metadata_steps)]
    max_metadata_steps: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_property_bytes)]
    max_property_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_preview_bytes)]
    max_preview_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_preview_pixels)]
    max_preview_pixels: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_geometry_bytes)]
    max_geometry_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_geometry_records)]
    max_geometry_records: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_geometry_steps)]
    max_geometry_steps: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_topology_bytes)]
    max_topology_bytes: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_topology_records)]
    max_topology_records: u64,
    #[arg(long,default_value_t=ParseLimits::default().max_topology_steps)]
    max_topology_steps: u64,
}

impl LimitsArgs {
    fn options(&self) -> ParseOptions {
        ParseOptions {
            limits: ParseLimits {
                max_input_bytes: self.max_input_bytes,
                max_read_bytes: self.max_read_bytes,
                max_directory_bytes: self.max_directory_bytes,
                max_streams: self.max_streams,
                max_extents: self.max_extents,
                max_stream_bytes: self.max_stream_bytes,
                max_total_stream_bytes: self.max_total_stream_bytes,
                max_nesting_depth: self.max_nesting_depth,
                max_scan_steps: self.max_scan_steps,
                max_name_bytes: self.max_name_bytes,
                max_metadata_bytes: self.max_metadata_bytes,
                max_metadata_records: self.max_metadata_records,
                max_metadata_steps: self.max_metadata_steps,
                max_property_bytes: self.max_property_bytes,
                max_preview_bytes: self.max_preview_bytes,
                max_preview_pixels: self.max_preview_pixels,
                max_geometry_bytes: self.max_geometry_bytes,
                max_geometry_records: self.max_geometry_records,
                max_geometry_steps: self.max_geometry_steps,
                max_topology_bytes: self.max_topology_bytes,
                max_topology_records: self.max_topology_records,
                max_topology_steps: self.max_topology_steps,
            },
        }
    }
}

fn exit_code(report: &ParseReport) -> ExitCode {
    if report.diagnostics.iter().any(|d| {
        matches!(
            d.code,
            DiagnosticCode::OpenFailed | DiagnosticCode::ReadFailed | DiagnosticCode::OutputFailed
        )
    }) {
        ExitCode::from(2)
    } else if report.status == ParseStatus::Rejected {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn write_report(output: &mut impl Write, report: &ParseReport) -> io::Result<()> {
    writeln!(output, "inspection: {:?}", report.status)?;
    if let Some(size) = report.input_size_bytes {
        writeln!(output, "input bytes: {size}")?;
    }
    for diagnostic in &report.diagnostics {
        let code = serde_json::to_value(diagnostic.code).map_err(io::Error::other)?;
        writeln!(
            output,
            "{}: {}",
            code.as_str().unwrap_or("unknown"),
            diagnostic
        )?;
    }
    Ok(())
}

fn write_json<T: Serialize>(output: &mut impl Write, outcome: &ParseOutcome<T>) -> io::Result<()> {
    serde_json::to_writer(&mut *output, outcome).map_err(io::Error::other)?;
    writeln!(output)
}

fn inspect(args: InspectArgs, output: &mut impl Write) -> io::Result<ExitCode> {
    let options = args.source.limits.options();
    let outcome = if args.source.input.as_os_str() == "-" {
        inspect_reader(io::stdin().lock(), &options)
    } else {
        inspect_file(&args.source.input, &options)
    };
    if args.json {
        write_json(output, &outcome)?;
    } else {
        write_report(output, &outcome.report)?;
        if let Some(DocumentInfo {
            kind,
            container,
            metadata,
            ..
        }) = &outcome.value
        {
            writeln!(
                output,
                "format: CATIA V5 CFV2; document kind: {kind:?}{}",
                match &outcome.value.as_ref().and_then(|v| v.declared_type.clone()) {
                    Some(declared) => format!(" (declared {declared})"),
                    None => String::new(),
                }
            )?;
            if let Some(container) = container {
                writeln!(
                    output,
                    "layout: {:?}; directories: {}; streams: {}",
                    container.layout,
                    container.directories.len(),
                    container.streams.len()
                )?;
                for stream in &container.streams {
                    writeln!(
                        output,
                        "stream {}: {} ({} bytes, {} extents)",
                        stream.id,
                        stream.name.as_deref().unwrap_or("<undecoded name>"),
                        stream.logical_length,
                        stream.extents.len()
                    )?;
                }
            }
            if let Some(metadata) = metadata {
                writeln!(output, "saved by: {:?}", metadata.saved_by.state)?;
                if let Some(v) = &metadata.saved_by.value {
                    writeln!(
                        output,
                        "V{}R{} SP{} HF{} ({})",
                        v.version,
                        v.release,
                        v.service_pack,
                        v.hot_fix.as_deref().unwrap_or("-"),
                        v.build_date
                    )?;
                }
                writeln!(output, "storage: {:?}", metadata.storage.state)?;
                if let Some(name) = &metadata.storage.saved_name {
                    writeln!(output, "saved name: {:?}", name.name)?;
                }
                let containers: Vec<_> = metadata
                    .storage
                    .declarations
                    .iter()
                    .filter(|d| d.role == StorageRole::Container)
                    .map(|d| d.type_name.as_str())
                    .collect();
                writeln!(
                    output,
                    "containers: {} ({})",
                    containers.len(),
                    containers.join(", ")
                )?;
                let sets: Vec<_> = metadata
                    .property_sets
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect();
                writeln!(output, "property sets: {}", sets.join(", "))?;
                writeln!(
                    output,
                    "links: {:?} ({} entries)",
                    metadata.links.state,
                    metadata.links.items.len()
                )?;
                for link in &metadata.links.items {
                    let Some(locator) = &link.locator else {
                        writeln!(output, "link {}: undecoded payload", link.ordinal)?;
                        continue;
                    };
                    if locator.document_type == "feat" {
                        continue;
                    }
                    writeln!(
                        output,
                        "link {}: {} {} {}",
                        link.ordinal,
                        locator.kind,
                        locator.document_type,
                        locator.path.as_deref().unwrap_or("<no path>")
                    )?;
                }
                writeln!(
                    output,
                    "previews: {:?} ({})",
                    metadata.previews.state,
                    metadata.previews.items.len()
                )?;
                for p in &metadata.previews.items {
                    writeln!(
                        output,
                        "preview {}: {} x {}, {} bytes, stream {}",
                        p.id, p.width, p.height, p.source.length, p.source.stream_id
                    )?;
                }
            }
        }
    }
    Ok(exit_code(&outcome.report))
}

#[derive(Serialize)]
struct DumpInfo {
    stream_id: u32,
    bytes_written: u64,
}

fn dump(args: DumpArgs, output: &mut impl Write) -> io::Result<ExitCode> {
    let options = args.source.limits.options();
    let extracted = if args.source.input.as_os_str() == "-" {
        extract_stream_reader(io::stdin().lock(), args.stream_id, &options)
    } else {
        extract_stream_file(&args.source.input, args.stream_id, &options)
    };
    let mut report = extracted.report;
    let mut value = None;
    if let Some(bytes) = extracted.value {
        let written = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args.output)
            .and_then(|mut file| file.write_all(&bytes));
        match written {
            Ok(()) => {
                value = Some(DumpInfo {
                    stream_id: args.stream_id,
                    bytes_written: bytes.len() as u64,
                })
            }
            Err(error) => {
                report.status = ParseStatus::Rejected;
                let mut diagnostic =
                    Diagnostic::new(DiagnosticCode::OutputFailed, Severity::Error, None);
                diagnostic.details.io_error_kind = Some(format!("{:?}", error.kind()));
                report.diagnostics.push(diagnostic);
            }
        }
    }
    let outcome = ParseOutcome { value, report };
    if args.json {
        write_json(output, &outcome)?;
    } else {
        write_report(output, &outcome.report)?;
        if let Some(info) = &outcome.value {
            writeln!(
                output,
                "stream {}: wrote {} bytes",
                info.stream_id, info.bytes_written
            )?;
        }
    }
    Ok(exit_code(&outcome.report))
}

#[derive(Serialize)]
struct PreviewOutput {
    preview: PreviewInfo,
    bytes_written: u64,
}

fn preview(args: PreviewArgs, output: &mut impl Write) -> io::Result<ExitCode> {
    let options = args.source.limits.options();
    let extracted = if args.source.input.as_os_str() == "-" {
        extract_preview_reader(io::stdin().lock(), args.preview_id, &options)
    } else {
        extract_preview_file(&args.source.input, args.preview_id, &options)
    };
    let mut report = extracted.report;
    let mut value = None;
    if let Some(image) = extracted.value {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args.output)
            .and_then(|mut f| f.write_all(&image.bytes))
        {
            Ok(()) => {
                value = Some(PreviewOutput {
                    preview: image.info,
                    bytes_written: image.bytes.len() as u64,
                })
            }
            Err(error) => {
                report.status = ParseStatus::Rejected;
                let mut d = Diagnostic::new(DiagnosticCode::OutputFailed, Severity::Error, None);
                d.details.io_error_kind = Some(format!("{:?}", error.kind()));
                report.diagnostics.push(d);
            }
        }
    }
    let outcome = ParseOutcome { value, report };
    if args.json {
        write_json(output, &outcome)?;
    } else {
        write_report(output, &outcome.report)?;
        if let Some(v) = &outcome.value {
            writeln!(
                output,
                "preview {}: {} x {}, wrote {} bytes",
                v.preview.id, v.preview.width, v.preview.height, v.bytes_written
            )?;
        }
    }
    Ok(exit_code(&outcome.report))
}

fn decode(args: DecodeArgs, output: &mut impl Write) -> io::Result<ExitCode> {
    let options = args.source.limits.options();
    let topology_options = TopologyOptions {
        expected_closure: match args.expect_closure.as_deref() {
            Some("closed") => ClosureExpectation::Closed,
            Some("open") => ClosureExpectation::Open,
            _ => ClosureExpectation::Unspecified,
        },
    };
    let request_topology = args.topology || args.require.as_deref() == Some("topology");
    let mut outcome = if request_topology && args.source.input.as_os_str() == "-" {
        decode_brep_reader(io::stdin().lock(), &options, &topology_options)
    } else if request_topology {
        decode_brep_file(&args.source.input, &options, &topology_options)
    } else if args.source.input.as_os_str() == "-" {
        decode_geometry_reader(io::stdin().lock(), &options)
    } else {
        decode_geometry_file(&args.source.input, &options)
    };
    if args.require.is_some()
        && outcome.report.status != ParseStatus::Rejected
        && (outcome.report.features.topology != FeatureStatus::Complete
            || (args.require.as_deref() == Some("geometry")
                && outcome.report.features.geometry != FeatureStatus::Complete))
    {
        outcome.report.status = ParseStatus::Rejected;
        outcome.report.diagnostics.push(Diagnostic::new(
            if args.require.as_deref() == Some("topology") {
                DiagnosticCode::RequiredTopologyUnavailable
            } else {
                DiagnosticCode::RequiredGeometryUnavailable
            },
            Severity::Error,
            None,
        ));
    }
    if let Some(path) = args.output {
        let saved = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| {
                serde_json::to_writer(&mut file, &outcome).map_err(io::Error::other)?;
                writeln!(file)
            });
        if let Err(error) = saved {
            outcome.report.status = ParseStatus::Rejected;
            let mut d = Diagnostic::new(DiagnosticCode::OutputFailed, Severity::Error, None);
            d.details.io_error_kind = Some(format!("{:?}", error.kind()));
            outcome.report.diagnostics.push(d);
        }
    }
    if args.json {
        write_json(output, &outcome)?;
    } else {
        write_report(output, &outcome.report)?;
        if let Some(value) = &outcome.value {
            let g = &value.geometry;
            writeln!(
                output,
                "geometry: {:?}; populations: {}; points: {}; surfaces: {}; curves: {}; unresolved: {}",
                g.layout,
                g.populations.len(),
                g.points.len(),
                g.surfaces.len(),
                g.curves.len(),
                g.unresolved.len()
            )?;
            if let Some(t) = &value.topology {
                writeln!(
                    output,
                    "topology: {:?}; bodies: {}; faces: {}; edges: {}; vertices: {}; unresolved: {}",
                    outcome.report.features.topology,
                    t.bodies.len(),
                    t.faces.len(),
                    t.edges.len(),
                    t.vertices.len(),
                    t.unresolved.len()
                )?;
            }
        }
    }
    Ok(exit_code(&outcome.report))
}

fn run() -> io::Result<ExitCode> {
    let mut output = io::stdout().lock();
    let exit = match Cli::parse().command {
        Command::Inspect(args) => inspect(args, &mut output)?,
        Command::DumpStream(args) => dump(args, &mut output)?,
        Command::ExtractPreview(args) => preview(args, &mut output)?,
        Command::Decode(args) => decode(args, &mut output)?,
    };
    output.flush()?;
    Ok(exit)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("unable to write inspection output: {error}");
            ExitCode::from(2)
        }
    }
}
