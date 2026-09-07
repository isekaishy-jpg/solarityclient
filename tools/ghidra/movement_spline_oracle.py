"""Capture original 0098CA00 spline clock, position, direction and orientation.

Runs the complete original evaluator and geometry callees. No native function
is replaced. Initial-cycle allocation and final Unit_C notifications are outside
this fixture, which records the evaluator's completion-request bit directly.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_path_oracle import invoke
from movement_ground_trajectory_oracle import bits
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, spline, result = [native.HEAP + i * 0x4000 for i in range(3)]
    path = spline + 0x34
    nodes = [(-5., 2., 30.), (0., 0., 30.), (5., -3., 27.), (10., 2., 25.)]
    uc.mem_write(native.STOP + 16, b'\xd9\x1d' + struct.pack('<I', path + 4))
    rows = ['# original Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# input: flags duration scale next-scale receipt elapsed now; output: flags start elapsed scale next-scale xyz facing direction']
    for flags in [0, 0x40000, 0x4000, 0x8000000, 0x400000, 0x80000, 0xc0000, 0x800, 0x200000, 0x200]:
        for duration, scale in [(2000, 1.), (2001, .5), (0, 1.), (1000, 1.25)]:
            for offset in [-301, 0, 1, 100, 999, 1500, 2000, 8000]:
                receipt, elapsed = 0xffffff00, 300
                now = (receipt + offset) & 0xffffffff
                uc.mem_write(unit, bytes(0x400))
                uc.mem_write(spline, bytes(0x300))
                native.write_words(uc, unit + 0xbc, spline)
                native.write_floats(uc, unit + 0x10, [0., 0., 30., 0., .7])
                native.write_words(uc, unit + 0x44, 1)
                native.write_floats(uc, unit + 0x64, [.3, .4, .5])
                native.write_floats(uc, unit + 0x84, [30.])
                native.write_words(uc, spline + 0x20, flags, (receipt - elapsed) & 0xffffffff, elapsed, duration, 37)
                native.write_floats(uc, spline + 0x1f8, [5., -3., 27., scale, 1.5, 4.])
                native.write_words(uc, spline + 0x210, 250)
                native.write_words(uc, path, 0x9e2f28)
                native.write_words(uc, path + 0x144, len(nodes))
                native.write_words(uc, path + 0x1c0, int(bool(flags & 0x42000)))
                for index, point in enumerate(nodes):
                    native.write_floats(uc, path + 8 + index * 12, point)
                uc.reg_write(UC_X86_REG_ECX, path)
                invoke(uc, 0x4c41c0, [])
                uc.reg_write(UC_X86_REG_ECX, path)
                invoke(uc, 0x4c41b0, [])
                uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
                uc.reg_write(UC_X86_REG_ECX, unit)
                invoke(uc, 0x98ca00, [now, result])
                state = native.read_words(uc, spline + 0x20, 3) + native.read_words(uc, spline + 0x204, 2)
                values = native.read_words(uc, result, 3) + native.read_words(uc, unit + 0x20, 1) + native.read_words(uc, unit + 0x64, 3)
                inputs = [flags, duration, bits(scale), bits(1.5), receipt, elapsed, now]
                rows.append(' '.join(f'{value:08x}' for value in [*inputs, *state, *values]))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} complete native spline evaluations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
