"""Capture complete original Path.cpp length and position/direction evaluation.

Executes 004C41C0, 004C41B0 and 004C3980 with their original vtable and callees.
Only native storage is initialized externally; no geometry routines are replaced.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP


def invoke(uc, address, arguments):
    """Allow a complete long smooth path to execute its twenty-sample caches."""
    sp = native.STACK + 0x18000
    native.write_words(uc, sp, native.STOP, *arguments)
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.emu_start(address, native.STOP, count=2_000_000)
    assert uc.reg_read(UC_X86_REG_EIP) == native.STOP, hex(uc.reg_read(UC_X86_REG_EIP))


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    path, result, extra_nodes, extra_lengths = [native.HEAP + i * 0x4000 for i in range(4)]
    uc.mem_write(native.STOP + 16, b'\xd9\x1d' + struct.pack('<I', path + 4))
    random_source = random.Random(12340)
    paths = [
        [(-5., 0., 0.), (0., 0., 0.), (5., 0., 0.), (10., 0., 0.)],
        [(-5., 0., 0.), (0., 0., 0.), (2., 0., 0.), (2., 8., 2.), (3., 14., 4.)],
        [(0., 0., 0.)] * 4,
    ]
    for count in [4, 5, 9, 25, 26, 31, 40]:
        paths.append([tuple(random_source.uniform(-200., 200.) for _ in range(3)) for _ in range(count)])
    rows = ['# original Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# path mode nodes(hex xyz) | cached-length(hex) | fraction(hex) position(hex xyz) direction(hex xyz)']
    for nodes in paths:
        for mode in [0, 1]:
            uc.mem_write(path, bytes(0x200))
            native.write_words(uc, path, 0x9e2f28)
            native.write_words(uc, path + 0x144, len(nodes))
            native.write_words(uc, path + 0x1c0, mode)
            native.write_words(uc, path + 0x13c, extra_nodes)
            native.write_words(uc, path + 0x1b4, extra_lengths)
            for index, point in enumerate(nodes):
                destination = path + 8 + index * 12 if index < 25 else extra_nodes + (index - 25) * 12
                native.write_floats(uc, destination, point)
            uc.reg_write(UC_X86_REG_ECX, path)
            invoke(uc, 0x4c41c0, [])
            uc.reg_write(UC_X86_REG_ECX, path)
            invoke(uc, 0x4c41b0, [])
            uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
            prefix = f'path {mode} ' + ' '.join(f'{bits(v):08x}' for point in nodes for v in point)
            prefix += f' | {native.read_words(uc, path + 4, 1)[0]:08x} | '
            for fraction in [-1., 0., .0001, .1, .25, .5, .75, .9999, 1., 2.]:
                native.write_floats(uc, result, [.3, .4, .5, 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.])
                uc.reg_write(UC_X86_REG_ECX, path)
                invoke(uc, 0x4c3980, [bits(fraction), result, 1])
                values = native.read_words(uc, result + 48, 3) + native.read_words(uc, result, 3)
                rows.append(prefix + f'{bits(fraction):08x} ' + ' '.join(f'{value:08x}' for value in values))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} original path samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
