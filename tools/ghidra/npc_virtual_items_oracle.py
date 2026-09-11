"""Capture build-12340 Unit_C virtual-item selection and attachment links.

Executes original 725010 (Item.dbc join), 71F440/718FC0 (disarm), 721ED0 and
715D00 (body readiness), 72DBC0/72B7F0 (component selection), and the link
selection prefix of 4EACD0. Asset-cache and model-engine calls are intercepted;
8273D0 records the selected link and reports it absent, ending that prefix.
This does not emulate full M2 resource creation or animated sheath callbacks.
"""
import argparse
import itertools
from pathlib import Path

import wmo_registration_oracle as n
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX,
    UC_X86_REG_ESP, UC_X86_REG_EIP,
)

# id, class, subclass, sound override, material, display, inventory, sheath
ITEMS = [
    [100, 2, 7, 0xffffffff, 1, 800, 13, 1],
    [101, 4, 6, 0xffffffff, 1, 801, 14, 4],
    [102, 2, 2, 0xffffffff, 1, 802, 15, 2],
    [103, 15, 0, 0xffffffff, 1, 803, 23, 0],
    [104, 2, 8, 0xffffffff, 1, 804, 17, 2],
    [105, 2, 3, 0xffffffff, 1, 805, 26, 1],
    [106, 2, 16, 0xffffffff, 1, 806, 25, 3],
    [107, 2, 0, 0xffffffff, 1, 807, 13, 1],
]


def capture(executable, output):
    n.initialize(executable)
    u = n.emulator()
    unit, vt, fields, body, table, rows, model, object_data, animation_table, animation = [
        n.HEAP + i * 0x4000 for i in range(10)]
    calls, links = [], []
    resolve_link = False

    def returned(value=0, pop=0):
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def hook(_u, address, _size, _data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in (0x8b7da0, 0x5eeb70, 0x4eb070, 0x4e79a0, 0x72afe0):
            returned()
        elif address == 0x4cfd90:
            display, out = n.read_words(u, sp + 4, 2)
            n.write_words(u, out, display)
            returned(int(display != 0), 8)
        elif address == 0x4eacd0 and not resolve_link:
            args = n.read_words(u, sp + 4, 8)
            calls.append([n.read_words(u, args[1], 1)[0], *args[2:]])
            returned(0xffffffff)
        elif address == 0x720170:
            returned(0, 8)
        elif address == 0x824f00:
            returned(1, 8)
        elif address == 0x8267e0:
            returned(1000, 4)
        elif address == 0x827560:
            returned(0, 4)
        elif address == 0x8273d0:
            links.append(n.read_words(u, sp + 4, 1)[0])
            returned(0, 4)
        elif address == 0x4d3790:
            u.reg_write(UC_X86_REG_EDX, 0)
            returned()

    u.hook_add(UC_HOOK_CODE, hook)
    n.write_words(u, unit, vt)
    n.write_words(u, vt + 0x12c, 0x71f440, 0x71f540, 0x718b10)
    n.write_words(u, unit + 8, object_data)
    n.write_words(u, object_data, 1, 0, 0)
    n.write_words(u, unit + 0xd0, fields)
    n.write_words(u, unit + 0xb4, body)
    n.write_words(u, unit + 0x970, model)
    n.write_words(u, unit + 0xb84, 0xffffffff)
    n.write_words(u, 0xad3d5c, 100)
    n.write_words(u, 0xad3d58, 107)
    n.write_words(u, 0xad3d6c, table)
    n.write_words(u, 0xad30d8, 1000)
    n.write_words(u, 0xad30d4, 1000)
    n.write_words(u, 0xad30e8, animation_table)
    n.write_words(u, animation_table, animation)
    for index, item in enumerate(ITEMS):
        n.write_words(u, table + index * 4, rows + index * 32)
        n.write_words(u, rows + index * 32, *item)

    lines = [
        '# Unit_C settled equipment selection; original native code, intercepted asset/model engine.',
        '# item rows: id class subclass sound material display inventory sheath',
        *('item ' + ' '.join(map(str, item)) for item in ITEMS),
        '# case primary secondary rawSheath bodyBehavior modelFlags main off ranged; slot:link ...',
    ]
    entry_sets = [[100, 101, 102], [103, 107, 105], [0, 107, 106],
                  [104, 101, 102], [100, 0, 0], [999, 101, 102],
                  [0, 103, 105], [101, 100, 102]]
    for primary, secondary, sheath, behavior, model_flags, entries in itertools.product(
            [0, 0x200000], [0, 0x80, 0x400, 0x480], range(3),
            [0, 10, 16, 25], [0, 0x10], entry_sets):
        n.write_words(u, unit + 0x998, 0, 0, 0)
        u.mem_write(unit + 0x9a4, bytes(24))
        n.write_words(u, fields + 0xd4, primary, secondary)
        n.write_words(u, fields + 0xc8, *entries)
        n.write_words(u, unit + 0xb5c, sheath)
        n.write_words(u, model + 4, model_flags)
        n.write_words(u, animation + 0x18, behavior)
        for slot in range(3):
            u.reg_write(UC_X86_REG_ECX, unit)
            invoke(u, 0x725010, [slot, 0])
        metadata = []
        for slot in range(2):
            u.reg_write(UC_X86_REG_ECX, unit)
            invoke(u, 0x71f440, [slot, 0])
            metadata.append(u.reg_read(UC_X86_REG_EAX))
        u.reg_write(UC_X86_REG_ECX, unit)
        invoke(u, 0x721ed0, [])
        if u.reg_read(UC_X86_REG_EAX) & 255:
            invoke(u, 0x715d00, [sheath, *metadata])
            n.write_words(u, unit + 0xb5c, u.reg_read(UC_X86_REG_EAX))
        calls.clear()
        for slot in range(3):
            u.reg_write(UC_X86_REG_ECX, unit)
            invoke(u, 0x72dbc0, [slot])
        resolved = []
        resolve_link = True
        for display, slot, sheathe, hidden, shield, right, extra in calls:
            links.clear()
            invoke(u, 0x4eacd0, [body, rows, slot, sheathe, hidden & 255, shield & 255, right & 255, extra])
            assert len(links) == 1, (calls, links)
            if links[0] != 0xffffffff:
                resolved.append(f'{slot - 15}:{links[0]}')
        resolve_link = False
        inputs = [primary, secondary, sheath, behavior, model_flags, *entries]
        lines.append('case ' + ' '.join(map(str, inputs)) + ';' + ' '.join(resolved))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', sum(line.startswith('case ') for line in lines), 'native NPC equipment cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
