#!/usr/bin/env python3
"""Construct exact procedural rolling-ball grammar fixtures, never native proof."""
import copy,math,json
from create_native_uv_fixtures import compact,doubles,frame
from create_topology_fixtures import DEST,Model,encode,quad


def rolling_payload(variable=False,alternative=False,explicit=False):
    knots=[0.,2.];rows=[];first=[]
    for t in knots:
        r=1.+(.2*t if variable else 0.);c=(t,r,r) if alternative else (t,0,0)
        rows.append([t,r,0,t,0,r,*c,math.pi/2]);first.append([1,.2 if variable else 0,0,1,0,.2 if variable else 0,1,0,0,0])
    tail=doubles(0,2,0,1,1)+b'\x03'+doubles(0,1)
    tail+=(b'\x01\x01'+doubles(1,1.4,.2,.2,0,0)+(1).to_bytes(4,'little')) if explicit else b'\x05'+bytes(5)
    tail+=doubles(0,2,1)
    return compact(2)+compact(5)+compact(2)+b'\x0c'+doubles(*knots)+doubles(*(x for row in rows for x in row))+doubles(*(x for row in first for x in row))+doubles(*([0]*20))+tail


def patch():
    m=Model();n=16;grid=[[m.handle((x,math.cos(i*math.pi/2/n),math.sin(i*math.pi/2/n))) for i in range(n+1)] for x in [0,1,2]]
    centers=[m.handle((x,0,0)) for x in [0,1,2]];m.points=[centers[0],centers[2]]+[grid[0][0],grid[0][-1],grid[2][0],grid[2][-1]]
    tris=[]
    for x in range(2):
        for i in range(n):tris+=quad([grid[x][i],grid[x+1][i],grid[x+1][i+1],grid[x][i+1]])
    m.face(tris);m.faces[0]['freeform']=True
    m.face([[centers[0],grid[0][i+1],grid[0][i]] for i in range(n)],normal=(-1,0,0))
    m.face([[centers[2],grid[2][i],grid[2][i+1]] for i in range(n)],normal=(1,0,0))
    m.face(sum((quad([centers[x],centers[x+1],grid[x+1][0],grid[x][0]]) for x in range(2)),[]),normal=(0,0,-1));m.face(sum((quad([centers[x],grid[x][-1],grid[x+1][-1],centers[x+1]]) for x in range(2)),[]),normal=(0,-1,0))
    m.edge(grid[0],[0,1],(0,0,0,1));m.edge(grid[2],[0,2],(2,0,0,1))
    for at,faces in [(0,[0,3]),(-1,[0,4])]:m.edge([row[at] for row in grid],faces);m.edges[-1]['spline']=True
    for x,center,cap in [(0,centers[0],1),(2,centers[2],2)]:
        m.edge([center,grid[x][0]],[cap,3]);m.edge([center,grid[x][-1]],[cap,4])
    m.edge(centers,[3,4]);return m


def tip_patch():
    m=Model();alpha=math.pi/8;n=16;knots=[0.,2.];tip=m.handle((0,1,0));grid=[]
    for u in [.25,.5,1.,1.5,2.]:grid.append([m.handle((u,math.cos(alpha*u*j/n),math.sin(alpha*u*j/n))) for j in range(n+1)])
    tris=[[tip,grid[0][j],grid[0][j+1]] for j in range(n)]
    for a,b in zip(grid,grid[1:]):
        for j in range(n):tris+=quad([a[j],b[j],b[j+1],a[j+1]])
    m.face(tris);m.faces[0]['freeform']=True;m.points=[tip,grid[-1][0],grid[-1][-1]]
    m.edge([tip]+[row[0] for row in grid],[0,0]);m.edges[-1]['spline']=True
    m.edge(grid[-1],[0,0],(2,0,0,1))
    m.edge([row[-1] for row in grid[::-1]]+[tip],[0,0]);m.edges[-1]['spline']=True
    values=[];first=[];second=[]
    for u in knots:
        c=math.cos(alpha*u);s=math.sin(alpha*u)
        values.append([u,1,0,u,c,s,u,0,0,alpha*u]);first.append([1,0,0,1,-alpha*s,alpha*c,1,0,0,alpha]);second.append([0,0,0,0,-alpha*alpha*c,-alpha*alpha*s,0,0,0,0])
    tail=doubles(0,2,0,1,1)+b'\x03'+doubles(0,1)+b'\x05'+bytes(5)+doubles(0,2,1)
    native=compact(2)+compact(5)+compact(2)+b'\x0c'+doubles(*knots)+b''.join(doubles(*(x for row in block for x in row)) for block in [values,first,second])+tail
    return m,frame(50,native)+frame(46,b'\x05',family='B')


if __name__=='__main__':
    m=patch();closing=frame(46,b'\x05',family='B');cylinders=b''
    for control in [0x24,0x04]:
        prefix=doubles(0,0,0)+bytes([control])+doubles(math.cos(.3),math.sin(.3),-math.sin(.3),math.cos(.3))+(doubles(-0.) if control==4 else b'')
        cylinders+=frame(40,prefix+doubles(1,0,2*math.pi,-1,1)+b'\x03'+doubles(0),family='B')
    payload=frame(50,rolling_payload())+cylinders+closing;records=[]
    records.append(encode('rolling-ball-patch.cfv2',m,extra_streams=[('NativeFrames',payload)]))
    sections=copy.deepcopy(m)
    for edge in sections.edges[:2]:edge['circle']=None;edge['spline']=True
    records.append(encode('rolling-ball-sections.cfv2',sections,extra_streams=[('NativeFrames',payload)]))
    bowed=copy.deepcopy(sections);h=bowed.edges[0]['handles'][8];x,y,z=bowed.xyz[h];bowed.xyz[h]=(x+.02,y,z)
    records.append(encode('rolling-ball-bowed-section.cfv2',bowed,extra_streams=[('NativeFrames',payload)]))
    records.append(encode('rolling-ball-interior-choice.cfv2',m,extra_streams=[('NativeFrames',frame(50,rolling_payload(alternative=True))+payload)]))
    records.append(encode('rolling-ball-variable.cfv2',m,extra_streams=[('NativeFrames',frame(50,rolling_payload(variable=True,explicit=True))+closing)]))
    bad=bytearray(rolling_payload());bad[-8:]=doubles(2.)
    records.append(encode('rolling-ball-invalid-tail.cfv2',m,extra_streams=[('NativeFrames',frame(50,bad)+closing)]))
    tip,native=tip_patch();records.append(encode('rolling-ball-tip.cfv2',tip,extra_streams=[('NativeFrames',native)]))
    (DEST/'rolling-expected.json').write_text(json.dumps(dict(provenance='constructed grammar fixtures, not CATIA-authored',fixtures=records),indent=2)+'\n')
