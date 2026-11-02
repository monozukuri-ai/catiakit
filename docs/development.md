# Development

The Cargo workspace contains `catiakit-core` and `catiakit-cli`. `rust-toolchain.toml` selects Rust 1.93.0 with rustfmt and Clippy. `Cargo.lock` fixes application and test dependencies. Package publication is disabled during initial development.

Run from the repository root:

```sh
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
python3 tools/license/check_license.py
python3 tools/check_public_tree.py
```

CI defines build and test jobs for Ubuntu 24.04, Windows Server 2025, and macOS 15. Formatting, Clippy, the license and public-tree checks, documentation checks, and the corpus tooling tests run on Linux. The matrix uses synthetic input and does not download the NIST corpus. A separate `corpus` job runs on pushes to `main`, on manual dispatch and weekly: it fetches the hash-pinned NIST archives, verifies them, and runs the container, metadata and document-property checks, uploading the JSON reports as an artifact.

## Inspection contract

Inspection validates the CFV2 outer directory pointer, directory signature and sentinel, and admitted descriptors and extent lists. Zero padding after the sentinel within the declared directory length is retained as an opaque range with a warning. Owned nested directories are inspected recursively. Outer named streams are decoded for property sets, storage declarations, the document link table, and JPEG previews. The document kind follows the unique kind-2 storage declaration. Overall status remains `partial`, and geometry and topology `not_requested` until `decode` requests them.

Metadata coverage is `unsupported` when no relevant stream was inspected, `partial` for the supported subset, and `invalid` when a typed scope or value is malformed. Metadata problems preserve container evidence; resource failures reject the request and mark metadata aggregates `incomplete`. Inspection can still return partial results for malformed metadata, while extraction requires an admitted candidate. See [metadata framing](format-notes/metadata.md).

Container coverage is `partial` when unknown directory ranges or warning diagnostics remain. It can be `complete` only for a fully consumed directory representation within the supported grammar. This describes container framing, not native document identity or geometry completeness. A recognized signature with an invalid directory is rejected.

Empty input, truncated recognizable input, and structural errors report `invalid` container coverage. Unknown formats report `unsupported`. Resource limits report `not_requested` before a directory is committed and `partial` if earlier directories were retained. A rejected inspection can retain valid outer evidence after a nested failure; extraction refuses rejected inspections.

The JSON schema version is `2`. Diagnostics use explicit wire names such as `input.truncated_header`, `format.unsupported`, and `limits.input_bytes`. A file byte offset and a logical-stream offset are distinct location variants. Arrays and struct field order are deterministic; reports contain no timestamps, source paths, or localized OS error messages. I/O diagnostics retain the error kind. Schema changes are listed in the [changelog](../CHANGELOG.md).

## Binary input

The loader reads no more than the input limit plus one byte. The extra byte detects an over-limit stream, and the full input length then remains unknown. Memory reservation is incremental and fallible. Slice inputs enforce the same input cap.

`ByteReader` uses checked arithmetic for ranges and source positions. Failed reads and seeks preserve the cursor. Child views inherit the read cap and retain their file base position. Primitive reads explicitly select endianness. Floating-point reads preserve raw IEEE values; geometry validation decides whether a value is acceptable for a geometric field.

Per-read limits apply to borrowed binary reads, including child views, rather than the fixed-size I/O buffer used to load the file. Additional limits bound directory bytes, admitted streams, extent rows, individual and summed logical bytes, nesting depth, scan work, and name bytes. Summed lengths include aliases and nested streams. Metadata, geometry and topology have separate byte, record, and work limits. See `ParseLimits` for defaults. Serialized limit configurations may omit newly added fields and receive their defaults.

Descriptor IDs preserve duplicate names and are deterministic for the same input. Unknown fields are raw hex or source ranges; the original source must be retained to recover opaque bytes. Names outside the admitted ASCII subset of UTF-16LE remain unknown. See [container format notes](format-notes/container.md) for admission rules and unsupported layouts.

## Fixtures

Ordinary workspace tests use static fixtures and need no native corpus or reference tool. None of the fixtures is a CATIA-authored document.

| Directory | Content | Generator |
| --- | --- | --- |
| `tests/fixtures/synthetic/` | Container framing: duplicate names, reversed extents, a nested directory, inline names, a zero-padded directory | `tools/corpus/create_synthetic.py` |
| `tests/fixtures/metadata/` | Typed property sets, storage declarations, the link table, JPEG previews, conflicts and unsupported variants | `tools/corpus/create_metadata_fixtures.py` (needs Pillow) |
| `tests/fixtures/geometry/` | Standard geometry populations: dimensions, placement, reversed extents, compact tables, unresolved relations, ambiguity | `tools/corpus/create_geometry_fixtures.py` |
| `tests/fixtures/topology/` | Box, cylinder/seam, through-hole, open shell, compressed tables, wide arities, polar cases, native UV records, bindings, parameter maps | `tools/corpus/create_topology_fixtures.py` and the other `create_*_fixtures.py` generators |

Each directory has a README describing its cases. Tests also mutate typed lengths, tags, values and budgets, and check that reports are deterministic under single-byte corruption.

## Corpus checks

The public NIST MBE PMI models are the native regression corpus. `tools/corpus/corpus.py fetch` downloads the hash-pinned archives into the ignored `data/` directory, `verify` rechecks every extracted file, and `tests/corpus-manifest.toml` records provenance, splits and expectations. The frozen byte inventory used by the checks is `tests/corpus/inventory.json`. See the [corpus tooling notes](../tools/corpus/README.md).

```sh
python3 tools/corpus/corpus.py fetch
python3 tools/corpus/corpus.py verify
cargo build --workspace --bins --examples --locked
python3 tools/corpus/check_m2.py
python3 tools/corpus/check_m3.py
python3 tools/corpus/check_document_properties.py
```

`check_m2.py` compares descriptor positions, names, lengths, extents and reconstructed stream hashes with the frozen inventory. `check_m3.py` compares saved-version values and positions, verifies extracted preview hashes and dimensions with Pillow as an independent JPEG decoder, and runs controlled byte mutations for absence, conflict and unsupported-owner cases. `check_document_properties.py` asserts the document kind, storage declarations, typed property sets and the link table for every manifest CATPart; `--extra` summarizes unpinned local files without asserting them. Reports are written to an ignored local directory by default; pass `--output` to choose a path. CI writes them to the runner's temporary directory and uploads them as an artifact.

Geometry and topology comparisons against external references, and the research tooling behind the topology work, are maintained outside the public tree. The published scope of those features is stated in [compatibility](compatibility.md) and the [format notes](format-notes/topology.md).

## Geometry and topology

`decode_geometry*` performs a separate geometry request after bounded inspection. It returns carrier definitions and serialized uses with partial geometry coverage; topology remains `not_requested`. Invalid native parameters become unknown definitions with raw bytes and diagnostics. Resource failures reject the request while retaining previously committed populations.

`decode_brep*` adds an explicit topology request to the geometry route. It uses unique indexed endpoint bindings, complete trim paths, and native incidence to form derived shell groups. Work/record limits include handles, triangles, UV intersection checks, ownership, and public-model validation. Each population commits its geometry refinements and topology together. Unsupported populations remain explicit and cannot be exported. The `validate_brep_json` example revalidates CLI JSON from stdin with the public validators.

## Fuzzing

The [fuzz harness](../fuzz/README.md) exercises directory inspection, stream reconstruction, metadata, JPEG framing, geometry and topology with bounded resources under AddressSanitizer. It is a separate Cargo workspace with its own lock file.

## License and public-tree checks

```sh
python3 tools/license/check_license.py
python3 tools/license/check_license.py --write
python3 tools/check_public_tree.py
```

The license check verifies the notice set (`LICENSE`, `COMMERCIAL-LICENSE.md`, `THIRD_PARTY_NOTICES.md`, `docs/license.md`), the pinned canonical license texts under `LICENSES/`, the `license` fields of every manifest, and that `LICENSES/rust-dependencies.txt` covers exactly the packages in `Cargo.lock`. `--write` regenerates that ledger from the locked registry sources; run it after changing dependencies and review any license expression the script does not already know.

The public-tree check lists the files Git would publish, rejects paths outside the allowed roots and known private names, and verifies that public documentation links resolve inside that set and contain no references to ignored directories or internal material.
