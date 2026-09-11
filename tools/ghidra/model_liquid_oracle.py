"""Capture original build-12340 model/liquid sphere classification.

Executes the unmodified 821C8C..821DDA block with resident M2 header, model-view,
and already transformed light-query plane inputs. This isolates the classifier;
it does not establish terrain/WMO selection, query caching, or GPU clipping.
"""
import argparse
import itertools
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_ECX
import wmo_registration_oracle as n
from world_shadow_volume_oracle import invoke


def capture(executable):
    n.initialize(executable)
    u = n.emulator()
    model, shared, header, light, scene, config = [n.HEAP + offset for offset in
                                               (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000)]
    bp = n.STACK + 0x18000
    n.write_words(u, model + 0x2c, shared)
    n.write_words(u, shared + 0x150, header)
    n.write_words(u, model + 0x2a8, light)
    n.write_words(u, scene + 4, config)
    n.write_words(u, bp - 4, scene)
    rows = ['# flags clip cameraBelow localBounds6 radius modelView16 viewPlane4 above below; 821C8C..821DDA build 12340']
    for flags, clip, camera, shape, transform, level in itertools.product(
            (0x20, 0x40, 0x60), (0, 1), (0, 1), range(3), range(3),
            (-4., -2.000000238418579, -2., -1.9999998807907104, 0.,
             1.9999998807907104, 2., 2.000000238418579, 4.)):
        bounds, radius = [([-1., -1., -1., 1., 1., 1.], 2.),
                          ([-2., 0., -3., 4., 2., 1.], 0.),
                          ([-2., 0., -3., 4., 2., 1.], 2.)][shape]
        matrix = [(1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.),
                  (2., 0., 0., 0., 0., 0.5, 0., 0., 0., 0., 3., 0., 0., 0., 1., 1.),
                  (0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 3., -1., 0., 1.)][transform]
        plane = [0., 0., 1., -level] if transform != 2 else [1., 0., 0., -level]
        n.write_words(u, light + 0x14, flags)
        n.write_words(u, config + 4, clip * 2)
        n.write_words(u, scene + 0x140, camera)
        n.write_floats(u, header + 0xa0, bounds + [radius])
        n.write_floats(u, model + 0xf4, matrix)
        n.write_floats(u, light + 0xc4, plane)
        u.reg_write(UC_X86_REG_EBP, bp)
        u.reg_write(UC_X86_REG_ESP, bp - 0x200)
        u.reg_write(UC_X86_REG_ESI, model)
        u.emu_start(0x821c8c, 0x821dda, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == 0x821dda
        above = bool(n.read_words(u, bp - 0x28, 1)[0])
        below = bool(n.read_words(u, bp - 0x2c, 1)[0])
        values = n.read_words(u, header + 0xa0, 7) + n.read_words(u, model + 0xf4, 16)
        values += n.read_words(u, light + 0xc4, 4)
        rows.append(f'{flags} {clip} {camera} ' + ' '.join(f'{value:08x}' for value in values)
                    + f' {int(above)} {int(below)}')
    return '\n'.join(rows) + '\n'


def capture_planes(executable):
    """Execute the complete 8350A0 query preparation with no local lights."""
    n.initialize(executable)
    u = n.emulator()
    light, scene = n.HEAP, n.HEAP + 0x1000
    n.write_words(u, light, scene)
    matrices = [
        [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.],
        [0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 3., -1., 7., 1.],
        [0.6, 0., -0.8, 0., 0., 1., 0., 0., 0.8, 0., 0.6, 0., 1100., -4500., 150., 1.],
        [2., 0., 0., 0., 0., 3., 0., 0., 0., 0., 0.5, 0., -1., 2., 3., 1.],
    ]
    rows = ['# worldHeight view16 -> viewPlane4; full 8350A0 build 12340']
    for matrix, height in itertools.product(matrices, (-100., 0., 2., 123.25)):
        n.write_words(u, light + 0x14, 0x60)
        n.write_floats(u, light + 0xc4, [0., 0., 1., -height])
        n.write_floats(u, scene + 0x84, matrix)
        u.reg_write(UC_X86_REG_ECX, light)
        invoke(u, 0x8350a0, [])
        n.write_floats(u, scene + 0x80, [height])
        values = n.read_words(u, scene + 0x80, 17) + n.read_words(u, light + 0xc4, 4)
        rows.append(' '.join(f'{value:08x}' for value in values))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--plane-output')
    args = parser.parse_args()
    result = capture(args.executable)
    Path(args.output).write_text(result, encoding='ascii')
    print(f'{len(result.splitlines()) - 1} native model/liquid classifications')
    if args.plane_output:
        planes = capture_planes(args.executable)
        Path(args.plane_output).write_text(planes, encoding='ascii')
        print(f'{len(planes.splitlines()) - 1} native model/liquid view planes')
