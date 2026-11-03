# Changelog

All notable changes to catiakit are recorded here. The project is an
unpublished alpha; the JSON report schema version changes whenever the report
shape changes.

## Unreleased

### Added

- License notice set: PolyForm Noncommercial 1.0.0 with separate commercial
  licensing by UnRobotics Inc. (`LICENSE`, `COMMERCIAL-LICENSE.md`,
  `THIRD_PARTY_NOTICES.md`, `LICENSES/`, `docs/license.md`) and
  `tools/license/check_license.py`, which CI runs and which regenerates the
  dependency ledger with `--write`.
- Document kind detection from the kind-2 root storage declaration
  (`CATPart`, `CATProduct`, `CATDrawing`, `CATShape`); other declared types stay
  `unknown` with `declared_type` retained.
- Document metadata: the saved document name (kind-1 declaration), the
  application container inventory (kind-3 declarations), fully typed property
  sets including name-only sets, and the `CATDocumentProperty` link table with
  `GenericLocate`, `CATIA` and `CATWin` locators.
- Container: zero padding after the directory sentinel is accepted and
  retained as an opaque range (`container.directory_padding`), which admits
  V5R12 documents.
- Older forms: storage envelopes starting with `N` (V5R14 to V5R19), the
  length-free `20` string atom, version tuples without `HotFix` and
  `22`-tagged link index words (V5R12).
- `tools/corpus/check_document_properties.py` and a CI `corpus` job that
  fetches the pinned NIST archives and runs the container, metadata and document checks.
- `sample_geometry` example: evaluates the edge curves, pcurves and
  rolling-ball surfaces of a decoded document for external adapters.
- `tools/occt/`: an Open CASCADE adapter that rebuilds decoded bodies,
  checks them with `BRepCheck` and compares them with the STEP references of
  the corpus (`compare_step.py`).
- `tools/corpus/check_topology.py` with `tests/corpus/topology-expectations.json`:
  decoded-topology regression for the sixteen development models under the
  default budgets, run by the CI `corpus` job.

### Changed

- Default topology budgets are 2,000,000 records and 2,000,000,000 work
  steps (previously 250,000 and 50,000,000), so every development model of
  the corpus decodes with the defaults; CTC02 needed explicit flags before.

- Repository layout for publication: milestone records, validation reports,
  reference-implementation and research tooling, and research examples are
  kept outside the public tree. The frozen corpus inventory moved to
  `tests/corpus/`, default report outputs go to an ignored local directory, and
  `tools/check_public_tree.py` guards the published set.

- JSON report schema version 2. `metadata.external_references` is replaced by
  `metadata.storage`, `metadata.property_sets` and `metadata.links`; the
  kind-1 storage slot is the saved document name, not an external reference.
  `saved_by.value.hot_fix` is optional. `DocumentInfo` gains `declared_type`.
- Diagnostics: `metadata.reference_invalid` is replaced by
  `metadata.storage_invalid`; `metadata.storage_conflict`,
  `metadata.link_invalid` and `container.directory_padding` are new.
