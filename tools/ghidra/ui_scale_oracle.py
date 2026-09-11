"""Capture build-12340 FrameXML root-scale policy and callback notifications.

Runs original 5240E0 (initial viewport), 5237E0 (useUiScale), 518B60 (uiScale),
51FB80 (automatic scale), 513240 (minimum), and 50F7C0 (configured aspect).
CVar lookup, number parsing, viewport queries, root mutation and event delivery
are controlled provider boundaries. This capture does not execute frame layout.
"""
import argparse
from collections import deque
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as n
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP


def capture(executable, output):
    n.initialize(executable)
    u = n.emulator()
    root, enabled, value, wide, resolution, viewport, device, vt, float_result = [
        n.HEAP + i * 0x1000 for i in range(9)]
    float_stub = float_result + 16
    u.mem_write(float_stub, b'\xdd\x05' + struct.pack('<I', float_result) + b'\xc2\x04\x00')
    integer_stub = float_stub + 16
    u.mem_write(integer_stub, b'\xdd\xd8\xc3')
    width, height, use, scale = 1024, 768, 0, 1.0
    applied, events = [], []
    integer_calls = []
    recent = deque(maxlen=8)

    def returned(result=0, pop=0):
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, result)
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    def hook(_u, address, _size, _data):
        recent.append(hex(address))
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x767440:
            name = n.read_words(u, sp + 4, 1)[0]
            returned(wide if name == 0x9f3e64 else resolution)
        elif address == 0x88be0d:
            _, _, out_width, out_separator, out_height = n.read_words(u, sp + 4, 5)
            n.write_words(u, out_width, width)
            n.write_words(u, out_height, height)
            u.mem_write(out_separator, b'x')
            returned(3)
        elif address == 0x76f0d0:
            returned(use, 4)
        elif address == 0x76fb80:
            u.mem_write(float_result, struct.pack('<d', scale))
            u.reg_write(UC_X86_REG_EIP, float_stub)
        elif address == 0x88b9c0:
            u.reg_write(UC_X86_REG_EAX, height if not integer_calls else width)
            integer_calls.append(address)
            u.reg_write(UC_X86_REG_EIP, integer_stub)
        elif address == 0x48f580:
            applied.append(n.read_words(u, sp + 4, 1)[0])
            returned(0, 8)
        elif address in (0x4898b0, 0x51d960):
            returned()
        elif address == 0x81b530:
            events.append(n.read_words(u, sp + 4, 1)[0])
            returned()
        elif address == vt + 0x100:
            out = n.read_words(u, sp + 4, 1)[0]
            u.mem_write(out, struct.pack('<4f', 0, 0, height, width))
            returned(0, 4)

    u.hook_add(UC_HOOK_CODE, hook)
    n.write_words(u, 0xbd0778, root)
    n.write_words(u, 0xbd09b0, enabled, value)
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device, vt)
    n.write_words(u, vt + 0x90, vt + 0x100)
    lines = ['# Original UI scale policy; provider boundaries intercepted.',
             '# case mode width height widescreen useUiScale uiScale; appliedFloatBits events...']
    for mode, (width, height), widescreen, use, scale in itertools.product(
            range(3), ((640, 480), (1024, 768), (1280, 720), (1600, 900),
                       (1920, 1080), (2560, 1440), (800, 1200)),
            range(2), range(2), (0.5, 0.64, 0.8, 0.9, 1.0, 1.5)):
        n.write_words(u, enabled + 0x30, use)
        u.mem_write(value + 0x2c, struct.pack('<f', scale))
        n.write_words(u, wide + 0x30, widescreen)
        n.write_words(u, resolution + 0x28, resolution + 0x100)
        n.write_words(u, 0xbd07f0, 0, 0)
        u.mem_write(0xbd07f8, struct.pack('<f', width / height if widescreen else 4 / 3))
        n.write_words(u, viewport + 16, width, height)
        applied.clear()
        events.clear()
        integer_calls.clear()
        try:
            if mode == 0:
                invoke(u, 0x5240e0, [viewport])
            else:
                invoke(u, 0x5237e0 if mode == 1 else 0x518b60, [0, 0, value])
        except Exception as error:
            raise RuntimeError((mode, width, height, widescreen, use, scale, list(recent),
                                hex(u.reg_read(UC_X86_REG_EIP)))) from error
        assert len(applied) <= 1, applied
        inputs = [mode, width, height, widescreen, use, scale]
        outputs = [str(applied[0]) if applied else '-', *map(str, events)]
        lines.append('case ' + ' '.join(map(str, inputs)) + ';' + ' '.join(outputs))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(lines) - 2, 'native UI scale cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
