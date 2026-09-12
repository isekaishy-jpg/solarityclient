"""Run native ribbon element initialization, common setup and shader selection.

The original 8226EE..822730 constructor overwrites poisoned element shadow
state. Native 81FB10 publishes it, then 8731C0/873160 select Particle_Unlit's
programs. Allocation, model admission, texture binding and draw dispatch are
outside this capture; lighting providers use controlled query counts. Original
BLS pixels run through Direct3D9 with neutral fog and opaque blending.
"""
import argparse
import itertools
from pathlib import Path
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX,
                              UC_X86_REG_EIP, UC_X86_REG_ESI, UC_X86_REG_ESP)
import wmo_registration_oracle as n
from ribbon_fog_oracle import NativeFog
from ribbon_shader_oracle import capture as verify_shaders, render
from liquid_shader_oracle import Renderer, shader_variants


def select(flags, previous_shadow, filtering, local_lights):
    state = NativeFog()
    u = state.u
    element = n.read_words(u, state.scene + 0x50, 1)[0]
    owner, emitter = n.HEAP + 0xa000, n.HEAP + 0xb000
    bp = n.STACK + 0x17000
    n.write_words(u, element + 0x3c, previous_shadow)
    n.write_words(u, 0xd43010, previous_shadow, filtering, local_lights)
    n.write_words(u, bp - 0x14, 7)
    n.write_floats(u, owner + 0x88, [12.])
    # Controlled x87 input to the original constructor's opacity store.
    u.mem_write(n.STOP + 0x100, b'\xdb\xe3\xd9\xe8')  # fninit; fld1
    u.emu_start(n.STOP + 0x100, n.STOP + 0x104)
    for register, value in [(UC_X86_REG_EAX, element), (UC_X86_REG_ESI, owner),
                            (UC_X86_REG_EBX, emitter), (UC_X86_REG_EBP, bp),
                            (UC_X86_REG_ESP, n.STACK + 0x18000)]:
        u.reg_write(register, value)
    u.emu_start(0x8226ee, 0x822730, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x822730
    element_type, element_shadow = n.read_words(u, element, 1)[0], n.read_words(u, element + 0x3c, 1)[0]
    coefficients, color = state.ribbon(flags, 0, 1, flags, 1000., 2000., 1., [.2, .4, .6])
    published_shadow = n.read_words(u, 0xd43010, 1)[0]
    n.invoke(u, 0x8731c0, [int(not (flags & 1))])
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, 0)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x873160, 0x8731a9, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x8731a9
    vertex, pixel = n.read_words(u, u.reg_read(UC_X86_REG_ESP), 2)
    return (element_type, element_shadow, published_shadow, vertex, pixel), coefficients, color


def capture(executable, shaders):
    verify_shaders(executable, shaders)
    vertex = shader_variants(Path(shaders) / 'SHADERS_VERTEX_VS_3_0_COLOR_T1.BLS')
    pixel = shader_variants(Path(shaders) / 'SHADERS_PIXEL_PS_3_0_COMBINERS_MOD.BLS')
    rows = ['# Native ribbon construction/common publication/shader selection; original BLS RGBA, neutral fog, opaque blend',
            '# flags previousShadow filtering localLights -> elementType elementShadow publishedShadow vertex pixel RGBA4']
    renderer = Renderer()
    try:
        for case in itertools.product((4, 5), (0, 1, 2), (0, 1), (0, 4)):
            selection, coefficients, fog = select(*case)
            rgba = render(renderer, vertex[selection[3]], pixel[selection[4]], coefficients, fog, .5)
            rows.append(' '.join(map(str, (*case, *selection, *rgba))))
    finally:
        renderer.close()
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('shaders')
    parser.add_argument('output')
    args = parser.parse_args()
    result = capture(args.executable, args.shaders)
    Path(args.output).write_text(result, encoding='ascii')
    print(f'{len(result.splitlines()) - 2} native ribbon shadow selections and frames')
