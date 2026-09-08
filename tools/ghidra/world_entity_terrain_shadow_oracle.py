"""Capture original 7A06A0 terrain-shadow addresses and packed-bit results.

Only the resident tile provider is substituted. Native map admission, coordinate
rounding, MCNK selection and the full-resolution packed-bit lookup execute.
"""
import argparse
import random
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(x, y, packed):
    u = n.emulator()
    point, tile, settings, chunks, shadow = [n.HEAP + at for at in (0, 0x100, 0x700, 0x1000, 0x20000)]
    n.write_floats(u, point, [x, y, 0.])
    n.write_words(u, tile + 0x68, settings)
    for i in range(256):
        n.write_words(u, tile + 0xbc + i * 4, chunks + i * 0x140)
        n.write_words(u, chunks + i * 0x140 + 0x128, shadow)
    u.mem_write(shadow, packed)
    result = [0xffffffff] * 5

    def provider(u, address, size, context):
        if address == 0x79b440:
            result[:2] = n.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 2)
            return_value(u, tile)
        elif address == 0x7a0767:
            result[2] = (u.reg_read(UC_X86_REG_ESI) - chunks) // 0x140
        elif address == 0x7a07e4:
            offset = u.reg_read(UC_X86_REG_ESI)
            result[3] = (offset % 8) * 8 + u.reg_read(UC_X86_REG_ECX)
            result[4] = offset // 8

    u.hook_add(UC_HOOK_CODE, provider)
    n.invoke(u, 0x7a06a0, [point])
    return struct.pack('<2f6I', x, y, *result, u.reg_read(UC_X86_REG_EAX) & 255)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('output', type=Path)
    args = p.parse_args()
    n.initialize(args.executable)
    rng = random.Random(0x7a06a0)
    packed = bytes(rng.randrange(256) for _ in range(512))
    points = [(rng.uniform(-18000., 18000.), rng.uniform(-18000., 18000.)) for _ in range(500)]
    points += [(edge + epsilon, axis) for edge in (-17066.666015625, 0., 17066.666015625) for epsilon in (-.01, -.001, 0., .001, .01) for axis in (0., 31., -47.)]
    points += [(17066.666015625 - cell / 1.9199999570846558 + epsilon, 10.) for cell in (0, 1, 62, 63, 64, 65, 1023, 1024, 32767, 32768) for epsilon in (-.001, 0., .001)]
    points += [(y, x) for x, y in points[500:]]
    rows = [capture(x, y, packed) for x, y in points]
    args.output.write_bytes(packed + b''.join(rows))
    print(f'{len(rows)} original terrain shadow point captures')


if __name__ == '__main__':
    main()
