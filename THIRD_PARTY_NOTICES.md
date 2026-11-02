# Third-party notices

catiakit project material is offered under PolyForm Noncommercial 1.0.0, with
separate commercial licensing available from UnRobotics Inc.
See `LICENSE` and `COMMERCIAL-LICENSE.md`. Third-party components are not
relicensed by that notice.

## Rust dependencies

The workspace lock file `Cargo.lock` pins every compiled dependency. The
runtime library depends on `serde`; the command-line tool adds `clap` and
`serde_json`; tests add `sha2`. Their license and copyright notices, including
transitive and build dependencies, are preserved in
`LICENSES/rust-dependencies.txt`, which `tools/license/check_license.py`
regenerates from the locked registry sources. MIT is selected where a crate
offers MIT as an alternative; the Unicode-3.0 data license of `unicode-ident`
is retained as a mandatory additional term. The license texts are in
`LICENSES/Apache-2.0.txt` and inside the dependency ledger.

The separate fuzzing harness under `fuzz/` has its own `fuzz/Cargo.lock`
(`libfuzzer-sys` and its dependencies, MIT OR Apache-2.0 and the LLVM
exception). It is a development tool and is not part of a distribution.

## Format references

- cadmpeg project, commit `6289a0d31901aae9445fb99c4bbf7c8effd1d3f1`:
  https://github.com/cadmpeg/cadmpeg/tree/6289a0d31901aae9445fb99c4bbf7c8effd1d3f1
  Its CATIA V5 format notes (CC BY 4.0) were used as a layout reference for the
  container directory, metadata atoms and standard geometry records. The
  notes are attributed in `docs/format-notes/`; the license text is preserved
  in `LICENSES/CC-BY-4.0.txt`. catiakit is an independent implementation and
  incorporates no cadmpeg source code. Its reference binaries were run only
  by development survey tooling outside the public tree and are not a
  dependency.
- ITU-T Recommendation T.81 (JPEG) defines the marker framing checked by the
  preview inspector. The specification text is not included.

## Test data and tooling

- NIST MBE PMI test models (STC, FTC/CTC, HTC) are public NIST test data.
  They are downloaded by `tools/corpus/corpus.py` into the ignored `data/`
  directory and are not redistributed. Their hashes and origin are recorded in
  `tests/corpus-manifest.toml`.
- Synthetic fixtures under `tests/fixtures/` are project-authored material
  under PolyForm Noncommercial 1.0.0. They are not CATIA-authored documents.
- The optional corpus checks under `tools/corpus/` use Pillow as an
  independent JPEG decoder. It is a development dependency installed
  separately; nothing from it is bundled.
- Commercial CAD SDKs and their sample models that were used locally during
  research stay outside the repository under their own license terms. No SDK
  material, license key or SDK output is part of this repository or of any
  distribution.
