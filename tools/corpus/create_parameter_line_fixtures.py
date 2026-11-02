#!/usr/bin/env python3
"""Construct explicit-range line programs, separate from native evidence."""
import json
from create_native_uv_fixtures import compact, doubles, frame, nurbs
from create_topology_fixtures import DEST, cube, encode


def context(programs):
    return frame(52, nurbs()) + b''.join(
        frame(24, compact(2 + 2 * i) + bytes([control]) + doubles(*values), family='B')
        for i, (control, values) in enumerate(programs))


def main():
    good = context([
        (0, [-2, 1, 4, 5, .6, -.8]),
        (0x40, [-2, 1, 4, 5]),
        (0x80, [2, 3, 4, 5]),
        (8, [-1, 4, 5, 1, 0]),
    ])
    bad = context([
        (0, [1, 1, 0, 0, 1, 0]),
        (0, [-2, 1, 0, 0, 1, 1]),
        (0x40, [-2, 1, 0, 0, 9]),
        (0x80, [-1e308, 1e308, 0, 0]),
    ]) + context([
        (0, [0, 1e308, 1e308, 0, 1, 0]),
        (0x40, [0, 1e308, 0, 1e308]),
        (0x80, [-2, 1, float('nan'), 0]),
        (0x48, [1, 0, 0]),
    ])
    closing = frame(46, b'\x05', family='B')
    records = [
        encode('native-asymmetric-parameter-lines.cfv2', cube(), extra_streams=[('NativeFrames', good + closing)]),
        encode('invalid-asymmetric-parameter-lines.cfv2', cube(), extra_streams=[('NativeFrames', bad + closing)]),
        encode('asymmetric-parameter-lines-decoy.cfv2', cube(), extra_streams=[('NativeFrames', frame(100, good) + closing)]),
    ]
    (DEST / 'parameter-lines-expected.json').write_text(json.dumps(dict(
        provenance='constructed grammar, not CATIA authored', fixtures=records), indent=2) + '\n')


if __name__ == '__main__':
    main()
