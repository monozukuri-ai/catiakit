# Constructed topology fixtures

These files encode the admitted standard framing locally. They are not CATIA
exports, native application tests, or evidence for a CATIA release family.
Regenerate with `python3 tools/corpus/create_topology_fixtures.py`.

| Case | Vertices / edges / faces / loops / coedges | Expected result |
| --- | --- | --- |
| box | 8 / 12 / 6 / 6 / 24 | Closed manifold shell, Euler 2 |
| box-compressed | Same | Compressed indexed coordinates, same topology |
| cylinder | 2 / 3 / 3 / 3 / 6 | Two closed circle edges and one shared seam, Euler 2 |
| wide-cylinder | Same | Wide counts and edge arity above 255 |
| through-hole | 10 / 15 / 7 / 9 / 30 | Two inner loops, reversed cylinder, Euler 0 |
| open-sheet | 4 / 4 / 1 / 1 / 4 | Valid open shell; caller expectation can reject closure |
| ambiguous-index | — | Two complete candidate tables; withhold body |
| coincident-identities | — | Multiple counted vertex identities at one position; withhold body |
| nonmanifold-mesh | — | Three triangle uses of an edge; withhold body |
| degenerate-no-uv | — | Zero-length line occurrence without native UV evidence; withhold body |
| cone-pole / sphere-pole | 3 / 4 / 1 / 1 / 4 | Point locus and nontrivial UV interval; open disk, Euler 1 |
| indexed-aliases | Same as cylinder | Distinct handles with exact indexed coordinate aliases |
| repeated-incidence | Same as box | Infer the second owner from complete boundary evidence, preserve raw slots |
| unsupported-curve | Boundary graph as box | Retain boundary incidence, withhold analytic UV and body export |
| cone-corner | 3 / 3 / 1 / 1 / 3 | Polar UV connector at a native-style vertex; no added physical edge |
| slanted-cylinder | Same as cylinder | Exact ellipse from cylinder-plane spline support |
| partial-closed | Boundary graph as cylinder | Preserve closed-circle use direction in partial analytic results |

Streams use reversed physical fragments, two full width-2 edge tables, explicit
plane normals, and deliberately remote bounds. The auxiliary indexed-table word
differs from the primary count. `expected.json` retains source hashes and logical
record boundaries for mutation tests. No fixture carries a validated-solid claim.

Polar cases preserve two different native handles at the same pole. They test
derived charts and do not establish a CATIA-authored degenerate production.

## Native UV record fixtures

`native-uv.cfv2`, `native-uv-decoy.cfv2`, and
`native-uv-nonzero-domain.cfv2` are constructed by
`tools/corpus/create_native_uv_fixtures.py`. They exercise stored quintic jets,
rational NURBS weight programs, fragmented source ranges, opaque nested markers,
and exact nonzero-domain endpoints. They establish no CATIA-authored positive
example, native support-to-face binding, or explicit native zero-length edge.
