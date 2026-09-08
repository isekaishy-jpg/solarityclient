"""Capture native floor target blending and MODD color splitting.

Runs 7A0D60 with only its decoded floor-color and daylight providers replaced;
the transform, 7C1AD0 split and 6ACC50 blend execute the pinned client image.
"""
import argparse
import random
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(color, exterior):
    u = n.emulator()
    entity, root, point, face, palette, output = [n.HEAP + x for x in (0, 0x400, 0x800, 0x900, 0x1000, 0x1400)]
    n.write_floats(u, root + 0xb0, [1., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0.])
    n.write_words(u, palette + 0x1a8, 0xff204080, 0xff806040)

    def provider(u, address, size, context):
        if address == 0x7ecef0:
            return_value(u, palette)
        elif address == 0x7aeb40:
            sp = u.reg_read(UC_X86_REG_ESP)
            args = n.read_words(u, sp + 4, 5)
            n.write_words(u, args[3], color)
            u.mem_write(args[4], bytes([exterior]))
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 24)

    u.hook_add(UC_HOOK_CODE, provider)
    u.reg_write(UC_X86_REG_ECX, entity)
    n.invoke(u, 0x7a0d60, [root, 0, face, point])
    targets = [n.read_words(u, entity + at, 1)[0] for at in (0x88, 0xc0, 0x7c)]
    n.write_words(u, output, color)
    n.invoke(u, 0x7c1ad0, [output, output + 4, 112, output + 8, 96])
    return struct.pack('<7I', color, exterior, *targets, *n.read_words(u, output + 4, 2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rng = random.Random(0x7a0d60)
    colors = [0, 0xffffffff, 0x10203040, 0xff000000]
    colors += [(alpha << 24) | rgb for alpha in (0, 1, 127, 128, 254, 255) for rgb in (0x102030, 0x8090a0, 0xffffff)]
    colors += [rng.getrandbits(32) for _ in range(170)]
    rows = [capture(color, exterior) for color in colors for exterior in (0, 1)]
    args.output.write_bytes(b''.join(rows))
    print(f'{len(rows)} native floor target and MODD split captures')


if __name__ == '__main__':
    main()
