#!/usr/bin/env python3
"""Constructed parameter-representation programs; not CATIA native proof."""
import json,struct
from create_native_uv_fixtures import compact,doubles,frame,nurbs
from create_topology_fixtures import DEST,cube,encode


def payload(selector=0x0d84,cls=0x25,source=(2,7),scale=2,offset=6,accuracy=.002,fit=.003,lead=10,refs=None,sampled=None,sites=None):
    refs=refs or compact(5)+b'\x17'
    result=b'\x82'+refs+bytes([lead])+struct.pack('<H',selector)+doubles(10,20,accuracy)
    mode=selector>>8
    if selector in [1,3]:mode=5 if selector==1 else 13
    if mode in [9,13,21]:result+=doubles(*source)
    if mode==13:result+=doubles(scale,offset)
    if mode==21:
        target,src=sampled or ([11,14,19],[8,5,3]);result+=bytes([128+len(target)])+doubles(*target)+doubles(*src)
    count=2 if selector&255 in [0x84,1,3] else 1 if selector&255 in [0x85,0x86] else 0
    if count:result+=doubles(*(sites if sites is not None else [12,18] if count==2 else [19]))
    if lead==10:result+=doubles(fit)
    return result


def context(break_adjacency=False):
    result=frame(52,nurbs())
    if break_adjacency:result+=frame(46,b'\x05',family='B')
    for op,control,values in [(2,0x11,[0,1]),(4,6,[1,1]),(6,0x48,[-.5,1,.5]),(8,0x88,[-.5,.5,0])]:
        result+=frame(24,compact(op)+bytes([control])+doubles(*values),family='B')
    return result


def positive():
    specs=[
        (0x24,payload(0x0587,cls=0x24,accuracy=-.004,fit=.004,refs=b'\x06\x47\x0a\xb1\x02')),
        (0x24,payload(0x0984,cls=0x24,source=(10,20),accuracy=-0.,lead=11)),
        (0x24,payload(0x0d84,cls=0x24,source=(1,3),scale=-5,offset=25,accuracy=.004,fit=.004)),
        (0x24,payload(3,cls=0x24,source=(1,3),scale=-5,offset=25,accuracy=.004,fit=.004)),
        (0x25,payload()),
        (0x25,payload(0x1584,source=(2,9),sampled=([11,14,19],[8,5,3]))),
        (0x25,payload(0x0585,accuracy=-0.,lead=11,refs=b'\x16\x04\xc8')),
    ]
    result=b''.join(frame(cls,p,family='B',width=2,operand=70) for cls,p in specs)
    target=[10+i*.5 for i in range(21)];src=[9-i*.35 for i in range(21)]
    result+=frame(0x25,payload(0x1587,source=(2,9),sampled=(target,src)),width=3,operand=300)
    return result


if __name__=='__main__':
    closing=frame(46,b'\x05',family='B');good=positive();bad=[]
    bad += [payload(scale=0),payload(offset=7),payload(accuracy=.01,fit=.001)]
    bad += [payload(0x1584,source=(2,9),sampled=([11,14,19],[8,3,5])),payload(0x1584,source=(2,9),sampled=([11,11,19],[8,5,3])),payload(0x1584,source=(2,9),sampled=([11,14,19],[8,5,1]))]
    bad += [payload()+b'\0',payload(source=(1,2),scale=1e308,offset=-1e308),payload(0x1587,lead=11),payload(0x0d87,lead=11,accuracy=.1)]
    invalid=b''.join(frame(0x25,p,family='B') if len(p)<=255 else frame(0x25,p) for p in bad)
    invalid+=frame(0x24,payload(0x0587,cls=0x24,accuracy=.001,fit=.002),family='B')
    records=[encode('native-parameter-maps.cfv2',cube(),extra_streams=[('NativeFrames',good+context()+closing)]),encode('invalid-parameter-maps.cfv2',cube(),extra_streams=[('NativeFrames',invalid+context(True)+closing)]),encode('parameter-maps-decoy.cfv2',cube(),extra_streams=[('NativeFrames',frame(100,good+context())+closing)]),encode('native-parameter-maps-only.cfv2',cube(),extra_streams=[('NativeFrames',good+closing)])]
    (DEST/'parameter-maps-expected.json').write_text(json.dumps(dict(provenance='constructed grammar, not CATIA authored',fixtures=records),indent=2)+'\n')
