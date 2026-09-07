"""Capture stock 70DAA0/70DC10 transport geometry with original quaternion math.

Only the virtual current-quaternion getter is provided for zero/single rotation
rows. Both interval searches, parent transforms and shortest-arc interpolation
execute original pinned PE instructions. No map model is admitted.
"""
import argparse
from pathlib import Path
import random
import struct

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as native


def bits(values):
    return ' '.join(f'{struct.unpack("<I", struct.pack("<f", x))[0]:08x}' for x in values)


def capture(positions, rotations, parent, current, phases):
    uc = native.emulator()
    owner, go, fields, position_rows, rotation_rows, output, vtable = [native.HEAP + x for x in
                                                                   (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000)]
    native.write_words(uc, owner + 4, go)
    native.write_words(uc, go, vtable)
    native.write_words(uc, vtable + 0x44, native.STOP + 16)
    native.write_words(uc, go + 0xd0, fields)
    native.write_floats(uc, fields + 0x10, parent)
    native.write_words(uc, owner + 0x40, position_rows if positions else 0, len(positions), 0,
                       rotation_rows if rotations else 0, len(rotations), 0)
    for i, (time, position, sequence) in enumerate(positions):
        native.write_words(uc, position_rows + i * 28, i + 1, 42, time)
        native.write_floats(uc, position_rows + i * 28 + 12, position)
        native.write_words(uc, position_rows + i * 28 + 24, sequence)
    for i, (time, rotation) in enumerate(rotations):
        native.write_words(uc, rotation_rows + i * 28, i + 1, 42, time)
        native.write_floats(uc, rotation_rows + i * 28 + 12, rotation)

    def virtual_quaternion(u, address, size, data):
        if address != native.STOP + 16:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        ret, out = native.read_words(u, sp, 2)
        native.write_floats(u, out, current)
        u.reg_write(UC_X86_REG_EAX, out)
        u.reg_write(UC_X86_REG_ESP, sp + 8)
        u.reg_write(UC_X86_REG_EIP, ret)

    uc.hook_add(UC_HOOK_CODE, virtual_quaternion)

    def call(address, *arguments, receiver=owner):
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, *arguments)
        uc.reg_write(UC_X86_REG_ECX, receiver)
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.emu_start(address, native.STOP, count=100_000)
        if uc.reg_read(UC_X86_REG_EIP) != native.STOP:
            raise RuntimeError('native geometry did not return')

    lines = []
    for phase in phases:
        call(0x70daa0, output, phase)
        call(0x70dc10, output + 16, phase)
        sequence = positions[native.read_words(uc, owner + 0x48, 1)[0]][2] if len(positions) >= 2 else '-'
        words = native.read_words(uc, output, 3) + native.read_words(uc, output + 16, 4)
        packed, unpacked, matrix = output + 0x100, output + 0x120, output + 0x140
        call(0x4f43b0, output + 16, receiver=packed)
        call(0x982340, packed, receiver=unpacked)
        # 4C1E20 supplies the affine identity around the 4C1C40 basis write.
        native.write_floats(uc, matrix, [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.])
        call(0x4c1c40, unpacked, matrix)
        native.write_words(uc, matrix + 48, *words[:3])
        packed_value = struct.unpack('<Q', uc.mem_read(packed, 8))[0]
        matrix_words = native.read_words(uc, matrix, 16)
        lines.append(f'sample {phase} {sequence} ' + ' '.join(f'{x:08x}' for x in words)
                     + f' {packed_value:016x} ' + ' '.join(f'{x:08x}' for x in matrix_words))
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    rng = random.Random(12340)
    lines = ['# native 70DAA0/70DC10/4C5100/982460/4F4320; pinned Wow.exe']
    for case in range(140):
        parent = [0., 0., 0., 1.] if case % 5 == 0 else [rng.uniform(-.7, .7) for _ in range(4)]
        current = [.1, -.2, .3, .9]
        positions = [(time, [rng.uniform(-1000, 1000) for _ in range(3)], 40 + i)
                     for i, time in enumerate((0, 200, 200, 400, 1000))]
        rotations = [(time, [rng.uniform(-.7, .7) for _ in range(4)])
                     for time in (0, 300, 700)]
        if case % 7 == 0:
            rotations = [(0, [0., 0., 0., 1.]), (600, [0., 0., 0., -1.])]
        if case % 7 == 1:
            rotations = [(0, [0., 0., 0., 1.]), (600, [0., 0., 1., 0.])]
        if case % 7 == 2:
            rotations = [(0, [0., 0., 0., 1.]), (600, [0., 0., .0000001, 1.])]
        if case % 7 == 3:
            positions, rotations = positions[-1:], rotations[:1]
            if case % 3 == 0:
                current = [1024., -2048., 4096.25, -1.]
            elif case % 3 == 1:
                current = [1.e20, -1.e20, .3, -1.]
        if case % 7 == 4:
            positions, rotations = [], []
        lines.append(f'case {len(positions)} {len(rotations)} ' + bits(parent + current))
        for time, position, sequence in positions:
            lines.append(f'position {time} {sequence} ' + bits(position))
        for time, rotation in rotations:
            lines.append(f'rotation {time} ' + bits(rotation))
        lines.extend(capture(positions, rotations, parent, current,
                             (0, 1, 199, 200, 201, 299, 300, 399, 400, 599, 600, 699, 700, 899, 999, 0, 701, 201)))
    Path(args.output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('captured 140 native geometry tracks')


if __name__ == '__main__':
    main()
