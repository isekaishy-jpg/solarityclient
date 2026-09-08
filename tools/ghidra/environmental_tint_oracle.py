"""Execute original environmental special-13 initialization and model tint update.

7265C0, 71A9A0, 6ACC50, and 720DB0 execute from the pinned PE. The only
providers are the unit's model accessor and 7FE1B0's spell classification.
No client or operating-system entry point executes.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from unit_water_effect_oracle import returned


def capture(executable, output):
    n.initialize(executable)
    uc = n.emulator()
    unit, kit, tint, model, vtable, accessor = [n.HEAP + x for x in (0,0x2000,0x3000,0x4000,0x5000,0x6000)]
    n.write_words(uc,unit,vtable)
    n.write_words(uc,vtable+0xd4,accessor)
    def provider(uc,address,size,context):
        if address == accessor: returned(uc,model)
        elif address == 0x7fe1b0: returned(uc,0,4)
    uc.hook_add(UC_HOOK_CODE,provider)
    cases = []
    for start in (0,1,1000,0xfffffff0):
        for color, hold, fade in ((14477467.,2.,2.),(0x102030,0.,1.),(0xabcdef,0.125,0.333)):
            words=[0]*38
            words[17:21]=[13,0xffffffff,0xffffffff,0xffffffff]
            for field,value in ((21,color),(25,hold),(29,fade)):
                words[field]=struct.unpack('<I',struct.pack('<f',value))[0]
            n.write_words(uc,kit,*words)
            n.write_words(uc,0xcd76ac,start)
            uc.reg_write(UC_X86_REG_ECX,unit)
            n.invoke(uc,0x7265c0,[0,0,kit,0,0,0,0])
            initial=n.read_words(uc,unit+0xb10,4)
            assert initial[0] == start
            for elapsed in (0,1,124,125,126,332,333,457,458,1999,2000,2001,2500,3000,3998,3999,4000,4001):
                n.write_words(uc,unit+0xb10,*initial)
                n.write_words(uc,tint,0xffffffff)
                uc.reg_write(UC_X86_REG_ECX,unit)
                now=(start+elapsed)&0xffffffff
                n.invoke(uc,0x71a9a0,[now,tint])
                result=n.read_words(uc,tint,1)[0]
                after=n.read_words(uc,unit+0xb10,1)[0]
                n.write_words(uc,unit+0xb10,*initial)
                uc.reg_write(UC_X86_REG_ECX,unit)
                n.invoke(uc,0x720db0,[now])
                rgb=n.read_words(uc,model+0x180,3)
                cases.append((*words[21:22],words[25],words[29],*initial,now,result,after,*rgb))
    Path(output).write_text('# 7265C0 special 13 / 71A9A0 / 720DB0; float parameters and RGB are IEEE words.\n'+''.join(' '.join(f'{x:08x}' for x in case)+'\n' for case in cases),encoding='ascii')
    print(f'Captured {len(cases)} original environmental tint cases')


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args=parser.parse_args()
    capture(args.executable,args.output)
