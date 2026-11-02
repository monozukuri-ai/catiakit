# Synthetic CFV2 framing

These small files encode directory and extent examples for parser tests. They
were not saved or verified by CATIA and establish no native geometry support.

- `outer.cfv2`: two streams with the same name; one uses physical extents in
  reverse file order. Their reconstructed bytes are `ghi01` and `4567`.
- `nested.cfv2`: adds an owned inner container and an unverified zero-length
  named declaration. The inner `MainDataStream` reconstructs to `DEABC`.
- `inline.cfv2`: the outer example using inline legacy name framing.
- `padded.cfv2`: the outer example whose declared directory length continues
  with 24 zero bytes after the `CB__END` sentinel, as observed in V5R12 files.
- `expected.json`: source positions, extent declarations, and expected bytes.

The encoder is `tools/corpus/create_synthetic.py`. Field-layout attribution is
recorded in `docs/format-notes/container.md`. Mutations of these examples test
invalid lengths, offsets, sentinels, aliasing, and resource limits.
