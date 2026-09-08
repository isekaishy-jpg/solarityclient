"""Capture original aura reader, slot replacement and resurrection admission.

7300A0 and 72F5D0 execute through 727E70's rebuilt aura-type bitmap, stopping
before visual/spell side effects. The full 256-slot resident bank avoids native
allocation. Object lookup, clock, spell scratch lifetime and localized error
formatting are supplied boundaries. 716510 and its wire readers are original.
727860 runs original admission against resident equivalent Spell records.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP, UC_X86_REG_EAX
import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def packed(guid):
    raw = guid.to_bytes(8, 'little')
    return bytes([sum(1 << i for i, b in enumerate(raw) if b)]) + bytes(b for b in raw if b)


def capture():
    uc = native.emulator()
    unit, descriptor, player, slots, packet, payload, bank, records = [native.HEAP + i * 0x3000 for i in range(8)]
    native.write_words(uc, unit + 8, descriptor)
    native.write_words(uc, descriptor, 7, 0, 0x19)
    native.write_words(uc, unit + 0x1008, player)
    native.write_words(uc, unit + 0xc54, 256, slots)
    native.write_words(uc, unit + 0xdd0, 0xffffffff)
    native.write_words(uc, 0xad49e0, 100)
    native.write_words(uc, 0xad49dc, 104)
    native.write_words(uc, 0xad49f0, bank)
    for index in range(5):
        record = records + index * 0x2a8
        native.write_words(uc, bank + index * 4, record)
        if index in [1, 2, 3]:
            native.write_words(uc, record + 0x17c + (index - 1) * 4, 314)
        if index == 4:
            native.write_words(uc, record + 0x2c, 0x08000000)
    uc.mem_write(0xc5dea0, b'\0')
    now, checkpoint, errors = 1000, False, []

    def hook(uc, address, size, context):
        nonlocal checkpoint
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x4d4db0:
            guid = native.read_words(uc, sp + 4, 2)
            return_value(uc, unit if guid == (7, 0) else 0)
        elif address == 0x86ae20:
            return_value(uc, now)
        elif address in [0x8b7da0, 0x5eeb70]:
            return_value(uc, 0)
        elif address == 0x4cfd20:
            spell, destination = native.read_words(uc, sp + 4, 2)
            found = 100 <= spell <= 104
            if found:
                uc.mem_write(destination, bytes(uc.mem_read(records + (spell - 100) * 0x2a8, 0x2a8)))
            return_value(uc, int(found))
            uc.reg_write(UC_X86_REG_ESP, sp + 12)
        elif address == 0x7fef10:
            errors.append(native.read_words(uc, sp + 12, 1)[0])
            return_value(uc, 0)
        elif address == 0x72f72f:
            checkpoint = True
            uc.emu_stop()

    uc.hook_add(UC_HOOK_CODE, hook)
    rows = ['# opcode now body slot spell flags level stacks caster duration end blocked selfSpell allowed']
    cases = [
        (0x495, 1000, [(3, 101, 9, 80, 1, 7, None)]),
        (0x496, 1001, [(4, 102, 9, 80, 2, 7, None)]),
        (0x496, 1002, [(3, 0, 0, 0, 0, 0, None)]),
        (0x496, 1003, [(4, 102, 10, 80, 3, 7, None)]),
        (0x496, 1004, [(255, 103, 0x24, 70, 4, 0xf130123456789abc, (5000, 2000))]),
        (0x496, 0xfffffff0, [(255, 103, 0x2c, 70, 1, 7, (100, 16))]),
        (0x495, 2000, []),
        (0x496, 2001, [(0, 999, 0xff, 80, 255, 7, (0xffffffff, 0xffffffff))]),
        (0x496, 2002, [(1, 101, 9, 80, 1, 7, None), (1, 100, 9, 80, 1, 7, None)]),
        (0x495, 2003, [(2, 101, 8, 80, 1, 7, None)]),
    ]
    for opcode, now, changes in cases:
        body = packed(7)
        for slot, spell, flags, level, stacks, caster, duration in changes:
            body += struct.pack('<BI', slot, spell)
            if spell:
                body += bytes([flags, level, stacks])
                if not flags & 8:
                    body += packed(caster)
                if flags & 0x20:
                    body += struct.pack('<II', *duration)
        uc.mem_write(payload, body)
        native.write_words(uc, packet, 0x9e0e24, payload, 0, len(body), len(body), 0)
        checkpoint = False
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, 0, opcode, 0, packet)
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.emu_start(0x7300a0, native.STOP, count=100000)
        assert checkpoint
        assert native.read_words(uc, packet + 20, 1)[0] == len(body)
        blocked = int(bool(uc.mem_read(unit + 0xf47, 1)[0] & 4))
        for bypass in [0, 104]:
            native.write_words(uc, player + 0x106c, bypass)
            uc.reg_write(UC_X86_REG_ECX, unit)
            native.invoke(uc, 0x727860, [])
            allowed = uc.reg_read(UC_X86_REG_EAX) & 255
            for slot in [0, 1, 2, 3, 4, 255]:
                data = bytes(uc.mem_read(slots + slot * 24, 24))
                caster, spell, flags, level, stacks, _, duration, end = struct.unpack('<QIBBBBII', data)
                rows.append(f'{opcode:04x} {now} {body.hex()} {slot} {spell} {flags} {level} {stacks} {caster:016x} {duration} {end} {blocked} {bypass} {allowed}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} aura slot/admission samples')
