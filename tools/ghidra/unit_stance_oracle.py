"""Capture build-12340 posture selection and primary-completion decisions.

Runs original 71E1F0, 73B510, 73F060, and nested 73AF80 instructions. Hooks
supply virtual stand/model queries, current behavior, and the dead predicate,
and intercept animation requests. The fixture covers the admitted posture
layer and changed-stand path without a vehicle controller, not other animation
priorities, bone layering, sequence timing, or spell/death side effects.
"""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, vtable, getter, result, movement, table, row = [native.HEAP + i * 0x2000 for i in range(7)]
    state, behavior, death_available, requested = 0, 0, 0, None
    dead, preserve_variation = 0, False
    has_standup = 0
    model_getter, model, guid, context = getter + 16, native.HEAP + 0x18000, native.HEAP + 0x19000, native.HEAP + 0x1a000
    def dependencies(u, address, _size, _data):
        nonlocal requested, preserve_variation
        answers = {getter: (state, 0), 0x717260: (behavior, 0), 0x71dfc0: (death_available, 0),
                   0x71f560: (dead, 0), 0x71e5b0: (0, 0), 0x71ee70: (0, 0),
                   0x4d3790: (1, 0), 0x4f5960: (context, 0), model_getter: (model, 0),
                   0x824f00: (1, 8), 0x825ee0: (has_standup, 4)}
        if address in answers:
            value, pop = answers[address]
        elif address == 0x7385c0:
            sp = u.reg_read(UC_X86_REG_ESP)
            requested = native.read_words(u, sp + 4, 1)[0]
            value, pop = 0, 8
        elif address == 0x73ac30:
            requested = -1
            value, pop = 0, 8
        elif address == 0x8266b0:
            sp = u.reg_read(UC_X86_REG_ESP)
            out = native.read_words(u, sp + 8, 1)[0]
            native.write_words(u, out, behavior, 3, 0)
            value, pop = 0, 8
        elif address == 0x7176f0:
            sp = u.reg_read(UC_X86_REG_ESP)
            value = native.read_words(u, sp + 4, 1)[0]
            pop = 8
        elif address == 0x735820:
            sp = u.reg_read(UC_X86_REG_ESP)
            args = native.read_words(u, sp + 4, 9)
            requested = args[2]
            preserve_variation = args[3] == 3
            assert preserve_variation, args
            value, pop = 0, 36
        else:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, unit, vtable)
    native.write_words(uc, vtable + 0x138, getter)
    native.write_words(uc, unit + 0xd8, movement)
    native.write_words(uc, unit + 0xa38, 0x40)
    rows = ['# select stand previous current swimming deathAvailable -> admitted requested (-1 unchanged)',
            '# complete stand behavior -> requested (-1 resume resolver)']
    states = [*range(11), 255]
    for state in states:
        for previous in states:
            for behavior in [0, 96, 97]:
                for swimming in [0, 1]:
                    for death_available in [0, 1]:
                        native.write_words(uc, unit + 0x9f8, previous)
                        native.write_words(uc, movement + 0x44, swimming * 0x200000)
                        native.write_words(uc, result, 0xffffffff)
                        uc.reg_write(UC_X86_REG_ECX, unit)
                        invoke(uc, 0x71e1f0, [0xffffffff, result])
                        admitted = uc.reg_read(UC_X86_REG_EAX) & 255
                        request = native.read_words(uc, result, 1)[0]
                        if request == 0xffffffff: request = -1
                        rows.append(f'select {state} {previous} {behavior} {swimming} {death_available} {admitted} {request}')
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 505)
    native.write_words(uc, 0xad30e8, table)
    for state in states:
        for behavior in [0, 96, 97, 98, 99, 100, 101, 102, 103, 104, 114, 115, 116]:
            native.write_words(uc, table + behavior * 4, row)
            native.write_words(uc, row + 0x18, behavior)
            requested = None
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, 0x73b510, [0, 0xffffffff, behavior])
            assert requested is not None
            rows.append(f'complete {state} {behavior} {requested}')
    rows.append('# death behavior dead alternate -> requested (-1 resume, -2 retain) preserveVariation')
    for behavior in [0, 1, 6, 131, 132, 201, 202, 466, 467, 468, 472]:
        native.write_words(uc, table + behavior * 4, row)
        native.write_words(uc, row + 0x18, behavior)
        for dead in [0, 1]:
            for alternate in [0, 1]:
                native.write_words(uc, unit + 0xa38, 0x40 | alternate * 0x4000000)
                requested, preserve_variation = -2, False
                uc.reg_write(UC_X86_REG_ECX, unit)
                invoke(uc, 0x73b510, [0, 0xffffffff, behavior])
                rows.append(f'death {behavior} {dead} {alternate} {requested} {int(preserve_variation)}')
    native.write_words(uc, unit + 8, guid)
    native.write_words(uc, guid, 2, 0)
    native.write_words(uc, vtable + 0xd4, model_getter)
    # Full changed-stand handler and nested 73AF80 run; non-animation side effects
    # are intercepted, with no vehicle controller or movement water-height probe.
    rows.append('# entry stand previous current swimming hasStandup -> requested (-1 resume, -2 retain)')
    for behavior in [0, 1, 6, 131, 132, 466, 467, 468, 472]:
        native.write_words(uc, table + behavior * 4, row)
        native.write_words(uc, row + 0x18, behavior)
        for state, previous in [(7, 0), (7, 7), (9, 0), (9, 9), (0, 9), (0, 1)]:
            for swimming in [0, 1]:
                for has_standup in [0, 1]:
                    native.write_words(uc, unit + 0x9f8, previous)
                    native.write_words(uc, movement + 0x44, swimming * 0x200000)
                    requested = -2
                    uc.reg_write(UC_X86_REG_ECX, unit)
                    invoke(uc, 0x73f060, [state])
                    rows.append(f'entry {state} {previous} {behavior} {swimming} {has_standup} {requested}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {sum(not line.startswith("#") for line in rows)} original posture decisions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
