# catiakit

A read-only Rust library and CLI for inspecting CATIA V5 files.

catiakit reads the CFV2 container of CATPart, CATProduct and CATDrawing
documents without CATIA, a commercial SDK, or a geometry kernel. It reports
directories, named streams and physical extents, the declared document type,
saved-by versions, typed property sets, application containers, the document
link table, and stored JPEG previews. It reconstructs a stream or extracts a
preview while retaining source positions, and it decodes a supported subset of
CATPart geometry and indexed B-rep topology.

Every report separates what was admitted from what remains unknown. Unknown
fields and byte ranges stay visible, coverage is reported per feature, and
diagnostics carry stable codes and byte positions. Overall coverage remains
`partial`. See [compatibility](docs/compatibility.md) for the exact scope and
limitations, and the [roadmap](docs/roadmap.md) for planned work.

## CLI

Build and run with the Rust toolchain selected by the repository:

```sh
cargo run -p catiakit-cli -- inspect path/to/model.CATPart
cargo run -p catiakit-cli -- inspect path/to/model.CATPart --json
cargo run -p catiakit-cli -- dump-stream path/to/model.CATPart --stream-id 0 -o stream.bin
cargo run -p catiakit-cli -- extract-preview path/to/model.CATPart -o preview.jpg
cargo run -p catiakit-cli -- decode path/to/model.CATPart --json -o geometry.json
cargo run -p catiakit-cli -- decode path/to/model.CATPart --topology --require topology --json -o brep.json
```

Use `-` as the input path to read standard input. File extensions do not
determine the document kind; the kind follows the root storage declaration
(`part`, `product`, `drawing`, `shape`), and other declared types stay `unknown`
with the declared string retained.

`inspect` lists directories, streams and metadata. Select a stream by its
numeric ID from `inspect` on the same input; names can repeat. `dump-stream`
creates a new file and preserves existing output files; `--json` reports the
result without embedding the stream payload.

`extract-preview` also preserves existing files. Specify `--preview-id ID` if
several previews are listed. It copies the framed JPEG through its end marker;
trailing bytes in its source stream remain opaque and produce a warning. JPEG
marker framing is checked without decoding pixels.

Saved-by versions retain every admitted observation. Conflicting or malformed
observations have no governing value. The saved document name and every link
locator path are reported as stored strings and are never opened or resolved.
The link table lists the document's own save path, its feature catalogs, and
for assemblies the saved paths of their components. `not_found` means no value
was admitted in the inspected scopes; it does not establish absence throughout
an unknown native document.

`inspect` keeps geometry `not_requested`. Use `decode` to request the supported
carrier subset. Its JSON separates definitions, serialized uses, native tags,
table ordinals, and source ranges. Unresolved carriers and references remain
explicit. `-o` creates a new JSON report file, including partial or rejected
outcomes. `--require geometry` returns a nonzero status while complete geometry
and required topology are unavailable.

`--topology` also requests ordered loops, oriented coedges, endpoint
identities, and pcurves for supported analytic and qualified native NURBS
boundaries. `--require topology` implies this request and fails if any selected
population remains unresolved. Closed and open shells are explicit; neither is
certified as a solid. `--topology --expect-closure closed` adds a
caller-supplied closure condition. Incomplete bodies have
`export_eligible: false`, and partial populations retain their boundary
evidence instead of a body.

Input is limited to 64 MiB by default. Input, directory, stream, metadata,
preview, nesting, geometry, topology and scan limits are configurable; see a
subcommand's `--help`.

| Exit code | Meaning |
| --- | --- |
| 0 | Inspection returned a report; coverage can still be partial |
| 1 | Input or a completeness requirement was rejected, or a configured limit was exceeded |
| 2 | Usage, input I/O, or output I/O failed |

## Rust API

```rust
use catiakit_core::{inspect_file, ParseOptions};

let outcome = inspect_file("model.CATPart", &ParseOptions::default());
println!("{:?}", outcome.report.status);
```

`ParseOutcome<DocumentInfo>` contains optional document information and a
`ParseReport`. `DocumentInfo` carries the derived `kind` and the
`declared_type` string; its `MetadataInfo` separates `saved_by`, `storage`
(document declaration, saved name, containers), `property_sets`, `links`,
`previews`, and per-stream `scopes`. JSON reports include a schema version,
per-feature coverage, stable diagnostic codes, and byte positions where
available. Branch on diagnostic codes and coverage rather than message text or
the presence of a value.

The core also accepts byte slices and `Read` sources through `inspect` and
`inspect_reader`. `binary::ByteReader` provides bounded borrowed reads with
explicit endianness and checked offsets.

`extract_stream`, `extract_stream_reader`, and `extract_stream_file` inspect
and extract from one source image. `container::reconstruct_stream` accepts an
admitted `StreamInfo` and the same source bytes used for inspection, and
revalidates ranges and lengths before copying.

`extract_preview`, `extract_preview_reader`, and `extract_preview_file` return
`ParseOutcome<PreviewImage>`. Pass `None` to select a unique admitted preview or
`Some(id)` to select explicitly. `PreviewImage` contains the JPEG bytes and its
`PreviewInfo`. Metadata sources include logical ranges and physical pieces in
logical order, including fragmented records.

`decode_geometry`, `decode_geometry_reader`, and `decode_geometry_file` return
`ParseOutcome<GeometryDocument>`. Resolved lines, circles, planes, cylinders,
cones, spheres, and tori expose bounded evaluation APIs. Their domains describe
canonical carriers; serialized edge intervals, face trimming, and body
assignment are resolved only by the topology request. Unit context records the
selected grammar convention and whether a native declaration was found.

`decode_brep` and `decode_brep_with_options` add an optional `TopologyModel`
to that document. Reader/file variants also accept `TopologyOptions`, including
an optional closure expectation. Recovered populations contain derived carrier
intervals and body assignments. `validate_topology`, `validate_boundaries` and
`validate_analytic_partial` recheck public or deserialized references and
boundary embeddings. Native face signs and derived chart senses remain
separate. Unsupported and degenerate boundaries retain diagnostics and source
ranges. `topology.native_uv` and `topology.native_boundary_uv` retain the
stored UV programs, parameter maps and their source bindings with evaluation
helpers; these are source evidence, not healed edges.

See [development](docs/development.md) for build checks, fixtures and the
corpus checks, and the [changelog](CHANGELOG.md) for report schema changes.

## License

catiakit is offered under [PolyForm Noncommercial 1.0.0](LICENSES/PolyForm-Noncommercial-1.0.0.md).
[Commercial licenses](COMMERCIAL-LICENSE.md) are available from
[UnRobotics Inc.](https://www.un-robotics.com/#contact).
Dependencies retain their own licenses and copyright notices. Read the
[licensing guide](docs/license.md), [license notice](LICENSE) and
[third-party notices](THIRD_PARTY_NOTICES.md) for details.

Required Notice: catiakit is licensed by UnRobotics Inc. (https://www.un-robotics.com/).

Required Notice: catiakit is offered under PolyForm Noncommercial 1.0.0; separate commercial licenses are available.
