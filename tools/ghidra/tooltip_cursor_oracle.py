"""Capture original 61B2E0 cursor anchor selection, offsets and effective scale.

The ordinary frame update returns without authored Lua; SetPoint records the
original call arguments. Cursor conversion and tooltip arithmetic are unmodified.
Fade is disabled. Requires the locally owned fingerprinted build-12340 image.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n


def capture_position(u, function, extent, cursor):
    root = n.HEAP + 0x2000
    n.write_words(u, 0xb499a8, root)
    n.write_floats(u, root + 0x1224, [cursor[0] / extent[0], cursor[1] / extent[1]])
    aspect = struct.unpack('<I', struct.pack('<f', extent[0] / extent[1]))[0]
    n.invoke(u, 0x47bf90, [aspect])
    result = []

    def push(uc, address, size, _):
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret = n.read_words(uc, sp, 1)[0]
        result.append(struct.unpack('<d', uc.mem_read(sp + 8, 8))[0])
        uc.reg_write(UC_X86_REG_ESP, sp + 4)
        uc.reg_write(UC_X86_REG_EIP, ret)

    hook = u.hook_add(UC_HOOK_CODE, push, begin=0x84e2a0, end=0x84e2a0)
    n.invoke(u, function, [0])
    u.hook_del(hook)
    assert len(result) == 2
    return result


def capture(u, kind, scale, cursor, offset):
    frame, root = n.HEAP, n.HEAP + 0x2000
    n.write_words(u, frame + 0xa0, root)
    n.write_words(u, frame + 0x2a0, kind)
    n.write_floats(u, frame + 0x7c, [scale])
    n.write_floats(u, frame + 0x3e0, offset)
    n.write_floats(u, root + 0x1224, [cursor[0] / 1024., cursor[1] / 768.])
    n.write_words(u, 0xb499a8, root)
    n.write_floats(u, 0xac0cb4, [1024., 768.])
    result = []

    def hook(uc, address, size, _):
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret = n.read_words(uc, sp, 1)[0]
        if address == 0x48a260:
            point, target, relative = n.read_words(uc, sp + 4, 3)
            assert target == root and relative == 6
            result.extend([point, *n.read_floats(uc, sp + 16, 2)])
        uc.reg_write(UC_X86_REG_ESP, sp + (28 if address == 0x48a260 else 8))
        uc.reg_write(UC_X86_REG_EIP, ret)

    hooks = [u.hook_add(UC_HOOK_CODE, hook, begin=a, end=a) for a in (0x490770, 0x48a260)]
    u.reg_write(UC_X86_REG_ECX, frame)
    n.invoke(u, 0x61b2e0, [0])
    for h in hooks:
        u.hook_del(h)
    assert len(result) == 3
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    rows = ['# kind scale cursorXY ownerOffsetXY point resultOffsetXY']
    for cursor, kind, scale, offset in itertools.product(
        [(512., 384.), (1022., 766.), (1., 2.)], [8, 11], [.5, 1., 1.75],
        [(0., 0.), (13., -9.)],
    ):
        values = [kind, scale, *cursor, *offset, *capture(u, kind, scale, cursor, offset)]
        rows.append(' '.join(map(str, values)))
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native cursor anchors')
    rows = ['# width height physicalXY resultXY; both Glue and World APIs agree']
    for extent, fraction in itertools.product(
        [(1024, 768), (1536, 1152), (1920, 1080), (3440, 1440)],
        [(0., 0.), (.5, .5), (1., 1.), (.2, .7)],
    ):
        cursor = [v * f for v, f in zip(extent, fraction)]
        result = capture_position(u, 0x4dcb60, extent, cursor)
        assert result == capture_position(u, 0x510a10, extent, cursor)
        rows.append(' '.join(map(str, [*extent, *cursor, *result])))
    args.output.with_name('ui_cursor_position_native.txt').write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native cursor positions through both APIs')
