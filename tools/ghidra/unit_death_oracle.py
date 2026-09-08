"""Capture resident 12340 death admission and entry without running the client.

Executes 71F560, 71DDE0 and 73AF80. The fixture supplies a stand accessor,
current animation ID and AnimationData rows; records 7385C0 requests. Transport
water-height admission is excluded here (no transport pointer).
"""
import argparse
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, fields, movement, vtable, row, bank = [native.HEAP + x for x in (0, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000)]
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 0xd0, fields)
    native.write_words(uc, unit + 0xd8, movement)
    native.write_words(uc, vtable + 0x138, native.STOP + 16)
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 0)
    native.write_words(uc, 0xad30e8, bank)
    native.write_words(uc, bank, row)
    stand = 0
    requests = []

    def hook(uc, address, size, data):
        if address == native.STOP + 16:
            return_value(uc, stand)
        elif address == 0x717260:
            return_value(uc, 0)
        elif address == 0x7385c0:
            sp = uc.reg_read(UC_X86_REG_ESP)
            requests.append(native.read_words(uc, sp + 4, 2)[0])
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 12)

    uc.hook_add(UC_HOOK_CODE, hook)
    lines = ['# Native 71F560 dead predicate and 73AF80 death entry, no transport.']
    for health in [0, 1, 100, 0x7fffffff, 0x80000000, 0xffffffff]:
        for secondary in [0, 1, 2]:
            for stand in [0, 1, 7, 9]:
                native.write_words(uc, fields + 0x48, health)
                native.write_words(uc, fields + 0xd8, secondary)
                uc.reg_write(UC_X86_REG_ECX, unit)
                native.invoke(uc, 0x71f560, [])
                lines.append(f'dead {health:08x} {secondary:08x} {stand} {uc.reg_read(UC_X86_REG_EAX)}')
    for behavior in [0, 1, 6, 8, 9, 37, 131, 132, 133, 465, 466, 467, 468, 469, 472, 473]:
        native.write_words(uc, row + 0x18, behavior)
        for flags in [0, 1, 0x1000, 0x200000, 0x200001]:
            native.write_words(uc, movement + 0x44, flags)
            requests.clear()
            uc.reg_write(UC_X86_REG_ECX, unit)
            native.invoke(uc, 0x73af80, [0])
            lines.append(f'entry {behavior} {flags:08x} {requests[0] if requests else -1}')
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    output = capture()
    Path(args.output).write_text(output, encoding='utf-8')
    print(f'Captured {len(output.splitlines()) - 1} native death cases')
