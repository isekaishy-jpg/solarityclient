"""Capture native spline snap decisions and missing-geometry travel normals.

Runs original instruction ranges from 6E9C30, 6E9470, and 75D3C0 without hooks.
The pure ranges stop before owner callbacks; they do not establish collision
collection, complete spline sampling, or finalization side effects.
"""

import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_ESI, UC_X86_REG_ESP


def encoded(values):
    """Serialize float storage without decimal conversion."""
    return ' '.join(f'{word:08x}' for word in struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values)))


def capture(executable, output):
    """Cover strict/non-strict snap boundaries and degenerate travel vectors."""
    native.initialize(executable)
    rng = random.Random(69470)
    cases = []
    for duration in [1, 16, 50, 250, 1000]:
        for distance in [0., .00001, .001, .5, 2.9999998, 3., 3.0000002, 6., 60.]:
            for direction in [[1., 0., 0.], [0., 0., 1.], [.6, .8, 0.]]:
                cases.append(([0., 0., 0.], [v * distance for v in direction], duration, 1, 0))
    for _ in range(200):
        start = [rng.uniform(-17000., 17000.) for _ in range(3)]
        end = [value + rng.uniform(-4., 4.) for value in start]
        cases.append((start, end, rng.randrange(1, 251), rng.choice([0, 1, 0x10, 0x1000, 0x200000, 0x40000000]), rng.randrange(2)))
    lines = ['# build 12340 original 6E9D3A snap branch, 6E9470 correction, 75D3C0 travel normal; no hooks', '# start3 target3 (hex), duration flags force pre_snap post_snap (decimal), normal3 (hex)']
    for start, end, duration, flags, force in cases:
        uc = native.emulator()
        movement, target = native.HEAP, native.HEAP + 0x1000
        native.write_floats(uc, movement + 0x10, start)
        native.write_floats(uc, target, end)
        native.write_words(uc, movement + 0x44, flags)
        frame = native.STACK + 0x18000
        native.write_words(uc, frame, 0, native.STOP, 0, duration)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
        uc.reg_write(UC_X86_REG_EBX, target)
        uc.reg_write(UC_X86_REG_ESI, movement)
        marker = native.HEAP + 0x2000
        uc.mem_write(native.STOP, b'\xc6\x05' + struct.pack('<I', marker) + b'\x01\xe9' + struct.pack('<i', 0x6e9db1 - (native.STOP + 12)))
        uc.emu_start(0x6e9d3a, 0x6e9db1, timeout=1_000_000, count=100_000)
        pre = int(native.read_words(uc, marker, 1)[0] == 0)
        native.write_words(uc, marker, 0)
        uc.mem_write(native.STOP, b'\xc6\x05' + struct.pack('<I', marker) + b'\x01\xe9' + struct.pack('<i', 0x6e94bd - (native.STOP + 12)))
        uc.ctl_remove_cache(native.STOP, native.STOP + 4096)
        native.write_words(uc, frame, native.STOP, target, force)
        uc.reg_write(UC_X86_REG_ESP, frame)
        uc.reg_write(UC_X86_REG_ECX, movement)
        uc.emu_start(0x6e9470, 0x6e94bd, timeout=1_000_000, count=100_000)
        post = int(native.read_words(uc, marker, 1)[0] == 0)
        native.write_words(uc, frame, native.STOP, target, 0, duration)
        uc.reg_write(UC_X86_REG_ESP, frame)
        uc.reg_write(UC_X86_REG_ECX, movement)
        uc.emu_start(0x75d3c0, 0x75d475, timeout=1_000_000, count=100_000)
        normal = native.read_floats(uc, movement + 0x38, 3)
        lines.append(encoded(start + end) + f' {duration} {flags} {force} {pre} {post} ' + encoded(normal))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(cases), 'original path ground transitions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
