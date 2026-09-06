"""Capture original build-12340 WMO portal and dual-result BSP floor probes.

Only the fingerprinted PE is mapped into Unicorn; no OS or client entry point
runs. Portal queries run without hooks. Cached BSP queries supply equivalent
predecoded leaf records through the 79B1F0 cache-provider boundary; traversal,
outcodes, triangles, face flags, and result selection execute original code.
Root probes also execute original group registration with controlled allocation
and complete resident inputs. This does not establish cache-builder, residency,
cross-root bank resolution, terrain selection, or dynamic-reference insertion.
Requires Unicorn and a locally owned fingerprinted Wow.exe.
"""
import hashlib
from pathlib import Path
import struct

from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPCW, UC_X86_REG_ECX, UC_X86_REG_EAX

import argparse
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('executable', type=Path)
parser.add_argument('--portal-output', type=Path, required=True)
parser.add_argument('--floor-output', type=Path, required=True)
parser.add_argument('--registration-output', type=Path, required=True)
parser.add_argument('--box-output', type=Path, required=True)
arguments = parser.parse_args()
data = arguments.executable.read_bytes()
assert hashlib.sha256(data).hexdigest() == 'aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8'
pe = struct.unpack_from('<I', data, 0x3c)[0]
section_count = struct.unpack_from('<H', data, pe + 6)[0]
optional_size = struct.unpack_from('<H', data, pe + 20)[0]
image_base = struct.unpack_from('<I', data, pe + 24 + 28)[0]
image_size = struct.unpack_from('<I', data, pe + 24 + 56)[0]
headers = pe + 24 + optional_size
STACK, HEAP, STOP = 0x02000000, 0x03000000, 0x04000000


def emulator():
    uc = Uc(UC_ARCH_X86, UC_MODE_32)
    uc.mem_map(image_base, (image_size + 4095) & ~4095)
    for index in range(section_count):
        base = headers + index * 40
        _, rva, size, raw = struct.unpack_from('<4I', data, base + 8)
        uc.mem_write(image_base + rva, data[raw:raw + size])
    uc.mem_map(STACK, 0x20000)
    uc.mem_map(HEAP, 0x40000)
    uc.mem_map(STOP, 4096)
    uc.reg_write(UC_X86_REG_FPCW, 0x037f)
    return uc


def write_words(uc, address, *words):
    uc.mem_write(address, struct.pack('<' + 'I' * len(words), *words))


def write_floats(uc, address, values):
    uc.mem_write(address, struct.pack('<' + 'f' * len(values), *values))


def read_words(uc, address, count):
    return struct.unpack('<' + 'I' * count, uc.mem_read(address, count * 4))


def read_floats(uc, address, count):
    return list(struct.unpack('<' + 'f' * count, uc.mem_read(address, count * 4)))


def invoke(uc, address, arguments):
    sp = STACK + 0x18000
    write_words(uc, sp, STOP, *arguments)
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.emu_start(address, STOP, timeout=1_000_000, count=100_000)
    assert uc.reg_read(UC_X86_REG_EIP) == STOP


PORTAL_VERTICES = [[-2., -2., 0.], [2., -2., 0.], [2., 2., 0.], [-2., 2., 0.]]

def portal_probe(start, end, side=1, maximum=1., normal=[0.,0.,1.], loaded=True, second=False, geometry=2, distance=0.):
    uc = emulator()
    root, group, other, vertices, portals, refs, segment, fraction, output = [HEAP + offset for offset in (0, 0x1000, 0x1400, 0x2000, 0x2200, 0x2400, 0x2600, 0x2700, 0x2710)]
    write_words(uc, root + 0x1e0, 1)
    write_words(uc, root + 0x1f8, group, other)
    write_words(uc, root + 0x134, vertices, portals, refs)
    write_words(uc, group + 0x198, 1)
    write_words(uc, other + 0x198, int(loaded))
    write_words(uc, group + 0x50, 0, 2 if second else 1)
    if geometry == 3:
        points = [[-5.,1.,1.],[-1.,-1.,1.],[5.,-1.,-1.],[1.,1.,-1.]]
    else:
        x,y = [(1,2),(2,0),(0,1)][geometry]
        points = []
        for vertex in PORTAL_VERTICES:
            point = [0.,0.,0.]
            point[x],point[y] = vertex[:2]
            points.append(point)
    for i, vertex in enumerate(points): write_floats(uc, vertices + i * 12, vertex)
    uc.mem_write(portals, struct.pack('<HH4f', 0, 4, *normal, distance))
    uc.mem_write(refs, struct.pack('<HHhH', 0, 1, side, 0))
    if second:
        uc.mem_write(portals + 20, struct.pack('<HH4f', 0, 4, *normal, distance))
        uc.mem_write(refs + 8, struct.pack('<HHhH', 1, 1, -side, 0))
    write_floats(uc, segment, start + end)
    write_floats(uc, fraction, [maximum])
    write_words(uc, output, 0xffffffff, 0xffffffff)
    uc.reg_write(UC_X86_REG_ECX, root)
    invoke(uc, 0x7af520, [0, segment, fraction, output])
    return uc.reg_read(UC_X86_REG_EAX) & 255, read_words(uc, fraction, 1)[0], read_words(uc, output, 2)

def capture_portals(output):
    cases = []
    for start,end in [([0,0,4],[0,0,-4]), ([0,0,-4],[0,0,4]), ([0,0,0],[0,0,4]), ([0,0,.099],[0,0,4]), ([0,0,-.099],[0,0,-4]), ([0,0,.1],[0,0,4]), ([0,0,-.1],[0,0,-4]), ([0,0,1],[0,0,1]), ([0,0,0],[1,0,0]), ([0,0,.2],[1,0,.2])]:
        for side in (-1,0,1): cases.append((start,end,side,1.,[0,0,1],True,False))
    for x,y in [(-2,0),(2,0),(0,-2),(0,2),(-2,-2),(2,2),(-2,2),(2,-2),(2.000001,0),(-2.000001,0),(1.999999,0),(-1.999999,0)]:
        cases.append(([x,y,4],[x,y,-4],1,1.,[0,0,1],True,False))
    for maximum in (0.,.49999997,.5,.50000006,1.):
        cases.append(([0,0,4],[0,0,-4],1,maximum,[0,0,1],True,False))
    for normal in ([0,0,2],[0,0,-1],[0,0,0]):
        cases.append(([0,0,4],[0,0,-4],1,1.,normal,True,False))
    for side in (-1,1):
        cases.append(([0,0,4],[0,0,-4],side,1.,[0,0,1],True,True))
    cases.append(([0,0,4],[0,0,-4],1,1.,[0,0,1],False,False))
    cases = [(*case,2,0.) for case in cases]
    for axis in (0,1):
        x,y = [(1,2),(2,0)][axis]
        normal = [0.,0.,0.]
        normal[axis] = 1.
        for a,b in [(0.,0.),(-2.,0.),(2.,0.),(0.,-2.),(0.,2.),(-2.,-2.),(2.,2.),(2.000001,0.)]:
            start = [0.,0.,0.]
            end = [0.,0.,0.]
            start[axis],end[axis] = 4.,-4.
            start[x],end[x],start[y],end[y] = a,a,b,b
            cases.append((start,end,1,1.,normal,True,False,axis,0.))
    import random
    rng = random.Random(12340)
    for _ in range(64):
        point = [rng.uniform(-3.,3.),rng.uniform(-1.,1.),0.]
        point[2] = -(point[0]+2*point[1])/3
        direction = [rng.uniform(-5.,5.) for _ in range(3)]
        start = [point[i]+direction[i] for i in range(3)]
        end = [point[i]-direction[i]*rng.uniform(.5,2.) for i in range(3)]
        cases.append((start,end,rng.choice([-1,0,1]),rng.choice([.25,.5,1.,1.05]),[1.,2.,3.],True,False,3,0.))
    for distance in (-.09,.09,-.1,.1):
        cases.append(([0,0,0],[0,0,-4],1,1.,[0,0,1],True,False,2,distance))
    cases.append(([0,0,0],[0,0,0],1,1.,[0,0,1],True,False,2,0.))
    lines = ['# Original build 12340 7AF520, fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# start3 end3 maximum normal3 planeD (hex floats), geometry side loaded second hit (decimal), fraction(hex), source destination(decimal).']
    for case in cases:
        start,end,side,maximum,normal,loaded,second,geometry,distance = case
        hit,fraction,groups = portal_probe(*case)
        floats = struct.unpack('<11I', struct.pack('<11f', *start,*end,maximum,*normal,distance))
        lines.append(' '.join(f'{v:08x}' for v in floats) + f' {geometry} {side} {int(loaded)} {int(second)} {hit} {fraction:08x} {groups[0]} {groups[1]}')
    output.write_text('\n'.join(lines)+'\n')
    print('Captured', len(cases), 'native portal cases')


VERTICES = [[-3.,-3.,0.],[3.,-3.,0.],[-3.,3.,0.],[3.,3.,0.],[-3.,-3.,2.],[3.,-3.,2.],[-3.,3.,2.],[3.,3.,2.]]
FACES = [[0,1,2],[1,3,2],[4,5,6],[5,7,6],[0,1,2],[4,5,6]]
FLAGS = [[8,8,4,4,32,2], [32]*6, [4]*6, [8]*6, [2]*6, [0]*6]
LEAVES = [[4,0,2,5],[1,3,0,4,2]]

def floor_geometry(geometry):
    if geometry == 1:
        return [[x,y,z+x*.125+y*.3+.12345] for x,y,z in VERTICES]
    if geometry == 2:
        return [[x*.127+10000.123,y*1.33-17000.234,z*.723+31.567] for x,y,z in VERTICES]
    if geometry == 3:
        return [[x+16.,y,z] for x,y,z in VERTICES]
    return VERTICES


def floor_tree(topology):
    if topology == 3:
        return [(0,1,2,0.), (4,-1,-1,0.), (1,3,4,0.), (4,-1,-1,0.), (4,-1,-1,0.)], {1:LEAVES[0],3:LEAVES[1],4:[5,2,4,0,3,1]}
    if topology == 4:
        return [(4,-1,-1,0.)], {0:[0,1,2,3,4,5]}
    return [(topology,1,2,1. if topology == 2 else 0.), (4,-1,-1,0.), (4,-1,-1,0.)], {1:LEAVES[0],2:LEAVES[1]}


def floor_probe(start,end,profile=0,maximum=1.05,cached=False,geometry=0,topology=0,secondary=1.05,registration=None):
    uc=emulator()
    model,nodes,refs,mopy,movi,vertices,segment,primary,fallback,pface,fface = [HEAP+v for v in (0,0x1000,0x1100,0x1200,0x1300,0x1400,0x1500,0x1600,0x1610,0x1620,0x1630)]
    write_words(uc,model+0x68,nodes,refs)
    write_words(uc,model+0xdc,mopy,movi,0,vertices)
    write_floats(uc,model+0xb0,[-16.,-16.,-16.,16.,16.,16.])
    node_defs,leaves = floor_tree(topology)
    points = floor_geometry(geometry)
    offset=0
    for i,(flags,negative,positive,plane) in enumerate(node_defs):
        faces = leaves.get(i, [])
        uc.mem_write(nodes+i*16,struct.pack('<HhhHIf',flags,negative,positive,len(faces),offset,plane))
        for j,face in enumerate(faces): uc.mem_write(refs+(offset+j)*2,struct.pack('<H',face))
        offset+=len(faces)
    for i,face in enumerate(FACES): uc.mem_write(movi+i*6,struct.pack('<3H',*face))
    for i,flag in enumerate(FLAGS[profile]): uc.mem_write(mopy+i*2,bytes([flag,255]))
    for i,vertex in enumerate(points): write_floats(uc,vertices+i*12,vertex)
    write_floats(uc,segment,start+end)
    write_floats(uc,primary,[maximum]);write_floats(uc,fallback,[secondary])
    write_words(uc,pface,0xffffffff);write_words(uc,fface,0xffffffff)
    write_words(uc,0xcdd7a0,int(cached))
    if cached:
        cache_by_leaf={}
        for slot,(i,faces) in enumerate(leaves.items()):
            cache=HEAP+0x3000+slot*0x3000
            cache_by_leaf[nodes+i*16]=cache
            uc.mem_write(cache+6,struct.pack('<H',len(points)))
            for j,vertex in enumerate(points): write_floats(uc,cache+8+j*12,vertex)
            uc.mem_write(cache+0x18a4,struct.pack('<H',len(faces)))
            for j,face in enumerate(faces):
                uc.mem_write(cache+0x18a6+j*6,struct.pack('<3H',*FACES[face]))
                uc.mem_write(cache+0x1fae+j*2,struct.pack('<H',FLAGS[profile][face]))
                uc.mem_write(cache+0x2206+j*2,struct.pack('<H',face))
        def cache_provider(uc,address,size,user):
            sp=uc.reg_read(UC_X86_REG_ESP)
            ret,owner,leaf=read_words(uc,sp,3)
            uc.reg_write(UC_X86_REG_EAX,cache_by_leaf[leaf])
            uc.reg_write(UC_X86_REG_ESP,sp+24)
            uc.reg_write(UC_X86_REG_EIP,ret)
        uc.hook_add(UC_HOOK_CODE,cache_provider,begin=0x79b1f0,end=0x79b1f0)
    if registration is not None:
        return registration_probe(uc,model,segment,maximum,secondary,**registration)
    uc.reg_write(UC_X86_REG_ECX,model)
    invoke(uc,0x7cb260,[segment,primary,pface,fallback,fface])
    return [read_words(uc,p,1)[0] for p in (primary,pface,fallback,fface)]

def capture_floors(output):
    cases=[]
    for cached in (False,True):
        for profile in range(6):
            for x,y in [(0.,0.),(-1.,0.),(1.,0.),(-.01,0.),(.01,0.),(-.010001,0.),(.010001,0.),(4.,0.),(0.,-3.),(0.,3.)]:
                cases.append(([x,y,4.],[x,y,-4.],profile,1.05,cached))
        for end in (.02,.01,0.,-.01): cases.append(([0.,0.,1.],[0.,0.,end],0,1.05,cached))
        for maximum in (0.,.25,.5,.75,1.05): cases.append(([0.,0.,4.],[0.,0.,-4.],0,maximum,cached))
    cases=[(*case,0,0,case[3]) for case in cases]
    import random
    rng=random.Random(12340)
    for cached in (False,True):
        for geometry in (1,2):
            for _ in range(64):
                x,y=rng.uniform(-3.01,3.01),rng.uniform(-3.01,3.01)
                if geometry == 1:
                    start=[x+rng.uniform(-2.,2.),y+rng.uniform(-2.,2.),5.]
                    end=[x,y,-2.]
                    topology=3
                else:
                    start=[x*.127+10000.123,y*1.33-17000.234,36.]
                    end=[(x+rng.uniform(-1.,1.))*.127+10000.123,(y+rng.uniform(-1.,1.))*1.33-17000.234,28.]
                    topology=4
                cases.append((start,end,rng.choice([0,1,2,3]),rng.choice([.25,.5,1.05]),cached,geometry,topology,rng.choice([.25,.5,1.05])))
        for topology in (0,1,2,3):
            for start,end in [([-2.,-1.,4.],[2.,1.,-4.]),([2.,1.,4.],[-2.,-1.,-4.]),([-1.,2.,4.],[1.,-2.,-4.]),([1.,-2.,4.],[-1.,2.,-4.]),([0.,.01,4.],[0.,-.01,-4.]),([0.,-.01,4.],[0.,.01,-4.])]:
                cases.append((start,end,0,1.05,cached,0,topology,1.05))
        for x in (15.98,15.989999,15.99,15.990001,16.,16.01):
            cases.append(([x,0.,4.],[x,0.,-4.],0,1.05,cached,3,0,1.05))
        for maximum,secondary in ((.25,1.05),(1.05,.25),(0.,1.05),(1.05,0.)):
            cases.append(([0.,0.,4.],[0.,0.,-4.],0,maximum,cached,0,0,secondary))
        cases.append(([0.,0.,0.],[0.,0.,0.],0,1.05,cached,0,0,1.05))
    lines=['# Original 7CB260 / 7CA600 / 7C6600 and 7C6790; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# Cached cases supply equivalent decoded cache records, not query decisions.', '# start3 end3 primaryMaximum fallbackMaximum(hex) profile cached geometry topology(decimal) primaryFraction primaryFace fallbackFraction fallbackFace(hex)']
    for case in cases:
        start,end,profile,maximum,cached,geometry,topology,secondary=case
        result=floor_probe(*case)
        bits=struct.unpack('<8I',struct.pack('<8f',*start,*end,maximum,secondary))
        lines.append(' '.join(f'{v:08x}' for v in bits)+f' {profile} {int(cached)} {geometry} {topology} '+' '.join(f'{v:08x}' for v in result))
    for profile in range(6,10):
        result=large_floor_probe(profile)
        bits=struct.unpack('<8I',struct.pack('<8f',0.,0.,4.,0.,0.,-4.,1.05,1.05))
        lines.append(' '.join(f'{v:08x}' for v in bits)+f' {profile} 0 0 4 '+' '.join(f'{v:08x}' for v in result))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases)+4,'native BSP probe cases')


def large_floor_probe(profile):
    uc=emulator()
    model,nodes,refs,mopy,movi,vertices,segment,primary,fallback,pface,fface = [HEAP+v for v in (0,0x1000,0x2000,0x8000,0x10000,0x20000,0x21000,0x21100,0x21110,0x21120,0x21130)]
    count=8193
    write_words(uc,model+0x68,nodes,refs)
    write_words(uc,model+0xdc,mopy,movi,0,vertices)
    write_floats(uc,model+0xb0,[-16.,-16.,-16.,16.,16.,16.])
    uc.mem_write(nodes,struct.pack('<HhhHIf',4,-1,-1,count,0,0.))
    for i in range(count):
        uc.mem_write(refs+i*2,struct.pack('<H',i))
        uc.mem_write(movi+i*6,struct.pack('<3H',0,1,2))
        flag={6:8,7:2,8:32,9:0}[profile]
        if i==8192 and profile in (7,9):flag=8
        uc.mem_write(mopy+i*2,bytes([flag,255]))
    for i,vertex in enumerate(VERTICES[:3]):write_floats(uc,vertices+i*12,vertex)
    write_floats(uc,segment,[0.,0.,4.,0.,0.,-4.])
    write_floats(uc,primary,[1.05]);write_floats(uc,fallback,[1.05])
    write_words(uc,pface,0xffffffff);write_words(uc,fface,0xffffffff)
    write_words(uc,0xcdd7a0,0)
    uc.reg_write(UC_X86_REG_ECX,model)
    sp=STACK+0x18000
    write_words(uc,sp,STOP,segment,primary,pface,fallback,fface)
    uc.reg_write(UC_X86_REG_ESP,sp)
    uc.emu_start(0x7cb260,STOP,timeout=10_000_000,count=5_000_000)
    assert uc.reg_read(UC_X86_REG_EIP)==STOP
    return [read_words(uc,p,1)[0] for p in (primary,pface,fallback,fface)]


def registration_probe(uc,model,segment,maximum,secondary,point,mogi,mogp,transformed,adjacent_floor,side,distance):
    other,placed0,placed1,link0,link1,vertices,portals,refs,shared,placed,info,containment,primary,fallback = [HEAP+v for v in (0xc000,0xd000,0xd100,0xd200,0xd220,0xd400,0xd600,0xd700,0xe000,0xf000,0xe400,0xe600,0xe700,0xe740)]
    if adjacent_floor:
        uc.mem_write(other,bytes(uc.mem_read(model,0x200)))
    write_words(uc,model+0x30,mogp[0]);write_words(uc,other+0x30,mogp[1])
    write_words(uc,model+0x198,1);write_words(uc,other+0x198,1)
    write_words(uc,model+0x50,0,1);write_words(uc,other+0x50,0,0)
    write_words(uc,shared+0x1e0,1)
    write_words(uc,shared+0x1f8,model,other)
    write_words(uc,shared+0x16c,2)
    write_words(uc,shared+0x130,info,vertices,portals,refs)
    for i in range(2):
        write_words(uc,info+i*32,mogi[i])
        write_floats(uc,info+i*32+4,[-3.,-3.,-1.,3.,3.,3.])
    for i,vertex in enumerate(PORTAL_VERTICES):
        write_floats(uc,vertices+i*12,vertex)
    uc.mem_write(portals,struct.pack('<HH4f',0,4,0.,0.,1.,distance))
    uc.mem_write(refs,struct.pack('<HHhH',0,1,side,0))
    write_words(uc,placed+0xc,0x400 if transformed else 0)
    write_floats(uc,placed+0x48,[-16.,-16.,-16.,16.,16.,16.])
    for offset in (0x70,0xb0):
        write_floats(uc,placed+offset,[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.])
    write_words(uc,placed+0xf4,shared)
    write_words(uc,placed+0x114,0x10)
    write_words(uc,placed+0x118,placed+0x118,(placed+0x118)|1)
    register_native_groups(uc,shared,placed,[placed0,placed1],[link0,link1])
    write_floats(uc,containment,point)
    for output,limit in ((primary,maximum),(fallback,secondary)):
        for bank in (0,1):
            write_words(uc,output+bank*16,0,0)
            write_floats(uc,output+bank*16+8,[limit])
            write_words(uc,output+bank*16+12,0xffffffff)
    invoke(uc,0x7f9480,[placed+0x48,segment,segment+12])
    if uc.reg_read(UC_X86_REG_EAX):
        invoke(uc,0x7c25d0,[placed,segment,segment+12,containment,primary,fallback])
    result=[]
    for output in (primary,fallback):
        owner,group,fraction,packed=read_words(uc,output+(0 if transformed else 16),4)
        result += [int(owner!=0),read_words(uc,group+0x50,1)[0] if owner else 0xffffffff,fraction,packed&0xffff,packed>>16]
    return result


def register_native_groups(uc,shared,placed,groups,links):
    allocations=iter(groups)
    link_by_group=dict(zip(groups,links))
    def allocate(uc,address,size,user):
        sp=uc.reg_read(UC_X86_REG_ESP)
        ret=read_words(uc,sp,1)[0]
        if address==0x7c0910:
            result=next(allocations)
        else:
            group=read_words(uc,sp+4,1)[0]
            result=link_by_group[group]
            write_words(uc,result+4,group)
        uc.reg_write(UC_X86_REG_EAX,result)
        uc.reg_write(UC_X86_REG_ESP,sp+4)
        uc.reg_write(UC_X86_REG_EIP,ret)
    hooks=[uc.hook_add(UC_HOOK_CODE,allocate,begin=address,end=address) for address in (0x7c0910,0x7c0750)]
    invoke(uc,0x7bde50,[shared,placed])
    for hook in hooks:uc.hook_del(hook)
    linked=[]
    entry=read_words(uc,placed+0x11c,1)[0]
    while entry and entry&1==0:
        group=read_words(uc,entry+4,1)[0]
        linked.append(read_words(uc,group+0x50,1)[0])
        entry=read_words(uc,entry+0x14,1)[0]
    assert linked==list(range(len(groups))), linked


def capture_registration(output):
    cases=[]
    for cached in (False,True):
        for mogi0 in (0,8,0x80,0x10000,0x400000):
            for mogp0 in (0,8):
                for transformed in (False,True):
                    for point in ([0.,0.,.15],[3.,0.,.15],[3.000001,0.,.15]):
                        cases.append(([0.,0.,4.],[0.,0.,-4.],1.05,1.05,0,cached,dict(point=point,mogi=[mogi0,8],mogp=[mogp0,8],transformed=transformed,adjacent_floor=False,side=1,distance=0.)))
        for distance in (0.,.00079,.0008,.00081,-2.,-2.00079,-2.0008,-2.00081):
            for side in (-1,1):
                for adjacent in (False,True):
                    cases.append(([0.,0.,4.],[0.,0.,-4.],1.05,1.05,0,cached,dict(point=[0.,0.,.15],mogi=[0,8],mogp=[0,8],transformed=False,adjacent_floor=adjacent,side=side,distance=distance)))
        for profile in (2,3,4,5):
            for maximum,secondary in ((1.05,1.05),(.25,1.05),(1.05,.25)):
                cases.append(([0.,0.,4.],[0.,0.,-4.],maximum,secondary,profile,cached,dict(point=[0.,0.,.15],mogi=[0,8],mogp=[0,8],transformed=True,adjacent_floor=False,side=-1,distance=0.)))
    lines=['# Original 7BDE50 + 7F9480 + 7C25D0 + 7C1DC0 + floor/portal helpers; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# Complete source groups and allocation are inputs; original 7BDE50 constructs the group list; cached leaves use controlled provider.', '# start3 end3 point3 primaryMax fallbackMax planeD(hex); profile cache MOGI0 MOGP0 MOGI1 MOGP1 transformed adjacentFloor side(decimal); primary and fallback(has group fraction face interior)(hex).']
    for start,end,maximum,secondary,profile,cached,settings in cases:
        result=floor_probe(start,end,profile,maximum,cached,secondary=secondary,registration=settings)
        bits=struct.unpack('<12I',struct.pack('<12f',*start,*end,*settings['point'],maximum,secondary,settings['distance']))
        ints=[profile,int(cached),settings['mogi'][0],settings['mogp'][0],settings['mogi'][1],settings['mogp'][1],int(settings['transformed']),int(settings['adjacent_floor']),settings['side']]
        lines.append(' '.join(f'{v:08x}' for v in bits)+' '+' '.join(str(v) for v in ints)+' '+' '.join(f'{v:08x}' for v in result))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native root registration cases')


def capture_boxes(output):
    import random
    rng=random.Random(12340)
    bounds=[-1.,-2.,-3.,1.,2.,3.]
    cases=[]
    for axis in range(3):
        for boundary in (bounds[axis],bounds[axis+3]):
            for delta in (-.00002,-.00001,-.000009,0.,.000009,.00001,.00002):
                start=[0.,0.,0.];end=[0.,0.,0.]
                start[axis]=boundary+delta;end[axis]=boundary-delta
                cases.append((start,end))
    for _ in range(128):
        start=[rng.uniform(-8.,8.) for _ in range(3)]
        end=[rng.choice(bounds[i::3])+rng.choice([-.00002,-.00001,0.,.00001,.00002]) for i in range(3)]
        cases.extend([(start,end),(end,start)])
    lines=['# Original 7F9480; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# bounds6 start3 end3(hex), hit(decimal)']
    for start,end in cases:
        uc=emulator()
        write_floats(uc,HEAP,bounds+start+end)
        invoke(uc,0x7f9480,[HEAP,HEAP+24,HEAP+36])
        result=int(uc.reg_read(UC_X86_REG_EAX)!=0)
        bits=struct.unpack('<12I',struct.pack('<12f',*bounds,*start,*end))
        lines.append(' '.join(f'{v:08x}' for v in bits)+f' {result}')
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native segment-box cases')


if __name__ == '__main__':
    capture_portals(arguments.portal_output)
    capture_floors(arguments.floor_output)
    capture_registration(arguments.registration_output)
    capture_boxes(arguments.box_output)
