#!/usr/bin/env python3
"""Construct map/reference contexts; these fixtures do not prove native roles."""
import json
from create_native_uv_fixtures import compact, frame
from create_parameter_map_fixtures import payload
from create_topology_fixtures import DEST, cube, encode


def vector(refs, tail, **kwargs):
    return frame(6, bytes([128 + len(refs)]) + b''.join(refs) + tail, family='B', **kwargs)


def stream():
    m = frame(0x25, payload(refs=b'\x06\x47\x0a\xb1\x02'), family='B')
    a = vector([compact(1), b'\x06\x48', b'\x0a\xb2\x02'], b'\x8a')
    z = vector([compact(2), b'\x06\x49', b'\x0a\xb3\x02'], b'\x85')
    mixed = vector([b'\x01', b'\x03', b'\x02', b'\x06\x00', b'\x0a\x00\x00', b'\x0c\x70\x11\x01'], b'\x85\x01\xff', width=3, operand=300)
    wide = vector([compact(2), b'\x10\xff\xff\xff\xff'], b'\x80\x00', width=2, operand=70)
    long = frame(0x25, payload(0x1587, source=(2, 9), sampled=([10 + .5*i for i in range(21)], [9 - .35*i for i in range(21)])), width=3, operand=300)
    return m, a, z, m + a + z + long + mixed + wide


def main():
    m, a, z, good = stream()
    end = frame(46, b'\x05', family='B')
    invalid = b''.join(m + frame(6, bad, family='B') + z for bad in [b'\x80\x85', b'\x84\x05\x09\x0d\x85', b'\x81\x0a\x01'])
    invalid += m + a + end + z
    rows = [
        encode('native-parameter-reference-contexts.cfv2', cube(), extra_streams=[('NativeFrames', good + end)]),
        encode('invalid-parameter-reference-contexts.cfv2', cube(), extra_streams=[('NativeFrames', invalid + end)]),
        encode('parameter-reference-contexts-decoy.cfv2', cube(), extra_streams=[('NativeFrames', frame(100, good) + end)]),
        encode('separate-parameter-reference-contexts.cfv2', cube(), extra_streams=[('MapFrames', m + end), ('VectorFrames', a + z + end)]),
    ]
    (DEST / 'reference-contexts-expected.json').write_text(json.dumps(dict(provenance='constructed, not CATIA authored', fixtures=rows), indent=2) + '\n')


if __name__ == '__main__':
    main()
