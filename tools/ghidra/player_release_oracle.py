"""Run native 6DC070/513A30/516210 release timer initialization and Lua query.

Only local GUID and the millisecond clock are supplied. The original x86 code
chooses the timer mode, stores its deadline and publishes the Lua number.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, guid, player = [native.HEAP + x for x in (0, 0x2000, 0x3000)]
    native.write_words(uc, unit + 8, guid)
    native.write_words(uc, guid, 7, 0)
    native.write_words(uc, unit + 0x1008, player)
    now = 0
    output = []

    def hook(uc, address, size, data):
        if address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x86ae20:
            return_value(uc, now)
        elif address == 0x84e2a0:
            sp = uc.reg_read(UC_X86_REG_ESP)
            output.append(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0])
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, hook)
    lines = ['# Player release timer: native 6DC070/513A30/516210.']
    for flags in [0, 8, 0x10, 0x18]:
        for player_flags in [0, 0x4000]:
            for anchor in [0, 1000, (1 << 32) - 360000]:
                native.write_words(uc, player + 0x1064, flags)
                native.write_words(uc, player + 8, player_flags)
                now = anchor
                uc.reg_write(UC_X86_REG_ECX, unit)
                native.invoke(uc, 0x6dc070, [])
                deadline = native.read_words(uc, 0xbd0848, 1)[0]
                no_timer = bytes(uc.mem_read(0xbd084c, 1))[0]
                for elapsed in [0, 1, 999, 1000, 359001, 359999, 360000, 360001, 0x80000000]:
                    now = (anchor + elapsed) & 0xffffffff
                    output.clear()
                    native.invoke(uc, 0x516210, [0])
                    assert len(output) == 1, output
                    lines.append(f'release {flags:02x} {player_flags:08x} {anchor:08x} {now:08x} {deadline:08x} {no_timer} {int(output[0])}')
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    result = capture()
    Path(args.output).write_text(result, encoding='utf-8')
    print(f'Captured {len(result.splitlines()) - 1} native release timer cases')
