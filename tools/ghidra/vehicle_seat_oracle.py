"""Capture native vehicle seat lookup and passenger entry fade eligibility.

The fingerprinted PE executes 5D3340/756EC0, 716650 and 74B8B0. Only the
transport virtual getter and world GUID lookup are supplied. Vehicle and seat
rows use the original 40/58-word WDBC layout, including a sparse seat bank.
No client entry point runs. Requires Unicorn and the locally owned executable.
"""

import argparse
import hashlib
import json
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX

import wmo_registration_oracle as n
from entity_opacity_oracle import return_value


def capture(output):
    u = n.emulator()
    obj, fields, parent, vehicle, row, bank, seat_a, seat_b, vtable = [
        n.HEAP + x for x in (0, 0x2000, 0x4000, 0x6000, 0x7000, 0x8000,
                            0x9000, 0xa000, 0xb000)]
    n.write_words(u, obj, vtable)
    n.write_words(u, obj + 0xd0, fields)
    n.write_words(u, obj + 0x790, 1, 0xf0500000)
    n.write_words(u, vtable + 0x40, n.STOP + 0x100)
    n.write_words(u, row, 1, *([0] * 39))
    n.write_words(u, row + 24, 10, 11, 12, 0, 9, 13, 10, 12)
    n.write_words(u, seat_a, 10, 0, 0xffffffff, *([0] * 55))
    n.write_words(u, seat_b, 12, 0x80000000, 21, *([0] * 55))
    n.write_words(u, 0xad4da8, 12, 10)
    n.write_words(u, 0xad4dbc, bank)
    n.write_words(u, bank, seat_a, 0, seat_b)
    present = [False]

    def transport(u, *_):
        u.reg_write(UC_X86_REG_EDX, 0xf0500000)
        return_value(u, 1)

    u.hook_add(UC_HOOK_CODE, transport, begin=n.STOP + 0x100, end=n.STOP + 0x100)
    u.hook_add(UC_HOOK_CODE, lambda u, *_: return_value(u, parent if present[0] else 0),
               begin=0x4d4db0, end=0x4d4db0)
    lines = ['# owner(0 absent,1 missing row,2 valid) seat_byte parent_present parent_duration resolved_seat_id fade_allowed; all hex']
    for owner in range(3):
        n.write_words(u, parent + 0xf5c, 0 if owner == 0 else vehicle)
        n.write_words(u, vehicle + 12, row if owner == 2 else 0)
        for index in range(256):
            u.mem_write(obj + 0x7d2, bytes([index]))
            u.reg_write(UC_X86_REG_ECX, parent)
            n.invoke(u, 0x5d3340, [index])
            resolved = u.reg_read(UC_X86_REG_EAX)
            seat_id = n.read_words(u, resolved, 1)[0] if resolved else 0
            for available in (0, 1):
                present[0] = bool(available)
                for duration in (0, 1000):
                    n.write_words(u, parent + 0xc4, duration)
                    u.reg_write(UC_X86_REG_ECX, obj)
                    n.invoke(u, 0x716650, [])
                    values = (owner, index, available, duration, seat_id, u.reg_read(UC_X86_REG_EAX))
                    lines.append(' '.join(f'{value:08x}' for value in values))
    output.write_text('\n'.join(lines) + '\n', encoding='ascii')
    return len(lines) - 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    count = capture(args.output)
    metadata = {'executable_sha256': hashlib.sha256(n.data).hexdigest(),
                'functions': ['5D3340', '756EC0', '716650', '74B8B0'],
                'records': count, 'sha256': hashlib.sha256(args.output.read_bytes()).hexdigest()}
    args.output.with_suffix('.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(json.dumps(metadata, indent=2))


if __name__ == '__main__':
    main()
