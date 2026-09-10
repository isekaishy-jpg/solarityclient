"""Capture build-12340's ordinary D3D9 font projection at 006C0BA0.

Only device-capability queries and the final matrix submission are hooked.
The original viewport rounding and 006BF5B0 matrix arithmetic execute intact.
Outputs are D3D9 framebuffer positions, before conversion to Vulkan's sample
grid. Requires Unicorn and the locally owned, fingerprinted Wow.exe.
"""
import argparse
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture():
    u = n.emulator()
    device = n.HEAP
    vtable = device + 0x2000
    caps = vtable + 0x1000
    submit = n.STOP + 0x100
    depth_convention = n.STOP + 0x110
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device, vtable)
    n.write_words(u, vtable + 0xa0, submit)
    n.write_words(u, vtable + 0x140, depth_convention)
    n.write_words(u, caps + 4, 0)  # D3D9 backend.
    n.write_floats(u, device + 0xf70, [0., 1., 0., 1., 0., 1.])
    matrix = None

    def hook(u, address, size, data):
        nonlocal matrix
        if address not in (0x532af0, submit, depth_convention):
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        ret, argument = n.read_words(u, sp, 2)
        if address == submit:
            matrix = n.read_floats(u, argument, 16)
        u.reg_write(UC_X86_REG_EAX, caps if address == 0x532af0 else 0)
        u.reg_write(UC_X86_REG_EIP, ret)
        u.reg_write(UC_X86_REG_ESP, sp + (8 if address == submit else 4))

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# width height inputX inputY nativeFramebufferX nativeFramebufferY']
    for width, height in [(1280, 720), (2560, 1440)]:
        n.write_words(u, 0xc7d2c4, height, width)
        matrix = None
        n.invoke(u, 0x6c0ba0, [])
        assert matrix is not None, 'font projection was not submitted'
        for x, y in [(0., 0.), (100., 200.), (-20., 347.), (600., 697.)]:
            projected_x = (matrix[0] * x + matrix[12] + 1.) * width / 2.
            projected_y = (1. - (matrix[5] * y + matrix[13])) * height / 2.
            rows.append(' '.join(map(str, [width, height, x, y, projected_x, projected_y])))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(), encoding='utf-8')
