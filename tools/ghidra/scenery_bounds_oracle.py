"""Execute original CMapObj 7BDB10 render bounds and size classification.

The original model accessors read a complete resident M2 header. All arithmetic,
including partially reversed boxes and the wholly reversed point fallback,
executes directly from the fingerprinted executable without provider hooks.
"""
import argparse
import itertools
from pathlib import Path

import wmo_registration_oracle as n
from world_shadow_volume_oracle import invoke


def capture(executable):
    n.initialize(executable)
    u = n.emulator()
    owner, model, shared, header = [n.HEAP + offset for offset in (0, 0x1000, 0x2000, 0x3000)]
    n.write_words(u, owner + 0x34, model)
    n.write_words(u, model + 0x2c, shared)
    n.write_words(u, shared + 8, 1)
    n.write_words(u, shared + 0x150, header)
    rows = ['# local6 matrix16 world6 category; 7BDB10, build 12340']
    for extent, reversed_axes, translated, basis in itertools.product(
            (0., 0.5, 2., 7.5, 50., 1.e10, float.fromhex('0x1.fffffep+127')),
            range(8), (False, True), range(3)):
        if extent > 1.e30 and reversed_axes != 7:
            continue  # The actual collision-only render sentinel reverses all axes.
        low = [extent if reversed_axes & (1 << axis) else -extent for axis in range(3)]
        high = [-value for value in low]
        axes = [(1., 0., 0., 0., 1., 0., 0., 0., 1.),
                (-1., 0., 0., 0., 2., 0., 0., 0., 0.5),
                (0.6, 0.8, 0., -0.8, 0.6, 0., 0., 0., 1.)][basis]
        position = [1100., -4500., 150.] if translated else [0., 0., 0.]
        matrix = [value for i in range(3) for value in (*axes[i*3:i*3+3], 0.)] + position + [1.]
        n.write_floats(u, header + 0xa0, low + high + [2.] + [-1., -1., -1., 1., 1., 1.])
        n.write_floats(u, owner + 0xd8, matrix)
        n.write_floats(u, owner + 0x6c, position + [1.])
        invoke(u, 0x7bdb10, [owner])
        words = n.read_words(u, header + 0xa0, 6) + n.read_words(u, owner + 0xd8, 16)
        words += n.read_words(u, owner + 0x48, 6)
        category = bytes(u.mem_read(owner + 0x24, 1))[0]
        rows.append(' '.join(f'{value:08x}' for value in words) + f' {category}')
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    output = capture(args.executable)
    Path(args.output).write_text(output, encoding='ascii')
    print(f'{len(output.splitlines()) - 1} original scenery bounds queries')
