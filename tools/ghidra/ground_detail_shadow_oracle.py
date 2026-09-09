"""Capture unchanged DetailDoodad.bls primary shadow pixels in D3D9.

Explicit coordinates and texture samples bound the five-tap filter, map edge,
baked-shadow minimum, and relief from the unnormalized interpolated normal.
Projection and caster policy are covered by independent native fixtures.
"""

import argparse
import hashlib
import itertools
import struct
from pathlib import Path

from liquid_shader_oracle import Renderer, shader_variants, EXTENT, call, floats

SHADERS = {
    'SHADERS_VERTEX_VS_2_0_DETAILDOODAD.BLS':
        '8319a237b6c2d36e90832329063ce065e686c53e3e123293ab28e4fbe5f9410d',
    'SHADERS_PIXEL_PS_2_0_DETAILDOODAD.BLS':
        'da085ba5672c077c470d6fcacc39ab9e654189d639213afac2707c01d28ab95f',
}


def capture(directory):
    """Execute the original vertex/pixel variant one with fixed primary registers."""
    for name, expected in SHADERS.items():
        if hashlib.sha256((directory / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f'original shader fingerprint mismatch: {name}')
    vertex = shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_DETAILDOODAD.BLS')[1]
    pixel = shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_DETAILDOODAD.BLS')[1]
    rows = ['# detail_shadow coordinate_xyz baked_byte normal_z map_pattern result_rgba']
    renderer = Renderer()
    try:
        # 7B2D30: alpha blending, alpha reference 128, comparison GEQUAL.
        for state, value in ((15, 1), (24, 128), (25, 7), (27, 1), (19, 5), (20, 6)):
            call(renderer.device, 57, 'uu', state, value)
        # 875D30's direct-depth map is point sampled and clamped.
        for state, value in ((1, 3), (2, 3), (5, 1), (6, 1), (7, 0), (11, 0)):
            call(renderer.device, 69, 'uuu', 4, state, value)
        offsets = [(0.8, -1.), (-0.2, -0.8), (0.2, -0.6), (1., -0.4),
                   (-0.6, -0.2), (0.6, 0.2), (-1., -0.4), (-0.4, -0.6)]
        ps_constants = [0.] * (13 * 4)
        # Primary fade plane c3 stays zero; c4 is the view-space adjusted ray.
        ps_constants[4 * 4:4 * 4 + 3] = [0., 0., -1.]
        for index, (x, y) in enumerate(offsets):
            ps_constants[(index + 5) * 4:(index + 5) * 4 + 2] = [x / 4., y / 4.]
        call(renderer.device, 109, 'upu', 0, floats(ps_constants), 13)
        for xy, depth, baked, normal, pattern in itertools.product(
                [(0., 0.), (0.5, -0.25), (0.8, 0.), (-0.95, 0.1), (1.1, 0.)],
                [0.499, 0.5, 0.501], [0, 102, 255], [0., 0.5, 1., 2.],
                [0, 0xffff, 0x5a5a]):
            constants = [0.] * (46 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            reg(0, [1., 0., 0., 0.]); reg(1, [0., 1., 0., 0.])
            reg(2, [0., 0., 1., 0.]); reg(3, [0., 0., 20., 1.])
            reg(4, [1., 0., 0., 0.]); reg(5, [0., 1., 0., 0.])
            reg(7, [-1 / EXTENT, 1 / EXTENT, .5, 1.])
            reg(8, [0., 1., 1., 0.]); reg(9, [0., 1., 0., 0.])
            reg(11, [1., 1., 1.])
            for index, value in enumerate([*xy, depth]):
                reg(23 + index, [0., 0., 0., value])
            color = (baked << 24) | 0xffffff
            vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., normal,
                color, 0., 0., 0., 0.) for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
            solid = struct.pack('<4f', 192 / 255., 96 / 255., 32 / 255., 1.) * 16
            samples = b''.join(struct.pack('<4f', .5 if (pattern >> index) & 1 else .25,
                0., 0., 1.) for index in range(16))
            pixels = renderer.render(vertex, pixel, constants, vertices,
                [solid, solid, solid, solid, samples], 0, None)
            center = pixels[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
            assert all(pixels[index:index + 3] == center[:3]
                for index in range(0, len(pixels), 4))
            rows.append('detail_shadow ' + ' '.join(map(str,
                [*xy, depth, baked, normal, pattern, center.hex()])))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
