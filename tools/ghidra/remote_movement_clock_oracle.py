"""Capture original 006EB730 clock admission and 006E97D0 delay history.

The original routine runs up to its first snapshot mutation, or returns its
future-command decision. Only the world clock accessor is substituted. All
integer arithmetic, clamps, history, and wrapping comparisons execute natively.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_path_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    owner, unit, frame, snapshot, adjustment, path = [native.HEAP + i * 0x4000 for i in range(6)]
    admitted = []

    def intercept(u, address, _size, _data):
        if address == 0x74b330:
            sp = u.reg_read(UC_X86_REG_ESP)
            u.reg_write(UC_X86_REG_EAX, frame)
            u.reg_write(UC_X86_REG_ESP, sp + 4)
            u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        elif address == 0x6eb87e:
            admitted.append(1)
            u.reg_write(UC_X86_REG_EIP, native.STOP)

    uc.hook_add(UC_HOOK_CODE, intercept)
    rows = ['# original Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# reset server receipt frame flags queued path | timeline server-anchor delay adjustment immediate']
    rng = random.Random(12340)
    for origin in [0, 5000, 0xfffff000]:
        uc.mem_write(owner, bytes(0x200))
        # Original 006EBD30 constructor's fixed sixteen -50 / sixteen +50
        # samples and retained delay. The rest of admission runs unchanged.
        uc.mem_write(owner + 0xe8, struct.pack('<32h', *([-50] * 16 + [50] * 16)))
        native.write_words(uc, owner + 0x12c, 50)
        native.write_words(uc, owner + 0x144, unit)
        server, local = origin, origin
        for index in range(160):
            server = (server + rng.choice([-400, 0, 50, 100, 500, 1100, 40000])) & 0xffffffff
            local = (local + rng.choice([10, 50, 100, 500, 2000, 50000])) & 0xffffffff
            receipt = (local + rng.choice([-30, 0, 50])) & 0xffffffff
            flags = rng.choice([0, 1, 0x30, 0x1000, 0x100, 0x800000])
            queued = rng.randrange(2)
            has_path = rng.randrange(4) == 0
            initialized = native.read_words(uc, owner + 0x44, 1)[0] & 0x80000000
            native.write_words(uc, owner + 0x44, flags | initialized)
            native.write_words(uc, owner + 0x140, 2 if queued else 0)
            native.write_words(uc, owner + 0xbc, path if has_path else 0)
            native.write_words(uc, frame + 0x128, local)
            native.write_words(uc, snapshot, server)
            admitted.clear()
            uc.reg_write(UC_X86_REG_ECX, owner)
            invoke(uc, 0x6eb730, [receipt, snapshot, adjustment, 0, 0])
            result = native.read_words(uc, owner + 0xc0, 2)
            result += native.read_words(uc, owner + 0x12c, 1)
            result += native.read_words(uc, adjustment, 1)
            result += (int(bool(admitted)),)
            inputs = [int(index == 0), server, receipt, local, flags, queued, int(has_path)]
            rows.append(' '.join(f'{value:08x}' for value in inputs) + ' | ' + ' '.join(f'{value:08x}' for value in result))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} original remote clock admissions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
