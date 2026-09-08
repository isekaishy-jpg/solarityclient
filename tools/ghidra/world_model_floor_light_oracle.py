"""Capture pinned 7C7FE0 WMO floor color interpolation and 7C1AD0 color split."""
import argparse
import random
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n


def capture(u, vertices, position, colors, ambient, flags, face, polygon):
    group, root, header, verts, indices, cv, mopy, point, output = [n.HEAP + x for x in (0, 0x400, 0x800, 0x1000, 0x1100, 0x1200, 0x1300, 0x1400, 0x1500)]
    n.write_words(u, group + 0x150, 1)
    for offset, value in [(0x18c, root), (0xe0, indices), (0xe8, verts), (0x108, cv), (0xdc, mopy)]:
        n.write_words(u, group + offset, value)
    n.write_words(u, root + 0x120, header)
    n.write_words(u, root + 0x1a0, ambient)
    n.write_words(u, header + 0x3c, flags)
    n.write_floats(u, verts, [v for vertex in vertices for v in vertex])
    u.mem_write(indices, struct.pack('<3H', 0, 1, 2))
    n.write_words(u, cv, *colors)
    n.write_words(u, mopy, polygon)
    n.write_floats(u, point, position)
    u.reg_write(UC_X86_REG_ECX, group)
    n.invoke(u, 0x7c7fe0, [point, face, output, output + 4])
    color, exterior = n.read_words(u, output, 2)
    n.invoke(u, 0x7c1ad0, [output, output + 8, 168, output + 12, 96])
    diffuse, ambient_split = n.read_words(u, output + 8, 2)
    return struct.pack('<12f10I', *(v for vertex in vertices for v in vertex), *position, *colors, ambient, flags, face, polygon, color, diffuse, ambient_split) + struct.pack('<I', exterior & 255)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rng = random.Random(0x7c7fe0)
    rows = []
    base = [(0., 0., 0.), (4., 0., 0.), (0., 4., 0.)]
    positions = [(0., 0., 0.), (4., 0., 0.), (0., 4., 0.), (1., 1., 0.), (2., 2., 0.), (-1., 2., 0.), (1., -2., 0.), (4., 4., 0.)]
    for axis in range(3):
        vertices = [v[axis:] + v[:axis] for v in base]
        for position in positions:
            position = position[axis:] + position[:axis]
            for flags in (0, 2):
                for colors in ([0, 0, 0], [0xff102030, 0x40205070, 0x8090a0b0], [0xffffffff] * 3):
                    for face in (0, 65535):
                        rows.append(capture(n.emulator(), vertices, position, colors, 0x743b2511, flags, face, 1))
    for case in range(480):
        vertices = [[rng.uniform(-30., 30.) for _ in range(3)] for _ in range(3)]
        position = [rng.uniform(-40., 40.) for _ in range(3)]
        colors = [rng.getrandbits(32) for _ in range(3)]
        rows.append(capture(n.emulator(), vertices, position, colors, rng.getrandbits(32), case % 4, 0, case % 2))
    args.output.write_bytes(b''.join(rows))
    print(f'{len(rows)} original WMO floor interpolation/color split captures')


if __name__ == '__main__':
    main()
