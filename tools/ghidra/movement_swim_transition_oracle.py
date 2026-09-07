"""Capture original 989660/98BFF0/9883F0 swimming command state.

Only the downstream 5EEB70 owner notification is intercepted. Basis selection,
speed retention, flag transitions, pitch and fall launch execute original code.
"""
import argparse
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, owner = native.HEAP, native.HEAP + 0x1000
    speeds = [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, 3.1415927410125732, 3.1415927410125732]
    records = bytearray()

    def notification(uc, address, size, data):
        if address == 0x5eeb70:
            stack = uc.reg_read(UC_X86_REG_ESP)
            destination = native.read_words(uc, stack, 1)[0]
            uc.reg_write(UC_X86_REG_ESP, stack + 4)
            uc.reg_write(UC_X86_REG_EIP, destination)

    uc.hook_add(UC_HOOK_CODE, notification)
    def invalid(uc, access, address, size, value, data):
        print(f'invalid memory {address:x} at {uc.reg_read(UC_X86_REG_EIP):x}')
        return False
    uc.hook_add(UC_HOOK_MEM_INVALID, invalid)
    cases = itertools.product(range(3), [0, 1, 2, 4, 5, 6], [0, 0x20], [0., .5, -.9])
    for operation, translation, secondary, pitch in cases:
        flags = translation | (0 if operation == 0 else 0x200000)
        uc.mem_write(unit, bytes(0x400))
        native.write_words(uc, unit + 0x28, owner)
        native.write_words(uc, owner, 1, 0)
        native.write_words(uc, unit + 0x44, flags, secondary)
        native.write_floats(uc, unit + 0x10, [10., 20., 30.])
        native.write_floats(uc, unit + 0x20, [.7, pitch])
        native.write_floats(uc, unit + 0x58, [.7, pitch])
        native.write_floats(uc, unit + 0x90, speeds)
        for address in [0x9880c0, 0x987ef0]:
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, address, [0])
        uc.reg_write(UC_X86_REG_ECX, unit)
        invoke(uc, [0x989660, 0x98bff0, 0x9883f0][operation], [1] if operation == 2 else [])
        values = [operation, flags, secondary, bits(pitch)]
        for offset, count in [(0x44, 1), (0x24, 1), (0x64, 3), (0x8c, 1), (0x80, 2), (0xb8, 1)]:
            values.extend(native.read_words(uc, unit + offset, count))
        records.extend(struct.pack('<13I', *values))
    Path(output).write_bytes(records)
    print(f'Captured {len(records) // 52} original swimming commands')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
