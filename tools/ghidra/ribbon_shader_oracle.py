"""Capture stock ribbon Color_T1 selection and original D3D9 shader pixels.

Executes 833934..8339C1's allocated material conversion, 980D1F..980D31's
lighting publication and 873160..8731A9's shader selection. Inputs are effective
root flags after shared-model setup. Fog/shadows are neutral, blend is opaque,
and D3D culling is disabled to isolate the selected shader's color/alpha rule.
Requires the pinned owned Wow.exe, extracted Particle_Unlit BLS files, Unicorn
and Windows Direct3D9. No game entry point or shader translation executes.
"""
import argparse
import ctypes as c
import hashlib
import struct
from pathlib import Path

import wmo_registration_oracle as n
from liquid_shader_oracle import Renderer, LockedRect, EXTENT, buffer, call, floats, shader_variants
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EDI, UC_X86_REG_EIP, UC_X86_REG_ESI, UC_X86_REG_ESP


def select(flags):
    u = n.emulator()
    material, passes, ribbon = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x2000
    n.write_words(u, material, flags)
    n.write_words(u, 0xd4123c, passes)
    u.reg_write(UC_X86_REG_ESI, material)
    u.reg_write(UC_X86_REG_EAX, 0)
    u.emu_start(0x833934, 0x8339c1, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x8339c1
    runtime = n.read_words(u, passes, 1)[0]
    n.write_words(u, 0xd43020, 1)
    n.write_words(u, ribbon + 0x11c, passes)
    u.reg_write(UC_X86_REG_ESI, ribbon)
    u.reg_write(UC_X86_REG_EDI, 0)
    u.reg_write(UC_X86_REG_EBP, n.STACK + 0x17000)
    u.reg_write(UC_X86_REG_ESP, n.STACK + 0x18000)
    u.emu_start(0x980d1f, 0x980d31, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x980d31
    n.write_words(u, 0xd43010, 0, 0, 0)  # No shadow, shadow filtering or local lights.
    n.write_floats(u, 0xd43064, [0.])
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, 0)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x873160, 0x8731a9, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x8731a9
    vertex, pixel = n.read_words(u, u.reg_read(UC_X86_REG_ESP), 2)
    return runtime, vertex, pixel


def render(renderer, vertex_shader, pixel_shader):
    device = renderer.device
    vertex = renderer.create(device, 91, 'p', buffer(vertex_shader))
    pixel = renderer.create(device, 106, 'p', buffer(pixel_shader))
    elements = [(0, 0, 2, 0, 0, 0), (0, 12, 4, 0, 10, 0),
                (0, 16, 1, 0, 5, 0), (255, 0, 17, 0, 0, 0)]
    declaration = renderer.create(device, 86, 'p', buffer(b''.join(struct.pack('<HHBBBB', *e) for e in elements)))
    call(device, 87, 'p', declaration)
    call(device, 92, 'p', vertex)
    call(device, 107, 'p', pixel)
    constants = [0.] * (35 * 4)
    constants[8:24] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    constants[24:32] = [1., 0., 0., 0., 0., 1., 0., 0.]
    constants[120:124] = [0., 1., 1., 0.]
    constants[124:136] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.]
    call(device, 94, 'upu', 0, floats(constants), 35)
    call(device, 109, 'upu', 2, floats([0., 0., 0., 0.]), 1)
    texel = struct.pack('<4f', 204 / 255, 153 / 255, 102 / 255, 192 / 255)
    call(device, 65, 'up', 0, renderer.texture(texel * 16))
    vertices = b''.join(struct.pack('<3f4B2f', x, y, .5, 191, 128, 64, 96, .5, .5)
                        for x, y in [(-.8, .8), (-.8, -.8), (.8, .8), (.8, -.8)])
    call(device, 43, 'upuufu', 0, None, 1, 0xff000000, 1., 0)
    call(device, 41)
    call(device, 83, 'uupu', 5, 2, buffer(vertices), 24)
    call(device, 42)
    call(device, 32, 'pp', renderer.target, renderer.readback)
    locked = LockedRect()
    call(renderer.readback, 13, 'ppu', c.byref(locked), None, 0x10)
    b, g, r, a = c.string_at(locked.bits + (EXTENT // 2) * locked.pitch + (EXTENT // 2) * 4, 4)
    call(renderer.readback, 14)
    return r, g, b, a


def capture(executable, shaders):
    n.initialize(executable)
    names = ['SHADERS_VERTEX_VS_3_0_COLOR_T1.BLS', 'SHADERS_PIXEL_PS_3_0_COMBINERS_MOD.BLS']
    paths = [Path(shaders) / name for name in names]
    expected_hashes = ['ec46462309e02362b52e98d810594406ace8712de5638c518c7024d0db5ec1f2',
                       'b0757bd321bcc7810d3d0120bf32259ee7ce8421d73c1b853919c249bcf0b65a']
    for path, expected in zip(paths, expected_hashes):
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f'Unexpected stock ribbon shader: {path.name}')
    rows = ['# Stock Particle_Unlit, opaque, neutral fog/shadow; tint RGBA=64,128,191,96 texture=204,153,102,192']
    rows += [f'# {p.name} sha256={hashlib.sha256(p.read_bytes()).hexdigest()}' for p in paths]
    rows += ['# effectiveFlags runtimeFlags vertexVariant pixelVariant red green blue alpha']
    vs, ps = [shader_variants(path) for path in paths]
    renderer = Renderer()
    try:
        for flags in (0, 1, 4, 5):
            runtime, vertex, pixel = select(flags)
            rgba = render(renderer, vs[vertex], ps[pixel])
            rows.append(' '.join(str(v) for v in (flags, runtime, vertex, pixel, *rgba)))
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
    print(result)
