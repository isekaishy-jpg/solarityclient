"""Capture original exterior/interior dynamic WMO reference destinations.

Original 7BDE50 constructs root groups using controlled allocation. Original
7C2BF0/7C2D30 then transform the object box, admit roots and groups, and insert
references. Allocation and list insertion are observed through hooks; no OS
or client entry point executes. Requires the pinned locally owned executable.
"""
import argparse
from pathlib import Path
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


TRANSFORMS = [
    [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.],
    [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,-100.,200.,-30.,1.],
    [0.,-2.,0.,0.,2.,0.,0.,0.,0.,0.,2.,0.,-20.,40.,-10.,1.],
]
BOXES = [
    [-16.,-16.,-16.,0.,16.,16.],
    [0.,-16.,-16.,16.,16.,16.],
    [-16.,0.,-16.,16.,16.,16.],
    [-16.,-16.,-16.,16.,0.,16.],
]


def destinations(bounds, transform, flags, selected):
    uc=native.emulator()
    placed,shared,owner,info,reference=[native.HEAP+v for v in (0,0x1000,0x2000,0x3000,0x4000)]
    groups=[native.HEAP+0x5000+i*0x100 for i in range(4)]
    models=[native.HEAP+0x6000+i*0x400 for i in range(4)]
    links=[native.HEAP+0x8000+i*0x20 for i in range(4)]
    native.write_words(uc,placed+0xf4,shared)
    native.write_words(uc,placed+0x114,0x10)
    native.write_words(uc,placed+0x118,placed+0x118,(placed+0x118)|1)
    native.write_floats(uc,placed+0x70,TRANSFORMS[0])
    native.write_floats(uc,placed+0xb0,TRANSFORMS[transform])
    native.write_words(uc,shared+0x16c,4)
    native.write_words(uc,shared+0x130,info)
    native.write_words(uc,shared+0x1e0,1)
    native.write_words(uc,shared+0x1f8,*models)
    native.write_floats(uc,shared+0x1a8,[-16.,-16.,-16.,16.,16.,16.])
    for i in range(4):
        native.write_words(uc,info+i*32,flags[i])
        native.write_floats(uc,info+i*32+4,BOXES[i])
        native.write_words(uc,models[i]+0x198,1)
    native.register_native_groups(uc,shared,placed,groups,links)
    native.write_words(uc,0xd25438,0,0,placed)
    native.write_words(uc,placed+4,1)
    native.write_floats(uc,owner+0x48,bounds)
    native.write_words(uc,owner+8,0x20)
    native.write_words(uc,owner+0x7c,2)
    result=[]
    def capture(uc,address,size,user):
        sp=uc.reg_read(UC_X86_REG_ESP)
        ret,arg=native.read_words(uc,sp,2)
        if address==0x7c0750:
            assert arg==owner
            uc.reg_write(UC_X86_REG_EAX,reference)
            pop=4
        else:
            assert arg==reference
            group=native.read_words(uc,reference+8,1)[0]
            assert uc.reg_read(UC_X86_REG_ECX)==group+0x84
            result.append(groups.index(group))
            pop=8
        uc.reg_write(UC_X86_REG_ESP,sp+pop)
        uc.reg_write(UC_X86_REG_EIP,ret)
    for address in (0x7c0750,0x7b5020):
        uc.hook_add(UC_HOOK_CODE,capture,begin=address,end=address)
    if selected<0:
        native.invoke(uc,0x7c2bf0,[owner])
    else:
        native.invoke(uc,0x7c2d30,[owner,placed,groups[selected]])
    return result


def capture(output):
    cases=[]
    for transform in range(3):
        translation=[(0.,0.,0.),(100.,-200.,30.),(20.,10.,5.)][transform]
        for box in ([-1.,-1.,-1.,1.,1.,1.],[0.,0.,0.,0.,0.,0.],[-17.,-1.,-1.,-16.,1.,1.],[16.,-1.,-1.,17.,1.,1.],[16.000002,-1.,-1.,17.,1.,1.],[-2.,-2.,-2.,-1.,-1.,-1.]):
            bounds=[box[i]+translation[i%3] for i in range(6)]
            for changed in (0,8,0x80,0x10000,0x400000):
                for selected in (-1,0,1,2,3):
                    cases.append((bounds,transform,[changed,8,0,8],selected))
    lines=['# Original 7BDE50 / 7C2BF0 / 7C2D30; allocation and insertion hooks only.', '# Fingerprint aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8', '# bounds6(hex) inverseTransform flags4 selected(-1 exterior) count groupIndices(decimal).']
    for bounds,transform,flags,selected in cases:
        result=destinations(bounds,transform,flags,selected)
        bits=struct.unpack('<6I',struct.pack('<6f',*bounds))
        lines.append(' '.join(f'{v:08x}' for v in bits)+' '+' '.join(str(v) for v in [transform,*flags,selected,len(result),*result]))
    output.write_text('\n'.join(lines)+'\n')
    print('Captured',len(cases),'native WMO group membership cases')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    native.initialize(args.executable)
    capture(args.output)
