# Inspection compatibility

catiakit currently provides the v0.1 inspection surface in an unpublished alpha
workspace. It reads supported CFV2 structures without CATIA, a commercial SDK,
or a geometry kernel.

| Source family | Container | Document kind and properties | Preview | Links |
| --- | --- | --- | --- | --- |
| NIST STC, V5-6R2023 files; stored release 33 | Outer and owned nested directories | `part`; typed summary sets, containers, empty saved name | First framed JPEG; opaque trailing data reported | Own locator, feature catalogs, related documents |
| NIST FTC/CTC, V5-6R2019 files; stored release 29 | Same | Same | Framed JPEG | Same |
| NIST FTC files in the V5-6R2021 subfolder; stored release 31 | Same | Same | Framed JPEG | Same |
| NIST HTC, V5-6R2023 file; stored release 33 | Same | Same | Framed JPEG | Same |
| Locally examined CATPart, CATProduct and CATDrawing samples, V5R12 to V5R21 (not redistributed) | Same; V5R12 directories are zero-padded after the sentinel | `part`, `product`, `drawing`; `N` envelopes and saved file names in V5R14 to V5R19; one unsupported container variant | Framed JPEG | Same; component paths for assemblies |
| Other producers, versions, and layouts | Inspectable when the admitted CFV2 grammar matches; support unconfirmed | Unconfirmed | Unconfirmed | Unconfirmed |

These rows describe the examined files, not every file saved by those releases.
Stored version fields are reported as recorded; filename years do not override
them. Unknown directory bytes and metadata fields keep coverage partial. The
locally examined samples are third-party sample models that are not
redistributed; their hashes are not pinned and they are summarized, not
asserted, by the corpus checks.

Saved-by observations come from `LastSaveVersion` in a `CATSummaryInformation`
set of an outer `CATPropertySets` stream. Equal tuples share a governing value;
conflicting, invalid, or unsupported summary candidates have no governing value.
V5R12 tuples have no `HotFix` field. Every property set is decoded with its
typed values (`u16`, `i32`, lists, bytes, printable ASCII text); non-ASCII text
is retained as bytes. Nested lists and other type codes are unsupported.

The document kind comes from the kind-2 declaration in an outer
`CATStorageProperty` stream (`CATPart`, `CATProduct`, `CATDrawing`, `CATShape`
map to a kind; other type strings are retained as `declared_type`). The kind-1
declaration holds the document name recorded at save time: the file name in
V5R14 to V5R19 samples and an empty string in V5R21 and later files. Kind-3
declarations list the application containers. A V5R14 kind-0 variant remains an
unsupported scope. Document links come from the `CATDocumentProperty` table:
the document's own save path, feature catalogs, and related documents such as
assembly components. Locator paths remain stored strings, including Windows
paths; no path resolution, filesystem access, or network access follows them.
Part number, nomenclature and other product properties are not assigned; see
the [format notes](format-notes/metadata.md).

Previews come from outer `CATPreview` streams. Supported images use 8-bit
baseline or progressive Huffman JPEG marker framing. The parser checks lengths,
frame/scan structure, dimensions, byte stuffing, and the EOI boundary. It does
not decode entropy-coded coefficients or establish intended image appearance.
Unknown bytes after a framed JPEG remain visible as source ranges and a warning.
Several preview streams require selection by ID.

`not_found` applies to admitted scopes. `invalid`, `unsupported`, `conflicting`,
and `incomplete` distinguish malformed values, unsupported framing, contradictory
versions, and work stopped by a resource limit. Inspect `scopes` and diagnostics
before treating any empty result as a statement about an entire native document.

CATProduct and CATDrawing documents are inspected for container, metadata and
links only; assembly structure, placements, drawing content, complete native
geometry inventory, manufacturing readiness, design history, native application
loading, and native writing are outside current support. Usage examples are in
the [README](../README.md).

## Analytic geometry subset

`decode` requests the analytic subset separately from inspection. Admission requires
one selected nested stream pair, a unique largest FBB face population, a complete
counted edge/vertex walk, a complete trim-packet chain, and a unique matching
surface/support roster. A signature or save-version label alone selects no route.
The standard full width-2 and compact single-table widths 1–3 are supported.
Other layouts and unmatched populations remain unresolved with recoverable bytes.

| Definition | Current admission |
| --- | --- |
| Points | Finite coordinates in the counted vertex table; allocation IDs require a complete matching trailing roster |
| Plane | Complete trim frame plus a geometrically admitted incident circle supplying the location |
| Cylinder, cone, sphere, torus | Finite, valid inline binary32 carrier parameters in an admitted surface roster |
| Circle | Inline center/radius and one consistent plane normal from admitted adjacent carrier relations |
| Line | Intersection of two resolved, nonparallel incident planes |
| Freeform or unresolved carrier | Identity, source, and unresolved reason retained |

Plane bounds and visualization data are retained as opaque data. Canonical
carrier charts and natural periodic domains are derived explicitly; native UV
charts, trimmed edge intervals, endpoint assignment, loops, and body membership
are not assigned. Face orientation signs and serialized incidence slots remain
separate from carrier definitions. Duplicate native tags retain all candidates.

Known world coordinates and binary32 precision are preserved. Millimeters and
radians are the selected standard grammar's convention; this is recorded as
convention evidence with no claimed native unit declaration. Other placements
and assembly transforms are unconfirmed.

Development geometry is qualified on CTC03 and synthetic dimension/placement
fixtures. The remaining development samples have a population census; the
reserved evaluation group of the corpus has not been decoded during
development. Carrier comparisons do not establish complete geometry, valid
trimming, a closed body, or STEP shape equivalence.

## Indexed topology subset

`decode --topology` additionally admits plane/cylinder faces bounded by supported
lines/circles when a unique indexed table binds all endpoints to counted native
vertices exactly and complete trim boundaries match serialized edge incidence.
Recovered topology includes ordered outer/inner loops, opposite radial coedges,
seam branches, directed parameter intervals, and derived pcurves. Closed and open
shell groups are distinguished. A body's native identity, material nesting, and
native UV charts remain undeclared.

This subset is qualified on CTC01, CTC02, CTC03, CTC05, STC06, STC09, FTC09, two FTC06 variants, and two FTC11 variants. CTC02 requires explicit increases to the record and work limits.

Source-closed parameter-map contexts retain the two immediately following counted reference frames, including their allocation wire forms and opaque tails. Their numeric relations do not assign endpoint, support, face or edge identities.
Other development models retain indexed boundary incidence and individually
validated analytic occurrences while complete UV remains unavailable; release
year alone never selects topology support.
Synthetic box, cylinder, through-hole, and open-sheet cases exercise the same
grammar without establishing native application support. Analytic inverse charts
also cover cones, spheres, and ring tori. Explicit collapsed polar line uses on
cones/spheres are qualified by constructed fixtures; an explicit collapsed
physical edge has no native positive qualification. Native CTC01 qualifies
derived apex UV connectors between noncollapsed generators, preserving the
physical edge set. Cylinder-plane ellipses and qualified cylinder-cylinder
intersection branches are supported, including a primary-cylinder switch when one angular branch cannot span the native interval. A source-closed saved sphere meridian can resolve an antipodal circle plane, and a stereographic chart can avoid poles on spherical trims. Native clamped NURBS rectangles require unique corners, complete side coverage and all boundary samples within the existing tolerance. Fit-free rolling-ball jets and qualified saved surface-jet boundary curves are supported. Source-defined limiting curves and qualified rolling-ball/analytic intersections extend trimming. General freeform trimming, other intersection supports, multiple
polar runs, and horn/spindle torus charts remain unsupported. Coincident native
vertex identities are never merged.

`features.topology: complete` describes the admitted populations' graph and
sampled boundary consistency. Overall and geometry coverage remain `partial`.
`closed_shell` is not a validated solid; `solid_validated` remains false.
`export_eligible` marks a complete candidate for a later adapter. A supplied
closure expectation can withhold export of an otherwise valid open shell.
See [topology admission rules](format-notes/topology.md).

`topology.native_uv` additionally decodes supported consolidated class-20 UV
jets, selected binary64 analytic carriers, and standalone class-34 clamped
NURBS carriers, selected scaled-reference class-39 ruled jets, and supported compressed cylinder frames. Stored bytes and source scopes remain attached. These records
can belong to historical geometry. Unique rectangular boundary incidence can qualify a NURBS carrier. A triangular trim or supported ruled patch can use a unique carrier qualified by native vertices, every boundary point and interior witnesses, with bounded inversion in the source domain. Ruled jets convert exactly to a tensor NURBS surface without fitting. A monotone polynomial NURBS boundary can qualify an individual edge against its endpoints, all occurrence paths and at least one resolved incident carrier; other resolved incident carriers must also agree. This does not qualify the entire source rectangle. Other native vertices on the same ellipse can select a sparse arc interval. Constant-station rolling-ball sections can supply circular edges after full path and incident-carrier checks. Coincident contact limits at a source-boundary rolling-ball tip can qualify a derived UV connector without adding a physical edge. Native cone charts retain reference-radius offsets separately from angular scaling, including zero-reference-radius charts. Cylinder charts support source-closed full and zero-elided frames, with explicit shifts or full-period closure. General persistent support and jet-to-coedge relations remain unresolved. Partial UV is retained when every edge curve is known but a face carrier remains unknown.

Native parameter relations additionally admit closed class-24/25 scalar maps and long class-25 sampled maps. Source-local allocation encodings, signed accuracy, fit words and exact sampled pairs remain distinct. Supported class-18 parameter lines are retained only in a closed NURBS-adjacent frame sequence. Adjacency does not assign a persistent support identity or a current coedge. Sampled relations imply no interpolation or extrapolation, and stored fit words do not widen the spatial gate.
