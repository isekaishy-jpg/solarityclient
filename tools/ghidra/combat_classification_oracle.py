"""Capture original 74DCB0 combat flags and 715440 faction-table reactions.

Group, directed-unit-reaction and selected-unit providers are controlled inputs.
Owner resolution, GUID family tests and flag precedence execute original code.
No operating-system or client entry point runs.
"""
import argparse
import random
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    uc=n.emulator()
    unit,fields,local,local_fields,guid_pointer,source_faction,target_faction=[n.HEAP+x for x in (0,0x2000,0x4000,0x6000,0x8000,0x8100,0x8200)]
    n.write_words(uc,unit+8,fields);n.write_words(uc,unit+0xd0,fields+0x100)
    n.write_words(uc,local+8,local_fields);n.write_words(uc,local+0xd0,local_fields+0x100)
    n.write_words(uc,local_fields,1,0,0x19)
    group,party,raid,role,marker,reaction=0,0,0,0,8,3
    guid=1

    def provider(uc,address,size,context):
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address==0x4d4db0:
            lo,hi=n.read_words(uc,sp+4,2);requested=lo+(hi<<32)
            return_value(uc,unit if requested==guid else local if requested==1 else 0)
        elif address==0x4d3790:
            uc.reg_write(UC_X86_REG_EDX,0);return_value(uc,1)
        elif address==0x52bd10:return_value(uc,group)
        elif address==0x52c680:return_value(uc,party)
        elif address==0x5726f0:return_value(uc,raid)
        elif address in (0x52c9a0,0x572900):return_value(uc,role)
        elif address==0x5728c0:return_value(uc,marker)
        elif address==0x7251c0:
            return_value(uc,reaction)
            uc.reg_write(UC_X86_REG_ESP,sp+8)

    uc.hook_add(UC_HOOK_CODE,provider)
    rng=random.Random(12340)
    cases=[]
    guids=[0,1,2,0xf130000000000003,0xf140000000000004,0xf150000000000005,0xf110000000000006]
    for guid in guids:
        for owner in (0,1,2,0xf130000000000009):
            for owned in (0,1):
                for reaction in (0,1,2,3,4,7):
                    group=rng.randrange(2);party=rng.randrange(2);raid=rng.randrange(2)
                    role=rng.randrange(8);marker=rng.randrange(10)
                    target=rng.randrange(2);focus=rng.randrange(2)
                    high=guid>>32;is_gameobject=high&0xf0f00000==0xf0100000
                    n.write_words(uc,fields,guid&0xffffffff,high,0x21 if is_gameobject else 0x19 if high==0 else 9)
                    uc.mem_write(fields+0x100,bytes(0x80))
                    slot=fields+0x100+(0 if is_gameobject else 0x18 if owned else 0x28)
                    n.write_words(uc,slot,owner&0xffffffff,owner>>32)
                    n.write_words(uc,guid_pointer,guid&0xffffffff,high)
                    n.write_words(uc,0xbeb608,group)
                    n.write_words(uc,0xbd07b0,(guid&0xffffffff) if target else 0,high if target else 0)
                    n.write_words(uc,0xbd07d0,(guid&0xffffffff) if focus else 0,high if focus else 0)
                    n.invoke(uc,0x74dcb0,[guid_pointer])
                    result=uc.reg_read(UC_X86_REG_EAX)
                    cases.append([guid,owner or guid,int(owned and owner!=0),reaction if not is_gameobject else -1,party,raid,group,target,focus,role,marker,result])
    factions=[]
    for index in range(256):
        a=[rng.randrange(8) for _ in range(14)];b=[rng.randrange(8) for _ in range(14)]
        a[2]=rng.randrange(4)*0x1000;b[2]=rng.randrange(4)*0x1000
        n.write_words(uc,source_faction,*a);n.write_words(uc,target_faction,*b)
        n.invoke(uc,0x715440,[source_faction,target_faction])
        factions.append(a+b+[uc.reg_read(UC_X86_REG_EAX)])
    return cases,factions


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable');parser.add_argument('output');args=parser.parse_args()
    n.initialize(args.executable)
    cases,factions=capture()
    Path(args.output).write_text('\n'.join(' '.join(map(str,row)) for row in cases)+'\n')
    Path(args.output).with_suffix('.factions.txt').write_text('\n'.join(' '.join(map(str,row)) for row in factions)+'\n')
    print(f'{len(cases)} native classifications; {len(factions)} directed faction reactions')
