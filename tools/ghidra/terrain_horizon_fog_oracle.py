"""Capture DayNight color publication through the native WDL draw-state boundary.

Runs 7F3230 depth arithmetic with a controlled palette, then 7816F0 through
7F16F0, then 7D5E70 through its GX fog-state stores. The 7816F0 sky producer
retains the already captured depth result; sky/model/device providers are
hooked. WMO reports no interior, and camera liquid is an explicit input.
This verifies color publication, not complete world rendering or visibility.
"""
import argparse
import itertools
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP
import wmo_registration_oracle as n
from camera_water_oracle import ret


def capture(color, depth, liquid, manual):
    u = n.emulator()
    row, pointers, gx, states, tile, caps = [n.HEAP + i * 0x4000 for i in range(6)]
    n.write_words(u, 0xad4070, 1, 1)
    n.write_words(u, 0xad4084, pointers)
    n.write_words(u, pointers, row)
    n.write_floats(u, row + 0x18, [10., .5, .5, .5])
    n.write_words(u, 0xcd8794, liquid)
    n.write_floats(u, 0xcd8790, [-depth])
    n.write_words(u, 0xd39008, 0)
    n.write_words(u, 0xc5df88, gx)
    n.write_words(u, gx + 0xf58, 1)
    n.write_words(u, gx + 0x28f4, states)
    n.write_words(u, caps + 0xb4, 2)
    n.write_words(u, tile + 0x40, 12)
    stages = []

    def hook(machine, address, size, unused):
        if address == 0x7ee750:
            for destination in (0xd38b8c, 0xd38cac, 0xd38ca8):
                n.write_words(machine, destination, color)
            ret(machine)
        elif address in (0x7f0530, 0x7817cc, 0x7d5fbd):
            machine.reg_write(UC_X86_REG_EIP, n.STOP)
        elif address in (0x7f3920, 0x7f1010, 0x7eea80):
            stages.append(address)
            ret(machine)
        elif address == 0x7f16f0:
            stages.append(address)
        elif address == 0x532af0:
            ret(machine, caps)
        elif address in (0x7eccb0, 0x7ecf00, 0x77fb90, 0x409670):
            ret(machine)
        elif address == 0x685970:
            ret(machine)  # cdecl GX dirty-state notification

    u.hook_add(UC_HOOK_CODE, hook)
    n.invoke(u, 0x7f3230, [])
    intermediate = n.read_words(u, 0xd38b8c, 1)[0]
    if liquid and depth >= 10.:
        assert intermediate != color, (color, depth, intermediate)
    n.write_words(u, 0xd38bf4, color)
    n.write_floats(u, 0xd38b40, [777.])
    n.write_floats(u, 0xd38c1c, [500., .5, 1.])
    n.write_words(u, 0xd38ad0, int(manual != 0))
    n.write_words(u, 0xd38d18, manual)
    n.write_floats(u, 0xd38aa8, [1., .7, 150.])
    n.invoke(u, 0x7816f0, [0, 0])
    assert stages == [0x7f3920, 0x7f1010, 0x7eea80, 0x7f16f0], stages
    ordinary = n.read_words(u, 0xd38b8c, 1)[0]
    n.invoke(u, 0x7d5e70, [tile])
    start, end = [n.read_floats(u, states + offset, 1)[0] for offset in (0xc0, 0xd8)]
    published = n.read_words(u, states + 0xf0, 1)[0]
    assert (start, end, published) == (0., 1., ordinary)
    return intermediate, ordinary, published


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    records = ['# palette depth liquid manual intermediate ordinary GX-color; packed colors are hexadecimal']
    for color, depth, liquid, manual in itertools.product(
            [0xff101010, 0xff202020, 0xff404040, 0xff808080, 0xfff0f0f0, 0xff123456],
            [0., 10., 50.], [0, 1], [0, 0xff4c4c63, 0xffffffff]):
        values = capture(color, depth, liquid, manual)
        records.append(f'{color:08x} {depth:g} {liquid} {manual:08x} '
                       + ' '.join(f'{value:08x}' for value in values))
    args.output.write_text('\n'.join(records) + '\n', encoding='utf-8')
    print(f'Captured {len(records)-1} native horizon fog publications')


if __name__ == '__main__':
    main()
