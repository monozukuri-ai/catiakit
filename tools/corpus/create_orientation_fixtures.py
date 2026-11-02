#!/usr/bin/env python3
"""Construct source-sense cases without changing earlier allocation fixtures."""
import json
from create_allocation_fixtures import factory
from create_nurbs_binding_fixtures import rectangle
from create_topology_fixtures import DEST, encode


def main():
    model, grid = rectangle()
    h = grid[0][2]
    x,y,z = model.xyz[h]
    model.xyz[h] = (x,y,z+.02)
    cases = [
        ('native-loop-sense', {}),
        ('native-loop-face-sense', dict(face_control=3,loop_flags=[6,6,6,2,2])),
        ('native-loop-compact-sense', dict(loop_control=0x50)),
        ('native-loop-reversed-edge', dict(reverse_edge=2,loop_flags=[7,7,6,3,3])),
        ('native-loop-broken-cycle', dict(loop_flags=[6,7,7,3,3])),
        ('native-loop-bad-count', dict(bad_loop='count')),
        ('native-loop-bad-padding', dict(bad_loop='padding')),
        ('native-loop-truncated-flags', dict(bad_loop='truncated')),
    ]
    rows=[]
    for name,options in cases:
        content,vertices=factory(model,**dict(dict(loop_flags=[7,7,7,3,3]),**options))
        rows.append(encode(name+'.cfv2',model,extra_streams=[('Factory',content)],vertex_tags=vertices))
    (DEST/'orientation-expected.json').write_text(json.dumps(dict(provenance='constructed, not CATIA authored',fixtures=rows),indent=2)+'\n')


if __name__ == '__main__':
    main()
