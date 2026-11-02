#!/usr/bin/env python3
"""Constructed CFV2 grammar cases, not CATIA-authored model evidence."""
import copy
import hashlib
import json
import math
from pathlib import Path
import struct
from create_synthetic import directory
from create_geometry_fixtures import DELIMITER

DEST = Path(__file__).resolve().parents[2] / 'tests/fixtures/topology'


def count(n):
    return bytes([n]) if n < 255 else b'\xff' + struct.pack('<I', n)


class Model:
    def __init__(self):
        self.xyz = []
        self.points = []
        self.faces = []
        self.edges = []

    def handle(self, p, alias=False):
        p = tuple(struct.unpack('<3f', struct.pack('<3f', *p)))
        if not alias and p in self.xyz:
            return self.xyz.index(p)
        self.xyz.append(p)
        return len(self.xyz)-1

    def vertex(self, p):
        h = self.handle(p)
        self.points.append(h)
        return h

    def face(self, triangles, normal=None, cylinder=False, sense=1, analytic=None):
        self.faces.append(dict(triangles=triangles, normal=normal, cylinder=cylinder, sense=sense, analytic=analytic))

    def edge(self, handles, faces, circle=None):
        self.edges.append(dict(handles=handles, faces=faces, circle=circle))


def quad(p):
    return [[p[0], p[1], p[2]], [p[0], p[2], p[3]]]


def cube(open_sheet=False):
    m = Model()
    for p in [(-10,-10,0),(10,-10,0),(10,10,0),(-10,10,0),
              (-10,-10,5),(10,-10,5),(10,10,5),(-10,10,5)][:4 if open_sheet else 8]:
        m.vertex(p)
    rings = [[0,1,2,3]] if open_sheet else [[3,2,1,0],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]]
    normals = [[0,0,1]] if open_sheet else [[0,0,-1],[0,0,1],[0,-1,0],[1,0,0],[0,1,0],[-1,0,0]]
    incidence = {}
    for f,(ring,normal) in enumerate(zip(rings,normals)):
        m.face(quad(ring), normal)
        for a,b in zip(ring,ring[1:]+ring[:1]):
            incidence.setdefault(tuple(sorted([a,b])),[]).append(f)
    for pair,fs in sorted(incidence.items()):
        m.edge(list(pair), (fs*2)[:2])
    return m


def circles(m, n=32):
    bottom=[];top=[]
    for z,ring in [(0,bottom),(5,top)]:
        ring.append(m.vertex((3,0,z)))
        for i in range(1,n):
            angle=2*math.pi*i/n
            ring.append(m.handle((3*math.cos(angle),3*math.sin(angle),z)))
    return bottom,top


def side(m,bottom,top,face,inward=False):
    b=bottom+[m.handle(m.xyz[bottom[0]],True)]
    t=top+[m.handle(m.xyz[top[0]],True)]
    triangles=[]
    for i in range(len(bottom)):
        triangles.extend(quad([b[i],b[i+1],t[i+1],t[i]]))
    if inward: triangles=[list(reversed(x)) for x in triangles]
    m.face(triangles,cylinder=True,sense=-1 if inward else 1)
    m.edge([bottom[0],*bottom[1:],bottom[0]],[0,face],(0,0,0,3))
    m.edge([top[0],*top[1:],top[0]],[1,face],(0,0,5,3))
    m.edge([bottom[0],top[0]],[face,face])


def cylinder(n=32):
    m=Model();bottom,top=circles(m,n)
    for ring,z,normal in [(bottom,0,[0,0,-1]),(top,5,[0,0,1])]:
        center=m.handle((0,0,z))
        triangles=[[center,ring[i],ring[(i+1)%len(ring)]] for i in range(len(ring))]
        if z==0:triangles=[list(reversed(t)) for t in triangles]
        m.face(triangles,normal)
    side(m,bottom,top,2)
    return m


def hole_box():
    m=cube();bottom,top=circles(m)
    for f,(ring,z) in enumerate([(bottom,0),(top,5)]):
        outer=[]
        for i in range(len(ring)):
            a=2*math.pi*i/len(ring);x=math.cos(a);y=math.sin(a);s=10/max(abs(x),abs(y))
            # Preserve exact native corner identities at the four square corners.
            p=(math.copysign(10,x),math.copysign(10,y),z) if i%8==4 else (x*s,y*s,z)
            outer.append(m.handle(p))
        triangles=[]
        for i in range(len(ring)):
            j=(i+1)%len(ring);triangles.extend(quad([outer[i],outer[j],ring[j],ring[i]]))
        if f==0:triangles=[list(reversed(t)) for t in triangles]
        m.faces[f]['triangles']=triangles
    side(m,bottom,top,6,True)
    return m


def polar_patch(sphere=False):
    m=Model();r=3;z=3 if sphere else 0
    a=m.vertex((r,0,0 if sphere else 3));c=m.vertex((0,r,0 if sphere else 3));pole=m.vertex((0,0,z));alias=m.handle(m.xyz[pole],True)
    n=16;base=[a]
    for i in range(1,n):
        angle=math.pi/2*i/n;base.append(m.handle((r*math.cos(angle),r*math.sin(angle),0 if sphere else 3)))
    base.append(c)
    if sphere:
        right=[c];left=[pole]
        for i in range(1,n):
            v=math.pi/2*i/n;right.append(m.handle((0,r*math.cos(v),r*math.sin(v))))
        right.append(pole)
        for i in range(1,n):
            v=math.pi/2*(1-i/n);left.append(m.handle((r*math.cos(v),0,r*math.sin(v))))
        left.append(a)
    else:right=[c,pole];left=[alias,a]
    boundary=[*base,*right[1:]]
    if sphere:boundary.extend(left[1:-1])
    # Use a cut pole in the mesh, preserving two native handle identities there.
    if sphere:
        right[-1]=pole;left[0]=alias;boundary=[*base,*right[1:],alias,*left[1:-1]]
    else:boundary.append(alias)
    center=m.handle((r/2,r/2,r/math.sqrt(2)) if sphere else (1,1,math.sqrt(2)))
    # A fan can lie inside the analytic patch only for boundary extraction; the
    # fan's interior coordinates do not define the analytic surface.
    triangles=[[center,boundary[i],boundary[(i+1)%len(boundary)]] for i in range(len(boundary))]
    m.face(triangles,analytic=(0x35,[0,0,0,r]) if sphere else (0x34,[0,0,0,0,0,math.pi/4]))
    m.edge(base,[0,0],(0,0,0 if sphere else 3,r))
    m.edge(right,[0,0],(0,0,0,r) if sphere else None)
    m.edge([pole,alias],[0,0])
    m.edge(left,[0,0],(0,0,0,r) if sphere else None)
    return m


def slanted_cylinder():
    m=Model();bottom=[];top=[];n=32
    for z,ring in [(0,bottom),(5,top)]:
        ring.append(m.vertex((3,0,0 if z==0 else 6.5)))
        for i in range(1,n):
            a=2*math.pi*i/n;x=3*math.cos(a);ring.append(m.handle((x,3*math.sin(a),0 if z==0 else 5+.5*x)))
    for ring,z,normal in [(bottom,0,[0,0,-1]),(top,5,[-1/math.sqrt(5),0,2/math.sqrt(5)])]:
        center=m.handle((0,0,z));triangles=[[center,ring[i],ring[(i+1)%n]] for i in range(n)]
        if z==0:triangles=[list(reversed(t)) for t in triangles]
        m.face(triangles,normal)
    side(m,bottom,top,2);m.edges[1]['circle']=None;m.edges[1]['spline']=True
    return m


def indexed(m, compressed):
    head=bytes.fromhex('ff ff 02 00 01 ff')+struct.pack('<I',len(m.xyz))+b'\xff'+struct.pack('<I',len(m.xyz)+7)+bytes(3)
    if not compressed:return head+b'\x01'+b''.join(struct.pack('<3f',*p) for p in m.xyz)
    controls=bytearray((len(m.xyz)+3)//4);values=[];previous=None
    for i,p in enumerate(m.xyz):
        code=0 if previous is None or p[0]!=previous[0] else 1 if p==previous else 2 if p[:2]==previous[:2] else 3
        controls[i//4]|=code<<(2*(i%4));first={0:0,1:3,2:2,3:1}[code];values.extend(p[first:]);previous=p
    return head+b'\0'+controls+b'\xff'+struct.pack('<I',len(values))+struct.pack('<'+'f'*len(values),*values)


def encode(name,m,compressed=False,ambiguous=False,extra_streams=(),vertex_tags=None):
    table=indexed(m,compressed);main=bytearray(b'fixture-prefix'+table+(table if ambiguous else b''))
    packet_ranges=[]
    for face in m.faces:
        start=len(main);triangles=face['triangles'];normal=face['normal']
        handles=[h for t in triangles for h in t]
        main.extend(bytes([1,0x49 if normal else 0x41])+count(len(triangles))+b'\xff'+struct.pack('<I',len(handles)))
        if normal:main.extend(struct.pack('<3f',*normal))
        for h in handles:main.extend(h.to_bytes(2,'big'))
        packet_ranges.append([start,len(main)])
    fbb=len(main)
    for _ in m.faces:main.extend(bytes.fromhex('30 04 04 ff 01 02 03 04'))
    cut=max(1,len(m.edges)//2);edge_ranges=[]
    for edges in [m.edges[:cut],m.edges[cut:]]:
        main.extend(b'\x01\x01'+count(len(edges)))
        for e in edges:
            start=len(main);main.extend(b'\x02'+count(len(e['handles'])))
            for h in e['handles']:main.extend(h.to_bytes(2,'big'))
            edge_ranges.append([start,len(main)])
        main.extend(DELIMITER)
    main.extend(b'\x01\x06'+count(len(m.points)))
    for h in m.points:main.extend(b'\x05\x08\x01'+struct.pack('<3f',*m.xyz[h]))
    main.extend(DELIMITER)
    surf=bytearray(b'fixture-surface-prefix')
    for i,f in enumerate(m.faces):
        if f.get('freeform'):
            surf.extend((200+i).to_bytes(3,'little')+bytes(3)+struct.pack('<10f',999,888,777,2,3,4,999,888,777,10)+bytes([f['sense']%256]))
            continue
        kind,params=f['analytic'] if f['analytic'] else (0x33,[0,0,0,0,0,3]) if f['cylinder'] else (0x32,[])
        surf.extend((200+i).to_bytes(3,'little')+struct.pack('>H',2+4*len(params))+bytes([0,0x33,kind]))
        surf.extend(struct.pack('>'+'f'*len(params),*params)+struct.pack('<10f',999,888,777,2,3,4,999,888,777,10)+bytes([f['sense']%256]))
    for i,e in enumerate(m.edges):
        surf.extend(b'\x60'+(100+i).to_bytes(3,'little'))
        if e.get('spline'):surf.extend(bytes(3))
        elif e['circle']:surf.extend(bytes.fromhex('00 12 00 33 37')+struct.pack('>4f',*e['circle']))
        else:surf.extend(bytes.fromhex('00 02 00 33 36'))
        for f in e['faces']:surf.extend(count(f))
    if vertex_tags is not None:
        assert len(vertex_tags)==len(m.points) and all(a<b for a,b in zip(vertex_tags,vertex_tags[1:]))
        for tag in vertex_tags:surf.extend(b'\x54'+tag.to_bytes(3,'little')+bytes(3))
    inner=bytearray(b'V5_CFV2\0'+bytes(8));rows=[]
    for key,payload in [('MainDataStream',main),('SurfacicReps',surf),*extra_streams]:
        cut=len(payload)//2;low=len(inner);inner.extend(payload[cut:]+bytes(7));high=len(inner);inner.extend(payload[:cut]+bytes(7))
        rows.append((key,[(high,cut),(low,len(payload)-cut)]))
    pos=len(inner);encoded,_=directory(rows);struct.pack_into('>II',inner,8,pos,len(encoded));inner.extend(encoded)
    outer=bytearray(b'V5_CFV2\0'+bytes(56));outer.extend(inner);pos=len(outer);encoded,_=directory([('Body',[(64,len(inner))])]);struct.pack_into('>II',outer,8,pos,len(encoded));outer.extend(encoded)
    (DEST/name).write_bytes(outer)
    return dict(path=name,sha256=hashlib.sha256(outer).hexdigest(),vertices=len(m.points),edges=len(m.edges),faces=len(m.faces),main_bytes=len(main),fbb=fbb,packet_ranges=packet_ranges,edge_ranges=edge_ranges)


if __name__=='__main__':
    DEST.mkdir(parents=True,exist_ok=True)
    records=[encode('cone-pole.cfv2',polar_patch()),encode('sphere-pole.cfv2',polar_patch(True)),encode('box.cfv2',cube()),encode('box-compressed.cfv2',cube(),True),encode('cylinder.cfv2',cylinder()),encode('wide-cylinder.cfv2',cylinder(256)),encode('through-hole.cfv2',hole_box()),encode('open-sheet.cfv2',cube(True)),encode('ambiguous-index.cfv2',cube(),ambiguous=True)]
    corner=polar_patch();pole=corner.points[-1];alias=corner.edges[2]['handles'][1]
    corner.faces[0]['triangles']=[[pole if h==alias else h for h in t] for t in corner.faces[0]['triangles']]
    corner.edges.pop(2);corner.edges[-1]['handles'][0]=pole
    records.append(encode('cone-corner.cfv2',corner))
    records.append(encode('slanted-cylinder.cfv2',slanted_cylinder()))
    closed=cylinder();closed.edges[2]['spline']=True;records.append(encode('partial-closed.cfv2',closed))
    partial=cube();partial.edges[0]['spline']=True;records.append(encode('unsupported-curve.cfv2',partial))
    repeated=cube();repeated.edges[0]['faces']=[repeated.edges[0]['faces'][0]]*2;records.append(encode('repeated-incidence.cfv2',repeated))
    aliases=cylinder()
    for edge in aliases.edges:
        if edge['circle']:
            edge['handles'][1:-1]=[aliases.handle(aliases.xyz[h],True) for h in edge['handles'][1:-1]]
    records.append(encode('indexed-aliases.cfv2',aliases))
    broken=copy.deepcopy(cube());broken.faces[0]['triangles'].append(broken.faces[0]['triangles'][0]);records.append(encode('nonmanifold-mesh.cfv2',broken))
    broken=cylinder();broken.edges[0]['circle']=None;records.append(encode('degenerate-no-uv.cfv2',broken))
    broken=copy.deepcopy(cube());broken.points.append(broken.points[0]);records.append(encode('coincident-identities.cfv2',broken))
    (DEST/'expected.json').write_text(json.dumps(dict(provenance='constructed grammar fixtures; not CATIA-authored',fixtures=records),indent=2)+'\n')
