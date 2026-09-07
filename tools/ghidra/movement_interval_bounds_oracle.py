"""Capture build-12340 interval collection bounds from original instructions.

Executes 75FF90 from entry until 760515, immediately before query-mask selection
and world collection. No function is replaced. Body bounds, mode branches,
controlled-unit step policy, and the fall trajectory execute native code.
Inputs have no passenger parent; transport lookup and geometry collection are
outside this capture. Outputs contain scalar float images, never PE code.
"""
import argparse
import math
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
    UC_X86_REG_FPCW, UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)


def words(values):
    return struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values))


def install_transport_provider(uc, unit, matrix):
    """Supply one resident parent matrix at the external GUID-provider boundary."""
    source = native.HEAP + 0x9000
    native.write_floats(uc, source, matrix)
    native.write_words(uc, unit + 8, 2, 0)
    native.write_words(uc, unit + 0x28, source + 64)
    native.write_words(uc, source + 64, 1, 0)
    # Cdecl 74B4C0 receives its output pointer as the third argument.
    code = b'\x8b\x44\x24\x0c'
    for offset in range(0, 64, 4):
        code += b'\x8b\x15' + struct.pack('<I', source + offset)
        code += b'\x89\x50' + bytes([offset])
    uc.mem_write(0x74b4c0, code + b'\xb8\x01\x00\x00\x00\xc3')


def capture(uc, mode, player, slow, duration, fall_ms, values, parent_matrix=None):
    origin, radius, height, distance = values[:3], values[3], values[4], values[5]
    direction, step, launch, downward = values[6:9], values[9], values[10], values[11]
    unit, owner, fields, kind = [native.HEAP + n for n in (0, 0x1000, 0x2000, 0x3000)]
    uc.mem_write(native.HEAP, bytes(0x4000))
    native.write_words(uc, unit + 0x144, owner)
    native.write_words(uc, owner + 0xd0, fields)
    native.write_words(uc, owner + 8, kind)
    native.write_words(uc, kind + 8, 16 if player else 0)
    flags = [1, 0x1000, 0x2000000][mode] | (0x20000000 if slow else 0)
    native.write_words(uc, unit + 0x44, flags)
    native.write_words(uc, unit + 0x80, fall_ms)
    native.write_floats(uc, unit + 0x10, origin)
    native.write_floats(uc, unit + 0xc8, [radius, height, step])
    native.write_floats(uc, unit + 0x84, [launch])
    native.write_floats(uc, unit + 0xb8, [downward])
    if parent_matrix is not None:
        install_transport_provider(uc, unit, parent_matrix)
    sp = native.STACK + 0x18000
    native.write_words(uc, sp, native.STOP, words([distance])[0], duration, *words(direction))
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.reg_write(UC_X86_REG_ECX, unit)
    uc.reg_write(UC_X86_REG_FPCW, 0x037f)
    uc.reg_write(UC_X86_REG_FPSW, 0)
    uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
    uc.emu_start(0x75ff90, 0x760515, count=100_000)
    assert uc.reg_read(UC_X86_REG_EIP) == 0x760515
    body = native.read_words(uc, uc.reg_read(UC_X86_REG_EBP) - 0x48, 6)
    query = native.read_words(uc, 0xca1660, 6)
    return body + query


def cases():
    # Include degenerate travel and branch thresholds, both unit profiles,
    # rising/descending falls, large clocks, and world-coordinate rounding.
    for mode in range(3):
        for player in (0, 1):
            for slow in (0, 1):
                for distance in (0., 0.001, 0.112, 0.5, 1., 2., 8.):
                    for step in (0., 0.25, 1., 2.):
                        yield mode, player, slow, 16, 71, [
                            0., 0., 0., 0.5, 2., distance, 0.6, 0.8, 0., step, 0.4, -7.95,
                        ]
    for clock in (0, 1, 500, 2000, 0xffffff, 0x1000001, 0x7fffffff, 0x80000000, 0xffffffff):
        for duration in (0, 1, 16, 0xffffffff):
            for slow in (0, 1):
                yield 1, 1, slow, duration, clock, [
                    7123.45, -9812.6, 101.123, 0.7, 3., 1.25, -0.3, 0.4, 0.5, 0.9, 123.456, 20.,
                ]
    rng = random.Random(12340)
    for _ in range(1000):
        mode = rng.randrange(3)
        angle, elevation = rng.uniform(-math.pi, math.pi), rng.uniform(-1.5, 1.5)
        direction = [math.cos(angle) * math.cos(elevation), math.sin(angle) * math.cos(elevation), math.sin(elevation)]
        origin = [rng.uniform(-16000, 16000), rng.uniform(-16000, 16000), rng.uniform(-100, 2000)]
        radius = rng.uniform(0.05, 3.)
        yield mode, rng.randrange(2), rng.randrange(2), rng.randrange(0, 10000), rng.randrange(0, 10000), [
            *origin, radius, radius * rng.uniform(2., 6.), rng.uniform(0., 40.), *direction,
            rng.uniform(0., 5.), origin[2] + rng.uniform(-20., 20.), rng.uniform(-20., 80.),
        ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    uc = native.emulator()
    for address in (0xa37f14, 0xa37f78, 0xa37f7c, 0xa37f80):
        print(f'constant {address:08x}: {native.read_words(uc, address, 1)[0]:08x}')
    lines = [
        '# Native 75FF90 entry through 760515; x87 037F; no substituted callees.',
        '# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
        '# No transport. Mode 0 ground / 1 falling / 2 swimming-or-flying bounds.',
        '# mode player slow duration_ms fall_ms | origin3 radius height distance direction3 step launch_height downward_speed | body6 query6',
    ]
    for mode, player, slow, duration, fall_ms, values in cases():
        # Both the recorded inputs and native image receive the same f32 values.
        inputs = words(values)
        values = struct.unpack('<12f', struct.pack('<12I', *inputs))
        result = capture(uc, mode, player, slow, duration, fall_ms, values)
        lines.append(f'{mode} {player} {slow} {duration} {fall_ms} ' + ' '.join(f'{v:08x}' for v in inputs + result))
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines) - 4} original interval bounds')


if __name__ == '__main__':
    main()
