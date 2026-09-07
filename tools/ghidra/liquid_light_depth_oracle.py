"""Capture original 7F3230 depth clamp and 7ED790 packed HSV darkening.

Only environment composition at 7EE750 is replaced with a controlled packed
color; stop at 7F0530 after the three depth-scaled colors have been written.
The actual query, LiquidType lookup, depth math and color callees run unchanged.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    row, pointers = native.HEAP, native.HEAP + 0x200
    native.write_words(uc, 0xad4070, 1, 1)
    native.write_words(uc, 0xad4084, pointers)
    native.write_words(uc, pointers, row)
    native.write_words(uc, 0xcd8794, 1)
    native.write_words(uc, 0xd39008, 0)
    colors = [0xd38b8c, 0xd38cac, 0xd38ca8]
    current = 0

    def composition(uc, address, size, data):
        if address == 0x7ee750:
            for destination in colors:
                native.write_words(uc, destination, current)
            stack = uc.reg_read(UC_X86_REG_ESP)
            uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, stack, 1)[0])
            uc.reg_write(UC_X86_REG_ESP, stack + 4)
        elif address == 0x7f0530:
            uc.reg_write(UC_X86_REG_EIP, native.STOP)

    uc.hook_add(UC_HOOK_CODE, composition)
    randomizer = random.Random(12340)
    records = bytearray()
    for color in [0, 0xffffff, 0x123456, 0xff00ff, 0xffff00, 0x00ffff,
                  0x808080, 0x010203] + [randomizer.randrange(0x1000000) for _ in range(120)]:
        for depth in [-1., 0., .125, 1., 5., 10., 25.]:
            for maximum, factors in [(0., [.5, .75, 1.]), (10., [.5, .75, 1.]),
                                     (7., [.2, .6, .95]), (10., [0., 0., 0.])]:
                current = color | 0xff000000
                native.write_floats(uc, row + 0x18, [maximum, *factors])
                native.write_floats(uc, 0xcd8790, [-depth])
                invoke(uc, 0x7f3230, [])
                values = [color, bits(depth), bits(maximum), *map(bits, factors)]
                values.extend(native.read_words(uc, address, 1)[0] & 0xffffff for address in colors)
                records.extend(struct.pack('<9I', *values))
    Path(output).write_bytes(records)
    print(f'Captured {len(records) // 36} original liquid depth samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
