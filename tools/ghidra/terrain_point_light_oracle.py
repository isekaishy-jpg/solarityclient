"""Capture native terrain light admission, constants, and unchanged shader pixels.

Runs 7B7AF0, 8356F0, 790440/81E400, 8349E0 and 7D0050 in the pinned image.
Only fog/directional setup and GPU providers are intercepted. Original shaders
execute on offscreen D3D9; no client or OS entry point is run.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
import liquid_shader_oracle as gpu
from liquid_material_oracle import return_value

POSITIONS = [(-38.,-16.,4.), (-16.,-16.,1.), (-13.,-16.,3.),
             (-16.,-12.,2.), (4.,-16.,2.), (-50.,-16.,4.)]
f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
UNIT = f32(f32(1600. / 3.) / 128.)


def constants(origin, count, palette):
    u = n.emulator()
    scene, owner, device, vtable, lights, grid, chunk = [n.HEAP+x for x in
        (0,0x1000,0x2000,0x3000,0x4000,0x8000,0xd000)]
    n.write_words(u, scene+0x14, 7)
    n.write_words(u, scene+0x24, grid)
    n.write_words(u, 0xcd754c, scene)
    lower = [f32(origin[0] - f32(8*UNIT)), f32(origin[1] - f32(8*UNIT)), origin[2]]
    n.write_floats(u, chunk+0x4c, lower + list(origin))
    n.write_floats(u, chunk+0x7c, origin)
    u.reg_write(UC_X86_REG_ECX, owner)
    n.invoke(u, 0x7b7af0, [chunk, 0, chunk+0x7c, 0])
    bounds = n.read_floats(u, owner+0x24, 4)
    eye = [f32(origin[0]-16), f32(origin[1]-16), f32(origin[2]+10)]
    n.write_floats(u, 0xcd8f5c, eye)
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device, vtable)
    n.write_words(u, vtable+0x118, n.STOP+0x100)
    for index, position in enumerate(POSITIONS[:count]):
        light = lights + index*0x80
        n.write_words(u, light, scene, 7, 1)
        n.write_floats(u, light+0xc, [f32(x+y) for x,y in zip(position, origin)])
        scale = 32. if palette else 1.
        n.write_floats(u, light+0x3c, [scale*(.15+.05*index), scale*(.4-.03*index), scale*(.1+.04*index)])
        n.write_floats(u, light+0x54, [0., .7, .03])
        u.reg_write(UC_X86_REG_ECX, light)
        n.invoke(u, 0x8356f0, [1])
    skips = {0x7b7bd0:4, 0x873ff0:0, 0x79e470:0, 0x685f50:8, n.STOP+0x100:16}
    def provider(u, address, size, context):
        if address in skips:
            return_value(u, 0)
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP)+skips[address])
    u.hook_add(UC_HOOK_CODE, provider)
    n.invoke(u, 0x7d0050, [owner, 2, 0])
    return bounds, n.read_floats(u, 0xd250a0, 148)


def capture(directory):
    gpu.EXTENT = 64
    vertices = gpu.shader_variants(directory / 'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')
    pixel = gpu.shader_variants(directory / 'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    indices = []
    for row in range(8):
        for column in range(8):
            tl = row*17+column
            center, tr, bl, br = tl+9, tl+1, tl+17, tl+18
            indices += [tl,center,tr,tr,center,br,br,center,bl,bl,center,tl]
    rows = ['# frame: origin XYZ, source count, palette, specular, native query XYZR, 9 point registers, 9 RGBA samples']
    renderer = gpu.Renderer()
    try:
        for origin in [(0.,0.,0.), (-600.,-4233.33349609375,38.)]:
            points = [(f32(origin[0]-f32(row*.5*UNIT)),
                       f32(origin[1]-f32((column+(row%2)*.5)*UNIT)), origin[2])
                      for row in range(17) for column in range(8 if row%2 else 9)]
            for count in [0,1,3,6]:
                for palette in range(2):
                    for specular in range(2):
                        bounds, block = constants(origin, count, palette)
                        point_constants = block[112:148]
                        block += [0.]*36
                        for index, values in [(0,[1.,0.,0.,0.]), (1,[0.,1.,0.,0.]),
                                              (2,[0.,0.,-1.,0.]),
                                              (3,[16.-origin[0],16.-origin[1],10.+origin[2],1.]),
                                              (4,[.1,0.,0.,0.]), (5,[0.,.1,0.,0.]),
                                              (6,[0.,0.,1/99.9,0.]), (7,[-1/64,1/64,-.1/99.9,1.]),
                                              (12,[0.,1.,1.,0.]), (24,[0.,0.,-1.,0.]),
                                              (25,[.2,.3,.4,0.]), (26,[.3,.2,.1,0.]),
                                              (27,[.65,.35,.15,20.])]:
                            block[index*4:index*4+len(values)] = values
                        color = 0xffff8040 if palette else 0
                        mesh = b''.join(struct.pack('<6fI4f', *points[i], 0.,0.,1.,color,0.,0.,0.,0.) for i in indices)
                        textures = [struct.pack('<4f', .2,.4,.6,85/255)*16,
                                    struct.pack('<4f', 0.,0.,0.,1.)*16]
                        variant = (64 if count else 0) + (4 if palette else 0) + specular*8
                        frame = renderer.render(vertices[variant], pixel, block, mesh, textures, 0, None)
                        samples = ''.join(frame[(y*64+x)*4:(y*64+x)*4+4].hex() for y in [24,32,40] for x in [24,32,40])
                        rows.append('frame ' + ' '.join(map(str, [*origin,count,palette,specular,*bounds,*point_constants])) + ' ' + samples)
    finally:
        renderer.close()
    return '\n'.join(rows)+'\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('directory', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(args.directory))
    print('Captured 32 native terrain point-light queries and shader frames')
