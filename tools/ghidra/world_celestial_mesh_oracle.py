"""Capture untouched 12340 celestial geometry, horizon clipping and orientation.

Inputs replace only the retained body/camera values. No native function is hooked.
"""
import argparse
import random
import struct
from pathlib import Path
import wmo_registration_oracle as n
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX


def capture():
    u = n.emulator()
    body, pos, uv, colors, indices, vc, ic, matrix, axis = [
        n.HEAP + offset for offset in (0, 64, 160, 224, 256, 280, 284, 320, 400)]
    rows = ['# Build 12340 7EDBE0/7EDEE0 and 9ABB60; little-endian words.']
    rng = random.Random(12340)
    cases = []
    for eye in [0., 128.125, -6789.25]:
        for size in [0.1, 1., 1.75, 2., 2.625, 3.]:
            heights = [-12., -size/2, 0., size/2, 12.]
            heights += [x + d for x in [-size/2, size/2, .4-size/2, .4+size/2,
                                        .401-size/2, .401+size/2]
                        for d in [-.000001, 0., .000001]]
            for height in heights:
                for alpha in [0, 64, 192, 255]:
                    cases.append((eye+height, eye, size, alpha << 24 | 0x123456))
    cases += [(rng.uniform(-4, 4), rng.uniform(-1, 1), rng.uniform(.1, 3),
               rng.randrange(256) << 24 | 0xabcdef) for _ in range(256)]
    for z, eye, size, color in cases:
        n.write_floats(u, body, [0., 0., z])
        n.write_words(u, body+12, color)
        n.write_floats(u, body+20, [size])
        n.write_floats(u, 0xd38b20, [eye])
        u.mem_write(pos, bytes(224))
        for address in [0x7edbe0, 0x7edee0]:
            u.reg_write(UC_X86_REG_ECX, body)
            n.invoke(u, address, [pos, uv, colors, indices, vc, ic])
        source = struct.pack('<fffI', z, eye, size, color)
        result = b''.join(bytes(u.mem_read(a, size)) for a, size in
                          [(vc, 8), (pos, 72), (uv, 48), (colors, 24), (indices, 12)])
        rows.append('mesh ' + source.hex() + ' ' + result.hex())
    axes = [(1., 0., 0.), (-1., 0., 0.), (0., 1., 0.), (0., -1., 0.),
            (0., 0., 1.), (0., 0., -1.), (1., 1., 1.), (.001, 1., 1.)]
    axes += [(x, 1., 0.) for x in [.000009999, .00001, .000010001]]
    axes += [tuple(rng.uniform(-1., 1.) for _ in range(3)) for _ in range(256)]
    for vector in axes:
        n.write_floats(u, axis, vector)
        n.write_floats(u, matrix, [float(i % 5 == 0) for i in range(16)])
        u.reg_write(UC_X86_REG_ECX, matrix)
        u.reg_write(UC_X86_REG_EAX, axis)
        n.invoke(u, 0x9abb60, [])
        rows.append('basis ' + bytes(u.mem_read(axis, 12)).hex() + ' '
                    + bytes(u.mem_read(matrix, 64)).hex())
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Wrote {len(rows)-1} native celestial mesh/basis rows to {args.output}')
