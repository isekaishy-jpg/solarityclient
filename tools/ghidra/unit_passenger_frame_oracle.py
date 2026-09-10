"""Capture native Unit_C's yaw matrix and ancestor composition.

Executes original 4C3380 (including 4C3290/4C1F00) and 4C2370 on
finite local poses and nested, pitched, scaled parent matrices. No OS or game
entry point runs. Requires Unicorn and the fingerprinted, locally owned PE.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from movement_interval_bounds_oracle import words


def capture(output):
    u = n.emulator()
    matrix, parent = n.HEAP, n.HEAP + 0x1000
    rng = random.Random(12340)
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    parents = [None, identity, [0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 23., -71., 4., 1.],
               [1.2, 0., 0., 0., 0., .7, .8, 0., 0., -.8, .7, 0., 14., 27., -4., 1.]]
    lines = ['# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
             '# has_parent position3 yaw parent16 | matrix16']
    for index in range(512):
        ancestor = parents[index % len(parents)]
        pose = [rng.uniform(-20000., 20000.) for _ in range(3)] + [rng.uniform(-20., 20.)]
        if index < 8:
            pose = [0., -0., 0., [0., -0., 1.5707963705062866, -3.1415927410125732][index % 4]]
        local = identity.copy()
        local[12:15] = pose[:3]
        n.write_floats(u, matrix, local)
        u.reg_write(UC_X86_REG_ECX, matrix)
        n.invoke(u, 0x4c3380, words([pose[3]]))
        if ancestor is not None:
            n.write_floats(u, parent, ancestor)
            u.reg_write(UC_X86_REG_ECX, matrix)
            n.invoke(u, 0x4c2370, [parent])
        result = n.read_words(u, matrix, 16)
        lines.append(str(int(ancestor is not None)) + ' ' +
                     ' '.join(f'{word:08x}' for word in words(pose + (ancestor or identity))) +
                     ' ' + ' '.join(f'{word:08x}' for word in result))
        # Feed original outputs back as nested parents, without reconstructing yaw.
        if index % 8 == 7:
            parents.append(list(struct.unpack('<16f', struct.pack('<16I', *result))))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return len(lines) - 2


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    count = capture(args.output)
    print(json.dumps({'records': count, 'sha256': hashlib.sha256(args.output.read_bytes()).hexdigest()}))
