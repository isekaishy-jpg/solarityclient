"""Execute 79CA70's original billboard loop, 79CBEE..79CD81.

Supplies the already acquired vertex/index buffers, camera rotation and liquid
row. All particle admission, atlas selection, quad vertices, indices and the
666-quad limit execute original instructions without hooks.
"""
import argparse
import math
import random
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_ESI, UC_X86_REG_EBX, UC_X86_REG_EIP
import wmo_registration_oracle as n


def capture(points, matrix, pattern):
    uc = n.emulator()
    pool, vertices, indices, row = [n.HEAP + offset for offset in [0, 0x20000, 0x30000, 0x34000]]
    frame = n.STACK + 0x18000
    n.write_floats(uc, pool, sum(points, []))
    n.write_words(uc, pool + 0xfa00, len(points))
    n.write_floats(uc, frame - 0x70, matrix)
    n.write_words(uc, frame - 0x28, pool)
    n.write_words(uc, frame - 0xc, indices)
    n.write_words(uc, frame - 8, row)
    n.write_words(uc, row + 0x34, pattern)
    uc.reg_write(UC_X86_REG_EBP, frame)
    uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
    uc.reg_write(UC_X86_REG_ESI, vertices)
    uc.reg_write(UC_X86_REG_EBX, pool)
    uc.emu_start(0x79cbee, 0x79cd81, count=2_000_000)
    assert uc.reg_read(UC_X86_REG_EIP) == 0x79cd81
    count = n.read_words(uc, frame - 4, 1)[0]
    return count, bytes(uc.mem_read(vertices, count * 24)), bytes(uc.mem_read(indices, count // 4 * 12))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    edge = [[0., 0., 1., .1], [0., 0., 0., .1], [0., 0., -1., .1],
            [1., 0., 1., .1], [-1., 0., 1., .1], [0., 1., 1., .1],
            [0., -1., 1., .1], [.99999994, 0., 1., .1]]
    cases = []
    rng = random.Random(12340)
    for pattern in range(5):
        cases.append((pattern, identity, edge))
        cases.append((pattern, identity, [[0., 0., 1., .1]] * 24))
        for angle in [0., .25, 1., 2.]:
            c, s = math.cos(angle), math.sin(angle)
            matrix = [c, s, 0., 0., -s, c, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
            points = [[rng.uniform(-20., 20.), rng.uniform(-20., 20.), rng.uniform(-5., 20.), rng.uniform(.01, .5)] for _ in range(80)]
            cases.append((pattern, matrix, points))
            # A tilted camera couples every world coordinate into view-space
            # admission and preserves the native row-specific x87 sum order.
            tilt_c, tilt_s = math.cos(angle + .43), math.sin(angle + .43)
            tilted = [c, s * tilt_c, s * tilt_s, 0.,
                      -s, c * tilt_c, c * tilt_s, 0.,
                      0., -tilt_s, tilt_c, 0., 0., 0., 0., 1.]
            cases.append((pattern, tilted, points))
    cases.extend([(0, identity, []), (4, identity, [[0., 0., 1., .1]] * 4000)])
    result = bytearray(struct.pack('<I', len(cases)))
    for pattern, matrix, points in cases:
        count, vertices, indices = capture(points, matrix, pattern)
        result += struct.pack('<III16f', pattern, len(points), count, *matrix)
        result += struct.pack('<' + 'f' * (len(points) * 4), *sum(points, []))
        result += vertices + indices
    args.output.write_bytes(result)
    print(f'Captured {len(cases)} native underwater billboard cases')


if __name__ == '__main__':
    main()
