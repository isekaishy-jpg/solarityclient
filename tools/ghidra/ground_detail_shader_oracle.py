"""Capture unchanged DetailDoodad.bls pixels with native blend, alpha, and fog state.

This offscreen D3D9 oracle supplies explicit vertex/light inputs. Placement and
mesh expansion are independently covered by ground_detail_oracle.py.
"""
import argparse
import itertools
import struct
from pathlib import Path
from liquid_shader_oracle import Renderer, shader_variants, EXTENT, call


def capture(directory):
    vertex = shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_DETAILDOODAD.BLS')[0]
    pixel = shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_DETAILDOODAD.BLS')[0]
    rows = ['# detail depth range exponent texture_argb mccv_rgb shadow ambient_rgb diffuse_rgb direction_xyz result_rgba']
    renderer = Renderer()
    try:
        # 7B2D30: GX blend 2, alpha reference 128; D3D ALPHAFUNC GEQUAL.
        for state, value in ((15, 1), (24, 128), (25, 7), (27, 1), (19, 5), (20, 6)):
            call(renderer.device, 57, 'uu', state, value)
        for depth, alpha, tint, shadow, light in itertools.product(
            [20., 85., 90., 92., 100.], [255, 192, 127],
            [(64, 96, 120), (127, 127, 127)], [0, 255],
            [((.2, .3, .4), (.7, .5, .1), (.6, 0., .8)),
             ((.8, .4, .2), (.7, .9, 1.2), (0., 0., 2.))]):
            ambient, diffuse, direction = light
            distance, exponent = 100., 1.5
            constants = [0.] * (46 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            # Positive native view Z; the Vulkan test uses its right-handed equivalent.
            reg(0, [1., 0., 0., 0.]); reg(1, [0., 1., 0., 0.])
            reg(2, [0., 0., 1., 0.]); reg(3, [0., 0., depth, 1.])
            reg(4, [1., 0., 0., 0.]); reg(5, [0., 1., 0., 0.])
            reg(7, [-1 / EXTENT, 1 / EXTENT, .5, 1.])
            reg(8, [-1 / 150., 1., exponent, 0.])
            reg(9, [-1 / (distance * .15), distance / (distance * .15), 0., 0.])
            reg(10, direction); reg(11, ambient); reg(12, diffuse)
            color = (shadow << 24) | (tint[0] * 2 << 16) | (tint[1] * 2 << 8) | tint[2] * 2
            vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., 1., color, 0., 0., 0., 0.)
                                for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
            texture = struct.pack('<4f', 192 / 255., 96 / 255., 32 / 255., alpha / 255.) * 16
            image = renderer.render(vertex, pixel, constants, vertices, [texture], 0, 0x204060)
            center = image[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
            assert all(image[i:i+3] == center[:3] for i in range(0, len(image), 4))
            rows.append('detail ' + ' '.join(map(str, [depth, distance, exponent,
                f'{alpha:02x}c06020', *tint, shadow, *ambient, *diffuse, *direction, center.hex()])))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
