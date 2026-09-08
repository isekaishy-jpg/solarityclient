"""Capture native death-dialog queries and explicit release packet admission.

Runs original 6123C0, 612430, 802270, 549AD0, 51ACD0, 51AA90,
6D2950 and the real player vtable's 6DAC10. Object lookup, spell/name
providers, 727860's restriction result and datastore writes are boundaries.
This fixture does not establish inventory enumeration or aura restriction
construction; those providers are supplied independently of the UI queries.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, fields, player, movement, name = [native.HEAP + x for x in (0, 0x2000, 0x3000, 0x5000, 0x6000)]
    # This is the actual constructor-installed player vtable, not a stand-in.
    native.write_words(uc, unit, 0xa326c8)
    assert native.read_words(uc, 0xa326c8 + 0x128, 1)[0] == 0x6dac10
    native.write_words(uc, unit + 0xd0, fields)
    native.write_words(uc, unit + 0xd8, movement)
    native.write_words(uc, unit + 0x1008, player)
    uc.mem_write(name, b'Self resurrection\0')
    present, allowed, spell_found, item = True, True, True, False
    outputs, packet = [], []

    def hook(uc, address, size, data):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x4d4db0:
            return_value(uc, unit if present else 0)
        elif address == 0x727860:
            return_value(uc, int(allowed))
        elif address == 0x84e2a0:
            outputs.append(str(int(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0])))
            return_value(uc, 0)
        elif address == 0x84e280:
            outputs.append('nil')
            return_value(uc, 0)
        elif address == 0x84e350:
            pointer = native.read_words(uc, sp + 8, 1)[0]
            outputs.append(bytes(uc.mem_read(pointer, 64)).split(b'\0')[0].decode('ascii').replace(' ', '_') or 'empty')
            return_value(uc, 0)
        elif address in (0x8b7da0, 0x5eeb70, 0x403ec0, 0x403f10):
            return_value(uc, 0)
        elif address == 0x4cfd20:
            destination = native.read_words(uc, sp + 8, 1)[0]
            native.write_words(uc, destination + 0x220, name)
            return_value(uc, int(spell_found))
            uc.reg_write(UC_X86_REG_ESP, sp + 12)
        elif address == 0x6d6640:
            return_value(uc, int(item))
        elif address == 0x707c20:
            uc.mem_write(native.read_words(uc, sp + 4, 1)[0], b'Inventory resurrection\0')
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 12)
        elif address in (0x47b0a0, 0x47afe0):
            value = native.read_words(uc, sp + 4, 1)[0]
            packet.append(value if address == 0x47b0a0 else value & 255)
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x6b0b50:
            datastore = native.read_words(uc, sp + 4, 1)[0]
            native.write_words(uc, datastore + 12, 0xffffffff)
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, hook)
    lines = ['# Original death dialog; fixture providers described by player_death_dialog_oracle.py.']
    for present in [False, True]:
        for health in [0, 1, 100, 0x7fffffff, 0x80000000, 0xffffffff]:
            for flags in [0, 0x10, 0x4000, 0x4010]:
                native.write_words(uc, fields + 0x48, health)
                native.write_words(uc, player + 8, flags)
                for allowed in [False, True]:
                    packet.clear()
                    native.invoke(uc, 0x51aa90, [])
                    lines.append(f'repop {int(present)} {health:08x} {flags:08x} {int(allowed)} ' + (','.join(f'{v:x}' for v in packet) if packet else 'none'))
        item = False
        for health in [0, 100]:
            native.write_words(uc, fields + 0x48, health)
            for spell in [0, 7]:
                native.write_words(uc, player + 0x106c, spell)
                for allowed in [False, True]:
                    packet.clear()
                    native.invoke(uc, 0x51add0, [])
                    lines.append(f'selfres {int(present)} {health:08x} {spell} {int(allowed)} ' + (','.join(f'{v:x}' for v in packet) if packet else 'none'))
        for flags in [0, 0x800, 0x1000, 0x1800, 0x200000, 0x201000]:
            native.write_words(uc, movement + 0x44, flags)
            outputs.clear()
            native.invoke(uc, 0x612430, [0])
            lines.append(f'falling {int(present)} {flags:08x} {outputs[0]}')
        for flag in [0, 4, 0xff]:
            uc.mem_write(unit + 0xf47, bytes([flag]))
            outputs.clear()
            native.invoke(uc, 0x802270, [0])
            lines.append(f'blocked {int(present)} {flag:02x} {outputs[0]}')
        for flags in [0, 0x10, 0x4000, 0xffffffff]:
            native.write_words(uc, player + 8, flags)
            outputs.clear()
            native.invoke(uc, 0x6123c0, [0])
            lines.append(f'bounds {int(present)} {flags:08x} {outputs[0]}')
        for health in [0, 1, 0xffffffff]:
            native.write_words(uc, fields + 0x48, health)
            for spell, spell_found, item in [(0, False, False), (0, False, True), (7, True, False), (7, False, True)]:
                native.write_words(uc, player + 0x106c, spell)
                outputs.clear()
                native.invoke(uc, 0x51acd0, [0])
                lines.append(f'soulstone {int(present)} {health:08x} {spell} {int(spell_found)} {int(item)} {outputs[0]}')
    for kind in [0, 3, 4]:
        native.write_words(uc, 0xbea570, kind)
        for index in [0, 1, 2, 0xffffffff]:
            native.write_words(uc, 0xacd16c, index)
            for registered in [0, 1]:
                for slot in [0, 1]:
                    native.write_words(uc, 0xbea4f0 + slot * 0x40, registered)
                outputs.clear()
                native.invoke(uc, 0x549ad0, [0])
                lines.append(f'arena {kind} {index:08x} {registered} ' + ','.join(outputs))
    for cinematic in [0, 1, 2, 0xffffffff]:
        native.write_words(uc, 0xbd07fc, cinematic)
        outputs.clear()
        native.invoke(uc, 0x516760, [0])
        lines.append(f'cinematic {cinematic:08x} {outputs[0]}')
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    result = capture()
    Path(args.output).write_text(result, encoding='utf-8')
    print(f'Captured {len(result.splitlines()) - 1} native death-dialog cases')
