"""Capture original build-12340 mounted Unit_C request routing.

Executes 7385C0, 7173F0, the 6E6F80 body/mount getter and behavior predicates.
Supplies resident CM2Model records/metadata, identity tier resolution, alive
state and inactive combat/passenger providers. Captures 735820 and 737EF0
submissions; those consumers and post-commit direction/effect state are not
executed. This is dispatch evidence, not a complete unit/vehicle simulation.
"""
import argparse
from collections import deque
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, body, mount, vtable, fields, movement, table, rows = [
        native.HEAP + i * 0x4000 for i in range(8)]
    dead, finished, exhausted = 0, 0, 0
    calls = []
    recent = deque(maxlen=12)

    def dependencies(u, address, _size, _data):
        recent.append(address)
        sp = u.reg_read(UC_X86_REG_ESP)
        answers = {
            unit + 0x3000: (0, 8),  # no equipped weapon models
            0x824f00: (1, 8), 0x71f560: (dead, 0),
            0x4f6210: (0, 0), 0x71c6c0: (0, 0), 0x4f6250: (0, 0),
            0x723e30: (0, 8), 0x71e400: (0, 36),
            0x738180: (0, 4), 0x71e5b0: (0, 0),
            0x8267e0: (91, 4), 0x826a60: (0, 4),
            0x52e570: (u.reg_read(UC_X86_REG_ECX), 0),
        }
        if address in answers:
            result, pop = answers[address]
        elif address == 0x7176f0:
            animation, _model = native.read_words(u, sp + 4, 2)
            result, pop = animation, 8
        elif address == 0x8266b0:
            key, out = native.read_words(u, sp + 4, 2)
            is_mount = u.reg_read(UC_X86_REG_ECX) == mount
            animation = 5 if is_mount else (91 if key == 0xffffffff else 0xffffffff)
            native.write_words(u, out, animation, 0, 0, bits(1.), 0, 0,
                               finished if is_mount else 0, exhausted if is_mount else 0)
            result, pop = 1, 8
        elif address == 0x82ced0:
            _animation, _variation, out = native.read_words(u, sp + 4, 3)
            native.write_words(u, out, 0, 1000, bits(0.))
            result, pop = 1, 12
        elif address == 0x735820:
            args = native.read_words(u, sp + 4, 9)
            assert args[0] == mount and args[1] == 0xffffffff
            calls.append(('mount', args[2]))
            result, pop = 0, 36
        elif address == 0x737ef0:
            record, _old, upper, _priority, _blend, _force = native.read_words(u, sp + 4, 6)
            animation = native.read_words(u, record, 1)[0]
            calls.append(('upper' if upper else 'body', animation))
            result, pop = 0, 24
        else:
            return
        u.reg_write(UC_X86_REG_EAX, result)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, unit, vtable)
    native.write_words(uc, vtable + 0xd4, 0x6e6f80)
    native.write_words(uc, vtable + 0x12c, unit + 0x3000)
    native.write_words(uc, unit + 0xb4, body)
    native.write_words(uc, unit + 0xbc, 0x40000)
    native.write_words(uc, unit + 0x98c, mount)
    native.write_words(uc, unit + 0xd0, fields, 0, movement)
    native.write_words(uc, unit + 0xb7c, 91)
    native.write_words(uc, unit + 0xb84, 0xffffffff)
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 505)
    native.write_words(uc, 0xad30e8, table)
    for animation in range(506):
        row = rows + animation * 32
        native.write_words(uc, table + animation * 4, row)
        native.write_words(uc, row, animation, 0, 0, 0, 0, 0, animation, 0)
    lines = ['# flags dead mountFinished mountExhausted request -> mount upper body (-1=no submission)',
             '# request flags=8: preserve supplied ready-weapon ID; weapon selection is upstream evidence']
    requests = [0, 1, 2, 4, 5, 6, 8, 11, 14, 37, 38, 39, 40, 57, 58,
                91, 96, 97, 118, 127, 131, 132, 187, 201, 466, 467, 468, 472]
    for flags in [0x70, 0x10, 0x40]:
        for dead in [0, 1]:
            for finished, exhausted in [(0, 0), (1, 0), (0, 1)]:
                for requested in requests:
                    calls.clear()
                    native.write_words(uc, unit + 0xa38, flags)
                    uc.reg_write(UC_X86_REG_ECX, unit)
                    try:
                        invoke(uc, 0x7385c0, [requested, 8])
                    except Exception:
                        print('Failed:', flags, dead, finished, exhausted, requested,
                              [hex(address) for address in recent])
                        raise
                    slots = dict(calls)
                    results = [slots.get(slot, 0xffffffff) for slot in ['mount', 'upper', 'body']]
                    results = [-1 if value == 0xffffffff else value for value in results]
                    lines.append(' '.join(map(str, [flags, dead, finished, exhausted, requested, *results])))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines) - 2} mounted Unit_C dispatches')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
