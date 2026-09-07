"""Capture unmodified build-12340 swimming basis, speed, pitch, and trajectories.

Original 9880C0, 987EF0, and 987B50 execute without replacing any callee.
No client entry point or window is started. Requires Unicorn and the pinned exe.
"""
import argparse
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, result = native.HEAP, native.HEAP + 0x1000
    speeds = [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, 3.1415927410125732, 3.1415927410125732]
    records = bytearray()
    cases = itertools.product(
        [0, 1, 2, 4, 8, 5, 9, 6, 10],
        [0, 0x10, 0x20], [0, 0x40, 0x80], [0, 0x400000, 0x800000],
        [0, 0x18], [(0., 0.), (.7, .5), (-1.2, -.9)],
        [0, 1, 17, 250, 10001],
    )
    for translation, turn, pitch_input, vertical, secondary, (facing, pitch), elapsed in cases:
        flags = 0x200000 | translation | turn | pitch_input | vertical
        uc.mem_write(unit, bytes(0x400))
        native.write_words(uc, unit + 0x44, flags, secondary)
        native.write_floats(uc, unit + 0x58, [facing, pitch])
        native.write_floats(uc, unit + 0x90, speeds)
        for address in [0x9880c0, 0x987ef0]:
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, address, [0])
        native.write_floats(uc, result, [0., 0., 0., facing, pitch])
        uc.reg_write(UC_X86_REG_ECX, unit)
        invoke(uc, 0x987b50, [elapsed, result, result + 12, result + 16])
        records.extend(struct.pack('<16I', flags, secondary, bits(facing), bits(pitch), elapsed,
            *native.read_words(uc, unit + 0x64, 5),
            *native.read_words(uc, unit + 0x8c, 1), *native.read_words(uc, result, 5)))
    Path(output).write_bytes(records)
    print(f'Captured {len(records) // 64} original swimming trajectories')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
