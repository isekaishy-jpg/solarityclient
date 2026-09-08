"""Execute build-12340 corpse minimap filtering and marker geometry.

The constructor's dimension block, atlas initialization, marker list owner,
inside vertex block and complete outside-arrow draw execute original code.
Archive dimensions, localized text, widget extents, rotation matrix and graphics
submission are supplied boundaries. Normalized native coordinates are captured
at an aspect of 4/3 (height .6); 768 logical units therefore equal .6 native units.
"""
import argparse
import math
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_ECX, UC_X86_REG_EDI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def word(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture():
    u = n.emulator()
    frame, vtable, widths, cvar, ptrs, inside, outside, entries, records, matrix, label = [n.HEAP + i * 0x1000 for i in range(11)]
    n.write_words(u, 0xbd093c, cvar)
    n.write_words(u, frame + 0x20, vtable)
    n.write_words(u, vtable + 0x28, n.STOP + 16, n.STOP + 32)
    u.mem_write(n.STOP + 16, b'\xd9\x05' + struct.pack('<I', widths) + b'\xc3')
    u.mem_write(n.STOP + 32, b'\xd9\x05' + struct.pack('<I', widths + 4) + b'\xc3')
    u.mem_write(label, b'Corpse\0')
    n.write_words(u, 0xd39588, label)
    n.write_words(u, 0xd395f8, ptrs)
    n.write_words(u, ptrs, 0xd39540)
    n.write_words(u, 0xd39444, 1)
    n.write_words(u, 0xd39604, inside)
    n.write_words(u, 0xd39610, outside)
    n.write_words(u, 0xd39614, 8, 0, entries, 0)
    n.write_words(u, 0xbf8088, 8, 0, records, 0)
    n.write_words(u, 0xbeba28, 123)
    draws = []
    def hook(u, address, size, context):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in [0x4b6610, 0x685f50, 0x682340, 0x5eeb70]:
            return_value(u, 0)
        elif address == 0x819d40:
            return_value(u, label)
        elif address == 0x4b6cb0:
            return_value(u, 321)
        elif address == 0x6828c0:
            args = n.read_words(u, sp + 4, 13)
            assert args[0] == 4 and args[2] == 12 and args[10] == 8
            draws.append((bytes(u.mem_read(args[1], 48)).hex(), bytes(u.mem_read(args[9], 32)).hex()))
            return_value(u, 0)
    u.hook_add(UC_HOOK_CODE, hook)
    def block(start, end):
        u.reg_write(UC_X86_REG_EBP, n.STACK + 0x18000)
        u.reg_write(UC_X86_REG_ESP, n.STACK + 0x17000)
        u.emu_start(start, end, timeout=1_000_000, count=100_000)
    n.invoke(u, 0x47bf90, [word(4 / 3)])
    block(0x57dd0b, 0x57df53)
    rows = ['# Original 12340 corpse minimap: native-space float vectors as little-endian bytes.']
    rows.append('dimensions ' + bytes(u.mem_read(0xbf7fc8, 48)).hex() + ' ' + bytes(u.mem_read(0xbf8028, 48)).hex() + ' ' + bytes(u.mem_read(0xbeba44, 4)).hex())
    block(0x583651, 0x583718)
    rows.append('atlas ' + bytes(u.mem_read(0xbf6148 + 8 * 32, 32)).hex())
    for indoor_mode in [0, 1]:
        u.mem_write(0xd39434, bytes([indoor_mode]))
        for zoom in range(6):
            n.write_words(u, 0xaf4e4c, zoom, zoom)
            radius = [150, 120, 90, 60, 40, 25][zoom] if indoor_mode else struct.unpack('<f', struct.pack('<f', [14, 12, 10, 8, 6, 4][zoom] * .5 * struct.unpack('<f', struct.pack('<f', 33.333332))[0]))[0]
            for px, py, x, y in [(0, 0, 0, 0), (10, 20, 10, 20), (0, 0, radius * .8, 0), (0, 0, radius * .8000001, 0), (0, 0, 0, -radius), (0, 0, -radius, 0), (1000, -2000, 100000, -50000), (0, 0, radius, radius), (0, 0, 1e-8, radius), (0, 0, radius, 1e-8)]:
                n.write_words(u, 0xaf4e48, 0)
                n.write_floats(u, 0xd39454, [px, py, 0])
                n.invoke(u, 0x7f4990, [word(x), word(y)])
                n.invoke(u, 0x7f4b60, [0xbf8088])
                values = n.read_words(u, inside, 1) + n.read_words(u, outside, 1) + n.read_words(u, 0xd39618, 1) + n.read_words(u, 0xbf808c, 1)
                angle = bytes(u.mem_read(records + 0x40, 4)).hex() if values[3] else '-'
                special = str(n.read_words(u, records + 0x48, 1)[0]) if values[3] else '-'
                rows.append(f'filter {indoor_mode} {zoom} ' + struct.pack('<4f', px, py, x, y).hex() + ' ' + ' '.join(map(str, values)) + f' {angle} {special}')
    # Geometry supplies the marker list and the caller's local-player snapshot.
    for scale in [.5, 1, 1.5]:
        for width, height in [(140, 140), (200, 100)]:
            for heading in [0, .7, 1.5707964]:
                for x, y in [(30, 20), (-100, 60), (10, -200)]:
                    native_width, native_height = width / 1280, height / 1280
                    n.write_floats(u, widths, [native_width, native_height])
                    n.write_floats(u, frame + 0x7c, [scale])
                    n.write_floats(u, frame + 0x2a4, [heading])
                    n.write_words(u, cvar + 0x30, int(heading != 0))
                    c, s = math.cos(-heading), math.sin(-heading)
                    cx, cy = native_width * scale * .5, native_height * scale * .5
                    rotation = [c, s, 0, 0, -s, c, 0, 0, 0, 0, 1, 0, cx - c * cx + s * cy, cy - s * cx - c * cy, 0, 1]
                    n.write_floats(u, matrix, rotation)
                    n.write_floats(u, 0xd39454, [10, 20, 0])
                    n.write_floats(u, 0xd39570, [x, y])
                    n.write_words(u, 0xd39618, 1)
                    n.write_words(u, entries, 0xd39540)
                    n.write_words(u, 0xbf5fc0, records + 0x100)
                    bp = n.STACK + 0x18000
                    n.write_words(u, bp - 0x28, 0xd39614)
                    n.write_floats(u, bp - 0x94, [10, 20, 0])
                    n.write_floats(u, bp - 0x98, [100])
                    n.write_floats(u, bp - 0x138, rotation)
                    u.reg_write(UC_X86_REG_EAX, 0)
                    u.reg_write(UC_X86_REG_EDI, 0)
                    u.reg_write(UC_X86_REG_EBX, frame)
                    block(0x5826c6, 0x582859)
                    vertices = bytes(u.mem_read(bp - 0x1b4, 48)).hex()
                    inputs = struct.pack('<6f', scale, width, height, heading, x, y).hex()
                    rows.append('inside ' + inputs + ' ' + vertices)
                    # 4F5130 computes the stored f32 arrow angle without hooks.
                    n.write_floats(u, entries + 0x100, [x, y, 0])
                    u.mem_write(n.STOP + 64, b'\xd9\x1d' + struct.pack('<I', 0xd39210) + b'\xc3')
                    sp = n.STACK + 0x17000
                    n.write_words(u, sp, n.STOP + 64, 0xd39454, entries + 0x100)
                    n.write_words(u, sp + 12, n.STOP)
                    # Store x87 return then terminate before stack arguments.
                    u.reg_write(UC_X86_REG_ESP, sp)
                    u.emu_start(0x4f5130, n.STOP + 70, timeout=1_000_000, count=100_000)
                    n.write_words(u, 0xd3944c, 0)
                    n.write_words(u, 0xd39448, 1)
                    n.write_words(u, 0xbf808c, 1)
                    n.write_words(u, records + 0x44, 1, 2)
                    draws.clear()
                    u.reg_write(UC_X86_REG_ECX, frame)
                    n.invoke(u, 0x57d860, [matrix])
                    assert len(draws) == 1
                    rows.append('outside ' + inputs + ' ' + draws[0][0] + ' ' + draws[0][1])
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} native corpse marker samples')
