"""Capture stock terrain specular over the complete flat MCNK grid.

Original Terrain VS8/12 and Terrain1 PS0 bytecode, camera (-16,-16,10),
64px square orthographic view. Keeps nonzero texture alpha, colored highlights,
MCCV, shadow endpoints, VS2 color saturation and the disabled permutation.
No client is launched.
"""
import argparse
import itertools
import struct
from pathlib import Path
import liquid_shader_oracle as native


def capture(directory):
    native.EXTENT = 64
    vertex = native.shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')
    pixel = native.shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    unit = f32(f32(1600. / 3.) / 128.)
    points = []
    for row in range(17):
        for column in range(8 if row % 2 else 9):
            points.append((-f32(row * .5 * unit), -f32((column + (row % 2) * .5) * unit), 0.))
    indices = []
    for row in range(8):
        for column in range(8):
            tl = row * 17 + column
            center, tr, bl, br = tl + 9, tl + 1, tl + 17, tl + 18
            indices += [tl, center, tr, tr, center, br, br, center, bl, bl, center, tl]
    rows = ['# specular: existing terrain fields through visibility, then RGB/enable/texture_alpha and 9 sampled RGBA values']
    renderer = native.Renderer()
    try:
        for alpha, color, visibility, direction, enabled, specular in itertools.product(
                [0, 96, 255], [None, 0xffff8040], [0, 255],
                [(0., 0., 1.), (.6, 0., .8)], [0, 1],
                [(.65, .35, .15), (6.5, 3.5, 1.5)]):
            constants = [0.] * (46 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            for index, values in [(0, [1., 0., 0., 0.]), (1, [0., 1., 0., 0.]),
                                  (2, [0., 0., -1., 0.]), (3, [16., 16., 10., 1.]),
                                  (4, [.1, 0., 0., 0.]), (5, [0., .1, 0., 0.]),
                                  (6, [0., 0., 1/99.9, 0.]),
                                  (7, [-1/64, 1/64, -.1/99.9, 1.]),
                                  (12, [0., 1., 1., 0.]),
                                  (24, [direction[0], direction[1], -direction[2], 0.]),
                                  (25, [.2, .3, .4, 0.]), (26, [.3, .2, .1, 0.]),
                                  (27, [*specular, 20.])]:
                reg(index, values)
            vertices = b''.join(struct.pack('<6fI4f', *points[index], 0., 0., 1., color or 0, 0., 0., 0., 0.) for index in indices)
            textures = [struct.pack('<4f', 51/255, 102/255, 153/255, alpha/255) * 16,
                        struct.pack('<4f', 0., 0., 0., visibility/255) * 16]
            variant = (8 if enabled else 0) + (4 if color is not None else 0)
            image = renderer.render(vertex[variant], pixel, constants, vertices, textures, 0, None)
            samples = ''.join(image[(y*64+x)*4:(y*64+x)*4+4].hex() for y in [24,32,40] for x in [24,32,40])
            values = [51, 102, 153, .2, .3, .4, .3, .2, .1, *direction,
                      'none' if color is None else f'{color:08x}', visibility,
                      samples, *specular, enabled, alpha]
            rows.append('specular ' + ' '.join(map(str, values)))
    finally:
        renderer.close()
    assert len(rows) == 97
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
    print('Captured 96 original terrain specular frames, including vertex color saturation')
