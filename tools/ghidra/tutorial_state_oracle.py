"""Execute build-12340 tutorial flags, triggers, history and packet reception.

Uses resident 256-bit banks, actual 530920/CDataStore readers, native bitset
updates and completed-history traversal. Hooks the growable-array capacity
provider, outgoing packet append/send/destruction, CVar lookup, sound and Lua
events/results. All state and ordering comes from the fingerprinted PE.
No client or operating-system entry point runs.
"""

import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    seen, completed, cvar, packet, payload = [native.HEAP+x for x in (0, 0x100, 0x200, 0x300, 0x400)]
    native.write_words(uc, 0xbd1ae0, 0, 8, 0, seen, 0, 8, 0, completed)
    sounds, events, outgoing, building = [], [], [], []
    lua_value = None

    def provider(uc, address, size, context):
        nonlocal lua_value
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4670d0:
            count = native.read_words(uc, sp + 4, 1)[0]
            assert count <= 8
            native.write_words(uc, uc.reg_read(UC_X86_REG_ECX) + 4, count)
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x767440:
            return_value(uc, cvar)
        elif address == 0x4c74a0:
            sounds.append(bytes(uc.mem_read(native.read_words(uc, sp+4, 1)[0], 40)).split(b'\0')[0])
            return_value(uc, 0)
        elif address == 0x81b530:
            event, fmt, argument = native.read_words(uc, sp+4, 3)
            assert event == 0x175
            events.append((argument, native.read_words(uc, seen, 8)))
            return_value(uc, 0)
        elif address == 0x47b0a0:
            building.append(native.read_words(uc, sp+4, 1)[0])
            return_value(uc, uc.reg_read(UC_X86_REG_ECX))
            uc.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x6b0b50:
            outgoing.append(list(building))
            building.clear()
            return_value(uc, 0)
        elif address == 0x47ae50:
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 16)
        elif address in (0x84e280, 0x84e2a0):
            if address == 0x84e280: value = 0
            else: value = int(struct.unpack('<d', uc.mem_read(sp+8,8))[0])
            lua_value = value
            return_value(uc, value)

    uc.hook_add(UC_HOOK_CODE, provider)
    # op: 0 receive, 1 trigger, 2 flag, 3 clear, 4 reset, 5 next, 6 previous,
    # 7 IsTutorialFlagged core, 8 CanResetTutorials Lua owner.
    cases = [(1,27,1), (8,0,0), (0,0,0), (1,27,1), (1,27,1),
             (7,27,0), (2,27,0), (2,27,0), (1,26,0), (1,28,1),
             (2,28,0), (2,0,0), (2,59,0), (5,27,0), (5,28,0),
             (6,27,0), (6,59,0), (6,256,0), (8,0,0), (3,0,0),
             (1,1,1), (5,1,0), (8,0,0), (4,0,0), (8,0,0),
             (0,1,0), (7,26,0), (1,26,1), (1,27,1), (1,28,1),
             (2,28,0), (5,28,0), (6,28,0), (0,2,0), (1,27,1),
             (0,0,0)]
    cases += [(2,index,0) for index in range(60)]
    cases += [(op,index,0) for op in (5,6,7) for index in (0,1,26,27,28,58,59,60,256)]
    rows = []
    for op, argument, show in cases:
        sounds.clear();events.clear();outgoing.clear();building.clear()
        native.write_words(uc,cvar+0x30,show)
        result = 0xffffffff
        if op == 0:
            data = {0: bytes(32), 1: struct.pack('<8I', 1<<26,0,0,0,0,0,0,0), 2: bytes()}[argument]
            uc.mem_write(payload,data or b'\0')
            native.write_words(uc,packet,0,payload,0,len(data),len(data),0)
            native.invoke(uc,0x530920,[0,0xfd,0,packet])
        elif op in (1,2):
            native.invoke(uc,0x530840 if op==1 else 0x530450,[argument])
        elif op in (3,4):
            native.invoke(uc,0x530510 if op==3 else 0x530630,[])
        elif op in (5,6):
            native.invoke(uc,0x530140 if op==5 else 0x530190,[(argument-1)&0xffffffff])
            result=uc.reg_read(UC_X86_REG_EAX)
        elif op == 7:
            # The public Lua function admits only IDs 1..60. This core accepts
            # zero-based IDs; keep this probe inside the resident bank.
            if argument >= 256: continue
            native.invoke(uc,0x5222b0,[argument])
            result=uc.reg_read(UC_X86_REG_EAX)&255
        elif op == 8:
            lua_value = None
            native.invoke(uc,0x530700,[0])
            assert uc.reg_read(UC_X86_REG_EAX) == 1 and lua_value is not None
            result=lua_value
        assert len(events)<=1 and len(outgoing)<=1 and len(sounds)<=1
        assert not sounds or sounds==[b'TutorialPopup']
        body=(outgoing[0]+[0xffffffff])[:2] if outgoing else [0xffffffff,0xffffffff]
        event=events[0][0] if events else 0xffffffff
        pre_seen=events[0][1] if events else (0,)*8
        row=[op,argument,show,native.read_words(uc,0xbd1ae0,1)[0],result,*body,event,len(sounds),
             *native.read_words(uc,seen,8),*native.read_words(uc,completed,8),
             *native.read_words(uc,0xbd19f0,60),*pre_seen]
        assert len(row)==93
        rows.append(' '.join(str(value) for value in row))
    return '\n'.join(rows)+'\n'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable');parser.add_argument('output');args=parser.parse_args()
    native.initialize(args.executable)
    data=capture();Path(args.output).write_text(data)
    print(f'{len(data.splitlines())} native tutorial state/event/packet captures')


if __name__=='__main__':main()
