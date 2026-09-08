"""Capture build-12340 player combat flag, event and Lua lockdown ordering.

Runs actual 728F70 flag callback, 524600 event/state owner and 511CC0 Lua
query. Hooks action-bar refresh providers and event/Lua output boundaries.
Only the fingerprinted PE is emulated; no operating-system entry point runs.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    manager, player, fields = (native.HEAP+x for x in (0, 0x2000, 0x4000))
    native.write_words(uc, 0xbd078c, manager)
    native.write_words(uc, 0xb499a8, manager)
    native.write_words(uc, manager+0x124c, 1)
    native.write_words(uc, player+0xd0, fields)
    events = []
    lua_value = None

    def provider(uc, address, size, context):
        nonlocal lua_value
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address in (0x53cf10, 0x5206e0):
            return_value(uc, 0)
        elif address == 0x81b530:
            event, fmt = native.read_words(uc, sp+4, 2)
            assert event in (0x98, 0x99) and fmt == 0
            events.append((event, int(native.read_words(uc, manager+0x124c, 1)[0] == 0)))
            return_value(uc, 0)
        elif address in (0x84e280, 0x84e2a0):
            lua_value = 0 if address == 0x84e280 else int(struct.unpack('<d', uc.mem_read(sp+8,8))[0])
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = []
    for flags, changed in ((0,0), (0x80000,0x80000), (0x80000,0),
                           (0,0x80000), (0,0), (0x80000,0x80000), (0,0x80000)):
        events.clear()
        native.write_words(uc, fields+0xd4, flags)
        uc.reg_write(UC_X86_REG_ECX, player)
        native.invoke(uc, 0x728f70, [0, changed])
        native.invoke(uc, 0x511cc0, [0])
        event, during = events[0] if events else (0xffffffff,0xffffffff)
        rows.append(' '.join(str(x) for x in (flags, changed, event, during, lua_value)))
    return '\n'.join(rows)+'\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    data = capture()
    Path(args.output).write_text(data)
    print(f'{len(data.splitlines())} native combat lockdown captures')


if __name__ == '__main__':
    main()
