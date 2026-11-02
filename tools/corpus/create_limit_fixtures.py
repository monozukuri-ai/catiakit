#!/usr/bin/env python3
"""Constructed source-limit and masked-cylinder cases, not native CATIA proof."""
import copy
import json
import math
from create_native_uv_fixtures import compact, doubles, frame
from create_topology_fixtures import DEST, Model, encode, quad


def surface(delta=0.):
    axis=compact(3)+compact(2)+b'\x0c'+doubles(0,1)
    poles=[(i,j,1.5*i*(i-1)+(delta if j==0 and i in [1,2] else 0)) for i in range(4) for j in range(4)]
    tail=b'\x05'*4+doubles(0,1,0,1,1,0,1,0)+b'\x01'*3+doubles(*([0]*8))+bytes([1,0,1,0,7,7])
    return frame(52,axis+axis+b'\x01'+b''.join(doubles(*p) for p in poles)+tail)


def patch():
    m=Model();n=12;grid=[[m.handle((3*i/n,3*j/n,(3*i/n)**2)) for j in range(n+1)] for i in range(n+1)]
    # One unrelated side is deliberately off the source surface, so recovering
    # the shared source boundary must not qualify the whole rectangle.
    h=grid[n//2][-1];x,y,z=m.xyz[h];m.xyz[h]=(x,y,z+.02)
    low=[m.handle((3*i/n,0,-1)) for i in range(n+1)]
    m.points=[grid[0][0],grid[n][0],grid[n][n],grid[0][n],low[0],low[n]]
    m.face(sum((quad([grid[i][j],grid[i+1][j],grid[i+1][j+1],grid[i][j+1]]) for i in range(n) for j in range(n)),[]));m.faces[0]['freeform']=True
    m.face(sum((quad([grid[i][0],low[i],low[i+1],grid[i+1][0]]) for i in range(n)),[]),normal=[0,-1,0])
    m.edge([grid[i][0] for i in range(n+1)],[0,1]);m.edges[-1]['spline']=True
    for path in [grid[-1],[grid[i][-1] for i in range(n,-1,-1)],grid[0][::-1]]:
        m.edge(path,[0,0]);m.edges[-1]['spline']=True
    for path in [[grid[0][0],low[0]],low,[low[-1],grid[-1][0]]]:m.edge(path,[1,1])
    return m,grid


def cylinder(mask,x,y,terminal=3,shift_error=0.):
    lane=[v for i,v in enumerate(x+y) if not mask&(1<<i)]
    result=doubles(0,0,0)+bytes([mask])+doubles(*lane,3,0,6*math.pi,-10,10)+bytes([terminal])
    if terminal==3:result+=doubles(shift_error)
    return frame(40,result,family='B')


def owner_gap():
    # The polynomial quarter circle interpolates its three mesh stations, but
    # bows away from the analytic cylinder between them. No fitted curve.
    m=Model();radius=10.;k=4/3*math.tan(math.pi/8);grid=[];below=[]
    for a in [0,math.pi/4,math.pi/2]:
        grid.append([m.handle((radius*math.cos(a),radius*math.sin(a),j)) for j in [0,.5,1]])
        below.append(m.handle((radius*math.cos(a),radius*math.sin(a),-1)))
    x,y,z=m.xyz[grid[1][-1]];m.xyz[grid[1][-1]]=(x,y,z+.02)
    m.points=[grid[0][0],grid[2][0],grid[2][2],grid[0][2],below[0],below[2]]
    m.face(sum((quad([grid[i][j],grid[i+1][j],grid[i+1][j+1],grid[i][j+1]]) for i in range(2) for j in range(2)),[]));m.faces[0]['freeform']=True
    m.face(sum((quad([grid[i][0],below[i],below[i+1],grid[i+1][0]]) for i in range(2)),[]),analytic=(0x33,[0,0,0,0,0,radius]))
    m.edge([grid[i][0] for i in range(3)],[0,1]);m.edges[-1]['spline']=True
    for path in [grid[-1],[grid[i][-1] for i in range(2,-1,-1)],grid[0][::-1]]:
        m.edge(path,[0,0]);m.edges[-1]['spline']=True
    m.edge([grid[0][0],below[0]],[1,1]);m.edge(below,[1,1],(0,0,-1,radius));m.edge([below[-1],grid[-1][0]],[1,1])
    u=compact(3)+compact(2)+b'\x0c'+doubles(0,1);v=compact(1)+compact(2)+b'\x0c'+doubles(0,1)
    poles=[(x,y,z) for x,y in [(radius,0),(radius,k*radius),(k*radius,radius),(0,radius)] for z in [0,1]]
    tail=b'\x05'*4+doubles(0,1,0,1,1,0,1,0)+b'\x01'*3+doubles(*([0]*8))+bytes([1,0,1,0,7,7])
    return m,frame(52,u+v+b'\x01'+b''.join(doubles(*p) for p in poles)+tail)


def unknown_cap():
    from create_ruled_fixtures import sparse_ellipse
    m,_=sparse_ellipse();m.faces[1]['freeform']=True
    # New 82-byte form: X=-world X, Y=world Y, Z=-world Z.
    result=cylinder(0x2e,[-1,0,0],[0,1,0],terminal=7)
    for i in range(4):
        ks=[math.pi/2*i+math.pi/4*j for j in range(3)]
        lanes=[[3*math.pi-3*u for u in ks],[-5-1.5*math.cos(u) for u in ks],[-3]*3,[1.5*math.sin(u) for u in ks],[0]*3,[1.5*math.cos(u) for u in ks]]
        jet=compact(1)+compact(5)+compact(len(ks))+b'\x0c'+doubles(*ks)+compact(len(ks))+b'\x01'+b''.join(doubles(*v) for v in lanes[:4])+b'\x05'+b''.join(doubles(*v) for v in lanes[4:])+doubles(ks[0],ks[-1])+b'\x07'
        result+=frame(32,jet)
    return m,result


if __name__=='__main__':
    rows=[];closing=frame(46,b'\x05',family='B');m,grid=patch()
    rows.append(encode('native-nurbs-independent-limit.cfv2',m,extra_streams=[('NativeFrames',surface()+closing)]))
    rows.append(encode('ambiguous-nurbs-independent-limit.cfv2',m,extra_streams=[('NativeFrames',surface()+surface(.0001)+closing)]))
    off=copy.deepcopy(m);h=grid[6][0];x,y,z=off.xyz[h];off.xyz[h]=(x,y,z+.02)
    rows.append(encode('off-nurbs-independent-limit.cfv2',off,extra_streams=[('NativeFrames',surface()+closing)]))
    a=math.sqrt(.5);native=cylinder(0,[a,0,a],[-a,0,a])+cylinder(0x2e,[-1,0,0],[0,1,0])+cylinder(0x33,[0,0,1],[1,0,0])+cylinder(0x2e,[-1,0,0],[0,1,0],7)
    # Non-unit frame, missing closure and inconsistent saved shift remain opaque.
    native+=cylinder(0x3f,[0,0,0],[0,0,0])+cylinder(0x2e,[-1,0,0],[0,1,0],shift_error=.01)
    rows.append(encode('native-masked-cylinder-charts.cfv2',m,extra_streams=[('NativeFrames',native+closing)]))
    m,native=owner_gap();rows.append(encode('native-nurbs-owner-gap.cfv2',m,extra_streams=[('NativeFrames',native+closing)]))
    m,native=unknown_cap();rows.append(encode('native-unknown-cap.cfv2',m,extra_streams=[('NativeFrames',native+closing)]))
    # Sparse ellipse quarters are selected by other native vertices on the same
    # ellipse, without depending on saved UV records.
    from create_ruled_fixtures import sparse_ellipse
    m,_=sparse_ellipse();rows.append(encode('coelliptic-sparse-arcs.cfv2',m))
    (DEST/'limits-expected.json').write_text(json.dumps(dict(provenance='constructed grammar fixtures, not native CATIA',fixtures=rows),indent=2)+'\n')
