"""Unmodified 984E50 portal-polygon distances used by indoor fog boundaries."""
import argparse
import itertools
import struct
from pathlib import Path
import wmo_registration_oracle as n
from world_model_fog_oracle import bits


def capture():
    point, vertices, plane, result, wrapper = [n.HEAP + i * 0x1000 for i in range(5)]
    rows = ['# Native 984E50 point/polygon distance; all floats stored as hex words.']
    shapes = [([0., 0., 1., 0.], [[-2., -3., 0.], [2., -3., 0.], [2., 3., 0.], [-2., 3., 0.]]),
              ([1., 0., 0., 0.], [[0., -2., -3.], [0., 2., -3.], [0., 2., 3.], [0., -2., 3.]]),
              ([0., -1., 0., 2.], [[-2., 2., -3.], [2., 2., -3.], [2., 2., 3.], [-2., 2., 3.]]),
              ([1., 0., 1., 0.], [[-2., -3., 2.], [2., -3., -2.], [2., 3., -2.], [-2., 3., 2.]]),
              ([.6, 0., .8, -.4], [[-2., -3., 2.], [2., -3., -1.], [0., 3., .5]])]
    for shape, (coefficients, polygon) in enumerate(shapes):
        # A fresh CPU avoids reusing a translated PUSH vertex-count from the
        # previous polygon when the wrapper changes from four vertices to three.
        u = n.emulator()
        n.write_floats(u, plane, coefficients)
        n.write_floats(u, vertices, sum(polygon, []))
        code = b''.join(b'\x68' + struct.pack('<I', value) for value in [plane, len(polygon), vertices, point])
        code += b'\xe8' + struct.pack('<i', 0x984e50 - (wrapper + len(code) + 5))
        code += b'\x83\xc4\x10\xd9\x1d' + struct.pack('<I', result) + b'\xc3'
        u.mem_write(wrapper, code)
        rows.append(f'polygon {shape} ' + ' '.join(f'{bits(v):08x}' for v in coefficients + sum(polygon, [])))
        for x, y, z in itertools.product([-3., -2., -1.99999, 0., 1.234, 2., 2.00001, 5.], [-4., -3., 0., 2., 3., 4.], [-25., -1., -.01, -.001, 0., .001, .01, 1., 24.99999, 25., 25.00001]):
            n.write_floats(u, point, [x, y, z])
            n.invoke(u, wrapper, [])
            rows.append(f'point {shape} ' + ' '.join(f'{bits(v):08x}' for v in [x, y, z]) + f' {n.read_words(u, result, 1)[0]:08x}')
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
