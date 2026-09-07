"""Capture 730D10 immersion decisions with original 77F1E0/4F53D0/986EA0.

Only virtual position/GUID/local-control accessors and downstream notifications
are intercepted. The original query, thresholds, eligibility, jump suppression,
animation mask, and splash crossings execute without substitution.
"""
import argparse
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EDX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, fields, registration, vtable, guid, point = [native.HEAP + i * 0x2000 for i in range(6)]
    position_getter, guid_getter = native.HEAP + 0xc000, native.HEAP + 0xc010
    movement = unit + 0x788
    records = bytearray()
    cases = list(itertools.product(
        [0, 0x200000, 0x600000, 0x2200000, 0x1000],
        [0, 8, 0x10, 0x800, 0x8000, 0x4008],
        [None, 0., .79, .81, 1.47, 1.48, 1.5, 1.51, 1.83, 1.84, 3.],
        [0, 1], [0, 1]))
    # Exercise float-store boundaries and upward-launch suppression separately.
    cases += [(0x1000, 8, 3., 0, 1)] * 8
    for index, (flags, unit_flags, depth, blocker, local) in enumerate(cases):
        secondary = 4 if index % 37 == 0 else 0
        height = 2.
        fall_time, downward = (100, -7.955547) if flags & 0x1000 else (0, 0.)
        if index >= len(cases) - 8:
            fall_time = [0, 1, 250, 412, 413, 414, 415, 1000][index - len(cases) + 8]
        previous = 1. if index % 2 else 0.
        native.write_words(uc, unit, vtable)
        native.write_words(uc, unit + 8, guid)
        native.write_words(uc, guid, 1, 0)
        native.write_words(uc, unit + 0xd0, fields, 0, movement)
        native.write_words(uc, fields + 0xd4, unit_flags)
        native.write_words(uc, unit + 0xb8, registration)
        native.write_words(uc, registration + 0x7c, 0x20 if depth is not None else 0)
        native.write_floats(uc, registration + 0x80, [depth or 0.])
        native.write_words(uc, registration + 0xbc, 1)
        native.write_floats(uc, point, [0., 0., 0.])
        native.write_words(uc, vtable + 0x2c, position_getter)
        native.write_words(uc, vtable + 0x40, guid_getter)
        native.write_words(uc, movement + 0x44, flags, secondary)
        native.write_words(uc, movement + 0x80, fall_time)
        native.write_floats(uc, movement + 0xb8, [downward])
        native.write_floats(uc, unit + 0x854, [height])
        native.write_floats(uc, unit + 0x784, [previous])
        native.write_words(uc, unit + 0xa30, 0)
        seen = [0, 0, 0, 0]
        def hook(uc, address, size, data):
            cleanup, result, high = 0, 0, 0
            if address == position_getter:
                cleanup, result = 4, point
            elif address == guid_getter:
                result = blocker
            elif address == 0x4d43c0:
                result = local
            elif address == 0x4d3790:
                result = 2
            elif address in (0x721210, 0x721290):
                seen[address == 0x721290] += 1
                cleanup = 8
            elif address == 0x72eb80:
                seen[2] += 1
                cleanup = 4
            elif address in (0x746720, 0x71cba0):
                seen[3] += address == 0x746720
                cleanup = 4
            else:
                return
            sp = uc.reg_read(UC_X86_REG_ESP)
            uc.reg_write(UC_X86_REG_EAX, result)
            uc.reg_write(UC_X86_REG_EDX, high)
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + cleanup)
            uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, sp, 1)[0])
        handles = [uc.hook_add(UC_HOOK_CODE, hook, begin=a, end=a) for a in
            [position_getter, guid_getter, 0x4d43c0, 0x4d3790, 0x721210, 0x721290, 0x72eb80, 0x746720, 0x71cba0]]
        uc.reg_write(UC_X86_REG_ECX, unit)
        try:
            native.invoke(uc, 0x730d10, [12345, 0])
        except Exception as error:
            raise RuntimeError((index, hex(uc.reg_read(UC_X86_REG_EIP)))) from error
        finally:
            for handle in handles:
                uc.hook_del(handle)
        record = [flags, secondary, unit_flags, blocker, local, bits(height), int(depth is not None),
            bits(depth or 0.), bits(previous), fall_time, bits(downward), *seen,
            native.read_words(uc, unit + 0xa30, 1)[0], native.read_words(uc, unit + 0x784, 1)[0]]
        records.extend(struct.pack('<17I', *record))
    Path(output).write_bytes(records)
    print(f'Captured {len(cases)} original immersion decisions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
