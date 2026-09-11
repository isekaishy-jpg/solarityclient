"""Capture original Terrain3, MapObjDiffuse, and DetailDoodad environment receivers.

The original vertex and pixel programs run unchanged in Direct3D 9. Constant
receiver coordinates isolate map selection, edge thresholds, baked visibility,
and the five/nine-tap distinction from the separately captured projections.
"""

import argparse
import hashlib
import itertools
import struct
from pathlib import Path

from liquid_shader_oracle import Renderer, shader_variants, EXTENT, call, floats


def capture(directory):
    """Run both environment modes with contradictory maps and exact boundaries."""
    rows = ['# environment_receiver family mode case depth baked eye_depth patterns4 rgba']
    renderer = Renderer()
    offsets = [(0.8, -1.), (-0.2, -0.8), (0.2, -0.6), (1., -0.4),
               (-0.6, -0.2), (0.6, 0.2), (-1., -0.4), (-0.4, -0.6)]
    coordinates = [
        [(0., 0.)] * 4,
        [(0.8, 0.), (0., 0.), (0., 0.), (0., 0.)],
        [(0.989, 0.), (0., 0.), (0., 0.), (0., 0.)],
        [(0.99, 0.), (0., 0.), (0., 0.), (0., 0.)],
        [(1.1, 0.), (0.999, 0.), (0., 0.), (0., 0.)],
        [(1.1, 0.), (1., 0.), (0.5, -0.25), (0., 0.)],
        [(1.1, 0.), (1.1, 0.), (-1., 0.), (0.95, 0.)],
        [(1.1, 0.), (1.1, 0.), (1.1, 0.), (1.1, 0.)],
    ]
    shaders = {
        'terrain': ('VS_2_0_TERRAIN', 1, 'TERRAIN3'),
        'wmo': ('VS_3_0_MAPOBJDIFFUSE_T1', 60, 'MAPOBJDIFFUSE'),
        'detail': ('VS_3_0_DETAILDOODAD', 2, 'DETAILDOODAD'),
    }
    try:
        for family, (vertex_name, variant, pixel_name) in shaders.items():
            vertex_path = directory / f'SHADERS_VERTEX_{vertex_name}.BLS'
            pixel_path = directory / f'SHADERS_PIXEL_PS_3_0_{pixel_name}.BLS'
            for path in [vertex_path, pixel_path]:
                rows.append(f'# {path.name} sha256={hashlib.sha256(path.read_bytes()).hexdigest()}')
            vertex = shader_variants(vertex_path)[variant]
            pixels = shader_variants(pixel_path)
            first_sampler = 5 if family == 'terrain' else 4
            for sampler in range(first_sampler, first_sampler + 4):
                for state, value in ((1, 3), (2, 3), (5, 1), (6, 1), (7, 0), (11, 0)):
                    call(renderer.device, 69, 'uuu', sampler, state, value)
            ps = [0.] * 52
            if family != 'terrain':
                ps[16:19] = [0., 0., -1.]
            for index, (x, y) in enumerate(offsets):
                start = (index + (3 if family == 'terrain' else 5)) * 4
                ps[start:start + 2] = [x / 4., y / 4.]
            call(renderer.device, 109, 'upu', 0, floats(ps), 13)
            for mode, case, depth, baked, eye, patterns in itertools.product(
                    [2, 3], range(len(coordinates)), [0.499, 0.5, 0.501], [0, 255], [5., 20.],
                    [(0xffff, 0, 0xffff, 0), (0, 0xffff, 0, 0xffff),
                     (0x5a5a, 0xa5a5, 0x3333, 0xcccc)]):
                constants = [0.] * (236 * 4)
                def reg(index, values):
                    constants[index * 4:index * 4 + len(values)] = values
                if family == 'wmo':
                    reg(31, [1., 0., 0., 0.]); reg(32, [0., 1., 0., 0.])
                    reg(33, [0., 0., 1., eye]); reg(30, [0., 1., 1., 0.])
                    reg(2, [1., 0., 0., -1/EXTENT])
                    reg(3, [0., 1., 0., 1/EXTENT])
                    reg(4, [0., 0., 0., 0.5]); reg(5, [0., 0., 0., 1.])
                    row_start = 224
                else:
                    reg(0, [1., 0., 0., 0.]); reg(1, [0., 1., 0., 0.])
                    reg(2, [0., 0., 1., 0.]); reg(3, [0., 0., eye, 1.])
                    reg(4, [1., 0., 0., 0.]); reg(5, [0., 1., 0., 0.])
                    reg(7, [-1/EXTENT, 1/EXTENT, 0.5, 1.])
                    if family == 'terrain':
                        reg(12, [0., 1., 1., 0.]); reg(25, [1., 1., 1.])
                        row_start = 37
                    else:
                        reg(8, [0., 1., 1., 0.]); reg(9, [0., 1., 0., 0.])
                        reg(11, [1., 1., 1.]); row_start = 23
                for index, xy in enumerate(coordinates[case]):
                    for axis, value in enumerate([*xy, depth]):
                        reg(row_start + index * 3 + axis, [0., 0., 0., value])
                color = (baked << 24) | (0x808080 if family == 'wmo' else 0xffffff)
                vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., 1.,
                    color, 0., 0., 0., 0.) for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
                solid = struct.pack('<4f', 192/255., 96/255., 32/255., 1.) * 16
                textures = [solid] * first_sampler
                if family == 'terrain':
                    textures[1] = struct.pack('<4f', 0., 0., 0., baked/255.) * 16
                for pattern in patterns:
                    textures.append(b''.join(struct.pack('<4f',
                        0.5 if (pattern >> index) & 1 else 0.25, 0., 0., 1.) for index in range(16)))
                pixel = pixels[(mode - 1) * 8 if family == 'terrain' else mode]
                # The shared liquid renderer writes only c0..45; MapObj and
                # Terrain's final map also need their higher native registers.
                call(renderer.device, 94, 'upu', 46, floats(constants[46 * 4:]), 190)
                result = renderer.render(vertex, pixel, constants, vertices, textures, 0, None)
                center = result[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
                assert all(result[index:index + 3] == center[:3] for index in range(0, len(result), 4))
                rows.append('environment_receiver ' + ' '.join(map(str,
                    [family, mode, case, depth, baked, eye, *patterns, center.hex()])))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
