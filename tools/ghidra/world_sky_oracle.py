"""Capture original build-12340 sky geometry and per-vertex color composition.

Only dynamic-array allocation is replaced with resident buffers. Mesh math,
indices, day tables, RGB/HSV conversion and gradient composition execute the
fingerprinted original executable. No client entry point or OS code executes.
"""
import argparse
import struct
import random
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture():
    u = n.emulator()
    owner, vertices, colors, indices = [n.HEAP + i * 0x2000 for i in range(4)]
    n.write_words(u, owner + 8, vertices)
    n.write_words(u, owner + 0x14, colors)
    n.write_words(u, owner + 0x20, indices)
    def hook(u, address, size, context):
        if address in (0x6c0270, 0x599820, 0x7f1f20):
            sp = u.reg_read(UC_X86_REG_ESP)
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x7f3978:
            return_value(u, 1)
    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Original build 12340 sky: raw little-endian vertex/index/color bytes.']
    for radius in (.5, 1., 2., 100.):
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x7f2470, [bits(radius)])
        vertex_count, index_count = struct.unpack('<2H', u.mem_read(owner + 0x28, 4))
        rows.append(f'mesh {radius} {vertex_count} {index_count} ' + bytes(u.mem_read(vertices, vertex_count * 12)).hex() + ' ' + bytes(u.mem_read(indices, index_count * 2)).hex())
    for address, count in [(0xa41a90, 7), (0xaf4b7c, 12), (0xaf4bac, 12), (0xa41ca8, 1), (0xa41b00, 1), (0xa41cec, 1), (0x9f193c, 1), (0xa1e8d4, 1), (0xa41b14, 1)]:
        rows.append(f'constant {address:08x} ' + bytes(u.mem_read(address, count * 4)).hex())
    rows.append('constant 009eaf48 ' + bytes(u.mem_read(0x9eaf48, 8)).hex())
    palettes = [[0xff123456, 0xff345678, 0xff56789a, 0xff789abc, 0xff9abcde, 0xffbcdef0], [0xff010305, 0xff050709, 0xff090b0d, 0xff0d0f11, 0xff111315, 0xff151719], [0xff000000, 0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff, 0xff808080]]
    rng = random.Random(12340)
    palettes.extend([[0xff000000 | rng.randrange(0x1000000) for _ in range(6)] for _ in range(128)])
    for palette_id, palette in enumerate(palettes):
        rows.append(f'palette {palette_id} ' + struct.pack('<6I', *palette).hex())
        for time in ((0., .125, .25, .375, .5, .625, .75, .875, 1.) if palette_id < 3 else (.27, .895)):
            for highlight in ((0., .5, 1.) if palette_id < 3 else (.3, .99)):
                for azimuth in ((0., 1., 3.926991, 6.2831855) if palette_id < 3 else (1.23,)):
                    n.write_words(u, 0xd38be0, *palette)
                    n.write_floats(u, 0xd38b04, [time])
                    n.write_floats(u, 0xd38c28, [highlight])
                    n.write_floats(u, 0xd38b3c, [azimuth])
                    u.reg_write(UC_X86_REG_ECX, owner)
                    n.invoke(u, 0x7f0530, [])
                    rows.append(f'gradient {palette_id} {time} {highlight} {azimuth} ' + bytes(u.mem_read(colors, vertex_count * 4)).hex())
    for forward in [[x,y,z] for x,y,z in [(1,0,0),(0,1,0),(-1,0,0),(0,-1,0),(0,0,1),(0,0,-1),(.0099,0,1),(.01,0,1),(.0101,0,1)]] + [[rng.uniform(-1,1) for _ in range(3)] for _ in range(64)]:
        raw = struct.pack('<3f', *forward)
        u.mem_write(0xd38b30, raw)
        n.invoke(u, 0x7f3920, [])
        rows.append('azimuth ' + raw.hex() + ' ' + bytes(u.mem_read(0xd38b3c,4)).hex())
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original sky records')
