#!/usr/bin/env python3
"""Constructed rectangular native NURBS incidence and sparse circular arcs."""
import copy
import json
import math
from create_native_uv_fixtures import compact,doubles,frame
from create_topology_fixtures import DEST,Model,encode,quad,cylinder


def carrier(ambiguous=False,domains=((0,1),(0,1))):
    axes=[compact(3)+compact(2)+b'\x0c'+doubles(*domain) for domain in domains]
    points=[(i,j,i*j/5+(1 if ambiguous and i in (1,2) and j in (1,2) else 0)) for i in range(4) for j in range(4)]
    tail=b'\x05\x05\x05\x05'+doubles(0,domains[0][1]-domains[0][0],0,domains[1][1]-domains[1][0],1,0,1,0)+bytes([1,1,1])+doubles(*([0]*8))+bytes([1,0,1,0,7,7])
    return frame(52,axes[0]+axes[1]+b'\x01'+b''.join(doubles(*p) for p in points)+tail)


def rectangle():
    m=Model();grid=[]
    for i in range(9):
        grid.append([])
        for j in range(9):grid[-1].append(m.handle((3*i/8,3*j/8,1.8*i*j/64)))
    m.points=[grid[0][0],grid[8][0],grid[8][8],grid[0][8],grid[0][4]]
    triangles=[]
    for i in range(8):
        for j in range(8):triangles+=quad([grid[i][j],grid[i+1][j],grid[i+1][j+1],grid[i][j+1]])
    m.face(triangles);m.faces[0]['freeform']=True
    for path in [[grid[i][0] for i in range(9)],[grid[8][j] for j in range(9)],[grid[i][8] for i in range(8,-1,-1)],[grid[0][j] for j in range(8,3,-1)],[grid[0][j] for j in range(4,-1,-1)]]:
        m.edge(path,[0,0]);m.edges[-1]['spline']=True
    return m,grid


def triangular_carrier(perturbation=0):
    axes=[compact(3)+compact(2)+b'\x0c'+doubles(*domain) for domain in [(5,7),(-3,2)]]
    # S(u,v)=(3u,3v,.6uv(u+v-1)): straight triangle borders,
    # curved interior, and a diagonal trim inside the native knot rectangle.
    points=[(i,j,.6*(i*(i-1)/6*j/3+i/3*j*(j-1)/6-i*j/9)+(perturbation if (i,j)==(1,1) else 0)) for i in range(4) for j in range(4)]
    tail=b'\x05\x05\x05\x05'+doubles(0,2,0,5,1,0,1,0)+bytes([1,1,1])+doubles(*([0]*8))+bytes([1,0,1,0,7,7])
    return frame(52,axes[0]+axes[1]+b'\x01'+b''.join(doubles(*p) for p in points)+tail)


def triangle():
    m=Model();n=12;grid={}
    for i in range(n+1):
        for j in range(n+1-i):
            u=i/n;v=j/n;grid[i,j]=m.handle((3*u,3*v,.6*u*v*(u+v-1)))
    m.points=[grid[0,0],grid[n,0],grid[0,n]];tris=[]
    for i in range(n):
        for j in range(n-i):
            tris.append([grid[i,j],grid[i+1,j],grid[i,j+1]])
            if i+j<n-1:tris.append([grid[i+1,j],grid[i+1,j+1],grid[i,j+1]])
    m.face(tris);m.faces[0]['freeform']=True
    for path in [[grid[i,0] for i in range(n+1)],[grid[n-j,j] for j in range(n+1)],[grid[0,j] for j in range(n,-1,-1)]]:m.edge(path,[0,0])
    return m,grid


def sphere_seam(tilted=False):
    m=Model();r=3;n=32;ring=[]
    for i in range(n):
        a=2*math.pi*i/n;x=r*math.sin(a);y=0.
        if tilted:x,y=x/math.sqrt(2),x/math.sqrt(2)
        ring.append(m.handle((x,y,r*math.cos(a))))
    m.points=[ring[0],ring[n//2]]
    normal=(-1/math.sqrt(2),1/math.sqrt(2),0) if tilted else (0,1,0)
    for side in [1,-1]:
        pole=m.handle(tuple(side*r*v for v in normal));triangles=[[pole,ring[i],ring[(i+1)%n]] for i in range(n)]
        if side<0:triangles=[list(reversed(t)) for t in triangles]
        m.face(triangles,analytic=(0x35,[0,0,0,r]))
    m.edge(ring[:n//2+1],[0,1],(0,0,0,r));m.edge(ring[n//2:]+[ring[0]],[0,1],(0,0,0,r));return m


def sphere_chart(phase=0,shift_error=0):
    r=3.;lo=phase;hi=phase+2*math.pi;payload=doubles(0,0,0,r,0,0,0,r,0,0,0,r,r,lo,hi,-math.pi/2,math.pi/2,r,math.nextafter(r*((lo+hi)/2-math.pi),math.inf)+shift_error)
    result=frame(42,payload,family='B')
    for i,u in enumerate([lo,hi,(lo+hi)/2]):result+=frame(24,compact(i+1)+b'\x08'+doubles(-r*math.pi/2,r*u,0,0,-math.nextafter(1.,0.)),family='B')
    return result


if __name__=='__main__':
    m,grid=rectangle();records=[];payload=carrier()+frame(46,b'\x05',family='B')
    records.append(encode('native-nurbs-rectangle.cfv2',m,extra_streams=[('NativeFrames',payload)]))
    records.append(encode('native-nurbs-local-limits.cfv2',m,extra_streams=[('NativeFrames',carrier(domains=((5,7),(-3,2)))+frame(46,b'\x05',family='B'))]))
    records.append(encode('ambiguous-nurbs-rectangle.cfv2',m,extra_streams=[('NativeFrames',carrier()+carrier(True))]))
    broken=copy.deepcopy(m);h=grid[0][2];x,y,z=broken.xyz[h];broken.xyz[h]=(x,y,z+.02)
    records.append(encode('off-nurbs-rectangle.cfv2',broken,extra_streams=[('NativeFrames',payload)]))
    tri,grid=triangle();native=triangular_carrier()+frame(46,b'\x05',family='B')
    records.append(encode('native-nurbs-triangle.cfv2',tri,extra_streams=[('NativeFrames',native)]))
    records.append(encode('ambiguous-nurbs-triangle.cfv2',tri,extra_streams=[('NativeFrames',triangular_carrier(.0001)+native)]))
    broken=copy.deepcopy(tri);h=grid[6,6];x,y,z=broken.xyz[h];broken.xyz[h]=(x,y,z+.02)
    records.append(encode('off-nurbs-triangle.cfv2',broken,extra_streams=[('NativeFrames',native)]))
    m=cylinder(4);original=m.edges[:];bottom=original[0]['handles'][:-1];top=original[1]['handles'][:-1];m.points=bottom+top;m.edges=[]
    for ring,z,faces in [(bottom,0,[0,2]),(top,5,[1,2])]:
        for i in range(4):m.edge([ring[i],ring[(i+1)%4]],faces,(0,0,z,3))
    m.edges.append(original[2]);records.append(encode('sparse-circle-cylinder.cfv2',m))
    records.append(encode('coordinate-sphere-seam.cfv2',sphere_seam(),extra_streams=[('NativeFrames',sphere_chart())]))
    records.append(encode('unresolved-tilted-sphere-seam.cfv2',sphere_seam(True),extra_streams=[('NativeFrames',sphere_chart())]))
    records.append(encode('native-meridian-tilted-sphere.cfv2',sphere_seam(True),extra_streams=[('NativeFrames',sphere_chart(math.pi/4))]))
    records.append(encode('invalid-native-sphere-domain.cfv2',sphere_seam(),extra_streams=[('NativeFrames',sphere_chart(shift_error=.01))]))
    (DEST/'nurbs-binding-expected.json').write_text(json.dumps(dict(provenance='constructed grammar fixtures; not CATIA-authored',fixtures=records),indent=2)+'\n')
