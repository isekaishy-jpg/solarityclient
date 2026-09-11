"""Capture original terrain UV constants and patterned native shader frames.

Executes 7C3D90/7C3C60 and 7D0050 with graphics/light providers intercepted,
then renders unchanged Terrain/Terrain1 bytecode through offscreen D3D9.
No client entry point runs. The capture covers ordinary nonanimated layers.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
import liquid_shader_oracle as gpu


def setup(origin, layers):
    u = n.emulator()
    owner, device, vtable = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x2000
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device, vtable)
    n.write_words(u, vtable + 0x118, n.STOP + 0x100)
    n.write_floats(u, owner + 0x18, origin)
    skips = {0x7ba340: 0, 0x790440: 4, 0x81e400: 4, 0x7b7bd0: 4,
             0x8349e0: 16, 0x873ff0: 0, 0x79e470: 0, 0x685f50: 8,
             n.STOP + 0x100: 16}

    def provider(u, address, size, context):
        if address in skips:
            return_value(u, 0)
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + skips[address])

    u.hook_add(UC_HOOK_CODE, provider)
    n.invoke(u, 0x7c3d90, [])
    n.invoke(u, 0x7d0050, [owner, layers + 1, 0])
    return list(struct.unpack('<148f', u.mem_read(0xd250a0, 148 * 4)))


def capture(directory):
    gpu.EXTENT = 64
    vertex = gpu.shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')[0]
    pixel = gpu.shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    unit = f32(f32(1600. / 3.) / 128.)
    rows = ['# original terrain constants c18..c23 and 64x64 patterned native RGBA frames']
    renderer = gpu.Renderer()
    try:
        # Match the ordinary linear, repeating diffuse sampler. One authored mip
        # isolates coordinate production from the separate filtering fixture.
        for state, value in ((1, 1), (2, 1), (5, 2), (6, 2)):
            gpu.call(renderer.device, 69, 'uuu', 0, state, value)
        for origin in [(0., 0., 0.), (-600., -4233.33349609375, 38.)]:
            for layers in range(1, 5):
                constants = setup(origin, layers)
                rows.append('constants ' + ' '.join(map(str, [*origin, layers, *constants[72:96]])))
            constants = setup(origin, 1) + [0.] * (9 * 4)
            def reg(index, values):
                constants[index * 4:index * 4 + len(values)] = values
            for index, values in [(0, [1., 0., 0., 0.]), (1, [0., 1., 0., 0.]),
                                  (2, [0., 0., -1., 0.]),
                                  (3, [16. - origin[0], 16. - origin[1], 10. + origin[2], 1.]),
                                  (4, [.1, 0., 0., 0.]), (5, [0., .1, 0., 0.]),
                                  (6, [0., 0., 1/99.9, 0.]),
                                  (7, [-1/64, 1/64, -.1/99.9, 1.]),
                                  (12, [0., 1., 1., 0.]),
                                  (24, [0., 0., -1., 0.]), (25, [1., 1., 1., 0.])]:
                reg(index, values)
            points = []
            for row in range(17):
                for column in range(8 if row % 2 else 9):
                    points.append((f32(origin[0] - f32(row * .5 * unit)),
                                   f32(origin[1] - f32((column + (row % 2) * .5) * unit)), origin[2]))
            indices = []
            for row in range(8):
                for column in range(8):
                    tl = row * 17 + column
                    center, tr, bl, br = tl + 9, tl + 1, tl + 17, tl + 18
                    indices += [tl, center, tr, tr, center, br, br, center, bl, bl, center, tl]
            vertices = b''.join(struct.pack('<6fI4f', *points[index], 0., 0., 1., 0, 0., 0., 0., 0.) for index in indices)
            pattern = [value / 255 for y in range(4) for x in range(4)
                       for value in (32 + x * 48, 24 + y * 56, 16 + ((x + y * 2) % 4) * 60, 255)]
            textures = [struct.pack('<64f', *pattern), struct.pack('<4f', 0., 0., 0., 1.) * 16]
            frame = renderer.render(vertex, pixel, constants, vertices, textures, 0, None)
            rows.append('frame ' + ' '.join(map(str, origin)) + ' ' + frame.hex())
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(args.directory))
    print('Captured 8 original terrain constant sets and 2 native shader frames')
