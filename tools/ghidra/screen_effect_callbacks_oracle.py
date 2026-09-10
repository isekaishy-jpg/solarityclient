"""Capture screen-effect callbacks after native aura packet installation.

The original readers, old/new aura comparison, visual admission, local-player
selector and visibility-mask helper execute. Non-screen visual notifications,
model cleanup and trailing UI broadcasts are supplied boundaries. The retained
visual spell bank is resident and eight aura slots are preallocated.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE, UcError
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ECX,
                               UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESI,
                               UC_X86_REG_ESP)
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from unit_aura_oracle import packed


def capture():
    u = n.emulator()
    unit, descriptor, fields, slots, packet, payload, bank, records, visuals = [
        n.HEAP + i * 0x3000 for i in range(9)]
    n.write_words(u, unit + 8, descriptor)
    n.write_words(u, descriptor, 7, 0, 0x19)
    n.write_words(u, unit + 0x1008, fields)
    n.write_words(u, unit + 0xc54, 8, slots)
    n.write_words(u, unit + 0xdd0, 0xffffffff)
    n.write_words(u, unit + 0xee0, 8, visuals)
    n.write_words(u, unit + 0xf1c, 0xffffffff)
    n.write_words(u, 0xad49e0, 100)
    n.write_words(u, 0xad49dc, 106)
    n.write_words(u, 0xad49f0, bank)
    for index, (types, misc, priority, visibility) in enumerate([
            ([0, 0, 0], [0, 0, 0], 0, 0),
            ([260, 0, 0], [81, 0, 0], 0, 0),
            ([260, 260, 0], [141, 242, 0], 0, 0),
            ([0, 0, 260], [0, 0, 0], 0, 0),
            ([19, 260, 0], [0, 141, 0], 0, 0),
            ([260, 0, 0], [242, 0, 0], 5, 0),
            ([260, 0, 0], [342, 0, 0], 0, 3)]):
        record = records + index * 0x2a8
        n.write_words(u, bank + index * 4, record)
        n.write_words(u, record, 100 + index)
        n.write_words(u, record + 0x17c, *types)
        n.write_words(u, record + 0x1b8, *misc)
        n.write_words(u, record + 0x21c, priority)
        n.write_words(u, record + 0x274, visibility)
    u.mem_write(0xc5dea0, b'\0')
    selected = []
    done = False

    def hook(u, address, size, context):
        nonlocal done
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x4d3790:
            u.reg_write(UC_X86_REG_EDX, 0)
            return_value(u, 7)
        elif address == 0x4d4db0:
            guid = n.read_words(u, sp + 4, 2)
            return_value(u, unit if guid == (7, 0) else 0)
        elif address == 0x86ae20:
            return_value(u, 1000)
        elif address in (0x8b7da0, 0x5eeb70, 0x7fef10, 0x5aab10, 0x53bd10,
                         0x53bd40, 0x752710, 0x752860, 0x752ba0, 0x6143f0):
            return_value(u, 0)
        elif address == 0x4cfd20:
            spell, destination = n.read_words(u, sp + 4, 2)
            found = 100 <= spell <= 106
            if found:
                u.mem_write(destination, bytes(u.mem_read(records + (spell - 100) * 0x2a8, 0x2a8)))
            return_value(u, int(found))
            u.reg_write(UC_X86_REG_ESP, sp + 12)
        elif address == 0x4f7020:
            selected.append(n.read_words(u, sp + 4, 1)[0])
            return_value(u, 0)
        elif address == 0x71ea41:
            # The remaining cleanup retires models and clears the visual spell ID.
            index = n.read_words(u, u.reg_read(UC_X86_REG_EBP) + 8, 1)[0]
            n.write_words(u, visuals + index * 4, 0)
            u.reg_write(UC_X86_REG_EIP, 0x71ec76)
        elif address == 0x724a2d:
            # Admission already installed the retained visual ID and callbacks.
            u.reg_write(UC_X86_REG_EIP, 0x724c9f)
        elif address == 0x72fb50:
            done = True
            u.emu_stop()

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# 72F5D0/71E930/724820: old-spell old-flags visual-spell ghost bytes-high arena packet-hex callback-count selected-IDs... final-visual-ID.']
    changes = [[], [(3, 0, 0)], [(3, 100, 9)], [(3, 101, 9)], [(3, 101, 8)],
               [(3, 102, 9)], [(3, 102, 8)], [(3, 103, 9)], [(3, 104, 9)],
               [(3, 105, 9)], [(3, 106, 9)], [(3, 999, 9)],
               [(3, 0, 0), (3, 101, 9)], [(4, 102, 8), (3, 101, 9)]]
    for old, old_flags, visual, replace, changeset, high in itertools.product(
            [0, 101, 102, 105], [8, 9], [0, 101, 105], [0, 1], changes, [0, 0x44]):
        u.mem_write(slots, bytes(8 * 24))
        u.mem_write(visuals, bytes(8 * 4))
        n.write_words(u, slots + 3 * 24 + 8, old, old_flags)
        n.write_words(u, visuals + 3 * 4, visual)
        n.write_words(u, fields + 8, 0x10)
        n.write_words(u, fields + 0x10e4, high << 24)
        n.write_words(u, 0xbea570, 0)
        body = packed(7)
        for index, spell, flags in changeset:
            body += struct.pack('<BI', index, spell)
            if spell:
                body += bytes([flags, 80, 1])
        u.mem_write(payload, body)
        n.write_words(u, packet, 0x9e0e24, payload, 0, len(body), len(body), 0)
        sp = n.STACK + 0x18000
        n.write_words(u, sp, n.STOP, 0, 0x495 if replace else 0x496, 0, packet)
        u.reg_write(UC_X86_REG_ESP, sp)
        selected.clear()
        done = False
        try:
            u.emu_start(0x7300a0, n.STOP, count=500000)
        except UcError:
            print('failed at', hex(u.reg_read(UC_X86_REG_EIP)), old, old_flags, visual, replace, changeset, high)
            raise
        assert done, hex(u.reg_read(UC_X86_REG_EIP))
        final_visual = n.read_words(u, visuals + 12, 1)[0]
        rows.append('callback ' + ' '.join(map(str, [old, old_flags, visual, 1, high, 0,
            0x495 if replace else 0x496, body.hex(), len(selected), *selected, final_visual])))
    rows.append('# 727A70: spell flags visual-spell changed-byte current-byte callback-count selected-IDs... final-visual-ID.')
    for spell, flags, visual, changed, high in itertools.product(
            [101, 102, 104, 106, 999], [8, 9], [0, 101, 105, 106],
            [4, 0x40, 0x44, 0x80], [0, 4, 0x40, 0x44]):
        u.mem_write(slots, bytes(8 * 24))
        u.mem_write(visuals, bytes(8 * 4))
        n.write_words(u, slots + 3 * 24 + 8, spell, flags)
        n.write_words(u, visuals + 12, visual)
        n.write_words(u, fields + 8, 0x10)
        n.write_words(u, fields + 0x10e4, high << 24)
        n.write_words(u, 0xbea570, 0)
        selected.clear()
        u.reg_write(UC_X86_REG_ECX, unit)
        n.invoke(u, 0x727a70, [changed])
        final_visual = n.read_words(u, visuals + 12, 1)[0]
        rows.append('vision ' + ' '.join(map(str, [spell, flags, visual, changed,
            high, len(selected), *selected, final_visual])))
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {sum(not row.startswith("#") for row in rows)} callback cases')
