"""Capture unmodified Terrain/Terrain2 primary shadow filtering in D3D9.

Explicit map samples and coordinates cover depth equality, the five-tap kernel,
border fading, and composition with baked MCSH visibility. Native projection and
caster selection have separate fixtures; this probe does not establish them.
"""
import argparse
import itertools
import struct
from pathlib import Path
from liquid_shader_oracle import Renderer, shader_variants, EXTENT, call, floats


def capture(directory):
    vertex = shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')[1]
    pixel = shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN2.BLS')[0]
    rows = ['# terrain_dynamic coordinate_xyz baked_visibility map_pattern result_rgba']
    renderer = Renderer()
    try:
        for state, value in ((1, 3), (2, 3), (5, 1), (6, 1), (7, 0), (11, 0)):
            call(renderer.device, 69, 'uuu', 5, state, value)
        offsets = [(0.8, -1.), (-0.2, -0.8), (0.2, -0.6), (1., -0.4),
                   (-0.6, -0.2), (0.6, 0.2), (-1., -0.4), (-0.4, -0.6)]
        ps_constants = [0.] * 44
        for index, (x, y) in enumerate(offsets):
            ps_constants[(index + 3) * 4:(index + 3) * 4 + 2] = [x / 4., y / 4.]
        call(renderer.device, 109, 'upu', 0, floats(ps_constants), 11)
        # The original VS writes three further plane rows, unused by Terrain2.
        call(renderer.device, 94, 'upu', 46, floats([0.] * 12), 3)
        for xy, depth, baked, pattern in itertools.product(
                [(0., 0.), (0.5, -0.25), (0.8, 0.), (-0.95, 0.1), (1.1, 0.)],
                [0.499, 0.5, 0.501], [0., 0.4, 1.], [0, 0xffff, 0x5a5a]):
            constants = [0.] * (46 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            for index in range(4):
                constants[index * 5] = 1.
            reg(4, [1., 0., 0., 0.]); reg(5, [0., 1., 0., 0.])
            reg(7, [-1/EXTENT, 1/EXTENT, 0.5, 1.])
            reg(12, [0., 1., 1., 0.])
            reg(25, [1., 1., 1.])
            for index, value in enumerate([*xy, depth]):
                reg(37 + index, [0., 0., 0., value])
            vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., 1., 0, 0., 0., 0., 0.)
                for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
            solid = struct.pack('<4f', 192/255, 96/255, 32/255, 1.) * 16
            material = struct.pack('<4f', 0., 0., 0., baked) * 16
            samples = b''.join(struct.pack('<4f', 0.5 if (pattern >> index) & 1 else 0.25, 0., 0., 1.)
                for index in range(16))
            pixels = renderer.render(vertex, pixel, constants, vertices,
                [solid, material, solid, solid, solid, samples], 0, None)
            center = pixels[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
            assert all(pixels[index:index + 3] == center[:3] for index in range(0, len(pixels), 4))
            rows.append('terrain_dynamic ' + ' '.join(map(str, [*xy, depth, baked, pattern, center.hex()])))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
