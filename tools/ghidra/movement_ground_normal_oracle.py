"""Capture unhooked build-12340 body-box surface normals (75EE60/75EC10).

The pinned executable receives complete ordered candidate triangles and a
synthetic movement owner. Its clipping, inclusion, summation and normalization
execute unchanged. This fixture does not establish scene or movement admission.
"""

import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_ECX


def floats(values):
    """Round all fixture inputs at the original float storage boundary."""
    return struct.unpack('<' + 'f' * len(values), struct.pack('<' + 'f' * len(values), *values))


def capture(executable, output):
    """Run deterministic clipping boundaries and mixed ordered candidate sets."""
    native.initialize(executable)
    rng = random.Random(1234075)
    cases = [([0., 0., 0., .5, 2.], [])]
    base = [-2., -2., 0., 2., -2., 0., 0., 2., 0.]
    for axis in range(3):
        for offset in [-3., -.5008, -.5007, -.5, 0., .5, .5007, .5008, 2., 2.0007, 2.0008, 3.]:
            for nz in [0., .0174524, .017452405765652657, .017452407628297806, .5, 1.]:
                vertices = base.copy()
                for i in range(axis, 9, 3):
                    vertices[i] += offset
                cases.append(([0., 0., 0., .5, 2.], [[.3, -.4, nz, *vertices]]))
    for _ in range(160):
        origin = [rng.uniform(-17000., 17000.) for _ in range(3)]
        radius = rng.uniform(.1, 4.)
        height = radius * rng.uniform(2., 8.)
        triangles = []
        for _ in range(rng.randrange(1, 12)):
            normal = [rng.uniform(-1., 1.), rng.uniform(-1., 1.), rng.uniform(-.1, 1.)]
            vertices = [origin[i % 3] + rng.uniform(-2., 2.) * (height if i % 3 == 2 else radius) for i in range(9)]
            triangles.append([*normal, *vertices])
        cases.append(([*origin, radius, height], triangles))
    lines = ['# build 12340 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# 75EE60/75EC10 unhooked; owner XYZ radius height, count decimal, ordered normal3 vertices9, result3; floats hex']
    for owner, triangles in cases:
        owner = floats(owner)
        triangles = [floats(triangle) for triangle in triangles]
        uc = native.emulator()
        movement = native.HEAP
        candidates = movement + 0x1000
        native.write_floats(uc, movement + 0x10, owner[:3])
        native.write_floats(uc, movement + 0xc8, owner[3:])
        native.write_words(uc, 0xadba38, len(triangles), candidates)
        for index, triangle in enumerate(triangles):
            normal, vertices = triangle[:3], triangle[3:]
            native.write_floats(uc, candidates + index * 0x34, [*normal, 0., *vertices])
        uc.reg_write(UC_X86_REG_ECX, movement)
        native.invoke(uc, 0x75ee60, [])
        encoded = lambda values: ' '.join(f'{value:08x}' for value in struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values)))
        line = encoded(owner) + ' ' + str(len(triangles))
        for triangle in triangles:
            line += ' ' + encoded(triangle)
        line += ' ' + ' '.join(f'{word:08x}' for word in native.read_words(uc, movement + 0x38, 3))
        lines.append(line)
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(cases), 'unhooked native ground normals')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
