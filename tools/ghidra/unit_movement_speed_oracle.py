"""Capture original nonspline movement speed, locomotion, and stride timing.

987570 and 714E80 execute completely. 717050 supplies only the unrelated
special-animation predicate (false). 7388B4..73898F executes the full speed /
phase block with resident new metadata and supplied old primary information;
model accessors are intercepted, but all timing decisions/math run natively.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_EBP, UC_X86_REG_ESI,
    UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
)


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, result, vtable, model_accessor, model_info = [native.HEAP + i * 0x4000 for i in range(5)]
    movement, frame = unit + 0x788, native.STACK + 0x10000
    old_speed, old_duration, old_phase = 0., 0, 0

    def dependencies(u, address, _size, _data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x4f5290:
            value, pop = 0, 0
        elif address == model_accessor:
            value, pop = model_info, 0
        elif address == 0x8266b0:
            _, destination = native.read_words(u, sp + 4, 2)
            native.write_words(u, destination, 5, 0, old_phase)
            value, pop = 1, 8
        elif address == 0x82ced0:
            _, _, destination = native.read_words(u, sp + 4, 3)
            native.write_words(u, destination, 0, old_duration, bits(old_speed))
            value, pop = 1, 12
        elif address == 0x52e570:
            value, pop = u.reg_read(UC_X86_REG_ECX), 0
        else:
            return
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 0xd0, model_info)
    native.write_words(uc, vtable + 0xd4, model_accessor)
    native.write_words(uc, unit + 0xa38, 0x40)
    # FSTP captures and pops the actual x87 GetSpeed result after its return.
    uc.mem_write(native.STOP + 16, b'\xd9\x1d' + struct.pack('<I', result))
    rows = ['# speed/select flags nine-speed-bits -> result bits / animation']
    profiles = [
        [2.5, 7., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.],
        [8., 5., 9., 2., 8., 3., 9., 3., 3.],
        [2.5, 5., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.],
        [2.5, 5.000000476837158, 4.5, 4.72, 2.5, 7., 4.5, 3., 3.],
        [2.5, 11., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.],
        [2.5, 11.000000953674316, 4.5, 4.72, 2.5, 7., 4.5, 3., 3.],
        [0., 0., 0., 0., 0., 0., 0., 3., 3.],
    ]
    for speeds in profiles:
        native.write_floats(uc, movement + 0x90, speeds)
        for mode in [0, 0x100, 0x200000, 0x2000000, 0x2200100]:
            for direction in [0, 1, 2, 4, 8, 5, 9, 6, 10, 0x10, 0x400000, 0x800000]:
                flags = mode | direction
                native.write_words(uc, movement + 0x44, flags)
                uc.reg_write(UC_X86_REG_ECX, movement)
                invoke(uc, 0x987570, [0])
                uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
                prefix = f'{flags:x} ' + ' '.join(f'{bits(v):08x}' for v in speeds)
                rows.append(f'speed {prefix} {native.read_words(uc, result, 1)[0]:08x}')
                if direction & 0xf:
                    native.write_words(uc, result, 0xffffffff)
                    uc.reg_write(UC_X86_REG_ECX, unit)
                    invoke(uc, 0x717050, [0xffffffff, result])
                    rows.append(f'select {prefix} {native.read_words(uc, result, 1)[0]}')
    rows.append('# timing animation flags actual-speed authored-speed duration old-speed old-duration old-phase -> rate offset (hex words except IDs/durations)')
    for animation in [0, 4, 5, 11, 12, 13, 37, 38, 39, 40, 41, 42, 43, 44, 45, 119, 135, 143, 187, 223, 234]:
        for flags in [0, 1, 0x10, 0x400000]:
            for speed, authored, duration, old_speed, old_duration, old_phase in [
                (7., 7., 1000, 7., 997, 333),
                (7., -4.7, 997, 2.5, 1200, 1751),
                (0., 7., 1000, 7., 997, 333),
                (7., 0., 1000, 7., 997, 333),
                (7., 7., 0, 7., 997, 333),
                (7., 7., 1000, 0., 997, 333),
                (7., 7., 1000, 7., 0, 333),
                (7., 7., 997, 7., 1200, 0xffffffff),
                (7., 7., 0xffffffff, 7., 997, 0x87654321),
            ]:
                native.write_words(uc, movement + 0x44, flags)
                native.write_floats(uc, movement + 0x90, [speed] * 9)
                native.write_words(uc, frame - 0x168, duration, bits(authored))
                native.write_words(uc, frame - 0x14, bits(1.))
                native.write_words(uc, frame - 0x1c, 0)
                uc.reg_write(UC_X86_REG_EBP, frame)
                uc.reg_write(UC_X86_REG_EBX, unit)
                uc.reg_write(UC_X86_REG_ESI, animation)
                uc.reg_write(UC_X86_REG_ESP, frame - 0x1000)
                uc.emu_start(0x7388b4, 0x73898f, count=10000)
                assert uc.reg_read(UC_X86_REG_EIP) == 0x73898f
                rate = native.read_words(uc, frame - 0x14, 1)[0]
                offset = native.read_words(uc, frame - 0x1c, 1)[0]
                actual = speed if flags & 0xc0000f else 0.
                rows.append(f'timing {animation} {flags:x} {bits(actual):08x} {bits(authored):08x} {duration} {bits(old_speed):08x} {old_duration} {old_phase:x} {rate:08x} {offset:x}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {sum(not row.startswith("#") for row in rows)} original unit speed/stride cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
