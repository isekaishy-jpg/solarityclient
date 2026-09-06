"""Capture original cross-root WMO registration and terrain precedence.

Uses the pinned PE loader from wmo_registration_oracle. Group allocation and
the terrain-height provider are controlled inputs. Root/group admission, BSP
floor queries, both result banks, fallback copies, owner-flag filtering, and
terrain precedence execute original instructions. No client entry point runs.
"""
import argparse
from pathlib import Path
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def root(uc,owner,flags,face_flags,height):
    placed=native.HEAP+owner*0x8000
    group,link,shared,model,nodes,refs,mopy,movi,vertices,info=[placed+v for v in (0x400,0x600,0x800,0x1000,0x2000,0x2100,0x2200,0x2300,0x2400,0x2500)]
    native.write_words(uc,placed+0xc,flags)
    native.write_floats(uc,placed+0x48,[-16.,-16.,-16.,16.,16.,16.])
    for offset in (0x70,0xb0):
        native.write_floats(uc,placed+offset,[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.])
    native.write_words(uc,placed+0xf4,shared)
    native.write_words(uc,placed+0x114,0x10)
    native.write_words(uc,placed+0x118,placed+0x118,(placed+0x118)|1)
    native.write_words(uc,shared+0x130,info)
    native.write_words(uc,shared+0x16c,1)
    native.write_words(uc,shared+0x1e0,1)
    native.write_words(uc,shared+0x1f8,model)
    native.write_words(uc,info,8)
    native.write_floats(uc,info+4,[-16.,-16.,-16.,16.,16.,16.])
    native.write_words(uc,model+0x30,8)
    native.write_words(uc,model+0x68,nodes,refs)
    native.write_floats(uc,model+0xb0,[-16.,-16.,-16.,16.,16.,16.])
    native.write_words(uc,model+0xdc,mopy,movi,0,vertices)
    native.write_words(uc,model+0x198,1)
    uc.mem_write(nodes,struct.pack('<HhhHIf',4,-1,-1,1,0,0.))
    uc.mem_write(refs,struct.pack('<H',0))
    uc.mem_write(mopy,bytes([face_flags,255]))
    uc.mem_write(movi,struct.pack('<3H',0,1,2))
    for i,vertex in enumerate([[-3.,-3.,height],[3.,-3.,height],[-3.,3.,height]]):
        native.write_floats(uc,vertices+i*12,vertex)
    native.register_native_groups(uc,shared,placed,[group],[link])
    return placed


def probe(roots,mode=0,height=0.,clear_secondary=False):
    uc=native.emulator()
    addresses=[root(uc,*entry) for entry in roots]
    owners={address:entry[0] for address,entry in zip(addresses,roots)}
    for i,address in enumerate(addresses):
        native.write_words(uc,address+4,addresses[i+1] if i+1<len(addresses) else 1)
    native.write_words(uc,0xd25438,0,0,addresses[0] if addresses else 0)
    native.write_words(uc,0xcdd7a0,0)
    segment,point,primary,fallback,obj,interior,hit=[native.HEAP+v for v in (0x30000,0x30020,0x30100,0x30200,0x31000,0x31200,0x31210)]
    native.write_floats(uc,segment,[0.,0.,4.,0.,0.,-4.])
    native.write_floats(uc,point,[0.,0.,.15])
    if mode == 0:
        native.invoke(uc,0x7c2700,[segment,segment+12,point,primary,fallback,0])
    else:
        native.write_floats(uc,obj+0x60,[0.,0.,0.])
        native.write_words(uc,obj+0x7c,0x2000 if clear_secondary else 0)
        native.write_words(uc,0xcf08f4,0)
        def terrain(uc,address,size,user):
            sp=uc.reg_read(UC_X86_REG_ESP)
            ret,start,output,chunk=native.read_words(uc,sp,4)
            native.write_floats(uc,output,[height])
            native.write_words(uc,chunk,native.HEAP+0x32000)
            uc.reg_write(UC_X86_REG_EAX,1 if mode==1 else 0)
            uc.reg_write(UC_X86_REG_ESP,sp+4)
            uc.reg_write(UC_X86_REG_EIP,ret)
        uc.hook_add(UC_HOOK_CODE,terrain,begin=0x7c1660,end=0x7c1660)
        native.invoke(uc,0x7c28f0,[obj,segment,segment+12,point,interior,hit,primary,fallback])
    result=[]
    for output in (primary,primary+16,fallback,fallback+16):
        owner,group,fraction,packed=native.read_words(uc,output,4)
        result += [owners[owner] if owner else 0xffffffff,
            native.read_words(uc,group+0x50,1)[0] if owner else 0xffffffff,
            fraction,packed&0xffff,packed>>16]
    return result


def capture(output):
    cases=[]
    bank_roots=[(0,0x400,8,0.),(1,0x400,4,1.),(2,0,8,2.),(3,0,4,3.)]
    for mask in range(16):
        selected=[entry for i,entry in enumerate(bank_roots) if mask&(1<<i)]
        cases.extend([(selected,0,0.,False),(list(reversed(selected)),0,0.,False)])
    for flags in (0,0x400):
        for heights in ((0.,0.),(2.,0.),(0.,2.)):
            cases.append(([(0,flags,32,heights[0]),(1,flags,32,heights[1])],0,0.,False))
    cases.append(([(0,0x420,32,3.),(1,0,8,0.)],0,0.,False))
    for clear in (False,True):
        for fraction in (.0,.12499999,.125,.25,.375,.5,.50000006,1.05):
            cases.append((bank_roots,1,4.-fraction*1000.,clear))
        for height in (4.000001,5.):
            cases.append((bank_roots,1,height,clear))
        cases.append((bank_roots,2,3.,clear))
    lines=['# Original build-12340 7BDE50 / 7C2700 / 7C28F0; fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# Allocation and terrain provider are controlled; all WMO query and bank/terrain decisions execute original instructions.', '# mode height(hex) clearSecondary rootCount(decimal); each root(owner flags MOPY(decimal) height(hex)); primary0 primary1 fallback0 fallback1(owner group fraction face interior)(hex).']
    for roots,mode,height,clear in cases:
        result=probe(roots,mode,height,clear)
        height_bits=struct.unpack('<I',struct.pack('<f',height))[0]
        fields=[str(mode),f'{height_bits:08x}',str(int(clear)),str(len(roots))]
        for owner,flags,mopy,z in roots:
            zbits=struct.unpack('<I',struct.pack('<f',z))[0]
            fields += [str(owner),str(flags),str(mopy),f'{zbits:08x}']
        lines.append(' '.join(fields)+' '+' '.join(f'{v:08x}' for v in result))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native scene registration cases')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    arguments=parser.parse_args()
    native.initialize(arguments.executable)
    capture(arguments.output)
