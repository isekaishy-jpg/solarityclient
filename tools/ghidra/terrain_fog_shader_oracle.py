"""Offscreen D3D9 capture of original Terrain.bls vertex fog and Terrain0 pixels.

The stock shaders run unchanged. Constant-depth geometry separates view Z from
radial distance and makes the captured result independent of terrain UVs.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from liquid_shader_oracle import Renderer, shader_variants, call, floats, EXTENT


def capture(directory):
    vertex_path = directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS'
    pixel_path = directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN0.BLS'
    vertex = shader_variants(vertex_path)[0]
    pixel = shader_variants(pixel_path)[0]
    rows = ['# Original D3D9 Terrain.bls/Terrain0.bls: depth exponent RGBA.']
    for path in [vertex_path, pixel_path]:
        rows.append(f'# {path.name} sha256={hashlib.sha256(path.read_bytes()).hexdigest()}')
    renderer = Renderer()
    try:
        for depth in [0., 5., 10., 15., 20., 25.]:
            for exponent in [1., 1.5, 2., 7.]:
                constants = [0.] * (46 * 4)
                def reg(index, values):
                    constants[index * 4:index * 4 + len(values)] = values
                for i in range(4):
                    constants[i * 5] = 1.
                reg(4, [1., 0., 0., 0.])
                reg(5, [0., 1., 0., 0.])
                reg(7, [-1/EXTENT, 1/EXTENT, .5, 1.])
                reg(12, [-1/20, 1., exponent, 0.])
                reg(25, [1., 1., 1., 1.])
                vertices = b''.join(struct.pack('<6fI4f', x, y, depth, 0., 0., 1., 0xffffffff, 0., 0., 0., 0.) for x, y in [(-1., -1.), (3., -1.), (-1., 3.)])
                textures = [struct.pack('<4f', 0., 1., 0., 1.) * 16, struct.pack('<4f', 0., 0., 0., 1.) * 16]
                call(renderer.device, 109, 'upu', 0, floats([0., 0., 0., 0.]), 1)
                image = renderer.render(vertex, pixel, constants, vertices, textures, 0, 0xff204060)
                center = image[(8 * EXTENT + 8) * 4:(8 * EXTENT + 8) * 4 + 4]
                assert all(image[i:i+3] == center[:3] for i in range(0, len(image), 4))
                rows.append(f'terrain {depth:g} {exponent:g} {center.hex()}')
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
