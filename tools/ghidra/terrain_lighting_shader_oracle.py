"""Capture colored terrain lighting with unchanged stock VS/PS bytecode.

Inputs are explicit shader constants, raw normalized texture bytes, optional
MCCV bytes, and alpha-map shadow visibility. This checks shader composition;
it does not establish the CPU light provider or MCSH upload conversion.
"""
import argparse
import itertools
import struct
from pathlib import Path
from liquid_shader_oracle import Renderer, shader_variants, EXTENT


def capture(directory):
    vertex = shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')
    pixel = shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    rows = ['# terrain texture_rgb ambient_rgb diffuse_rgb direction_xyz mccv_argb_or_none shadow_visibility_u8 result_rgba']
    renderer = Renderer()
    try:
        for texture, light, color, shadow in itertools.product(
            [(51, 102, 153), (192, 96, 32)],
            [((1., 1., 1.), (0., 0., 0.), (0., 0., 1.)),
             ((.2, .3, .4), (.7, .5, .1), (.6, 0., .8)),
             ((.8, .4, .2), (.7, .9, 1.2), (0., 0., 2.)),
             ((.3, .4, .5), (.7, .6, .5), (0., 0., -.5))],
            [None, 0xff7f7f7f, 0xffff8040], [0, 170, 255]):
            ambient, diffuse, direction = light
            constants = [0.] * (46 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            for i in range(4):
                constants[i * 5] = 1.
            reg(4, [1., 0., 0., 0.])
            reg(5, [0., 1., 0., 0.])
            reg(7, [-1/EXTENT, 1/EXTENT, .5, 1.])
            reg(12, [0., 1., 1., 0.])
            reg(24, direction)
            reg(25, ambient)
            reg(26, diffuse)
            vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., 1., color or 0, 0., 0., 0., 0.)
                                for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
            textures = [struct.pack('<4f', *[v/255 for v in texture], 1.) * 16,
                        struct.pack('<4f', 0., 0., 0., shadow/255) * 16]
            image = renderer.render(vertex[0 if color is None else 4], pixel, constants, vertices, textures, 0, None)
            center = image[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
            assert all(image[i:i+3] == center[:3] for i in range(0, len(image), 4))
            values = [*texture, *ambient, *diffuse, *direction, 'none' if color is None else f'{color:08x}', shadow, center.hex()]
            rows.append('terrain ' + ' '.join(map(str, values)))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
