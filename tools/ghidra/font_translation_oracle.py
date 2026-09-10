"""Capture unmodified 006C6190 alignment at fractional screen positions.

No hooks or arithmetic substitutions are used. The original executable's
string has one line and no embedded textures; the output is in screen pixels.
"""
import argparse
import itertools
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n


def capture():
    u = n.emulator()
    owner = n.HEAP
    rows = ['# width height left bottom boxWidth boxHeight lineHeight horizontal vertical nativeX nativeY']
    for (width, height), left, bottom, box_height, horizontal, vertical in itertools.product(
        [(1280, 720), (2560, 1440)], [100.25, -20.375], [347.375, 697.625],
        [13.25, 18.125, 40.375], range(3), range(3)
    ):
        ui_width = width / height * 768.
        line_height = int(18. * height / 768. + .5) / (height / 768.)
        box_width = 244.375
        n.write_words(u, 0xc7d2c4, height, width)
        n.write_floats(u, owner + 0x1c, [line_height / 768., left / ui_width, bottom / 768., 0.])
        n.write_floats(u, owner + 0x3c, [box_width / ui_width, box_height / 768.])
        n.write_words(u, owner + 0x50, vertical, horizontal)
        n.write_words(u, owner + 0xb0, 1)
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x6c6190, [])
        x, y = n.read_floats(u, owner + 0x70, 2)
        rows.append(' '.join(map(str, [width, height, left, bottom, box_width, box_height, line_height, horizontal, vertical, x, y])))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(), encoding='utf-8')
