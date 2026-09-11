"""Capture ordinary non-local Unit_C effective weapon state from build 12340.

Runs original 738180 and 736D30 with original metadata/readiness helpers.
The body engine provides a chosen AnimationData row; completed state changes
intercept the component relocation methods. Spell providers, forced poses, and
local-player behavior are outside this capture.
"""
import argparse
import itertools
from pathlib import Path

import wmo_registration_oracle as n
from movement_ground_trajectory_oracle import invoke
from npc_virtual_items_oracle import ITEMS
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX,
    UC_X86_REG_ESP, UC_X86_REG_EIP,
)


def capture(executable, output):
    n.initialize(executable)
    u = n.emulator()
    unit, vt, fields, body, template, object_data, anim_table, anim = [
        n.HEAP + i * 0x4000 for i in range(8)]
    current_animation = 1000

    def returned(value=0, pop=0):
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def hook(_u, address, _size, _data):
        if address == 0x824f00:
            returned(1, 8)
        elif address == 0x8267e0:
            returned(current_animation, 4)
        elif address == 0x826a60:
            returned(0, 4)
        elif address == 0x4d3790:
            u.reg_write(UC_X86_REG_EDX, 0)
            returned()
        elif address == 0x4cee50:
            returned()
        elif address == 0x720400:
            returned(0, 8)
        elif address == 0x726090:
            returned(0, 4)
        elif address in (0x731f40, 0x736b60):
            returned()

    u.hook_add(UC_HOOK_CODE, hook)
    n.write_words(u, unit, vt)
    n.write_words(u, unit + 8, object_data)
    n.write_words(u, object_data, 1, 0, 0)
    n.write_words(u, vt + 0x12c, 0x71f440, 0x71f540, 0x718b10)
    n.write_words(u, unit + 0xd0, fields)
    n.write_words(u, unit + 0xb4, body)
    n.write_words(u, unit + 0x964, template)
    n.write_words(u, unit + 0xb84, 0xffffffff)
    n.write_words(u, 0xad30d8, 0)
    n.write_words(u, 0xad30d4, 1200)
    n.write_words(u, 0xad30e8, anim_table)
    lines = [
        '# Original 738180/736D30; ordinary non-local, non-casting, forcePose=false.',
        *('item ' + ' '.join(map(str, item)) for item in ITEMS),
        '# case current raw animation behavior weaponFlags attack templateFlags primary secondary main off; effective',
    ]
    bodies = [(1000, b) for b in (0, 16, 25, 46, 97)]
    bodies += [(a, 0) for a in (105, 106, 112, 0xffffffff)]
    for current, raw, (animation, behavior), flags, attack, freeze, disarm, entries in itertools.product(
            range(3), range(3), bodies, (0, 4, 16, 32), range(2),
            (0, 0x10000000), ((0, 0), (0x200000, 0), (0, 0x80), (0x200000, 0x80)),
            ((100, 101), (0, 103))):
        current_animation = animation
        if animation != 0xffffffff:
            n.write_words(u, anim_table + animation * 4, anim)
        n.write_words(u, anim, animation, 0, flags, 0, 0, 0, behavior, 0)
        n.write_words(u, unit + 0xb58, current, current)
        n.write_words(u, unit + 0xa20, attack, 0)
        n.write_words(u, fields + 0x1d0, raw)
        n.write_words(u, fields + 0xd4, *disarm)
        n.write_words(u, template + 12, freeze)
        for slot, entry in enumerate(entries):
            row = next((item for item in ITEMS if item[0] == entry), None)
            n.write_words(u, unit + 0x998 + slot * 4, 0 if row is None else row[5])
            u.mem_write(unit + 0x9a4 + slot * 8, bytes(8) if row is None else
                        bytes([row[1], row[2], 255, row[4], row[6], row[7], 0, 0]))
        u.reg_write(UC_X86_REG_ECX, unit)
        invoke(u, 0x738180, [0])
        inputs = [current, raw, animation, behavior, flags, attack, freeze, *disarm, *entries]
        lines.append('case ' + ' '.join(map(str, inputs)) + ';' +
                     str(n.read_words(u, unit + 0xb5c, 1)[0]))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', sum(line.startswith('case ') for line in lines), 'native NPC weapon states')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
