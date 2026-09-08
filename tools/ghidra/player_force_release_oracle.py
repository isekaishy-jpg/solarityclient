"""Run 6DC0F0's local death callback and original 6D2950/6DAC10 release gate.

Object lookup, unrelated death side effects, restriction result, clock, and
outgoing datastore writes are supplied boundaries. Timer initialization and
all local/field/health/ghost decisions execute the original instructions.
"""
import argparse
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP
import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, descriptor, fields, player = [native.HEAP + i*0x3000 for i in range(4)]
    native.write_words(uc,unit,0xa326c8)
    native.write_words(uc,unit+8,descriptor)
    native.write_words(uc,unit+0xd0,fields)
    native.write_words(uc,unit+0x1008,player)
    native.write_words(uc,player+0x1064,8)
    allowed,events = 1,[]
    def hook(uc,address,size,context):
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address==0x4d3790:
            uc.reg_write(UC_X86_REG_EDX,0)
            return_value(uc,7)
        elif address==0x86ae20:
            return_value(uc,1000)
        elif address==0x727860:
            return_value(uc,allowed)
        elif address in [0x524bf0,0x518d50,0x523640,0x809ac0,0x5129f0]:
            return_value(uc,0)
        elif address==0x513a30:
            events.append('timer')
        elif address in [0x47b0a0,0x47afe0]:
            word=native.read_words(uc,sp+4,1)[0]
            events.append(f'{word:x}' if address==0x47b0a0 else f'byte{word&255}')
            return_value(uc,0)
            uc.reg_write(UC_X86_REG_ESP,sp+8)
        elif address==0x6b0b50:
            store=native.read_words(uc,sp+4,1)[0]
            native.write_words(uc,store+12,0xffffffff)
            return_value(uc,0)
    uc.hook_add(UC_HOOK_CODE,hook)
    rows=['# local health ghost dynamicFlags unitFlags allowed callbackOrder']
    for local in [0,1]:
        native.write_words(uc,descriptor,7 if local else 8,0,0x19)
        for health in [0,1,0xffffffff]:
            native.write_words(uc,fields+0x48,health)
            for ghost in [0,0x10]:
                native.write_words(uc,player+8,ghost)
                for dynamic in [0,0x20]:
                    native.write_words(uc,fields+0x124,dynamic)
                    for flags in [0,0x100000]:
                        native.write_words(uc,fields+0xd4,flags)
                        for allowed in [0,1]:
                            events.clear()
                            uc.reg_write(UC_X86_REG_ECX,unit)
                            native.invoke(uc,0x6dc0f0,[])
                            rows.append(f'{local} {health:08x} {ghost:08x} {dynamic:08x} {flags:08x} {allowed} '+(','.join(events) or 'none'))
    return rows


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable');parser.add_argument('output')
    args=parser.parse_args();native.initialize(args.executable)
    rows=capture();Path(args.output).write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'Captured {len(rows)-1} forced-release callbacks')
