"""Capture recursive M2 render bounds from the pinned build-12340 8254F0.

Five resident model headers form a root with two children, each with one child.
The full native recursion, extent expansion and unions execute without hooks.
Each 144-byte record contains five header boxes followed by the resulting box.
"""

import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as n
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    n.initialize(executable)
    u = n.emulator()
    models = [n.HEAP + index * 0x1000 for index in range(5)]
    for model in models:
        n.write_words(u, model + 0x10, 1)
        n.write_words(u, model + 0x2c, model + 0x400)
        n.write_words(u, model + 0x550, model + 0x800)
    for parent, child in [(0, 1), (1, 3), (2, 4)]:
        n.write_words(u, models[parent] + 0x58, models[child])
    n.write_words(u, models[1] + 0x60, models[2])
    destination = n.HEAP + 0x8000
    rng = random.Random(12340)
    result = bytearray()
    for case in range(256):
        for index, model in enumerate(models):
            minimum = [rng.uniform(-100, 100) for _ in range(3)]
            maximum = [value + rng.uniform(0, 30) for value in minimum]
            if case < 16 and index != 0:
                for axis in range(3):
                    if case & (1 << axis):
                        maximum[axis] = minimum[axis] - 1
                    elif case & 8:
                        maximum[axis] = minimum[axis]
            raw = struct.pack('<6f', *minimum, *maximum)
            u.mem_write(model + 0x8a0, raw)
            result.extend(raw)
        u.reg_write(UC_X86_REG_ECX, models[0])
        n.invoke(u, 0x8254f0, [destination])
        result.extend(u.mem_read(destination, 24))
    Path(output).write_bytes(result)
    print(f'Captured {len(result) // 144} recursive render boxes')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
