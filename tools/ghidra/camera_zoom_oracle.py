"""Capture ordinary build-12340 wheel zoom requests and timed camera sampling.

Executes original 5FF950/5FFA60 and 6000E0 with ordinary distance CVars,
no special camera subject, and no view interpolation. Each row is a complete
ordered request/sample history; output includes both retained timer banks.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as native


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    histories = [
        [(0, 0, 1.), (2, 60, 0.), (2, 250, 0.)],
        [(1, 0, 1.), (2, 60, 0.), (2, 250, 0.)],
        [(0, 0, 1.), (0, 50, 2.), (2, 100, 0.), (2, 500, 0.)],
        [(1, 0, 2.), (0, 50, 1.), (2, 100, 0.), (2, 500, 0.)],
        [(0, 0, 2.), (2, 50, 0.), (1, 50, 1.), (2, 100, 0.), (2, 500, 0.)],
        [(0, 0, 0.), (2, 50, 0.), (2, 500, 0.)],
        [(1, 0, -1.), (2, 50, 0.), (2, 500, 0.)],
        [(0, 0, 0.001), (2, 50, 0.), (2, 500, 0.)],
        [(0, 0, 1.), (1, 0, 1.), (0, 0, 1.), (2, 500, 0.)],
        [(1, 0, 100.), (2, 500, 0.), (2, 20000, 0.)],
    ]
    lines = ['# speed maximum factor initial | (kind time amount)* | distance flags start[2] stop[2] deadline[2]; all hex float bits/u32']
    for speed, maximum, factor, initial, base_time, history in itertools.product(
        [8.33, 20.], [15., 100.], [1., 2.], [0., 5.55, 49.],
        [1000, 0xfffffff0], histories,
    ):
        uc = native.emulator()
        camera = native.HEAP
        for i, (pointer, value) in enumerate([(0xC24E58, speed), (0xC24988, maximum), (0xC2498C, factor)]):
            address = native.HEAP + 0x1000 + i * 0x100
            native.write_words(uc, pointer, address)
            native.write_floats(uc, address + 0x2c, [value])
        native.write_floats(uc, camera + 0x118, [initial])
        native.write_floats(uc, camera + 0x1e8, [initial])
        actions = []
        for kind, offset, amount in history:
            time = (base_time + offset) & 0xffffffff
            uc.reg_write(UC_X86_REG_ECX, camera)
            if kind == 2:
                native.invoke(uc, 0x6000e0, [time])
            else:
                native.invoke(uc, [0x5ff950, 0x5ffa60][kind], [bits(amount), time, 0])
            actions.extend([kind, time, bits(amount)])
        output = [*native.read_words(uc, camera + 0x118, 1),
                  *native.read_words(uc, camera + 0x160, 1),
                  *native.read_words(uc, camera + 0x164, 2),
                  *native.read_words(uc, camera + 0x17c, 2),
                  *native.read_words(uc, camera + 0x194, 2)]
        groups = [[bits(v) for v in (speed, maximum, factor, initial)], actions, output]
        lines.append(' | '.join(' '.join(f'{v:08x}' for v in group) for group in groups))
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines)-1} original zoom histories')


if __name__ == '__main__':
    main()
