"""Capture native replicated-health admission and local life-event dispatch.

Runs 73F330, 520F70, 6E0FD0 and 6DF710. Object lookup, animation/death side
effects, cursor cancellation, display refresh and Lua event delivery are
supplied boundaries. Original instructions choose health and ghost transitions;
event names come from the executable's literal registration instructions.
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
    unit, fields, guid, vtable, previous, player = [native.HEAP + x for x in (0, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000)]
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 8, guid)
    native.write_words(uc, guid, 7, 0, 0x19)
    native.write_words(uc, unit + 0xd0, fields)
    native.write_words(uc, unit + 0x1008, player)
    native.write_words(uc, fields + 0x68, 100)
    native.write_words(uc, vtable + 0x128, native.STOP + 16)
    events, transitions = [], []

    def hook(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX, 0)
            return_value(uc, 7)
        elif address == 0x4d4db0:
            return_value(uc, unit)
        elif address == native.STOP + 16:
            return_value(uc, 1)
        elif address in (0x53cf10, 0x519280, 0x523eb0, 0x51f690, 0x530840, 0x524a30, 0x4f88b0, 0x6dc5a0, 0x4d4b30, 0x7e5550):
            return_value(uc, 0)
        elif address == 0x71f8f0:
            return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x60bf10:
            events.append(native.read_words(uc, sp + 8, 1)[0])
            return_value(uc, 0)
        elif address in (0x729220, 0x73d530):
            transitions.append('dead' if address == 0x729220 else 'alive')
            return_value(uc, 0)
            if address == 0x73d530:
                uc.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x81b530:
            events.append(native.read_words(uc, sp + 4, 1)[0])
            return_value(uc, 0)

    uc.hook_add(UC_HOOK_CODE, hook)
    lines = ['# Native 73F330/520F70 health; 6E0FD0/6DF710 ghost flags.']
    image = bytes(uc.mem_read(native.image_base, native.image_size))
    for event in [0x101, 0x102, 0x188, 0x18f]:
        pattern = b'\xc7\x05' + struct.pack('<I', 0xc24eb0 + event * 4)
        position = image.index(pattern) + len(pattern)
        pointer = struct.unpack_from('<I', image, position)[0]
        name = bytes(uc.mem_read(pointer, 64)).split(b'\0')[0].decode('ascii')
        lines.append(f'# event {event:x} {name}')
    for old in [0, 1, 100, 0x7fffffff, 0x80000000, 0xffffffff]:
        for health in [0, 1, 100, 0x7fffffff, 0x80000000, 0xffffffff]:
            native.write_words(uc, previous, old)
            native.write_words(uc, fields + 0x48, health)
            native.write_words(uc, unit + 0xfb0, 1234)
            transitions.clear()
            events.clear()
            native.invoke(uc, 0x73f330, [7, 0, 0x48, 0, previous])
            native.invoke(uc, 0x520f70, [])
            assert len(transitions) <= 1 and len(events) == 1
            predicted = native.read_words(uc, unit + 0xfb0, 1)[0]
            lines.append(f'health {old:08x} {health:08x} {predicted:08x} {transitions[0] if transitions else "none"} {events[0]:x}')
    for old in [0, 0x10]:
        for flags in [0, 0x10]:
            native.write_words(uc, player + 8, flags)
            events.clear()
            uc.reg_write(UC_X86_REG_ECX, unit)
            native.invoke(uc, 0x6e0fd0, [old])
            lines.append(f'flags {old:08x} {flags:08x} ' + ','.join(f'{event:x}' for event in events))
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    result = capture()
    Path(args.output).write_text(result, encoding='utf-8')
    print(f'Captured {sum(not line.startswith("#") for line in result.splitlines())} native life cases')
