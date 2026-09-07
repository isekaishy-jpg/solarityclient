"""Capture build-12340 WMO liquid geometry with original grid and clipping code.

Executes 7A7CC0, 7A7920, 7A7F60 and their math/attribute callbacks. Only
resident neighboring-group lookup and the device RGBA capability are supplied.
No client process or operating-system code runs.
"""
import argparse
import random
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_ESP
import wmo_registration_oracle as native
import liquid_material_oracle as material

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('executable')
parser.add_argument('--output', required=True)
args = parser.parse_args()
native.initialize(args.executable)
tables = material.depth_coordinates()

def capture(width, height, uv_mode, depth_bank, depth_x, color, clipped=False, planes=()):
    """Serialize inputs followed by every native vertex bit and index."""
    uc = native.emulator()
    group, matrix, tint, split, vertex, tiles, output, pointers, indices, index_pointer, bank, row, mat, caps = [native.HEAP + offset for offset in
        (0, 0x1000, 0x1100, 0x1200, 0x2000, 0x2800, 0x3000, 0x5000, 0x6000, 0x6800, 0x7000, 0x7100, 0x7200, 0x7300)]
    native.write_words(uc, group + 0x114, width + 1, height + 1, width, height)
    corner = [-1.12345, 21.334, 3.5]
    native.write_floats(uc, group + 0x124, corner)
    native.write_words(uc, group + 0x134, vertex, tiles)
    native.write_words(uc, group + 0x144, 1)
    count = (width + 1) * (height + 1)
    raw = b''.join(struct.pack('<hhf', i * 128 - 512, 512 - i * 256, i * 0.12345) for i in range(count))
    uc.mem_write(vertex, raw)
    mask = bytes(15 if i % 5 == 2 else (128 if clipped and i % 3 != 1 else 0) for i in range(width * height))
    uc.mem_write(tiles, mask)
    native.write_floats(uc, matrix, [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.])
    native.write_words(uc, tint, color)
    native.write_words(uc, pointers, output, output+12, output+24, output+36, 0 if uv_mode else output+28)
    native.write_words(uc, index_pointer, indices)
    native.write_words(uc, 0xad4070, 1, 1)
    native.write_words(uc, 0xad4084, bank)
    native.write_words(uc, bank, row)
    native.write_words(uc, row + 0x38, 1)
    native.write_words(uc, row + 0xa4, min(depth_bank, 1))
    native.write_words(uc, 0xad4094, 1, 1)
    native.write_words(uc, 0xad40a8, bank+4)
    native.write_words(uc, bank+4, mat)
    native.write_words(uc, mat+4, 1 if depth_bank==2 else 0)
    native.write_words(uc, 0xadfbb4, 0xcdf7d0, 0xcdfbd0)
    for address, table in zip((0xcdf7d0, 0xcdfbd0), tables): native.write_words(uc,address,*table)
    native.write_words(uc, caps+0x14, 1)
    root,other,refs,portals=[native.HEAP+off for off in (0x10000,0x11000,0x12000,0x13000)]
    native.write_words(uc,root+0x138,portals,refs)
    native.write_words(uc,group+0x50,0,len(planes))
    
    for i,plane in enumerate(planes):
        uc.mem_write(refs+i*8,struct.pack('<HHhH',i,i+1,plane[4],0))
        uc.mem_write(portals+i*20,struct.pack('<HH4f',0,4,*plane[:4]))
        native.write_words(uc,other+i*0x200+0x11c,*plane[7:9])
        native.write_floats(uc,other+i*0x200+0x124,[*plane[5:7],0.])
    def provider(uc, address, size, ctx):
        if address == 0x532af0: material.return_value(uc,caps)
        elif address == 0x7aea80:
            neighbor=native.read_words(uc,uc.reg_read(UC_X86_REG_ESP)+4,1)[0]
            material.return_value(uc,other+(neighbor-1)*0x200)
    uc.hook_add(UC_HOOK_CODE, provider)
    uc.reg_write(UC_X86_REG_ECX, 0)
    native.invoke(uc,0x7a7cc0,[group,matrix,tint,uv_mode,struct.unpack('<I',struct.pack('<f',depth_x))[0],split,44,*[pointers+i*4 for i in range(5)]])
    assert uc.reg_read(UC_X86_REG_EAX)==count
    native.invoke(uc,0x7a7920,[group,split,index_pointer,0])
    if clipped:
        uc.reg_write(UC_X86_REG_ECX,native.HEAP+0x10000)
        native.invoke(uc,0x7a7f60,[group,matrix,tint,uv_mode,struct.unpack('<I',struct.pack('<f',depth_x))[0],44,*[pointers+i*4 for i in range(5)],index_pointer,count])
        count += uc.reg_read(UC_X86_REG_EAX)
    index_count=(native.read_words(uc,index_pointer,1)[0]-indices)//2
    vertex_output=bytes(uc.mem_read(output,count*44))
    plane_bytes=b''.join(struct.pack('<4fi2f2I',*plane) for plane in planes)
    header=struct.pack('<4IfI3f4I',width,height,uv_mode,depth_bank,depth_x,color,*corner,len(raw)//8,count,index_count,len(planes))
    return header+raw+mask+plane_bytes+vertex_output+bytes(uc.mem_read(indices,index_count*2))

records=[]
for width,height in ((1,1),(3,2),(2,5)):
    for uv_mode,depth_bank,depth_x,color in ((0,0,0.,0xffffffff),(0,1,1.,0xa1234567),(1,2,0.,0xffffffff),(1,2,1.,0x71335577)):
        records.append(capture(width,height,uv_mode,depth_bank,depth_x,color))

for width,height in ((1,1),(3,2),(2,5)):
    for uv_mode,depth_bank,depth_x,color in ((0,0,0.,0xffffffff),(0,1,1.,0xa1234567),(1,2,0.,0xffffffff),(1,2,1.,0x71335577)):
        for planes in ((), ((1.,0.,0.,0.,1,-100.,-100.,100,100),),
                ((1.,0.,0.,0.,1,-100.,-100.,100,100),(0.,1.,0.,-22.,1,-100.,-100.,100,100)),
                ((1.,0.,0.,0.,-1,-100.,-100.,100,100),),
                ((1.,0.,0.,0.,1,100.,100.,1,1),),
                ((0.3,-0.7,0.5,16.5,1,-100.,-100.,100,100),)):
            records.append(capture(width,height,uv_mode,depth_bank,depth_x,color,True,planes))
rng = random.Random(0x7a7f60)
for case in range(64):
    planes = []
    for _ in range(3):
        nx, ny, nz = [rng.uniform(-1., 1.) for _ in range(3)]
        d = -(nx * rng.uniform(-1., 7.) + ny * rng.uniform(22., 29.) + nz * 0.5)
        planes.append((nx,ny,nz,d,1,-100.,-100.,100,100))
    uv_mode = case % 3 == 2
    records.append(capture(2,2,int(uv_mode),case % 3,float(case % 2),0xa1234567,True,planes))
Path(args.output).write_bytes(b''.join(records))
print(f'captured {len(records)} WMO liquid geometry cases')
