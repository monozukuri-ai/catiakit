# Synthetic metadata fixtures

These files are parser framing examples, not CATIA-authored or native-application
validated models. Every payload is split across reversed physical extents.

- `metadata.cfv2`: typed property sets (summary, a name-only `CATPreview` set,
  a list of texts, a list of bytes, non-ASCII text), two equal saved-version
  observations, an older FirstStreamed value, a fake labelled value inside
  another string, a saved-name declaration, a `CATPart` document declaration,
  containers with `M` and `N` envelopes and the length-free `20` string atom,
  an unsupported kind-0 storage variant, a link table with `GenericLocate`,
  `CATIA`, `CATWin`, non-UTF-8 and undecodable payloads, and a JPEG with
  opaque trailing data. Similar bytes in `Data` are untyped.
- `absent.cfv2`: FirstStreamed only, an empty saved name, no document
  declaration, no preview.
- `conflict.cfv2`: two distinct saved-version tuples.
- `storage-conflict.cfv2`: disagreeing document and saved-name declarations.
- `other-kind.cfv2`: a `CATProcess` declaration, a V5R12-style version tuple
  without `HotFix`, and a link table with the `22`-tagged index words.
- `ambiguous.cfv2`: two separately owned previews requiring explicit selection.
- `tiny.jpg` and `progressive.jpg`: locally generated 2 × 3 RGB JPEGs.
- `expected.json`: source descriptor positions and independent fixture facts.

The regeneration helper is `tools/corpus/create_metadata_fixtures.py`. It uses
Pillow; ordinary Rust tests use the checked-in bytes and do not require Pillow.
Tests also mutate typed lengths, tags, version values, storage atoms, link
trailers, image markers, and budgets.
The field-layout attribution is in `docs/format-notes/metadata.md`.
