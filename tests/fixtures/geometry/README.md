# Synthetic standard geometry populations

These CFV2 files are locally encoded framing examples, not CATIA-authored or
native-application validated documents. Every inner logical stream is fragmented
and stored in reverse physical order.

- `analytic.cfv2`: two planes, two cylinders, one cone, sphere, torus and opaque
  freeform carrier; two circles, one plane-intersection line and four points.
- `placed.cfv2`: the same definitions after a cyclic coordinate rotation and
  translation `(7, -11, 13)`. Canonical chart directions are derived explicitly.
- `unresolved.cfv2`: circle normals and plane locations lack independent evidence.
- `duplicate-tag.cfv2`: one native tag has two retained carrier candidates.
- `ambiguous-stream.cfv2`: equal largest MainDataStream descriptors.
- `ambiguous-population.cfv2`: equal largest FBB populations.
- `compact-wide.cfv2`: width-3 compact edge/trim grammar.
- `expected.json`: dimensions, placement, counts, and source descriptor facts.

Bounds are deliberately remote from the actual planes, and an uncounted
vertex-like signature precedes the real population. Tests verify that neither
becomes geometry. The generator is `tools/corpus/create_geometry_fixtures.py`.
Field-layout attribution and admission rules are in `docs/format-notes/geometry.md`.
