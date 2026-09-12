"""Capture original M2/MapObj/MapObjU depth-fog pixels through Direct3D9.

Original fingerprinted programs run with a controlled orthographic projection,
constant eye depth and lateral offset. Fog setup uses the original coefficients;
lighting is unlit, base RGB is 64/128/192, texture and surface alpha are opaque.
"""
import argparse
import hashlib
import itertools
import struct
from pathlib import Path
from liquid_shader_oracle import Renderer, shader_variants, call, floats, EXTENT

HASHES = {
    'SHADERS_VERTEX_VS_3_0_DIFFUSE_T1.BLS': 'e552498ae61b769c49ba87fd9ea8ff8c9d682df1b09ddc3802f6354e7be42f08',
    'SHADERS_PIXEL_PS_3_0_COMBINERS_OPAQUE.BLS': 'bb61fa1731eff60a814085c3dff459032393500a2ade5426a2a1e992e71a1a87',
    'SHADERS_VERTEX_VS_3_0_MAPOBJDIFFUSE_T1.BLS': '031ff46fe910b7b4def18a31ba94244e1da13277632a21cb4a664084df45eca2',
    'SHADERS_PIXEL_PS_3_0_MAPOBJDIFFUSE.BLS': 'ce80fa39432a1ead3b7595309da1a51eb5eeb9f05d06fdd5392a1f6a2d8b1b3a',
    'SHADERS_VERTEX_VS_2_0_MAPOBJUDIFFUSE_T1.BLS': '0ba45c23442e7c245ba29af1e3ff3ad523452797a24c95460200e38729a18f7e',
}


def capture(directory):
    shaders = {}
    for name, expected in HASHES.items():
        path = directory / name
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f'Shader fingerprint mismatch: {name}')
        shaders[name] = shader_variants(path)[0]
    rows = ['# Original surface shaders: family depth lateral exponent RGBA; start=2 end=22 fogRGB=204,51,102']
    rows += [f'# {name} sha256={value}' for name, value in HASHES.items()]
    renderer = Renderer()
    try:
        for family, vertex_name, pixel_name in [
            ('m2', 'SHADERS_VERTEX_VS_3_0_DIFFUSE_T1.BLS', 'SHADERS_PIXEL_PS_3_0_COMBINERS_OPAQUE.BLS'),
            ('wmo', 'SHADERS_VERTEX_VS_3_0_MAPOBJDIFFUSE_T1.BLS', 'SHADERS_PIXEL_PS_3_0_MAPOBJDIFFUSE.BLS'),
            ('wmo_u', 'SHADERS_VERTEX_VS_2_0_MAPOBJUDIFFUSE_T1.BLS', 'SHADERS_PIXEL_PS_3_0_MAPOBJDIFFUSE.BLS'),
        ]:
            for depth, lateral, exponent in itertools.product([2., 7., 12., 22., 30.], [0., 6.], [0., 1., 1.5, 2.]):
                constants = [0.] * (46 * 4)
                def reg(index, values):
                    constants[index * 4:index * 4 + len(values)] = values
                reg(2, [1., 0., 0., -lateral - 1/EXTENT])
                reg(3, [0., 1., 0., 1/EXTENT])
                reg(4, [0., 0., 0., .5]); reg(5, [0., 0., 0., 1.])
                reg(6, [1., 0., 0., 0.]); reg(7, [0., 1., 0., 0.])
                reg(28, [64/255, 128/255, 192/255, 1.])
                reg(30, [-1/20, 22/20, exponent, 0.])
                reg(31, [1., 0., 0., lateral]); reg(32, [0., 1., 0., 0.])
                reg(33, [0., 0., 1., depth])
                vertices = b''.join(struct.pack('<6fI4f', x, y, 0., 0., 0., 1., 0xff204060, 0., 0., 0., 0.)
                                    for x,y in [(-1., -1.), (3., -1.), (-1., 3.)])
                call(renderer.device, 109, 'upu', 2, floats([204/255,51/255,102/255,0.]), 1)
                image = renderer.render(shaders[vertex_name], shaders[pixel_name], constants, vertices,
                                        [struct.pack('<4f',1.,1.,1.,1.) * 16], 0, None)
                pixel = image[(EXTENT//2 * EXTENT + EXTENT//2)*4:][:4]
                rows.append(' '.join(map(str, (family, depth, lateral, exponent, *pixel))))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    result = capture(args.directory)
    args.output.write_text(result, encoding='ascii')
    print('Captured', sum(not line.startswith('#') for line in result.splitlines()), 'original surface fog frames')
