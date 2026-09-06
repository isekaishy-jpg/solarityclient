"""Capture original terrain registration grid selection and point height.

The 7C1660 grid capture supplies resident tile/chunk tables and intercepts only
the subsequent height provider to record its global-square arguments. Height
captures execute original 7AD3B0 and 7912C0 without hooks, for both CPU paths.
Uses the fingerprinted PE loader; no client entry point or OS code executes.
"""
import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def f32(value):
    return struct.unpack('<f',struct.pack('<f',value))[0]


def heights(profile):
    if profile==0:return [0.]*145
    if profile==1:return [f32(((i*17)%31)/7.) for i in range(145)]
    if profile==2:return [f32((i%17)*.1234567+(i//17)*.7654321) for i in range(145)]
    return [f32(10000.+((i*37)%113)/13.) for i in range(145)]


def point_height(base,point,row,column,profile,sse,holes):
    uc=native.emulator()
    chunk,header,values,position,output=[native.HEAP+v for v in (0,0x400,0x1000,0x1400,0x1500)]
    native.write_words(uc,chunk+0x110,header,0,0,values)
    uc.mem_write(header+0x3c,struct.pack('<H',holes))
    native.write_floats(uc,chunk+0x7c,base)
    native.write_floats(uc,values,heights(profile))
    native.write_floats(uc,position,[*point,0.])
    native.write_words(uc,output,0xdeadbeef)
    native.write_words(uc,0xcf08f8,int(sse))
    native.invoke(uc,0x7ad3b0,[chunk,position,column,row,output])
    return uc.reg_read(UC_X86_REG_EAX),native.read_words(uc,output,1)[0]


def grid(point):
    uc=native.emulator()
    tile,chunk,position,output,owner=[native.HEAP+v for v in (0,0x1000,0x2000,0x2100,0x2200)]
    native.write_words(uc,0xce48d0,*([tile]*4096))
    native.write_words(uc,tile+0xbc,*([chunk]*256))
    native.write_floats(uc,position,[*point,0.])
    selected=[]
    def capture(uc,address,size,user):
        sp=uc.reg_read(UC_X86_REG_ESP)
        ret,actual_chunk,position,column,row,output=native.read_words(uc,sp,6)
        assert actual_chunk==chunk
        selected.extend([row,column])
        native.write_floats(uc,output,[0.])
        uc.reg_write(UC_X86_REG_EAX,1)
        uc.reg_write(UC_X86_REG_ESP,sp+4)
        uc.reg_write(UC_X86_REG_EIP,ret)
    uc.hook_add(UC_HOOK_CODE,capture,begin=0x7ad3b0,end=0x7ad3b0)
    native.invoke(uc,0x7c1660,[position,output,owner])
    assert uc.reg_read(UC_X86_REG_EAX)==1 and len(selected)==2
    return selected


def capture_grid(output):
    points=[]
    origin=f32(17066.666)
    scale=f32(.24)
    for index in (0,1,7,8,127,128,4095,4096,8191,8192):
        boundary=f32(origin-index/scale)
        for offset in (-.002,-.001,-.000001,0.,.000001,.001,.002):
            points.extend([(f32(boundary+offset),0.),(0.,f32(boundary+offset))])
    rng=random.Random(12340)
    points.extend([(f32(rng.uniform(-17067.,17067.)),f32(rng.uniform(-17067.,17067.))) for _ in range(128)])
    lines=['# Original 7C1660; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# Resident tile/chunk arrays are inputs; height-provider hook records original global-square arguments.', '# worldX worldY globalRow globalColumn (hex bits)']
    for point in points:
        selected=grid(point)
        bits=struct.unpack('<2I',struct.pack('<2f',*point))
        lines.append(' '.join(f'{v:08x}' for v in (*bits,*selected)))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(points),'native terrain grid cases')


def capture_height(output):
    cases=[]
    spacing=f32(-4.1666665)
    for sse in (False,True):
        for profile in range(4):
            for row,column in ((0,0),(3,5),(7,7)):
                for base in ([0.,0.,0.],[17066.666,-12345.125,123.456]):
                    for x,y in ((.1,.2),(.8,.1),(.9,.7),(.2,.9),(.5,.5)):
                        point=[f32(base[0]+(row+x)*spacing),f32(base[1]+(column+y)*spacing)]
                        cases.append((base,point,row,column,profile,sse,0))
        for row,column in ((0,0),(2,4),(7,7)):
            for holes in (1<<(row//2*4+column//2),0xffff,1<<((row//2*4+column//2+1)%16)):
                cases.append(([0.,0.,0.],[(row+.5)*spacing,(column+.5)*spacing],row,column,1,sse,holes))
    lines=['# Original 7AD3B0 / 7912C0 without hooks; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# base3 worldX worldY(hex), row column profile SSE holes(decimal), hit(decimal) height(hex).']
    for case in cases:
        base,point,row,column,profile,sse,holes=case
        hit,height=point_height(*case)
        bits=struct.unpack('<5I',struct.pack('<5f',*base,*point))
        lines.append(' '.join(f'{v:08x}' for v in bits)+f' {row} {column} {profile} {int(sse)} {holes} {hit} {height:08x}')
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native terrain height cases')


def chunk_references(bounds, minimum_z, category, flags, tile_state):
    uc=native.emulator()
    tile,chunk,owner,reference=[native.HEAP+v for v in (0,0x1000,0x2000,0x3000)]
    native.write_words(uc,0xce48d0,*([tile if tile_state!=1 else 0]*4096))
    native.write_words(uc,tile+0x70,int(tile_state==2))
    native.write_words(uc,tile+0xbc,*([chunk if tile_state!=3 else 0]*256))
    native.write_words(uc,0xcf08f4,0)
    native.write_floats(uc,chunk+0x54,[minimum_z])
    native.write_floats(uc,owner+0x48,bounds)
    native.write_words(uc,owner+8,category)
    native.write_words(uc,owner+0x7c,flags)
    selected=[]
    insertions=[]
    def capture(uc,address,size,user):
        sp=uc.reg_read(UC_X86_REG_ESP)
        ret,arg=native.read_words(uc,sp,2)
        if address==0x7c0750:
            assert arg==owner
            bp=uc.reg_read(UC_X86_REG_EBP)
            row=native.read_words(uc,bp-0xc,1)[0]
            column=uc.reg_read(UC_X86_REG_EBX)
            selected.extend([row,column])
            uc.reg_write(UC_X86_REG_EAX,reference)
            pop=4
        else:
            assert arg==reference
            target=uc.reg_read(UC_X86_REG_ECX)-chunk
            insertions.append((target,int(address==0x7b5020)))
            pop=8
        uc.reg_write(UC_X86_REG_ESP,sp+pop)
        uc.reg_write(UC_X86_REG_EIP,ret)
    for address in (0x7c0750,0x6ded60,0x7b5020):
        uc.hook_add(UC_HOOK_CODE,capture,begin=address,end=address)
    native.invoke(uc,0x7c2040,[owner])
    result=uc.reg_read(UC_X86_REG_EAX)
    assert result==int(bool(selected))
    assert not insertions or len(insertions)==len(selected)//2
    assert not insertions or all(v==insertions[0] for v in insertions)
    return selected,insertions[0] if insertions else (0,0)


def capture_chunks(output):
    cases=[]
    origin=f32(17066.666)
    scale=f32(.03)
    for index in (0,1,15,16,511,512,1023,1024):
        boundary=f32(origin-index/scale)
        for offset in (-.002,-.001,-.000001,0.,.000001,.001,.002):
            position=f32(boundary+offset)
            for axis in (0,1):
                point=[0.,0.,-1.]
                point[axis]=position
                cases.append(([*point,point[0],point[1],1.],0.,0x20,2,0))
    rng=random.Random(12340)
    for _ in range(128):
        x,y=[f32(rng.uniform(-17100.,17100.)) for _ in range(2)]
        dx,dy=[rng.uniform(0.,100.) for _ in range(2)]
        cases.append(([x,y,-1.,f32(x+dx),f32(y+dy),1.],0.,0x20,2,0))
    for category in (0,0x20,0x40,0x60):
        for flags in (0,2):
            for z in (-10.,1.,1.0000001192092896):
                for tile_state in range(4):
                    cases.append(([-1.,-1.,-1.,34.,34.,1.],z,category,flags,tile_state))
    lines=['# Original 7C2040: controlled reference allocation and list insertion hooks; native traversal, rounding, residency, and Z filtering.', '# Fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# bounds6 minimumZ(hex), category flags tileState listOffset prepend count(decimal), global row/column pairs(hex). tileState: 0 loaded 1 missing tile 2 loading tile 3 missing chunk.']
    for bounds,z,category,flags,tile_state in cases:
        selected,insertion=chunk_references(bounds,z,category,flags,tile_state)
        bits=struct.unpack('<7I',struct.pack('<7f',*bounds,z))
        lines.append(' '.join(f'{v:08x}' for v in bits)+f' {category} {flags} {tile_state} {insertion[0]} {insertion[1]} {len(selected)//2}'+''.join(f' {v:08x}' for v in selected))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native terrain reference cases')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable',type=Path)
    parser.add_argument('--grid-output',type=Path,required=True)
    parser.add_argument('--height-output',type=Path,required=True)
    parser.add_argument('--chunks-output',type=Path,required=True)
    args=parser.parse_args()
    native.initialize(args.executable)
    capture_grid(args.grid_output)
    capture_height(args.height_output)
    capture_chunks(args.chunks_output)
