"""Capture ordinary relative-yaw follow policy and original interpolation.

Runs 602760 and 6041F5..6043A5. Only object lookup and the clock are supplied
by hooks. Camera data, CVars, gates, profile selection, requests, shared axis
duration and cosine sampling execute the pinned build-12340 instructions.
Saved-view smoothing is enabled; tracking, roll and special subjects are absent.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP,
    UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_EBX, UC_X86_REG_EBP,
    UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)
import wmo_registration_oracle as n


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture(style, held, angles, flags, base_time, custom):
    uc = n.emulator()
    camera, controls, unit, movement = [n.HEAP + v for v in (0, 0x1000, 0x2000, 0x4000)]
    next_cvar = n.HEAP + 0x5000

    def cvar(pointer, value):
        nonlocal next_cvar
        n.write_words(uc, pointer, next_cvar)
        n.write_floats(uc, next_cvar + 0x2c, [value])
        n.write_words(uc, next_cvar + 0x30, int(value))
        next_cvar += 0x40

    for table, defaults, count in [(0xC24C98, 0xAD1C30, 70), (0xC24C20, 0xAD1D48, 30)]:
        for i in range(count):
            address = n.read_words(uc, defaults + i * 4, 1)[0]
            value = float(bytes(uc.mem_read(address, 32)).split(b'\0')[0])
            if custom and table == 0xC24C98:
                value = 0.125 if i % 2 == 0 else 2.5
            cvar(table + i * 4, value)
    for pointer, value in [(0xC24C1C, style), (0xC24DC0, 1), (0xC24DB0, 1),
                           (0xC24DB8, 1), (0xC24984, 0), (0xC24980, 30),
                           (0xC2497C, 0), (0xC24978, 0), (0xC24974, .1),
                           (0xC24970, 2), (0xC24E30, 180), (0xC24E38, 45)]:
        cvar(pointer, value)
    n.write_words(uc, camera + 0x98, flags)
    n.write_words(uc, camera + 0xb4, 2)
    n.write_floats(uc, camera + 0xd0, [5.55, 0.17453292, 0])
    n.write_floats(uc, camera + 0x11c, angles)
    n.write_floats(uc, camera + 0x230, [angles[1]])
    n.write_floats(uc, camera + 0x260, [angles[0]])
    n.write_words(uc, controls + 4, held)
    n.write_words(uc, unit + 0xd8, movement)
    current_time = base_time

    def hook(machine, address, size, data):
        if address not in (0x4D4DB0, 0x86AE20):
            return
        sp = machine.reg_read(UC_X86_REG_ESP)
        machine.reg_write(UC_X86_REG_EAX, unit if address == 0x4D4DB0 else current_time)
        machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])
        machine.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, hook)
    result = []
    # A second identical request must retain the start and anchor.
    for kind, offset in [(0, 0), (1, 50), (0, 50), (1, 100), (1, 200), (1, 500), (1, 2100), (1, 2101)]:
        current_time = (base_time + offset) & 0xffffffff
        uc.reg_write(UC_X86_REG_FPSW, 0)
        uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
        if kind == 0:
            uc.reg_write(UC_X86_REG_ECX, camera)
            n.invoke(uc, 0x602760, [controls, 0])
        else:
            frame = n.STACK + 0x18000
            uc.reg_write(UC_X86_REG_EBP, frame)
            uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
            uc.reg_write(UC_X86_REG_ESI, camera)
            uc.reg_write(UC_X86_REG_EDI, 0)
            uc.reg_write(UC_X86_REG_EBX, current_time)
            # The preceding roll lane leaves a zero on the x87 stack.
            uc.mem_write(n.STOP, b'\xd9\xee')
            uc.emu_start(n.STOP, n.STOP + 2)
            uc.emu_start(0x6041f5, 0x6043a5, count=10000)
            assert uc.reg_read(UC_X86_REG_EIP) == 0x6043a5
        result += [kind, current_time, *n.read_words(uc, camera + 0x98, 1),
                   *n.read_words(uc, camera + 0x11c, 2),
                   *n.read_words(uc, camera + 0x228, 6),
                   *n.read_words(uc, camera + 0x258, 6)]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    lines = ['# style held yaw pitch flags time custom | (kind time flags yaw pitch pitchLane[6] yawLane[6])*; all hex']
    for style, held, angles, flags, base_time, custom in itertools.product(
        range(5), [0, 0x10, 0x40, 0x100, 0x1040],
        [(1.5, -.4), (-2.5, 1.), (6., .2)], [0, 0x20, 1], [1000, 0xfffffff0], [0, 1],
    ):
        header = [style, held, *map(bits, angles), flags, base_time, custom]
        output = capture(style, held, angles, flags, base_time, custom)
        lines.append(' '.join(f'{v:08x}' for v in header) + ' | ' + ' '.join(f'{v:08x}' for v in output))
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines)-1} ordinary camera follow histories')


if __name__ == '__main__':
    main()
