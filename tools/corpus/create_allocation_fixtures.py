#!/usr/bin/env python3
"""Construct typed allocation graphs; fixtures are not CATIA-authored."""
import json
from create_native_uv_fixtures import compact, doubles, frame
from create_nurbs_binding_fixtures import rectangle, carrier
from create_topology_fixtures import DEST, encode


def reference(owner, target):
    if target < owner:
        return compact(owner - target)
    distance = target - owner - 1
    assert 0 <= distance < 64
    return bytes([distance * 4 + 3])


def factory(m, bad_endpoint=False, wrong_surface=False, long_header=False, uv_outside=False,
            loop_flags=None, face_control=5, loop_control=0x41, reverse_edge=None, bad_loop=None):
    # Fixed IDs agree with the separate standard-population fixture grammar.
    vertices = [96, 97, 98, 99, 105]
    records = {1: carrier()[8:]}
    classes = {1: 52}
    lines = [b'\x16'+doubles(1), b'\x05'+doubles(1, 1),
             b'\x88'+doubles(-.5, .5, 1), b'\x11'+doubles(.5, 1),
             b'\x11'+doubles(0, .5)]
    if uv_outside:
        lines[0] = b'\x06'+doubles(2, 1)
    for i, line in enumerate(lines):
        tag = i + 2
        records[tag] = reference(tag, 1) + line
        classes[tag] = 24
    for i in range(5):
        lo, hi = [(0, 1), (0, 1), (-.5, .5), (.5, 1), (0, .5)][i]
        scale = 1/(hi-lo)
        records[10+i] = b'\x82'+reference(10+i, 2+(i+1)%5)+reference(10+i, 2+i)+b'\x0b\x87\x0d'+doubles(0, 1, 0, lo, hi, scale, -lo*scale)
        classes[10+i] = 36
        for j in range(2):
            tag = 20+2*i+j
            records[tag] = b'\x81'+compact(1)+b'\x85'
            classes[tag] = 6
    for tag in vertices:
        records[tag] = b'\x03\x00'
        classes[tag] = 93
    for i, edge in enumerate(m.edges):
        tag = 100+i
        ids = [m.points.index(edge['handles'][j]) for j in (0, -1)]
        endpoints = [vertices[j] for j in ids]
        if i == reverse_edge:
            endpoints.reverse()
        if i == 0 and bad_endpoint:
            endpoints[0] = vertices[-1]
        records[tag] = b''.join(reference(tag, target) for target in
                               [10+i, *endpoints, 20+2*i, 21+2*i])+b'\x21'
        classes[tag] = 94
    records[200] = b'\x82'+reference(200, 1)+reference(200, 201)+bytes([face_control])
    classes[200] = 95
    pairs = [value for i in range(5) for value in (2+i, 100+i)]
    records[201] = b'\x8b'+b''.join(reference(201, t) for t in [*pairs, 2 if wrong_surface else 1])+b'\x85\x41\x00'
    if loop_flags is not None:
        packed = sum(value << (3*i) for i,value in enumerate(loop_flags)).to_bytes(2,'little')
        if bad_loop == 'padding': packed = packed[:1]+bytes([packed[1] | 0x80])
        tail = bytes([0x84 if bad_loop == 'count' else 0x85, loop_control])+packed+b'\x00'
        if bad_loop == 'truncated': tail = tail[:3]
        records[201] = records[201][:-3]+tail
    classes[201] = 98
    if long_header:
        records[100000] = b'\x81'+reference(100000, 200)+b'\x03'
        classes[100000] = 91
    previous = 0
    result = b''
    for tag in sorted(records):
        delta = tag - previous
        family = 'A' if tag == 1 else 'B'
        result += frame(classes[tag], records[tag], family=family, width=len(compact(delta)), operand=delta)
        previous = tag
    return result, vertices


def main():
    model, grid = rectangle()
    # Keep existing strict mesh/spatial admission partial, independently of the
    # exact reference identity of the stored UV programs.
    h = grid[0][2]
    x, y, z = model.xyz[h]
    model.xyz[h] = (x, y, z + .02)
    rows = []
    for name, options in [('native-allocation-uv', {}),
                          ('wrong-allocation-endpoint', {'bad_endpoint': True}),
                          ('wrong-allocation-support', {'wrong_surface': True}),
                          ('native-allocation-uv-outside-domain', {'uv_outside': True}),
                          ('long-allocation-header', {'long_header': True})]:
        content, vertices = factory(model, **options)
        rows.append(encode(name+'.cfv2', model, extra_streams=[('Factory', content)], vertex_tags=vertices))
    content, vertices = factory(model)
    rows.append(encode('ambiguous-allocation-factories.cfv2', model,
                       extra_streams=[('FactoryA', content), ('FactoryB', content)], vertex_tags=vertices))
    rows.append(encode('allocation-opaque-decoy.cfv2', model,
                       extra_streams=[('Factory', frame(100, content)+frame(46,b'\x05',family='B'))], vertex_tags=vertices))
    (DEST/'allocations-expected.json').write_text(json.dumps({'provenance': 'constructed, not CATIA authored', 'fixtures': rows},indent=2)+'\n')


if __name__ == '__main__':
    main()
