# Fuzzing

This separate Cargo workspace uses cargo-fuzz/libFuzzer with AddressSanitizer.
Its targets exercise inspection and reconstruction, including outer evidence
retained after a nested failure, metadata and JPEG framing, geometry admission
and topology recovery. Resource caps keep malformed declarations bounded.
`fuzz/Cargo.lock` pins this harness separately from the application workspace.
The corpus, generated artifacts, coverage, and build output are ignored.

Install cargo-fuzz and a compatible nightly toolchain, then run from the
repository root. A short smoke run is evidence for its sampled inputs, not an
exhaustive proof; successful topology results must pass the public validator
and never carry a validated-solid claim.

## Container target

```sh
mkdir -p fuzz/corpus/container
cp tests/fixtures/synthetic/*.cfv2 fuzz/corpus/container/
cargo +nightly-2026-02-02 fuzz run container -- -max_total_time=30 -max_len=600000 -timeout=3 -rss_limit_mb=512
```

Optionally add locally acquired, hash-verified native files as seeds.

## Metadata and JPEG targets

`metadata` keeps a one-stream CFV2 envelope intact while mutating its payload.
Its first input byte selects CATPropertySets (0), CATStorageProperty (1),
CATDocumentProperty (2), or CATPreview (3). `jpeg` calls the marker walker
directly and checks that an admitted prefix also passes exact-boundary
inspection.

Prepare seeds from the static fixtures after building the CLI:

```sh
cargo build --locked -p catiakit-cli
python3 - <<'PY'
import json, subprocess
from pathlib import Path
base = Path('fuzz/corpus')
for name in ('metadata', 'jpeg'):
    (base / name).mkdir(parents=True, exist_ok=True)
data = Path('tests/fixtures/metadata/metadata.cfv2').read_bytes()
container = json.loads(subprocess.check_output(
    ['target/debug/catiakit', 'inspect', '-', '--json'], input=data))['value']['container']
selectors = {'CATPropertySets': 0, 'CATStorageProperty': 1, 'CATDocumentProperty': 2, 'CATPreview': 3}
for stream in container['streams']:
    selector = selectors.get(stream['name'])
    if selector is None:
        continue
    payload = b''.join(data[e['file_range']['offset']:e['file_range']['offset']+e['file_range']['length']]
                       for e in stream['extents'])
    (base / 'metadata' / str(stream['id'])).write_bytes(bytes([selector])+payload)
for source in Path('tests/fixtures/metadata').glob('*.jpg'):
    (base / 'jpeg' / source.name).write_bytes(source.read_bytes())
PY
cargo +nightly-2026-02-02 fuzz run metadata -- -max_total_time=30 -max_len=131072 -timeout=3 -rss_limit_mb=512
cargo +nightly-2026-02-02 fuzz run jpeg -- -max_total_time=30 -max_len=131072 -timeout=3 -rss_limit_mb=512
```

## Geometry target

`geometry` uses selector 0 for a complete CFV2 input and selectors 1/2 for
mutations of the logical MainDataStream/SurfacicReps payload in a fixed
synthetic envelope. It exercises population admission and evaluates every
resolved carrier at a finite parameter. Work, byte and record limits are
tightened for fuzzing.

```sh
python3 - <<'PY'
from pathlib import Path
dest = Path('fuzz/corpus/geometry')
dest.mkdir(parents=True, exist_ok=True)
for source in Path('tests/fixtures/geometry').glob('*.cfv2'):
    (dest / source.name).write_bytes(bytes([0])+source.read_bytes())
PY
cargo +nightly-2026-02-02 fuzz run geometry -- -max_total_time=30 -max_len=65536 -timeout=3 -rss_limit_mb=512
```

## Topology target

`topology` uses selectors divisible by 4 for a complete CFV2 input. Other
selectors choose a fixed box, cylinder, through-hole, cone-pole, or sphere-pole
envelope and apply three-byte `(u16 LE offset, xor byte)` patches throughout
it. Localized changes retain enough framing to exercise indexed bindings, mesh
boundaries, UV paths, native UV records, bindings and validation. The harness
permits more topology work steps than the parser default while retaining its
byte and record caps; every retained partial and completed body is revalidated.

```sh
python3 - <<'PY'
from pathlib import Path
dest = Path('fuzz/corpus/topology')
dest.mkdir(parents=True, exist_ok=True)
for source in Path('tests/fixtures/topology').glob('*.cfv2'):
    (dest / source.name).write_bytes(bytes([0])+source.read_bytes())
for selector in (1, 2, 3, 5, 9):
    (dest / f'structured-{selector}').write_bytes(bytes([selector]))
PY
cargo +nightly-2026-02-02 fuzz run topology -- -max_total_time=30 -max_len=65536 -timeout=3 -rss_limit_mb=512
```

Sanitizer allocation settings such as
`ASAN_OPTIONS=quarantine_size_mb=16:thread_local_quarantine_size_kb=64:malloc_context_size=10`
with a 512 MiB RSS ceiling keep long topology campaigns within memory; they do
not change the parser's byte, record and work caps.
