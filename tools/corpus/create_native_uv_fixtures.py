#!/usr/bin/env python3
"""Constructed native-frame grammar cases; not native positive evidence."""
import hashlib
import json
import math
import struct
from create_topology_fixtures import DEST, cube, encode


def compact(n):
    if n<64:return bytes([4*n+1])
    w=max(1,(n.bit_length()+7)//8);return bytes([4*w])+n.to_bytes(w,'little')


def doubles(*values):return struct.pack('<'+'d'*len(values),*values)


def frame(cls,payload,width=1,flag=3,operand=1,family='A'):
    token=compact(operand);assert len(token)==width
    return bytes([(164 if family=='A' else 177)+width,flag,cls])+ (struct.pack('<I',len(payload)) if family=='A' else bytes([len(payload)]))+token+payload


def jet(support):
    knots=[-1.,0.5,2.];n=len(knots)
    lanes=[knots,[t**5+2*t for t in knots],[3*t**4-t+1 for t in knots],[5*t**4+2 for t in knots],[12*t**3-1 for t in knots],[20*t**3 for t in knots],[36*t*t for t in knots]]
    return compact(support)+compact(5)+compact(n)+b'\x0c'+doubles(*knots)+compact(n)+b'\x01'+b''.join(doubles(*a) for a in lanes[1:5])+b'\x05'+b''.join(doubles(*a) for a in lanes[5:])+doubles(-1,2)+b'\x07'


def nurbs(row_program=False,domain=(0,1)):
    degree=3 if row_program else 1;size=degree+1
    axis=compact(degree)+compact(2)+b'\x0c'+doubles(*domain)
    points=[(i,j,i*j/4) for i in range(size) for j in range(size)]
    grid=b''.join(doubles(*p) for p in points)
    weights=(b'\x01\x03\x00'+doubles(1,2)+b'\x02'*3) if row_program else b'\x00'+doubles(1,2,1,2)
    tail=b'\x05\x05\x05\x05'+doubles(*domain,*domain,1,0,1,0)+bytes([1,1,1])+doubles(*([0]*8))+bytes([1,0,1,0,7,7])
    return axis+axis+b'\x05'+grid+weights+tail


def stream():
    packet=b'\x82'+compact(2)+compact(1)+b'\x0a\x85\x0d'+doubles(-1,2,1e-6,-1,2,1,-1,2,1e-6)
    result=frame(32,jet(42),flag=19)+frame(32,jet(257),width=2,operand=70)+frame(35,packet,family='B')+frame(32,jet(13),width=2,operand=70,family='B')
    sphere=doubles(0,0,0,2,0,0,0,2,0,0,0,2,2,0,2*math.pi,-math.pi/2,math.pi/2,2,0)
    cylinder=doubles(0,0,0)+bytes([29])+doubles(1,1,3,0,6*math.pi,-5,5)+b'\x07'
    cone=doubles(0,0,0,1,0,0,0,1,0,0,0,1,math.pi/4,7,0,2*math.pi,0,5,1,1,0,0,2*math.pi)
    torus=doubles(0,0,0,1,0,0,0,1,0,0,0,1,4,1,0,2*math.pi,0,2*math.pi,0,2*math.pi,0,2*math.pi,4,1,0)
    for cls,payload in [(42,sphere),(40,cylinder),(41,cone),(43,torus)]:result+=frame(cls,payload,family='B')
    result+=frame(52,nurbs())+frame(52,nurbs(True),width=2,operand=70)
    return result


if __name__=='__main__':
    payload=stream();records=[]
    for name,content in [('native-uv.cfv2',payload),('native-uv-decoy.cfv2',frame(100,payload)),('native-uv-nonzero-domain.cfv2',frame(52,nurbs(domain=(-4.922583739043711,4.922588557932056)))+frame(52,nurbs(True,domain=(-4.922583739043711,4.922588557932056))))]:
        records.append(encode(name,cube(),extra_streams=[('NativeFrames',content)]))
    (DEST/'native-uv-expected.json').write_text(json.dumps(dict(provenance='constructed, not CATIA authored',native_stream_sha256=hashlib.sha256(payload).hexdigest(),fixtures=records),indent=2)+'\n')
