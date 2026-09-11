"""Capture unchanged ShadowMapSL coverage from native byte-normalized textures.

The 128/255 M2 and 224/255 WMO references are passed through c2.w. Both point
and linear sampling retain equality with the original D3D9 byte texture format.
"""

import argparse
import ctypes as c
import hashlib
import struct
from pathlib import Path

from liquid_shader_oracle import Renderer, LockedRect, call, floats, shader_variants


class ByteRenderer(Renderer):
    """Use A8R8G8B8 rather than the floating textures of receiver probes."""

    def texture(self, pixels):
        texture = self.create(self.device, 23, 'uuuuuup', 4, 4, 1, 0, 21, 1, None,
                              output_before_last=True)
        locked = LockedRect()
        call(texture, 19, 'uppu', 0, c.byref(locked), None, 0)
        for row in range(4):
            c.memmove(locked.bits + row * locked.pitch, pixels[row * 16:(row + 1) * 16], 16)
        call(texture, 20, 'u', 0)
        return texture


def capture(directory):
    """Execute original vertex zero and pixel eight at each coverage boundary."""
    rows = ['# shadow_alpha filter reference_byte alpha_byte result_rgba']
    programs = []
    for name, variant in [('SHADERS_VERTEX_VS_2_0_SHADOWMAP.BLS', 0),
                          ('SHADERS_PIXEL_PS_2_0_SHADOWMAPSL.BLS', 8)]:
        path = directory / name
        rows.append(f'# {name} sha256={hashlib.sha256(path.read_bytes()).hexdigest()}')
        programs.append(shader_variants(path)[variant])
    constants = [0.] * 184
    for index, values in {
        31: [1, 0, 0, 0], 32: [0, 1, 0, 0], 33: [0, 0, 1, 2000],
        2: [1, 0, 0, -1/16], 3: [0, 1, 0, 1/16], 4: [0, 0, 0, .5],
        5: [0, 0, 0, 1], 6: [1, 0, 0, 0], 7: [0, 1, 0, 0],
    }.items():
        constants[index * 4:index * 4 + 4] = values
    vertices = b''.join(struct.pack('<6fI4f', x, y, 0, 0, 0, 1, 0xffffffff, 0, 0, 0, 0)
                        for x, y in [(-1, -1), (3, -1), (-1, 3)])
    renderer = ByteRenderer()
    try:
        for filtering in [1, 2]:
            for state in [5, 6]:
                call(renderer.device, 69, 'uuu', 0, state, filtering)
            for reference in [128, 224]:
                call(renderer.device, 109, 'upu', 0,
                     floats([0, 0, 0, .00025, 0, 0, 0, 0, 0, 0, 0, reference / 255]), 3)
                for alpha in [reference - 1, reference, reference + 1]:
                    image = renderer.render(*programs, constants, vertices,
                                            [bytes([255, 255, 255, alpha]) * 16], 0, None)
                    pixel = image[(8 * 16 + 8) * 4:(8 * 16 + 8) * 4 + 4]
                    rows.append(f'shadow_alpha {filtering} {reference} {alpha} {pixel.hex()}')
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.directory))
