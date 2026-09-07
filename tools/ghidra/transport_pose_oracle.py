"""Execute the original transport matrix and packed-quaternion operations.

Uses synthetic Euler angles only. All FSINCOS, matrix, quaternion, and integer
packing instructions run unmodified; no calculation is replaced with Python.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from movement_path_oracle import invoke
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    """Cover trace/diagonal branches, negative W, and independently varying angles."""
    native.initialize(executable)
    uc = native.emulator()
    matrix, quaternion, packed = native.HEAP, native.HEAP + 0x100, native.HEAP + 0x120
    assert native.read_words(uc, 0xaa2e4c, 3) == (1, 2, 0)
    cases = [(0., 0., 0.), (1., 0., 0.), (1., .3, .2), (-2.8, 1.3, -1.1)]
    for axis in range(3):
        for angle in [-3.1415927, -1.5707964, 1.5707964, 3.1415927]:
            angles = [0., 0., 0.]
            angles[axis] = angle
            cases.append(tuple(angles))
    rng = random.Random(12340)
    cases.extend(tuple(rng.uniform(-3.1415927, 3.1415927) for _ in range(3)) for _ in range(512))
    lines = ['# Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# xyz/yaw/pitch/roll IEEE754 hex, packed quaternion hex, 16 matrix IEEE754 hex']
    for angles in cases:
        xyz = tuple(rng.uniform(-20000, 20000) for _ in range(3))
        native.write_floats(uc, matrix, [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, *xyz, 1])
        for address, angle in zip((0x4c3380, 0x4c3340, 0x4c3300), angles):
            uc.reg_write(UC_X86_REG_ECX, matrix)
            invoke(uc, address, [bits(angle)])
        uc.reg_write(UC_X86_REG_ECX, quaternion)
        invoke(uc, 0x982910, [matrix])
        uc.reg_write(UC_X86_REG_ECX, packed)
        invoke(uc, 0x4f43b0, [quaternion])
        rotation = struct.unpack('<Q', uc.mem_read(packed, 8))[0]
        fields = [*(f'{bits(v):08x}' for v in (*xyz, *angles)), f'{rotation:016x}',
                  *(f'{v:08x}' for v in native.read_words(uc, matrix, 16))]
        lines.append(' '.join(fields))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'captured {len(cases)} original transport poses')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
