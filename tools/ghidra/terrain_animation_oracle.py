"""Capture original world texture clocks, MCLY offsets, and animated BLS pixels.

Runs bounded instruction ranges in the fingerprinted executable, then unchanged
Terrain/Terrain1 BLS through offscreen D3D9. No client entry point is executed.
"""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_EBX
import wmo_registration_oracle as n
import liquid_shader_oracle as gpu
from terrain_texture_coordinates_oracle import setup


def capture(directory):
    u = n.emulator()
    frame = n.STACK + 0x14000
    u.reg_write(UC_X86_REG_EBP, frame)
    u.reg_write(UC_X86_REG_ESP, n.STACK + 0x18000)
    n.write_floats(u, 0xd25488, [0.24000000953674316])
    rows = ['# delta seconds; offset MCLY_flags native_c13_x native_c13_y; pixels four MCLY flags and 9 RGBA samples']
    gpu.EXTENT = 64
    vertex = gpu.shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')[48]
    pixel = gpu.shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[6]
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
    vertices = b''.join(struct.pack('<6fI4f', *points[i], 0.,0.,1.,0,0.,0.,0.,0.) for i in indices)
    textures = []
    for layer in range(4):
        pattern = [v/255 for y in range(4) for x in range(4) for v in
                   (32 + ((x+layer)%4)*48, 24 + ((y+layer)%4)*56,
                    16 + ((x+y*2+layer)%4)*60, 255)]
        textures.append(struct.pack('<64f', *pattern))
    textures.append(struct.pack('<4f', 34/255, 51/255, 68/255, 1.) * 16)
    renderer = gpu.Renderer()
    try:
        for sampler in range(4):
            for state, value in ((1,1), (2,1), (5,2), (6,2)):
                gpu.call(renderer.device, 69, 'uuu', sampler, state, value)
        for delta in [0., .016, .25, 63.75, .125, 64.5]:
            n.write_floats(u, 0xcd76a0, [delta])
            u.emu_start(0x783284, 0x7832ec, count=10000)
            rows.append(f'delta {delta}')
            offsets = {}
            for flags in range(0x40, 0x80):
                u.reg_write(UC_X86_REG_EBX, n.HEAP)
                n.write_words(u, n.HEAP, flags)
                n.write_words(u, frame - 8, 0xd25174)
                u.emu_start(0x7d2e9f, 0x7d2f05, count=10000)
                offsets[flags] = n.read_floats(u, 0xd25170, 2)
                rows.append('offset ' + ' '.join(map(str, [flags, *offsets[flags]])))
            for flags in [(0x40,0x59,0x6b,0), (0,0x47,0x5a,0x7c)]:
                constants = setup((0.,0.,0.), 4) + [0.] * 36
                for index, values in [(0,[1.,0.,0.,0.]), (1,[0.,1.,0.,0.]),
                                      (2,[0.,0.,-1.,0.]), (3,[16.,16.,10.,1.]),
                                      (4,[.1,0.,0.,0.]), (5,[0.,.1,0.,0.]),
                                      (6,[0.,0.,1/99.9,0.]), (7,[-1/64,1/64,-.1/99.9,1.]),
                                      (12,[0.,1.,1.,0.]), (24,[0.,0.,-1.,0.]), (25,[.5,.5,.5,0.])]:
                    constants[index*4:index*4+len(values)] = values
                for layer, flag in enumerate(flags):
                    if flag:
                        constants[(13+layer)*4:(13+layer)*4+2] = offsets[flag]
                image = renderer.render(vertex, pixel, constants, vertices, textures, 0, None)
                samples = ''.join(image[(y*64+x)*4:(y*64+x)*4+4].hex() for y in [24,32,40] for x in [24,32,40])
                rows.append('pixels ' + ' '.join(map(str, flags)) + ' ' + samples)
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
    print('Captured 384 native offsets and 12 animated terrain frames')
