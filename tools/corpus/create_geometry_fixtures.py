#!/usr/bin/env python3
"""Synthetic standard population fixtures; never described as CATIA-authored."""
import json
import math
from pathlib import Path
import struct
from create_synthetic import directory

DEST=Path(__file__).resolve().parents[2]/"tests/fixtures/geometry"
DELIMITER=bytes.fromhex("10 24 04 ff ff 00 00 00")


def transform(p, rotated=False, vector=False):
    if not rotated:return p
    # 90 degrees about X followed by 90 degrees about Z, then a translation.
    result=[p[2],p[0],p[1]]
    return result if vector else [v+t for v,t in zip(result,[7,-11,13])]


def main_stream(rotated=False, tie=False, width=2):
    data=bytearray(b"uninterpreted-prefix"+b"\x05\x08\x01"+struct.pack("<3f",1000,2000,3000))
    normals=[[0,0,1],[0,1,0]]+[None]*6
    for normal in normals:
        data.extend(bytes([1,0x49 if normal else 0x41,1,255])+struct.pack("<I",3))
        if normal:data.extend(struct.pack("<3f",*transform(normal,rotated,True)))
        for h in [1,2,3]:data.extend(h.to_bytes(width,"big"))
    fbb=len(data)
    for _ in normals:data.extend(bytes.fromhex("30 04 04 ff 01 02 03 04"))
    data.extend(bytes([1,1,3]))
    for a,b in [(1,2),(2,3),(3,1)]:
        data.extend(bytes([2,2]));data.extend(a.to_bytes(width,"big"));data.extend(b.to_bytes(width,"big"))
    data.extend(DELIMITER);data.extend(bytes([1,6,4]))
    for point in [[0,0,0],[10,0,0],[0,20,0],[0,0,30]]:
        data.extend(b"\x05\x08\x01"+struct.pack("<3f",*transform(point,rotated)))
    data.extend(DELIMITER)
    result=bytes(data)
    return (result+b"second-population"+result if tie else result),fbb


def surface_stream(rotated=False, duplicate=False, unresolved=False):
    data=bytearray(b"native-prefix")
    specs=[(0x32,[],[0,0,0]),(0x32,[],[0,0,0]),(0x33,[0,0,0,0,0,5],None),
           (0x33,[0,0,0,0,1,3],None),(0x34,[10,0,0,0,0,math.pi/6],None),
           (0x35,[-10,0,0,2],None),(0x38,[0,-10,0,0,0,4,1],None),(0,[],None)]
    for i,(kind,values,_) in enumerate(specs):
        tag=200+i if not duplicate or i!=1 else 200
        if kind:
            params=list(values)
            if kind!=0x32:
                params[:3]=transform(params[:3],rotated)
                if kind in [0x33,0x34,0x38]:
                    a=transform([values[3],values[4],math.sqrt(max(0,1-values[3]**2-values[4]**2))],rotated,True)
                    params[3:5]=a[:2];params[5]=abs(params[5])*(-1 if a[2]<0 else 1)
            data.extend(tag.to_bytes(3,"little")+struct.pack(">H",2+4*len(params))+b"\0\x33"+bytes([kind]))
            data.extend(struct.pack(">"+"f"*len(params),*params))
        else:data.extend(tag.to_bytes(3,"little")+bytes(3))
        # Deliberately remote bounds. They are not points or surface origins.
        data.extend(struct.pack("<10f",999,888,777,2,3,4,999,888,777,10));data.extend(b"\x01")
    for i,(center,radius,refs) in enumerate([([0,0,10],5,[0,2]),([0,20,0],3,[1,3]),(None,None,[0,1])]):
        data.extend(b"\x60"+(100+i).to_bytes(3,"little"))
        if center:
            data.extend(bytes.fromhex("00 12 00 33 37"));data.extend(struct.pack(">4f",*transform(center,rotated),radius))
            if unresolved:refs=[0,1]
        else:data.extend(bytes.fromhex("00 02 00 33 36"))
        data.extend(bytes(refs))
    data.extend(b"uninterpreted-tail")
    return bytes(data)


def encode(name,rotated=False,tie=False,duplicate=False,unresolved=False,width=2,ambiguous=False):
    main,fbb=main_stream(rotated,tie,width);surf=surface_stream(rotated,duplicate,unresolved)
    inner=bytearray(b"V5_CFV2\0"+bytes(8));rows=[]
    for key,payload in [("MainDataStream",main),("SurfacicReps",surf)]+([("MainDataStream",main)] if ambiguous else []):
        cut=len(payload)//2
        low=len(inner);inner.extend(payload[cut:]);inner.extend(bytes(7))
        high=len(inner);inner.extend(payload[:cut]);inner.extend(bytes(7))
        rows.append((key,[(high,cut),(low,len(payload)-cut)]))
    inner_offset=len(inner);encoded,records=directory(rows)
    struct.pack_into(">II",inner,8,inner_offset,len(encoded));inner.extend(encoded)
    outer=bytearray(b"V5_CFV2\0"+bytes(56));outer.extend(inner)
    outer_offset=len(outer);encoded,_=directory([("Body",[(64,len(inner))])]);struct.pack_into(">II",outer,8,outer_offset,len(encoded));outer.extend(encoded)
    (DEST/name).write_bytes(outer)
    return {"path":name,"main_bytes":len(main),"surface_bytes":len(surf),"fbb_logical_offset":fbb,"faces":8,"edges":3,"points":4,"rotation":rotated,"width":width,"streams":records}


if __name__=="__main__":
    DEST.mkdir(parents=True,exist_ok=True)
    records=[encode("analytic.cfv2"),encode("placed.cfv2",rotated=True),encode("unresolved.cfv2",unresolved=True),
             encode("duplicate-tag.cfv2",duplicate=True),encode("ambiguous-stream.cfv2",ambiguous=True),
             encode("ambiguous-population.cfv2",tie=True),encode("compact-wide.cfv2",width=3)]
    (DEST/"expected.json").write_text(json.dumps({"fixtures":records,"translation":[7,-11,13],"rotation_columns":[[0,1,0],[0,0,1],[1,0,0]],"base_plane_origins":[[0,0,10],[0,20,0]],"base_line_origin":[0,20,10]},indent=2)+"\n")
