"""Capture original held-input resolvers, with unit side effects intercepted.

Only 5FAE70/5FAFB0/5FB0B0 arithmetic, branching, and active-bit writes are
claimed. Unit readiness/vehicle answers are supplied; emitted movement calls
are recorded without invoking their asynchronous native movement event owner.
"""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EDX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    control, unit, fields, movement, vtable = [native.HEAP + i * 0x1000 for i in range(5)]
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 0xd0, fields)
    native.write_words(uc, unit + 0xd8, movement)
    native.write_words(uc, vtable + 0x138, native.STOP + 0x100)
    emitted = []
    forced = False
    yaw = False
    def hook(u, address, _size, _data):
        calls = {0x71ae10: (4, 0xb7), 0x71ae20: (4, 0xba), 0x71ae40: (4, 0xbe)}
        starts = {0x72e5d0: (0xb5, 0xb6), 0x72e680: (0xb8, 0xb9), 0x72e7e0: (0xbc, 0xbd)}
        if address in starts:
            positive = native.read_words(u, u.reg_read(UC_X86_REG_ESP) + 8, 1)[0]
            pop, event = 8, starts[address][0 if positive else 1]
            emitted.append(event)
            answer = 0
        elif address in calls:
            pop, event = calls[address]; emitted.append(event); answer = 0
        elif address in [0x74bb90, 0x74b9a0, 0x4d3790, 0x4d4db0, native.STOP + 0x100]:
            pop = 0
            answer = int(forced) if address == 0x74bb90 else int(yaw) if address == 0x74b9a0 else 0
            u.reg_write(UC_X86_REG_EDX, 0)
        else:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, answer)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
    uc.hook_add(UC_HOOK_CODE, hook)
    lines = ['# held activeFlags forced yaw -> resultingHeld eventOpcodes']
    controls = [0x10, 0x20, 0x40, 0x80, 0x100, 0x200, 0x1000, 1, 2]
    for mask in range(512):
        held = sum(bit for index, bit in enumerate(controls) if mask & (1 << index))
        for active in range(8):
            before = held | (active << 16)
            for flags in [0, 1, 2]:
                forced = mask % 17 == 0
                yaw = mask % 11 == 0
                native.write_words(uc, control + 4, before)
                native.write_words(uc, movement + 0x44, flags)
                emitted.clear()
                for entry in [0x5fae70, 0x5fafb0, 0x5fb0b0]:
                    uc.reg_write(UC_X86_REG_ECX, control)
                    invoke(uc, entry, [1234, unit])
                after = native.read_words(uc, control + 4, 1)[0]
                lines.append((f'{before:x} {flags:x} {int(forced)} {int(yaw)} {after:x} ' + ' '.join(f'{event:x}' for event in emitted)).rstrip())
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines)-1} native resolver combinations')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
