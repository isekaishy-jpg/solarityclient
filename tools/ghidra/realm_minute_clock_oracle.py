"""Native 76D900 frame accumulator, 76D740 minute rollover and 76C360 getter.

Only calendar-day mutation, registered minute callbacks and OS time are supplied;
the original repeated float stores and strict one-minute boundary remain intact.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture():
    u = n.emulator()
    owner = n.HEAP
    now = 0

    def hook(u, address, size, context):
        if address in [0x76c280, 0x76d650, 0x86ae20]:
            return_value(u, now if address == 0x86ae20 else 0)
            count = {0x76c280: 2, 0x76d650: 1, 0x86ae20: 0}[address]
            u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + count * 4)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Native minute-clock frame progression: elapsed-u32 minute-u32 fraction-f32-bits.']
    for minute in [0, 719, 1439]:
        for rate in [0., 1/60, .25, 1., 2.]:
            for start in [0, 0xfffff000]:
                rate = f32(rate)
                u.mem_write(owner, bytes(0x100))
                n.write_words(u, owner, minute % 60, minute // 60)
                n.write_words(u, owner + 0x30, bits(rate))
                now = start
                rows.append(f'reset {minute} {bits(rate):08x} {now}')
                for delta in [0, 1, 16, 17, 33, 999, 1000, 30000, 60000, 100000] + [17] * 120:
                    now = (now + delta) & 0xffffffff
                    seconds = f32(f32(delta) * f32(.001))
                    u.reg_write(UC_X86_REG_ECX, owner)
                    n.invoke(u, 0x76d900, [bits(seconds)])
                    u.reg_write(UC_X86_REG_ECX, owner)
                    n.invoke(u, 0x76c360, [])
                    value = n.read_words(u, owner, 2)
                    fraction = n.read_words(u, owner + 0x34, 1)[0]
                    rows.append(f'tick {now} {value[0] + value[1]*60} {fraction:08x}')
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
