"""Capture original 4893C0 region anchors, scale and screen clamping.

Runs original edge and anchor functions against retained target rectangles.
Only virtual width/height and clamp-inset getters return supplied region facts;
the target's virtual origin-rebase query returns false, as for ordinary frames.
No anchor arithmetic, edge solve, scale or clamp result is replaced.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n

REGION, TARGET, VTABLE, ANCHORS, OUTPUT = [n.HEAP + i * 0x1000 for i in range(5)]
WIDTH, HEIGHT, INSETS, FALSE = [n.STOP + i * 0x100 for i in range(1, 5)]


def setup():
    u = n.emulator()
    # Width and height getters: fld [ecx+80/84], ret.
    u.mem_write(WIDTH, bytes.fromhex('d98180000000c3'))
    u.mem_write(HEIGHT, bytes.fromhex('d98184000000c3'))
    u.mem_write(FALSE, bytes.fromhex('31c0c3'))
    n.write_words(u, VTABLE + 0x28, WIDTH, HEIGHT, 0, INSETS, FALSE)
    n.write_words(u, REGION, VTABLE)
    n.write_words(u, TARGET, VTABLE)
    n.write_words(u, TARGET + 0x40, 0x100)
    n.write_floats(u, 0xac0cb4, [1024., 768.])

    def hook(uc, address, size, _):
        if address != INSETS:
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret, *pointers = n.read_words(uc, sp, 5)
        facts = uc.reg_read(UC_X86_REG_ECX) + 0x90
        for i, pointer in enumerate(pointers):
            uc.mem_write(pointer, bytes(uc.mem_read(facts + i * 4, 4)))
        uc.reg_write(UC_X86_REG_ESP, sp + 20)
        uc.reg_write(UC_X86_REG_EIP, ret)
    u.hook_add(UC_HOOK_CODE, hook, begin=INSETS, end=INSETS)
    return u


def capture(u, scale, extent, target, points, clamp=False, insets=(0, 0, 0, 0)):
    u.mem_write(REGION + 0xc, bytes(0x54))
    n.write_words(u, REGION + 0x40, 0x1000 if clamp else 0)
    n.write_floats(u, REGION + 0x5c, [scale])
    n.write_floats(u, REGION + 0x80, extent)
    n.write_floats(u, REGION + 0x90, insets)
    # Native CRect is bottom,left,top,right.
    n.write_floats(u, TARGET + 0x44, [target[1], target[0], target[3], target[2]])
    for i, (point, relative, x, y) in enumerate(points):
        anchor = ANCHORS + i * 32
        n.write_words(u, REGION + 0xc + point * 4, anchor)
        n.write_floats(u, anchor, [x, y])
        n.write_words(u, anchor + 8, TARGET, relative)
    u.reg_write(UC_X86_REG_ECX, REGION)
    n.invoke(u, 0x4893c0, [OUTPUT])
    assert u.reg_read(UC_X86_REG_EAX) == 1
    bottom, left, top, right = n.read_floats(u, OUTPUT, 4)
    return [left, bottom, right, top]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = setup()
    rows = ['# scale width height targetLTRB clamp insetsLRTB count [point relative x y]* resultLTRB']
    for scale, extent, target, offset, clamp in itertools.product(
        [.5, 1., 1.75], [(180., 60.), (1300., 900.)],
        [(0., 0., 1024., 768.), (40., 80., 280., 200.)],
        [(3., -4.), (-500., 800.)], [False, True],
    ):
        insets = (10., -20., 30., -40.) if clamp else (0., 0., 0., 0.)
        configurations = [[(p, r, *offset)] for p in range(9) for r in (0, 4, 8)]
        if offset == (3., -4.):
            configurations += [[(0, 0, *offset), (8, 8, -offset[0], -offset[1])]]
        for points in configurations:
            result = capture(u, scale, extent, target, points, clamp, insets)
            values = [scale, *extent, *target, int(clamp), *insets, len(points)]
            values += [v for point in points for v in point]
            rows.append(' '.join(map(str, [*values, *result])))
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native region layouts')
