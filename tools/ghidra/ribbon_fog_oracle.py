"""Capture retained stock model/ribbon fog registers and original BLS pixels.

Runs 81FB10 through fog setup, complete 873210/873390, then the extracted
Color_T1/Combiners_Mod programs on Direct3D9. Lighting setup is excluded;
caps, global fog enable and constant upload are controlled provider boundaries.
Each row is the final opaque, unlit pass of one ribbon; preceding pass blend
is varied to select common fog. The retained register bank survives rows.
"""
import argparse
import itertools
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from liquid_shader_oracle import Renderer, shader_variants
from ribbon_shader_oracle import render


class NativeFog:
    def __init__(self):
        self.u = u = n.emulator()
        self.scene, self.material, self.bank, element, gx, vtable, self.caps = [
            n.HEAP + x for x in (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x8000, 0x9000)]
        n.write_words(u, self.scene + 0x98, self.material)
        n.write_words(u, self.scene + 0x70, self.bank)
        n.write_words(u, self.scene + 0x50, element)
        n.write_words(u, 0xc5df88, gx)
        n.write_words(u, gx, vtable)
        n.write_words(u, vtable + 0x118, n.STOP + 0x10)
        n.write_words(u, 0xd43020, 1)
        n.write_floats(u, 0xd4300c, [1.])
        self.uploads = {(4, 2): [0.] * 4, (0, 30): [0.] * 4}
        u.hook_add(UC_HOOK_CODE, self.provider)

    def provider(self, u, address, size, context):
        value, pop = 0, 0
        if address == 0x873ca0:
            pass  # Lighting setup does not participate in this capture.
        elif address == 0x532af0:
            value = self.caps
        elif address == 0x683100:
            value, pop = 1, 4
        elif address == n.STOP + 0x10:
            stage, slot, data, count = n.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 4)
            self.uploads[(stage, slot)] = n.read_floats(u, data, count * 4)
            pop = 16
        else:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def ribbon(self, flags, blend, enabled, follow_flags, start, end, exponent, color):
        u = self.u
        n.write_words(u, self.material, flags | blend << 16)
        n.write_floats(u, self.bank + 0xa8, [start, end, float(enabled), exponent, *color])
        sp = n.STACK + 0x18000
        n.write_words(u, sp, n.STOP)
        u.reg_write(UC_X86_REG_ESP, sp)
        u.reg_write(UC_X86_REG_ECX, self.scene)
        u.emu_start(0x81fb10, 0x81fd31, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == 0x81fd31
        n.invoke(u, 0x873390, [int(not (follow_flags & 2))])
        return self.uploads[(0, 30)], self.uploads[(4, 2)]


def cases():
    # No preceding draw has published fog since the zero-initialized PE bank.
    yield 7, 0, 1, 5, 2., 22., 2., .213, .421, .637, 12.
    for blend, exponent, depth in itertools.product(range(7), (1., 2.), (2., 12., 22., 30.)):
        yield 5, blend, 1, 5, 2., 22., exponent, .213, .421, .637, depth
    for seed in (0, 3, 6):
        yield 5, seed, 1, 5, 2., 22., 2., .213, .421, .637, 12.
        yield 7, 0, 1, 5, 5., 45., 3., .8, .1, .3, 12.
        yield 5, 0, 0, 5, 5., 45., 3., .8, .1, .3, 12.
        yield 7, 0, 1, 7, 5., 45., 3., .8, .1, .3, 12.
        yield 7, 0, 1, 5, 5., 45., 3., .8, .1, .3, 12.


def capture(executable, shaders):
    # Verify both the PE and BLS files through the existing pinned capture.
    from ribbon_shader_oracle import capture as verify_shaders
    verify_shaders(executable, shaders)
    n.initialize(executable)
    state, renderer = NativeFog(), Renderer()
    vertex = shader_variants(Path(shaders) / 'SHADERS_VERTEX_VS_3_0_COLOR_T1.BLS')[0]
    pixel = shader_variants(Path(shaders) / 'SHADERS_PIXEL_PS_3_0_COMBINERS_MOD.BLS')[0]
    rows = ['# Serial native 81FB10/873210/873390 and original BLS final opaque pass pixels; tint/texture as ribbon_shader_native.txt',
            '# firstFlags firstBlend sceneEnabled finalFlags start end exponent R G B eyeDepth -> RGBA4 VSfog4 PSfogRGB3']
    try:
        for case in cases():
            flags, blend, enabled, follow, start, end, exponent, *tail = case
            color, depth = tail[:3], tail[3]
            coefficients, fog = state.ribbon(flags, blend, enabled, follow, start, end, exponent, color)
            rgba = render(renderer, vertex, pixel, coefficients, fog, depth)
            rows.append(' '.join(map(str, (*case, *rgba, *coefficients, *fog[:3]))))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('shaders')
    parser.add_argument('output')
    args = parser.parse_args()
    text = capture(args.executable, args.shaders)
    Path(args.output).write_text(text, encoding='ascii')
    print(f'{len(text.splitlines()) - 2} original ribbon fog frames')
