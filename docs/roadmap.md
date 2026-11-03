# Roadmap

catiakit is developed as a read-only CATIA V5 reader that needs neither CATIA
nor a commercial SDK at runtime. Support is added only where native files and
independent checks qualify it; see [compatibility](compatibility.md) for the
current scope.

## Current state

- Inspection of CATPart, CATProduct and CATDrawing containers: directories,
  streams and extents, the declared document type, saved-by versions, typed
  property sets, application containers, the document link table and JPEG
  previews.
- Decoding of the standard CATPart geometry grammar for analytic carriers
  (points, lines, circles, planes, cylinders, cones, spheres, tori) and of
  qualified native NURBS, ruled and rolling-ball records.
- Indexed B-rep recovery with ordered loops, coedges, seams, polar connectors
  and pcurves. Of the sixteen development models in the public NIST corpus,
  eleven produce a complete shell; five freeform models retain their boundary
  evidence and partial UV without a body.
- Stable JSON reports with per-feature coverage, diagnostic codes and source
  positions, a fuzz harness, and corpus checks that run in CI, including a
  decoded-topology regression for the sixteen development models.
- An Open CASCADE adapter, separate from the parser, that rebuilds the
  complete development models, checks them with the kernel and compares them
  with the published STEP exports; nine of the eleven agree with a reference
  and the two others differ from every reference in the stored data.

## Next

- Close the remaining freeform trimming cases and define the tolerance model
  for shared edges, so that every development model produces a body or an
  explicit reason.
- Extend the external comparison to open shells and to the partial models
  once they produce bodies, and settle how kernel-strict face flags on inner
  loops are reported.
- Qualify the reserved evaluation models of the corpus after the development
  models are complete.
- Product properties such as part number and nomenclature, once the
  product-container record grammar is decoded rather than inferred.

## Later

- Multiple bodies, attributes (colors, layers, names) and further storage
  layouts.
- CATProduct structure and placements beyond the stored link paths, CATDrawing
  content, PMI and design history, each with its own corpus and completion
  criteria.
- Python bindings and package publication after the Rust API stabilizes.

CATIA V4, native writing and 3DEXPERIENCE formats are out of scope.
