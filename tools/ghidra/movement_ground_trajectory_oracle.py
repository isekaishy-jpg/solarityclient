"""Capture unmodified build-12340 ground basis, speed, and analytic motion.

Requires Unicorn and the locally owned fingerprinted executable. No native
callee is replaced. This covers ground flags, not swimming/flight or splines.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def invoke(uc, address, arguments):
    sp = native.STACK + 0x18000
    native.write_words(uc, sp, native.STOP, *arguments)
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.emu_start(address, native.STOP, count=100_000)
    assert uc.reg_read(UC_X86_REG_EIP) == native.STOP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, result = native.HEAP, native.HEAP + 0x1000
    speeds = [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, 3.1415927410125732, 3.1415927410125732]
    lines = ['# flags secondary facing elapsed; directionXY speed deltaXYZ facing (float hex bits)']
    for translation in [0, 1, 2, 4, 8, 5, 9, 6, 10]:
        for turn in [0, 0x10, 0x20]:
            for walking in [0, 0x100]:
                for secondary in [0, 8]:
                    for facing in [0., .7, -1.2, 6.28]:
                        for elapsed in [0, 1, 16, 250, 1000, 10001, 0x1000001, 0xffffffff]:
                            flags = translation | turn | walking
                            uc.mem_write(unit, bytes(0x400))
                            native.write_words(uc, unit + 0x44, flags, secondary)
                            native.write_floats(uc, unit + 0x58, [facing, 0.])
                            native.write_floats(uc, unit + 0x90, speeds)
                            uc.reg_write(UC_X86_REG_ECX, unit)
                            invoke(uc, 0x9880c0, [0])
                            uc.reg_write(UC_X86_REG_ECX, unit)
                            invoke(uc, 0x987ef0, [0])
                            native.write_floats(uc, result, [0., 0., 0., facing, 0.])
                            uc.reg_write(UC_X86_REG_ECX, unit)
                            invoke(uc, 0x987b50, [elapsed, result, result + 12, result + 16])
                            values = [*native.read_words(uc, unit + 0x70, 2), *native.read_words(uc, unit + 0x8c, 1), *native.read_words(uc, result, 4)]
                            lines.append(f'{flags:x} {secondary:x} {bits(facing):08x} {elapsed} ' + ' '.join(f'{v:08x}' for v in values))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines)-1} unmodified native calls')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
