"""Capture original deferred landing flags and extended vertical displacement.

The deferred resolver runs its original movement callees; only 5EEB70's
unrelated observer is intercepted. The subject is not the selected input owner,
so input/camera callbacks are excluded. The curve wrapper stores the native
x87 result after negation, matching 7618B0's collision-vector boundary.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP


def capture(executable, deferred_output, vertical_output):
    native.initialize(executable)
    uc = native.emulator()
    movement, unit, guid, wrapper, result = [native.HEAP + i * 0x2000 for i in range(5)]

    def observer(u, address, _size, _data):
        if address == 0x5eeb70:
            sp = u.reg_read(UC_X86_REG_ESP)
            u.reg_write(UC_X86_REG_ESP, sp + 4)
            u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])

    uc.hook_add(UC_HOOK_CODE, observer)
    native.write_words(uc, guid, 123, 0)
    native.write_words(uc, unit + 8, guid)
    rows = ['# before secondary after: original 6EB3B0 deferred transition']
    for axes in [0, 1, 2, 4, 8, 5, 9, 6, 10]:
        for pending in range(128):
            for secondary in [0, 1]:
                before = axes | (pending << 14)
                uc.mem_write(movement, bytes(0x400))
                native.write_words(uc, movement + 0x28, guid)
                native.write_words(uc, movement + 0x144, unit)
                native.write_words(uc, movement + 0x44, before, secondary)
                native.write_floats(uc, movement + 0x90, [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, 3.14, 3.14])
                uc.reg_write(UC_X86_REG_ECX, movement)
                invoke(uc, 0x6eb3b0, [])
                after = native.read_words(uc, movement + 0x44, 1)[0]
                rows.append(f'{before:x} {secondary:x} {after:x}')
    Path(deferred_output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    rows = ['# mode launchSpeed elapsed currentHeight launchHeight vertical: original 9870D0']
    for mode in [0, 0x20000000]:
        for speed in [-7.955547332763672, 0., 12.345, 100.]:
            for elapsed in [0, 1, 17, 249, 411, 823, 3333, 0x1000001, 0xffffffff]:
                for current, launch in [(0., 0.), (1.2345, 0.), (12345.5, 12344.3), (-100.3, -102.1)]:
                    native.write_words(uc, movement + 0x44, mode | 0x1000)
                    native.write_floats(uc, movement + 0x18, [current])
                    native.write_floats(uc, movement + 0x84, [launch])
                    native.write_floats(uc, movement + 0xb8, [speed])
                    # push elapsed; mov ecx, movement; call original; fchs;
                    # fstp dword [result]; ret. No arithmetic replacement.
                    code = b'\x68' + struct.pack('<I', elapsed) + b'\xb9' + struct.pack('<I', movement)
                    code += b'\xe8' + struct.pack('<i', 0x9870d0 - (wrapper + 15))
                    code += b'\xd9\xe0\xd9\x1d' + struct.pack('<I', result) + b'\xc3'
                    uc.mem_write(wrapper, code)
                    uc.ctl_remove_cache(wrapper, wrapper + len(code))
                    invoke(uc, wrapper, [])
                    value = native.read_words(uc, result, 1)[0]
                    rows.append(f'{int(mode != 0)} {bits(speed):08x} {elapsed} {bits(current):08x} {bits(launch):08x} {value:08x}')
    Path(vertical_output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print('Captured 2304 deferred transitions and 288 extended vertical samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('deferred_output')
    parser.add_argument('vertical_output')
    args = parser.parse_args()
    capture(args.executable, args.deferred_output, args.vertical_output)
