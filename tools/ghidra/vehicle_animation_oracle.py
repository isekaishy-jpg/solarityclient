"""Capture native passenger animation selection and independent completion bits.

747B20, 747BD0, 748560, 7485B0 and 7484E0 run unmodified, including 74BB60's
special-exit flag selection. Synthetic unit health, movement flags and seat rows
are inputs. This covers selection/completion policy, not M2 slot publication.
"""
import argparse
import hashlib
import itertools
import json
import random
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(output):
    u = n.emulator()
    owner, child, fields, seat, selected = [n.HEAP + i * 0x1000 for i in range(5)]
    n.write_words(u, owner + 0xc, child)
    n.write_words(u, owner + 0x54, seat)
    n.write_words(u, child + 0xd0, fields)

    def call(address, *args):
        sp = n.STACK + 0x18000
        n.write_words(u, sp, n.STOP, *args)
        u.reg_write(UC_X86_REG_ESP, sp)
        u.reg_write(UC_X86_REG_ECX, owner)
        # The instruction bound also terminates malformed inputs, without a
        # Windows watchdog thread for every small selector invocation.
        u.emu_start(address, n.STOP, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == n.STOP
        return u.reg_read(UC_X86_REG_EAX)

    def movement(address):
        n.write_words(u, selected, 506)
        admitted = call(address, 0, selected)
        return n.read_words(u, selected, 1)[0] if admitted else 506

    cases = []
    flags = [0, 1, 2, 4, 6, 7, 8, 0x800f]
    for phase, flag, completed, special, alive, missing, key in itertools.product(
        range(6), flags, (0, 2, 4, 6), (0, 1), (0, 1), (0, 1), (-1, 26, 4),
    ):
        sequences = [96, 91, 115, 116, 117, 118, 99, 100]
        if missing:
            sequences[::2] = [-1] * 4
        cases.append((phase, flag, completed, special, alive, *sequences, key))
    rng = random.Random(748560)
    for _ in range(512):
        cases.append((rng.randrange(6), rng.getrandbits(32), rng.getrandbits(32),
                      rng.randrange(2), rng.randrange(2),
                      *(rng.choice([-1, 0, 96, 506, 65535, -2]) for _ in range(8)),
                      rng.choice([-1, 0, 4, 6, 26, 27])))
    lines = ['# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
             '# phase flags completed special alive enter2 seated2 secondary2 exit2 key | primary secondary early late completedAfter primaryAfter secondaryAfter']
    for values in cases:
        phase, flag, completed, special, alive, *rest = values
        sequences, key = rest[:8], rest[8]
        n.write_words(u, owner + 0x10, completed & 0xffffffff, phase)
        n.write_words(u, fields + 0x48, alive)
        n.write_words(u, child + 0x7d0, special * 0x40)
        n.write_words(u, seat + 4, flag)
        n.write_words(u, seat + 0x34, *(value & 0xffffffff for value in sequences[:6]))
        n.write_words(u, seat + 0x68, *(value & 0xffffffff for value in sequences[6:]))
        before = [call(0x747b20, seat), call(0x747bd0, seat), movement(0x748560), movement(0x7485b0)]
        call(0x7484e0, key & 0xffffffff)
        after = [n.read_words(u, owner + 0x10, 1)[0], call(0x747b20, seat), call(0x747bd0, seat)]
        lines.append(' '.join(f'{value & 0xffffffff:08x}' for value in (*values, *before, *after)))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return dict(records=len(cases), sha256=hashlib.sha256(output.read_bytes()).hexdigest())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
