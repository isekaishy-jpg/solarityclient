"""Capture original 752ED0 death-log construction and 74E290 Lua arguments.

Original allocation/initialization, object classification, event selection and
Lua argument composition run in the pinned image. Supplied boundaries are
resident-object lookup, names, creature/spell records, list insertion and the
Lua/event bridge. The invocation is a death already admitted by 7561E0; this
does not prove animation/aura admission to that caller. The supplied Spell bank
is empty and each invocation starts with a zeroed stack: 752ED0 has a native
read of untouched output after a failed lookup; this fixture does not model
arbitrary prior stack residue for that branch.
"""
import argparse
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, descriptor, fields, log, name, subscriber, creature = [native.HEAP + x for x in (0, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000)]
    native.write_words(uc, unit + 8, descriptor)
    native.write_words(uc, unit + 0xd0, fields)
    uc.mem_write(name, b'WaterTest\0')
    native.write_words(uc, subscriber + 8, 2)
    event_name = native.HEAP + 0x8000
    uc.mem_write(event_name, b'COMBAT_LOG_EVENT\0COMBAT_LOG_EVENT_UNFILTERED\0')
    native.write_words(uc, 0xc25788, event_name, event_name + 17)
    native.write_words(uc, 0xca1388, 1000, 1_700_000_000)
    native.write_words(uc, 0xcd76ac, 2250)
    native.write_words(uc, 0xad49dc, 0, 1)
    output, events = [], []
    guid, creature_type = 1, 0

    def string(pointer):
        return bytes(uc.mem_read(pointer, 256)).split(b'\0')[0].decode() if pointer else None

    def ret(value=0, pop=0):
        sp = uc.reg_read(UC_X86_REG_ESP)
        return_value(uc, value)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def provider(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d4db0:
            low, high, mask = native.read_words(uc, sp + 4, 3)
            ret(unit if low + (high << 32) == guid else 0)
        elif address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            ret(1)
        elif address == 0x67b6a0:
            native.write_words(uc, creature + 0x10, creature_type)
            ret(creature if creature_type else 0, 20)
        elif address == 0x5eeb70:
            ret()
        elif address == 0x52bd10:
            ret()
        elif address == 0x74f2d0:
            uc.mem_write(log, bytes(0x80))
            ret(log, 12)
        elif address == 0x47cf80:
            ret(0, 4)
        elif address == 0x74fd40:
            ret(name, 12)
        elif address == 0x86e200:
            ret(0, 4)
        elif address in (0x4fb400, 0x817db0):
            ret()
        elif address == 0x74f6c0:
            ret(1)
        elif address == 0x81b510:
            ret(subscriber)
        elif address == 0x81aa00:
            event, lua, count = native.read_words(uc, sp + 4, 3)
            values = output[-count:][1:]
            events.append([event, values.copy()])
            ret()
        elif address == 0x84dcc0:
            index = struct.unpack('<i', uc.mem_read(sp + 8, 4))[0]
            index = index - 1 if index > 0 else len(output) + index
            value = output.pop()
            output.insert(index, value)
            ret()
        elif address == 0x84dbf0:
            index = struct.unpack('<i', uc.mem_read(sp + 8, 4))[0]
            index = index if index >= 0 else len(output) + index + 1
            del output[index:]
            ret()
        elif address == 0x84dab0:
            ret(1)
        elif address in (0x84e280, 0x84e350, 0x84e2a0, 0x84e2d0):
            if address == 0x84e280:
                value = None
            elif address == 0x84e350:
                value = string(native.read_words(uc, sp + 8, 1)[0])
            elif address == 0x84e2a0:
                value = struct.unpack('<d', uc.mem_read(sp + 8, 8))[0]
            else:
                value = struct.unpack('<i', uc.mem_read(sp + 8, 4))[0]
            output.append(value)
            ret()

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = []
    for guid, kind in [(1, 0x19), (2, 0x19), (0xf130000007000002, 9)]:
        native.write_words(uc, descriptor, guid & 0xffffffff, guid >> 32, kind, 7)
        for creature_type in [0, 13]:
            uc.mem_write(native.STACK, bytes(0x20000))
            events.clear()
            output.clear()
            native.write_words(uc, 0xca1394, 0)
            native.invoke(uc, 0x752ed0, [unit])
            rows.append(dict(guid=f'{guid:016x}', creature_type=creature_type, events=events.copy()))
    return rows


def admission():
    """Run 718A90 unhooked: its AL result suppresses the death record."""
    uc = native.emulator()
    unit, descriptor, template = native.HEAP, native.HEAP + 0x2000, native.HEAP + 0x3000
    native.write_words(uc, unit + 8, descriptor)
    rows = []
    for kind in [9, 0x19]:
        native.write_words(uc, descriptor + 8, kind)
        for present in [0, 1]:
            native.write_words(uc, unit + 0x964, template if present else 0)
            for flags in [0, 0x200, 0x400, 0x800, 0xffffffff]:
                native.write_words(uc, template + 0xc, flags)
                uc.reg_write(UC_X86_REG_ECX, unit)
                native.invoke(uc, 0x718a90, [])
                rows.append(f'{kind:x} {present} {flags:08x} {int((uc.reg_read(UC_X86_REG_EAX) & 255) == 0)}')
    return rows


def timer_at_log():
    """Execute 729220/6DC0F0/6DC070/7561E0 up to the death-log boundary."""
    uc = native.emulator()
    unit, descriptor, fields, player = [native.HEAP + x for x in (0, 0x2000, 0x3000, 0x5000)]
    native.write_words(uc, unit + 8, descriptor)
    native.write_words(uc, descriptor, 7, 0, 0x19)
    native.write_words(uc, unit + 0xd0, fields)
    native.write_words(uc, unit + 0x1008, player)
    output = []

    def hook(uc, address, size, context):
        if address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x86ae20:
            return_value(uc, 1000)
        elif address in (0x524bf0, 0x518d50, 0x523640, 0x809ac0):
            return_value(uc, 0)
        elif address == 0x752ed0:
            output.append('log')
            uc.reg_write(UC_X86_REG_EIP, native.STOP)
            uc.emu_stop()
        elif address == 0x84e2a0:
            sp = uc.reg_read(UC_X86_REG_ESP)
            output.append(int(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0]))
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, hook)
    rows = []
    for dynamic in [0, 0x20]:
        native.write_words(uc, fields + 0x124, dynamic)
        for flags in [0, 8, 0x10, 0x18]:
            native.write_words(uc, player + 0x1064, flags)
            for player_flags in [0, 0x4000]:
                native.write_words(uc, player + 8, player_flags)
                native.write_words(uc, 0xbd0848, 0, 0)
                output.clear()
                uc.reg_write(UC_X86_REG_ECX, unit)
                native.invoke(uc, 0x729220, [])
                assert output == ['log'], output
                native.invoke(uc, 0x516210, [0])
                rows.append(f'{dynamic:x} {flags:x} {player_flags:x} {output[1]}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text(json.dumps(rows, indent=2) + '\n', encoding='utf-8')
    def lua(value):
        return 'nil' if value is None else (str(int(value)) if isinstance(value, float) and value.is_integer() else str(value))
    Path(args.output + '.txt').write_text('\n'.join(
        f"{row['guid']}|{row['creature_type']}|" + ';'.join(
            f"{event}:" + ','.join(map(lua, values)) for event, values in row['events'])
        for row in rows) + '\n', encoding='utf-8')
    Path(args.output + '.admission.txt').write_text('\n'.join(admission()) + '\n', encoding='utf-8')
    Path(args.output + '.timer.txt').write_text('\n'.join(timer_at_log()) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)} native death-log cases')
