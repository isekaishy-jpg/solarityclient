"""Capture weighted and per-layer unlit terrain through unchanged stock BLS.

Offscreen D3D9 only. Covers every unlit mask for one through four layers,
small/large alpha modes, MCCV, shadow endpoints, and unequal specular masks.
"""
import argparse
import struct
from pathlib import Path
import liquid_shader_oracle as native


def capture(directory):
    native.EXTENT = 64
    vertex = native.shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')
    pixel = native.shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    unit = f32(f32(1600. / 3.) / 128.)
    points = [(-f32(row * .5 * unit), -f32((column + (row % 2) * .5) * unit), 0.)
              for row in range(17) for column in range(8 if row % 2 else 9)]
    indices = []
    for row in range(8):
        for column in range(8):
            tl = row * 17 + column
            center, tr, bl, br = tl + 9, tl + 1, tl + 17, tl + 18
            indices += [tl, center, tr, tr, center, br, br, center, bl, bl, center, tl]
    colors = [(51,102,153,34), (153,51,85,85), (85,153,51,170), (119,68,153,255)]
    rows = ['# material: weighted, layers, unlit_mask, palette, 9 native RGBA samples at (24,32,40)^2']
    renderer = native.Renderer()
    try:
        for weighted in range(2):
            for layers in range(1, 5):
                for mask in range(1 << layers):
                    for palette in range(2):
                        constants = [0.] * (46 * 4)
                        for index, values in [(0, [1.,0.,0.,0.]), (1, [0.,1.,0.,0.]),
                                              (2, [0.,0.,-1.,0.]), (3, [16.,16.,10.,1.]),
                                              (4, [.1,0.,0.,0.]), (5, [0.,.1,0.,0.]),
                                              (6, [0.,0.,1/99.9,0.]), (7, [-1/64,1/64,-.1/99.9,1.]),
                                              (12, [0.,1.,1.,0.]), (24, [0.,0.,-1.,0.]),
                                              (25, [.2,.3,.4,0.]), (26, [.3,.2,.1,0.]),
                                              (27, [.65,.35,.15,20.])]:
                            constants[index*4:index*4+len(values)] = values
                        color = 0xffff8040 if palette else 0
                        vertices = b''.join(struct.pack('<6fI4f', *points[i], 0.,0.,1.,color,0.,0.,0.,0.) for i in indices)
                        alpha = [34,51,68] if not palette else [170,153,136]
                        alpha = [value if i < layers - 1 else 0 for i, value in enumerate(alpha)]
                        textures = [struct.pack('<4f', *[v/255 for v in color]) * 16 for color in colors[:layers]]
                        textures.append(struct.pack('<4f', *[v/255 for v in alpha], float(not palette)) * 16)
                        # 7D2D70 writes zero for MCLY 0x80, one for ordinary layers.
                        native.call(renderer.device, 109, 'upu', 1,
                                    native.floats([float(not (mask & (1 << i))) for i in range(4)]), 1)
                        vs = (layers-1)*16 + 8 + (4 if palette else 0)
                        ps = (layers-1)*2 + weighted*8 + (16 if mask else 0)
                        image = renderer.render(vertex[vs], pixel[ps], constants, vertices, textures, 0, None)
                        samples = ''.join(image[(y*64+x)*4:(y*64+x)*4+4].hex() for y in [24,32,40] for x in [24,32,40])
                        rows.append(f'material {weighted} {layers} {mask} {palette} {samples}')
    finally:
        renderer.close()
    assert len(rows) == 121
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
    print('Captured 120 original terrain material frames')
