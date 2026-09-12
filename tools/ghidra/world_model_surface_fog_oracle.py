"""Capture per-draw fog from complete original MapObj submission callbacks.

Runs 7AC6A0/7AC9F0 (including their 7A9380 dispatch), 7A8440, 873210 and
873390. Resident texture, accepted visibility, lighting, shader preparation,
GX state invalidation and GPU submission are controlled provider boundaries.
Fog bank selection, enable decisions and physical pass order are original code.
"""
import argparse
import itertools
import struct
import hashlib
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from ribbon_fog_oracle import NativeFog
from liquid_shader_oracle import Renderer, shader_variants, call, floats, EXTENT
from surface_fog_shader_oracle import HASHES


class SurfaceFog(NativeFog):
    def __init__(self):
        super().__init__()
        self.root, self.group, self.momt, self.owner, self.header, self.day, self.gx_state = [
            n.HEAP + x for x in (0x10000, 0x11000, 0x12000, 0x13000, 0x14000, 0x15000, 0x18000)]
        self.batches = n.HEAP + 0x16000
        u = self.u
        n.write_words(u, self.root + 0x160, self.momt)
        n.write_words(u, self.group + 0x18c, self.owner)
        n.write_words(u, self.owner + 0x120, self.header)
        n.write_words(u, self.group + 0xf8, self.batches)
        n.write_words(u, self.group + 0x16c, 1)
        n.write_words(u, self.momt + 0x38, 1, 0)
        gx = n.read_words(u, 0xc5df88, 1)[0]
        n.write_words(u, gx + 0xf58, 1)
        n.write_words(u, gx + 0x28f4, self.gx_state)
        vtable = n.read_words(u, gx, 1)[0]
        n.write_words(u, vtable + 0xa8, n.STOP + 0x20)
        for offset, color in [(0x8c, 0xff336699), (0xa0, 0xffcc6633)]:
            n.write_words(u, self.day + offset, color)
            n.write_floats(u, self.day + offset + 4, [2., 22., 2.])
        self.draws = []
        self.enabled = 0

    def provider(self, u, address, size, context):
        value, pop = 0, 0
        if address == 0x873390:
            self.enabled = n.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 1)[0]
            return  # Execute the native enable upload too.
        if address == 0x7ecef0:
            value = self.day
        elif address == 0x4b6cb0:
            value = 1
        elif address == 0x4b54f0:
            value = 1
        elif address == n.STOP + 0x20:
            blend = n.read_words(u, self.gx_state + 0x90, 1)[0]
            self.draws.append([blend, self.enabled, *self.uploads[(0, 30)], *self.uploads[(4, 2)][:3]])
            pop = 8
        elif address == 0x685f50:
            pop = 8
        elif address == 0x685970:
            pop = 4
        elif address in (0x7cbcb0, 0x7c9d80, 0x7c9cb0, 0x409670, 0x7a7630,
                          0x7a8940, 0x681450, 0x872f90, 0x7a8b10, 0x8745d0,
                          0x873ff0, 0x873ee0, 0x7a84d0, 0x685fb0):
            pass
        else:
            return super().provider(u, address, size, context)
        sp = u.reg_read(UC_X86_REG_ESP)
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def capture(self, unified, colors, exterior, batch_class, unfogged, blend, selected):
        u = self.u
        n.write_words(u, self.header + 0x3c, unified * 2)
        n.write_words(u, self.group + 0x30, colors * 4 | exterior)
        counts = [int(batch_class == i) for i in range(3)]
        if not colors and not unified:
            counts = [0, 0, 1]
        u.mem_write(self.group + 0x5c, struct.pack('<3H', *counts))
        u.mem_write(self.batches, bytes(24))
        n.write_words(u, self.momt, 5 | unfogged * 2, 0, blend)
        n.write_words(u, 0xcfbeb8, selected)
        self.draws.clear()
        u.reg_write(UC_X86_REG_ECX, self.root)
        n.invoke(u, 0x7ac9f0 if colors else 0x7ac6a0, [self.group, 0])
        return list(self.draws)


def pixels(renderer, shaders, unified, draw):
    """Run original unlit programs with captured registers and GX blend state."""
    blend, enabled, *registers = draw
    constants = [0.] * (46 * 4)
    for index, values in [(2, [1.,0.,0.,-1/EXTENT]), (3,[0.,1.,0.,1/EXTENT]),
                          (4,[0.,0.,0.,.5]), (5,[0.,0.,0.,1.]), (6,[1.,0.,0.,0.]),
                          (7,[0.,1.,0.,0.]), (30, registers[:4]),
                          (31,[1.,0.,0.,0.]), (32,[0.,1.,0.,0.]), (33,[0.,0.,1.,12.])]:
        constants[index*4:index*4+4] = values
    call(renderer.device, 109, 'upu', 2, floats(registers[4:] + [0.]), 1)
    # Direct EGxBlend factors; depth and alpha testing are immaterial to this
    # isolated, fully covered triangle with vertex alpha 128/255.
    factors = [(2,1,2,1), (2,1,2,1), (5,6,2,6), (5,2,1,2), (9,1,7,1),
               (9,3,7,5), (9,2,7,2), (6,2,6,2), (6,1,6,1), (5,1,5,1), (2,2,1,2)]
    src, dst, srca, dsta = factors[blend]
    for state, value in [(27,int(blend > 1)), (19,src), (20,dst), (206,1), (207,srca), (208,dsta)]:
        call(renderer.device, 57, 'uu', state, value)
    # Opaque/alpha-key cases use full coverage; other passes use half coverage.
    alpha = 255 if blend < 2 else 128
    vertices = b''.join(struct.pack('<6fI4f', x,y,0.,0.,0.,1.,alpha<<24|0x204060,0.,0.,0.,0.)
                        for x,y in [(-1.,-1.),(3.,-1.),(-1.,3.)])
    image = renderer.render(shaders[unified], shaders[2], constants, vertices,
                            [struct.pack('<4f',1.,1.,1.,1.)*16], 0, None, 0x661a334c)
    return image[(EXTENT//2*EXTENT+EXTENT//2)*4:][:4]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    parser.add_argument('shaders', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    state = SurfaceFog()
    names = ['SHADERS_VERTEX_VS_3_0_MAPOBJDIFFUSE_T1.BLS',
             'SHADERS_VERTEX_VS_2_0_MAPOBJUDIFFUSE_T1.BLS', 'SHADERS_PIXEL_PS_3_0_MAPOBJDIFFUSE.BLS']
    for name in names:
        assert hashlib.sha256((args.shaders / name).read_bytes()).hexdigest() == HASHES[name]
    shaders = [shader_variants(args.shaders / name)[0] for name in names]
    renderer = Renderer()
    rows = ['# Native 7AC6A0/7AC9F0/7A9380 programmable passes; ordinary BGRA=ff336699 selected=ffcc6633; start=2 end=22 exponent=2',
            '# unified colors exteriorBits class unfogged blend selectedBank pass -> GXblend enabled VSfog4 PSfogRGB3 RGBA4; vertexRGBA=32,64,96,128 (255 alpha for GX0/1); clearRGBA=26,51,76,102']
    try:
        for case in itertools.product(range(2), range(2), (0, 8, 64), range(3), range(2), range(11), range(2)):
            for index, draw in enumerate(state.capture(*case)):
                rows.append(' '.join(map(str, (*case, index, *draw, *pixels(renderer, shaders, case[0], draw)))))
    finally:
        renderer.close()
    args.output.write_text('\n'.join(rows) + '\n', encoding='ascii')
    print('Captured', len(rows) - 2, 'native physical surface passes')
