"""Capture build-12340 liquid depth coordinates and animated texture indices.

Executes original 79E3C0 through its two texture-creation calls and original
8A1D60 with resident texture handles and a controlled engine clock. Texture
creation, handle resolution, and the clock are provider boundaries; all depth
arithmetic and frame selection execute the fingerprinted PE. No client entry
point or operating-system code runs. Requires Unicorn and a locally owned PE.
"""
import argparse
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as native


def return_value(uc, value):
    """Return from a cdecl provider without changing its caller-owned arguments."""
    sp = uc.reg_read(UC_X86_REG_ESP)
    uc.reg_write(UC_X86_REG_EAX, value)
    uc.reg_write(UC_X86_REG_ESP, sp + 4)
    uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, sp, 1)[0])


def depth_coordinates():
    """Execute the complete lookup-table arithmetic with texture allocation stubbed."""
    uc = native.emulator()

    def provider(uc, address, size, context):
        if address in (0x79e1a0, 0x8a2e20):
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, provider)
    native.invoke(uc, 0x79e3c0, [])
    return [list(native.read_words(uc, address, 256)) for address in (0xcdf7d0, 0xcdfbd0)]


def texture_frame(count, period, clock):
    """Run resident 8A1D60, returning the selected handle's zero-based ordinal."""
    uc = native.emulator()
    owner, handles = native.HEAP, native.HEAP + 0x1000
    native.write_words(uc, owner + 0x364, 1)
    native.write_words(uc, owner + 0x380, count, handles)
    native.write_words(uc, handles, *range(1, count + 1))

    def provider(uc, address, size, context):
        if address == 0x86ae20:
            return_value(uc, clock)
        elif address == 0x4b6cb0:
            sp = uc.reg_read(UC_X86_REG_ESP)
            return_value(uc, native.read_words(uc, sp + 4, 1)[0])

    uc.hook_add(UC_HOOK_CODE, provider)
    uc.reg_write(UC_X86_REG_ECX, owner)
    native.invoke(uc, 0x8a1d60, [0, period])
    return uc.reg_read(UC_X86_REG_EAX) - 1


def depth_texture(kind, colors, alphas):
    """Execute complete 8A2BF0/8A2AC0 pixel callbacks, including the HSV tail."""
    uc = native.emulator()
    environment, pitch, output = native.HEAP, native.HEAP + 0x400, native.HEAP + 0x404
    native.write_words(uc, environment + 0x10c, *colors)
    native.write_floats(uc, environment + 0x140, alphas)

    def provider(uc, address, size, context):
        if address == 0x7ecef0:
            return_value(uc, environment)

    uc.hook_add(UC_HOOK_CODE, provider)
    native.invoke(uc, 0x8a2ac0 if kind == 2 else 0x8a2bf0,
                  [1, 8, 64, 0, 0, kind, pitch, output])
    assert native.read_words(uc, pitch, 1)[0] == 32
    return bytes(uc.mem_read(native.read_words(uc, output, 1)[0], 2048))


def main():
    """Write small external fixtures with exact float bits and original outputs."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    depths = depth_coordinates()
    frames = []
    for count, period in ((1, 1250), (30, 1250), (60, 1000), (7, 28), (3, 0), (17, 0xffffffff)):
        clocks = {0, 1, 2, 16, 100, 999, 1249, 1250, 1251, 0x7fffffff, 0x80000000, 0xfffffffe, 0xffffffff}
        for frame in range(count + 1):
            boundary = frame * period // count
            clocks.update(max(0, min(0xffffffff, boundary + delta)) for delta in (-1, 0, 1))
        for clock in sorted(clocks):
            frames.append([count, period, clock, texture_frame(count, period, clock)])
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    (output / 'liquid_depth_coordinates.bin').write_bytes(struct.pack('<512I', *depths[0], *depths[1]))
    (output / 'liquid_texture_frames.bin').write_bytes(b''.join(struct.pack('<4I', *row) for row in frames))
    textures = []
    palettes = [
        ([0x00112233, 0x0088aacc, 0x00446699, 0x00bb7733], [0.1, 0.9, 0.25, 0.75]),
        ([0x00000000, 0x00ffffff, 0x00ffffff, 0x00000000], [0., 1., 1., 0.]),
        ([0x00ff0080, 0x000080ff, 0x0000ff00, 0x00ff0000], [0.333, 0.667, 0.15, 0.85]),
        ([0x00666666, 0x00666666, 0x00000000, 0x00ffffff], [0.5, 0.5, 0.5, 0.5]),
    ]
    for colors, alphas in palettes:
        for kind in range(3):
            textures.append(struct.pack('<5I4f', kind, *colors, *alphas)
                            + depth_texture(kind, colors, alphas))
    (output / 'liquid_depth_textures.bin').write_bytes(b''.join(textures))
    print(json.dumps({'depth_values': 512, 'frame_cases': len(frames), 'texture_cases': len(textures)}))


if __name__ == '__main__':
    main()
