"""Capture ordinary unit fall, landing, turn, and jump-completion decisions.

Executes original build-12340 instructions. Landing's speed predicate, model
behavior query, and unrelated combat predicates are supplied at call boundaries;
animation requests and side effects are intercepted. No vehicle, spline, spell,
alternate death, or procedural bone controller is admitted by these fixtures.
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
    unit, movement, vtable, side_effect, table, row, result = [native.HEAP + i * 0x2000 for i in range(7)]
    requested, slow, behavior = -2, 0, 0
    def dependencies(u, address, _size, _data):
        nonlocal requested
        if address == 0x7385c0:
            requested = native.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 1)[0]
            value, pop = 0, 8
        elif address == 0x73ac30:
            requested = -1
            value, pop = 0, 8
        elif address == side_effect:
            value, pop = 0, 8
        elif address == 0x716fa0:
            value, pop = slow, 0
        elif address == 0x717260:
            value, pop = behavior, 0
        elif address in [0x71d940, 0x71d380, 0x71da20, 0x71da60, 0x71daa0]:
            value, pop = 0, 0
        else:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, unit, vtable)
    native.write_words(uc, vtable + 0x118, side_effect)
    native.write_words(uc, unit + 0xd8, movement)
    native.write_words(uc, 0xad30d8, 0)
    native.write_words(uc, 0xad30d4, 505)
    native.write_words(uc, 0xad30e8, table)
    rows = ['# land previous current force slow -> request (-1 resolver, -2 retain)']
    for previous in [0, 0x1000, 0x3000, 0x3001]:
        for flags in [0, 1, 2, 4, 8, 0x101, 0x200000, 0x2000001, 0x400]:
            for force in [0, 1]:
                for slow in [0, 1]:
                    native.write_words(uc, unit + 0x7cc, flags)
                    native.write_words(uc, movement + 0x44, flags)
                    native.write_words(uc, unit + 0xa38, 0x40)
                    requested = -2
                    uc.reg_write(UC_X86_REG_ECX, unit)
                    invoke(uc, 0x73d2b0, [previous, force])
                    rows.append(f'land {previous} {flags} {force} {slow} {requested}')
    rows.append('# turn flags secondary procedural blocked -> request (-1 no turn)')
    for flags in [0, 0x10, 0x20, 0x30, 0x410, 0x200010, 0x2000020, 0x40000010]:
        for secondary in [0, 4]:
            for procedural in [0, 0x800, 0x1000, 0x1800]:
                for blocked in [0, 4, 8, 0x400000]:
                    native.write_words(uc, unit + 0x7cc, flags, secondary)
                    native.write_words(uc, movement + 0x44, flags, secondary)
                    native.write_words(uc, unit + 0xa38, 0x40 | procedural | blocked)
                    native.write_words(uc, result, 0xffffffff)
                    uc.reg_write(UC_X86_REG_ECX, unit)
                    invoke(uc, 0x71e180, [0xffffffff, result])
                    request = native.read_words(uc, result, 1)[0]
                    rows.append(f'turn {flags} {secondary} {procedural} {blocked} {request if request != 0xffffffff else -1}')
    rows.append('# fall flags vertical -> admitted')
    for flags in [0, 1, 0x1000, 0x2000, 0x3000, 0x2001000]:
        for vertical in [0., -7.95555, 2.]:
            native.write_words(uc, movement + 0x44, flags, 0)
            native.write_floats(uc, movement + 0xb8, [vertical])
            uc.reg_write(UC_X86_REG_ECX, movement)
            invoke(uc, 0x723350, [])
            rows.append(f'fall {flags} {vertical} {uc.reg_read(UC_X86_REG_EAX)}')
    rows.append('# complete behavior flying -> request (-1 resolver, -2 retain)')
    for behavior in [37, 38, 39, 40, 187]:
        native.write_words(uc, table + behavior * 4, row)
        native.write_words(uc, row + 0x18, behavior)
        for flying in [0, 1]:
            native.write_words(uc, movement + 0x44, flying * 0x2000000)
            requested = -2
            uc.reg_write(UC_X86_REG_ECX, unit)
            invoke(uc, 0x73b510, [0, 0xffffffff, behavior])
            rows.append(f'complete {behavior} {flying} {requested}')
    rows.append('# family behavior -> retain')
    for behavior in [0, 36, 37, 38, 39, 40, 41, 187, 467, 468]:
        native.write_words(uc, table + behavior * 4, row)
        native.write_words(uc, row + 0x18, behavior)
        invoke(uc, 0x71dc20, [behavior])
        rows.append(f'family {behavior} {uc.reg_read(UC_X86_REG_EAX)}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {sum(not row.startswith("#") for row in rows)} original movement animation decisions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
