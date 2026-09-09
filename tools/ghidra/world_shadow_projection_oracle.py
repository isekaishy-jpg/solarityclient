"""Capture original primary shadow projections from the fingerprinted 12340 PE.

The only substituted routine is the camera-origin provider, supplied explicitly
by each case. Original 7BAC10/8750B0 matrix arithmetic, 7BB570 light adjustment,
and 875F80 center quantization execute in Unicorn without an OS entry point.
The frame probe stops at the projection callback before any GPU work.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_EAX, UC_X86_REG_EBP
import wmo_registration_oracle as native


def float_word(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture(executable):
    native.initialize(executable)
    lines = ['# primary size center_xyz origin_xyz daynight_direction_xyz adjusted_xyz snapped_xyz caster_view16 receiver_rows12']
    identity = [float(index % 5 == 0) for index in range(16)]
    for size, center, origin, sunlight in itertools.product(
            [1024, 2048],
            [(0.1251, -0.6254, 3.25), (-8949.951, -132.493, 83.531)],
            [(0., 0., 0.), (-8956.75, -129.25, 88.5)],
            [(0.3, 0.4, -0.8660254), (-0.7, 0.2, -0.1), (0.6, -0.8, -0.4)]):
        uc = native.emulator()
        device, light, anchor, up, output = [native.HEAP + offset for offset in
                (0, 0x3000, 0x3200, 0x3300, 0x3400)]
        native.write_words(uc, 0xc5df88, device)
        native.write_floats(uc, device + 0x1b00, identity)
        native.write_words(uc, 0xce04a8, light)
        native.write_floats(uc, light + 0x7c, sunlight)
        native.write_floats(uc, anchor, center)
        native.write_floats(uc, up, [1., 0., 0.])
        adjusted, snapped, caster_view = [], [], []

        def hook(machine, address, instruction_size, user_data):
            sp = machine.reg_read(UC_X86_REG_ESP)
            if address == 0x4f6650:
                destination = native.read_words(machine, sp + 4, 1)[0]
                native.write_floats(machine, destination, origin)
                machine.reg_write(UC_X86_REG_EAX, destination)
                machine.reg_write(UC_X86_REG_EIP, native.read_words(machine, sp, 1)[0])
                machine.reg_write(UC_X86_REG_ESP, sp + 4)
            elif address == 0x875c10:
                adjusted[:] = native.read_floats(machine, native.read_words(machine, sp + 4, 1)[0], 3)
                machine.reg_write(UC_X86_REG_EIP, native.STOP)
            elif address == native.STOP + 16:
                snapped[:] = native.read_floats(machine, 0xd43260, 3)
                scene = native.read_words(machine, sp + 16, 1)[0]
                assert native.read_floats(machine, scene + 0x930, 3) == native.read_floats(machine, anchor, 3)
                machine.reg_write(UC_X86_REG_EIP, native.STOP)
            elif address == 0x7baece:
                caster_view[:] = native.read_floats(machine, machine.reg_read(UC_X86_REG_EBP) - 0xf0, 16)

        uc.hook_add(UC_HOOK_CODE, hook)
        native.invoke(uc, 0x7bb570, [])
        native.write_floats(uc, 0xd43180, adjusted)
        native.write_words(uc, 0xb1d51c, 0)
        native.write_words(uc, 0xd43154, 1 if size == 1024 else 2)
        native.write_words(uc, 0xd43150, size)
        native.write_floats(uc, 0xd43258, [20.])
        native.write_words(uc, 0xd43158, 0x7bac10)
        native.write_words(uc, 0xd4315c, native.STOP + 16)
        for address in (0xd43160, 0xd43164):
            native.write_words(uc, address, 1)
        native.invoke(uc, 0x875f80, [anchor, 1])
        native.write_floats(uc, 0xd43278, [1., 0., 0.])
        native.write_floats(uc, 0xd431bc, [float(size)])
        native.write_floats(uc, 0xd431c8, [0.00025])
        native.invoke(uc, 0x8750b0, [])
        receiver = native.read_floats(uc, 0xd43348, 12)
        # Execute the common original eye/look-at path with the unsnapped caster center.
        native.invoke(uc, 0x7bac10, [anchor, float_word(20.), output, up, 0xffffffff])
        values = [size, *native.read_floats(uc, anchor, 3), *origin, *sunlight,
                  *adjusted, *snapped, *caster_view, *receiver]
        lines.append('primary ' + ' '.join(map(str, values)))
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.executable))
