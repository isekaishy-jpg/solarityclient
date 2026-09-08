"""Capture native local-player health Lua queries and field UI callbacks.

Runs 60EB60/60EC60/60F480/60F580, 71C2C0 and 60C240/60BF10.
Resident object lookup, local group membership, Lua inputs/outputs and event
delivery are provider boundaries. No client or OS entry point executes.
"""
import argparse
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, fields, cvar, token = [native.HEAP + x for x in (0, 0x2000, 0x4000, 0x5000)]
    native.write_words(uc, unit + 8, fields)
    native.write_words(uc, unit + 0xd0, fields + 0x100)
    native.write_words(uc, unit + 0x1008, fields + 0x800)
    native.write_words(uc, fields, 1, 0, 0x19)
    native.write_words(uc, 0xbd0a04, cvar)
    uc.mem_write(token, b'player\0')
    native.write_words(uc, token + 16, token)
    output, events = [], []

    def provider(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address in (0x84df60, 0x512a30):
            return_value(uc, 1)
        elif address == 0x84e0e0:
            return_value(uc, token)
        elif address == 0x60abf0:
            native.write_words(uc, native.read_words(uc, sp + 8, 1)[0], 1, 0)
            return_value(uc, 1)
        elif address == 0x4d4db0:
            return_value(uc, unit)
        elif address == 0x60bb70:
            native.write_words(uc, native.read_words(uc, sp + 8, 1)[0], 1)
            return_value(uc, token + 16)
        elif address == 0x81b530:
            events.append(native.read_words(uc, sp + 4, 1)[0])
            return_value(uc, 0)
        elif address == 0x84e2a0:
            output.append(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0])
            return_value(uc, 0)
        elif address == 0x84e280:
            output.append(None)
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = []
    for health, maximum, predicted in [(500,1000,400),(0,1000,1),(1,1000,1),
                                        (0xffffffff,0x80000000,501),(0x80000000,0xffffffff,1)]:
        for enabled in (0,1):
            for ghost in (0,0x10):
                for dynamic in (0,0x20):
                    output.clear()
                    native.write_words(uc, fields+0x148, health)
                    native.write_words(uc, fields+0x168, maximum)
                    native.write_words(uc, fields+0x224, dynamic)
                    native.write_words(uc, fields+0x808, ghost)
                    native.write_words(uc, unit+0xfb0, predicted)
                    native.write_words(uc, cvar+0x30, enabled)
                    for function in (0x60eb60,0x60ec60,0x60f480,0x60f580):
                        native.invoke(uc,function,[0])
                    rows.append(dict(health=health,maximum=maximum,predicted=predicted,
                                     enabled=enabled,ghost=ghost,dynamic=dynamic,queries=output.copy()))
    for offset in (0x48,0x68):
        native.invoke(uc,0x60c240,[1,0,offset,0,0])
    predictions=[]
    for current in (0,1,500,1000,0x7fffffff,0x80000000,0xffffffff):
        for maximum in (0,1,1000,0x80000000,0xffffffff):
            for flags in (0,0x100):
                for delta in (0,1,100,-1,-100,-500,-1000,-0x80000000):
                    native.write_words(uc,unit+0xfb0,current)
                    native.write_words(uc,fields+0x168,maximum)
                    native.write_words(uc,fields+0x1d8,flags)
                    uc.reg_write(UC_X86_REG_ECX,unit)
                    native.invoke(uc,0x71c260,[delta & 0xffffffff])
                    predictions.append([current,maximum,flags,delta,native.read_words(uc,unit+0xfb0,1)[0]])
    return dict(queries=rows,field_events=events,predictions=predictions)


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable');parser.add_argument('output');args=parser.parse_args()
    native.initialize(args.executable)
    result=capture();Path(args.output).write_text(json.dumps(result,indent=2)+'\n')
    lines=[]
    for row in result['queries']:
        values=[row[key] for key in ('health','maximum','predicted','enabled','ghost','dynamic')]
        values += ['nil' if value is None else str(int(value)) for value in row['queries']]
        lines.append(' '.join(map(str,values)))
    Path(args.output).with_suffix('.txt').write_text('\n'.join(lines)+'\n')
    Path(args.output).with_suffix('.prediction.txt').write_text('\n'.join(' '.join(map(str,row)) for row in result['predictions'])+'\n')
    print(f"{len(result['queries'])} native health cases; field events {result['field_events']}")
