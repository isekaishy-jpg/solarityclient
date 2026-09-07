"""Execute build-12340 AreaTrigger containment with its original matrix callees."""

import argparse
import math
from pathlib import Path
import struct

from unicorn.x86_const import UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_EAX

import wmo_registration_oracle as native


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    record, point = native.HEAP, native.HEAP + 0x100
    bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    rows = ['# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# 6CE140 with original 4C1B30/4C3380/4C2FC0/4C21B0/6CB930',
            '# map row-map center-xyz radius dimensions-xyz angle point-xyz -> inside; hex f32 words']
    shapes = []
    for center in ((0., 0., 0.), (1811.78, -4410.5, -18.55)):
        for radius in (0., 1., 4.):
            for angle in (0., .37, math.pi / 2):
                shapes.append((*center, radius, 8., 2., 6., angle))
    for shape in shapes:
        center, radius, dimensions, angle = shape[:3], shape[3], shape[4:7], shape[7]
        points = [center, (center[0] + 20., center[1], center[2])]
        for axis in range(3):
            boundary = radius if radius else dimensions[axis] * .5
            for sign in (-1., 1.):
                for offset in (-.0001, 0., .0001):
                    delta = [0., 0., 0.]
                    delta[axis] = sign * (boundary + offset)
                    if not radius:
                        x, y = delta[:2]
                        delta[0] = x * math.cos(angle) - y * math.sin(angle)
                        delta[1] = x * math.sin(angle) + y * math.cos(angle)
                    points.append(tuple(a + b for a, b in zip(center, delta)))
        for position in points:
            for map_id in (1, 2):
                native.write_words(uc, 0xc9d338, map_id)
                native.write_words(uc, record, 123, 1, *map(bits, shape))
                native.write_floats(uc, point, position)
                uc.reg_write(UC_X86_REG_ESI, record)
                uc.reg_write(UC_X86_REG_EDI, point)
                native.invoke(uc, 0x6ce140, [])
                inside = uc.reg_read(UC_X86_REG_EAX) & 255
                inputs = (map_id, 1, *map(bits, shape), *map(bits, position))
                rows.append(' '.join(f'{value:08x}' for value in inputs) + f' {inside}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 3} native AreaTrigger containment cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
