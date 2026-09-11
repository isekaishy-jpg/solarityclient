"""Capture unmodified 4F5E80 -> 780240 registered model sphere preparation.

Supplies loaded model bounds and a placement matrix. The original math and
native radius threshold run without hooks; parameter six defers re-registration.
Does not execute animated bounds production, scene traversal or GPU rendering.
"""
import argparse
import itertools
import math
from pathlib import Path
import struct

from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n


def words(values):
    return ' '.join(f'{struct.unpack("<I", struct.pack("<f", value))[0]:08x}' for value in values)


def capture(executable):
    n.initialize(executable)
    rows = ['# lower3 upper3 radius matrix16 -> native registered center3 radius; hex f32; 4F5E80 and 780240']
    matrices = []
    for angle, scale, origin in [
            (0., [1., 1., 1.], [0., 0., 0.]),
            (.73, [.25, 4., 2.], [4., -7., 8.]),
            (-1.2, [3., .5, .25], [1100., -4500., 20.]),
            (2.3, [.001, .004, .002], [-12345.3, 23111.7, 612.4]),
            (.7, [-2., 3., 4.], [20000.25, -16000.75, 4096.5])]:
        c, s = math.cos(angle), math.sin(angle)
        matrices.append([c * scale[0], s * scale[0], .125 * scale[0], 0.,
                         -s * scale[1], c * scale[1], -.25 * scale[1], 0.,
                         .3 * scale[2], -.4 * scale[2], scale[2], 0., *origin, 1.])
    threshold = struct.unpack('<I', struct.pack('<f', .001))[0]
    radii = [0., *(struct.unpack('<f', struct.pack('<I', threshold + offset))[0]
                  for offset in [-1, 0, 1]), 1.7, 100.]
    for bounds, radius, matrix in itertools.product(
            [([-1., -2., -3.], [4., 5., 6.]),
             ([0., 0., 0.], [0., 0., 0.]),
             ([12345.1, -4.33, -9.77], [12348.7, 1.25, 16.2]),
             ([1., 2., 3.], [-4., -5., -6.])], radii, matrices):
        u = n.emulator()
        model, shared, source, sphere, owner, transform, box, center = [n.HEAP + i * 0x1000 for i in range(8)]
        values = [*bounds[0], *bounds[1], radius]
        n.write_words(u, model + 0x2c, shared)
        n.write_words(u, shared + 8, 1)
        n.write_words(u, shared + 0x150, source)
        n.write_floats(u, source + 0xa0, values)
        u.reg_write(UC_X86_REG_ECX, model)
        n.invoke(u, 0x4f5e80, [sphere])
        n.write_floats(u, transform, matrix)
        n.write_floats(u, box, values[:6])
        n.invoke(u, 0x780240, [owner, transform, box, sphere, center, 1, 0])
        rows.append(words(values + matrix) + ' ' +
                    ' '.join(f'{word:08x}' for word in n.read_words(u, owner + 0x38, 4)))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    result = capture(args.executable)
    args.output.write_text(result, encoding='ascii')
    print(f'{len(result.splitlines()) - 1} native registered model spheres')
