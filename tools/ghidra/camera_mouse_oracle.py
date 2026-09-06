"""Capture build-12340's mouse-to-camera angular scaling instructions.

Executes 602134..6021FC, including original 47C020 coordinate conversion,
against the fingerprinted PE. Input deltas are already in converted pixel
coordinates (the coordinate scale is one). Unit/camera admission, banking,
following, and cursor lifetime are outside this arithmetic capture.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_ESI, UC_X86_REG_EBX,
    UC_X86_REG_EDI, UC_X86_REG_FPSW, UC_X86_REG_FPTAG, UC_X86_REG_EIP,
)
import wmo_registration_oracle as native


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    uc = native.emulator()
    frame = native.STACK + 0x18000
    camera = native.HEAP
    variables = [0xC24E50, 0xC24E54, 0xC249A4, 0xC249A8, 0xC24E6C, 0xC24E70]
    for index, variable in enumerate(variables):
        native.write_words(uc, variable, native.HEAP + 0x1000 + index * 0x100)
    native.write_floats(uc, 0xAC0CB4, [1., 1.])
    lines = ['# dx dy yawSpeed pitchSpeed invertYaw invertPitch | yawAngle pitchAngle; floats are raw hex bits']
    for delta, speeds, flips in itertools.product(
        [(1., 1.), (-1., -1.), (0., 0.), (0.125, -0.375), (17., -29.), (800., 600.), (-1600., 1200.)],
        [(180., 90.), (0.1, 0.1), (360., 360.), (72.5, 123.75)],
        [(0, 0), (1, 0), (0, 1), (1, 1)],
    ):
        for index, value in enumerate([*speeds, 0., 0.05]):
            native.write_floats(uc, native.HEAP + 0x102c + index * 0x100, [value])
        for index, value in enumerate([flips[1], flips[0]]):
            native.write_words(uc, native.HEAP + 0x1430 + index * 0x100, value)
        native.write_floats(uc, frame + 8, delta)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x20)
        uc.reg_write(UC_X86_REG_ESI, camera)
        uc.reg_write(UC_X86_REG_EDI, 0)
        uc.reg_write(UC_X86_REG_EBX, 0)
        uc.reg_write(UC_X86_REG_FPSW, 0)
        uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
        uc.emu_start(0x602134, 0x6021FC, count=10000)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x6021FC
        yaw = native.read_floats(uc, frame - 12, 1)[0]
        pitch = native.read_floats(uc, frame + 16, 1)[0]
        # The original block supplies the two inversion signs; multiplication
        # by +/-1 preserves the stored float bits including signed zero.
        yaw *= native.read_floats(uc, frame - 8, 1)[0]
        pitch *= native.read_floats(uc, frame - 4, 1)[0]
        def bits(value):
            return f'{struct.unpack("<I", struct.pack("<f", value))[0]:08x}'
        lines.append(' '.join(map(bits, [*delta, *speeds])) +
                     f' {flips[0]} {flips[1]} | {bits(yaw)} {bits(pitch)}')
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines) - 1} native mouse-angle cases')


if __name__ == '__main__':
    main()
