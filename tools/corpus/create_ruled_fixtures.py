#!/usr/bin/env python3
"""Constructed scaled-reference ruled-jet grammar cases, not CATIA native proof."""
import copy,json,math
from create_native_uv_fixtures import compact,doubles,frame
from create_topology_fixtures import DEST,Model,encode,quad


def payload(perturbation=0):
    knots=[0.,1.,2.];blocks=[[],[],[]]
    for u in knots:
        z=u*u+(perturbation if u==1 else 0)
        blocks[0].append([0,u,z,1,u,z]);blocks[1].append([0,1,2*u,0,1,2*u]);blocks[2].append([0,0,2,0,0,2])
    return compact(3)+compact(5)+compact(3)+b'\x0c'+doubles(*knots)+b''.join(doubles(*(x for row in block for x in row)) for block in blocks)+doubles(0,2,0,2,0,1)


def patch():
    m=Model();n=12;grid=[[m.handle((2*j/n,2*i/n,(2*i/n)**2)) for j in range(n+1)] for i in range(n+1)]
    m.points=[grid[0][0],grid[-1][0],grid[-1][-1],grid[0][-1]]
    m.face(sum((quad([grid[i][j],grid[i+1][j],grid[i+1][j+1],grid[i][j+1]]) for i in range(n) for j in range(n)),[]));m.faces[0]['freeform']=True
    m.edge([grid[i][0] for i in range(n+1)],[0,0]);m.edges[-1]['spline']=True
    m.edge(grid[-1],[0,0]);m.edge([grid[i][-1] for i in range(n,-1,-1)],[0,0]);m.edges[-1]['spline']=True
    m.edge(grid[0][::-1],[0,0]);return m,grid


def cone_patch(apex_chart=False):
    m=Model();n=12;grid={};angle=math.pi/6;apex=[4,5,6-2*math.sqrt(3)]
    for i in range(n+1):
        for j in range(n+1-i):
            u=math.pi/2*i/n;v=3+j/n
            grid[i,j]=m.handle((4+v*math.sin(angle)*math.cos(u),5+v*math.sin(angle)*math.sin(u),apex[2]+v*math.cos(angle)))
    tris=[]
    for i in range(n):
        for j in range(n-i):
            tris.append([grid[i,j],grid[i+1,j],grid[i,j+1]])
            if i+j<n-1:tris.append([grid[i+1,j],grid[i+1,j+1],grid[i,j+1]])
    m.points=[grid[0,0],grid[n,0],grid[0,n]];m.face(tris,analytic=(0x34,[*apex,0,0,angle]))
    m.edge([grid[i,0] for i in range(n+1)],[0,0],(4,5,apex[2]+3*math.cos(angle),1.5))
    m.edge([grid[n-j,j] for j in range(n+1)],[0,0]);m.edges[-1]['spline']=True
    m.edge([grid[0,j] for j in range(n,-1,-1)],[0,0])
    origin=apex if apex_chart else [4,5,6];radius=0 if apex_chart else 2;v0,v1=(3,4) if apex_chart else (-1,0)
    native=frame(41,doubles(*origin,1,0,0,0,1,0,0,0,1,math.pi/3,radius,0,math.pi/2,v0,v1,2,1,0,-3*math.pi/4,5*math.pi/4),family='B')
    lanes=[[math.pi,0],[v0,v1],[-math.pi,-math.pi],[1,1],[0,0],[0,0]]
    jet=compact(1)+compact(5)+compact(2)+b'\x0c'+doubles(0,1)+compact(2)+b'\x01'+b''.join(doubles(*v) for v in lanes[:4])+b'\x05'+b''.join(doubles(*v) for v in lanes[4:])+doubles(0,1)+b'\x07'
    return m,native+frame(32,jet)


def cylinders():
    out=b''
    for mask,x,y in [(0x12,[math.cos(.3),0,math.sin(.3)],[-math.sin(.3),0,math.cos(.3)]),(0x02,[math.cos(.3),0,math.sin(.3)],[-math.sin(.3),0,math.cos(.3)]),(0x09,[0,math.cos(.3),math.sin(.3)],[0,-math.sin(.3),math.cos(.3)])]:
        axes=x+y;lane=[v for i,v in enumerate(axes) if not mask&(1<<i)]
        out+=frame(40,doubles(0,0,0)+bytes([mask])+doubles(*lane,2,0,4*math.pi,-1,1)+b'\x03'+doubles(0),family='B')
    return out


def sparse_ellipse():
    # Reuse the slanted-cap grammar at four boundary stations, so each quarter
    # ellipse has only its two endpoints in the native trim mesh.
    from create_topology_fixtures import side
    m=Model();bottom=[];top=[]
    for z,ring in [(0,bottom),(5,top)]:
        for i in range(4):
            a=math.pi/2*i;x=3*math.cos(a);ring.append(m.handle((x,3*math.sin(a),0 if z==0 else 5+.5*x)))
    m.points=[bottom[0],*top]
    for ring,z,normal in [(bottom,0,[0,0,-1]),(top,5,[-1/math.sqrt(5),0,2/math.sqrt(5)])]:
        center=m.handle((0,0,z));tris=[[center,ring[i],ring[(i+1)%4]] for i in range(4)]
        if z==0:tris=[list(reversed(t)) for t in tris]
        m.face(tris,normal)
    side(m,bottom,top,2);old=m.edges;m.edges=[old[0]]
    native=frame(40,doubles(0,0,0)+b'\x24'+doubles(1,0,0,1,3,0,6*math.pi,0,6.5)+b'\x03'+doubles(0),family='B')
    for i in range(4):
        m.edge([top[i],top[(i+1)%4]],[1,2]);m.edges[-1]['spline']=True;ks=[math.pi/2*i+math.pi/4*j for j in range(3)]
        lanes=[[3*u for u in ks],[5+1.5*math.cos(u) for u in ks],[3]*3,[-1.5*math.sin(u) for u in ks],[0]*3,[-1.5*math.cos(u) for u in ks]]
        pc=compact(1)+compact(5)+compact(3)+b'\x0c'+doubles(*ks)+compact(3)+b'\x01'+b''.join(doubles(*v) for v in lanes[:4])+b'\x05'+b''.join(doubles(*v) for v in lanes[4:])+doubles(ks[0],ks[-1])+b'\x07'
        native+=frame(32,pc)
    m.edges.append(old[2]);return m,native


if __name__=='__main__':
    m,grid=patch();closing=frame(46,b'\x05',family='B');carrier=frame(0x39,payload());records=[]
    records.append(encode('native-ruled-patch.cfv2',m,extra_streams=[('NativeFrames',carrier+closing)]))
    records.append(encode('ambiguous-ruled-patch.cfv2',m,extra_streams=[('NativeFrames',frame(0x39,payload(.0001))+carrier+closing)]))
    off=copy.deepcopy(m);h=grid[6][0];x,y,z=off.xyz[h];off.xyz[h]=(x,y,z+.02)
    records.append(encode('off-ruled-patch.cfv2',off,extra_streams=[('NativeFrames',carrier+closing)]))
    broken=bytearray(payload());broken[-8:]=doubles(0.)
    records.append(encode('invalid-ruled-tail.cfv2',m,extra_streams=[('NativeFrames',frame(0x39,broken)+closing)]))
    m,native=cone_patch();records.append(encode('native-cone-shift.cfv2',m,extra_streams=[('NativeFrames',native+cylinders()+closing)]))
    m,native=cone_patch(True);records.append(encode('native-cone-apex-chart.cfv2',m,extra_streams=[('NativeFrames',native+cylinders()+closing)]))
    m,native=sparse_ellipse();records.append(encode('native-sparse-ellipse.cfv2',m,extra_streams=[('NativeFrames',native+closing)]))
    (DEST/'ruled-expected.json').write_text(json.dumps(dict(provenance='constructed grammar fixtures, not CATIA-authored',fixtures=records),indent=2)+'\n')
