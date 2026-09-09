"""Capture 799F80's moving-group overlap and full-frustum admission.

The original window construction, overlap loop, and six-plane box tests run.
Hooks observe the already-tested 7B3A10 traversal boundary and suppress its
render callbacks; they do not replace any admission condition under test.
"""
import argparse
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_EAX
import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from scene_depth_oracle import encoded


def capture(frame, bounds, visible, exterior):
    """Supply one moving group and optional prior visible group to 799F80."""
    u = n.emulator()
    group, reference, root, window, admitted = [n.HEAP + i * 0x1000 for i in range(5)]
    n.write_floats(u, 0xcdb108, frame[64:88])
    n.write_words(u, 0xcd8798, 0)
    n.write_floats(u, 0xadf59c, [exterior])
    n.write_floats(u, window, [0., 0., 1., 1.])
    # One already-built moving list, with the usual link at group + 0x18.
    n.write_words(u, 0xcdaf48, 0x18, group + 0x18, group)
    n.write_words(u, group + 0x18, 0xcdaf4c, 0xcdaf4d)
    n.write_words(u, group + 0x20, reference)
    n.write_words(u, reference + 8, root)
    n.write_floats(u, group + 0x24, bounds)
    n.write_words(u, 0xcdb088, admitted if visible is not None else 0)
    if visible is not None:
        n.write_floats(u, admitted + 0x24, visible)
        n.write_words(u, admitted + 0xb4, 0)
    callbacks = []

    def boundary(uc, address, size, data):
        if address not in [0x7b3a10, 0x78fb60, 0x799b70, 0x793270]:
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7b3a10:
            assert n.read_words(uc, sp + 4, 3) == (group, window, 1)
            callbacks.append('group')
        if address == 0x793270:
            assert n.read_words(uc, sp + 4, 4) == (group + 0x84, 0, 1, 0)
            callbacks.append('unit')
        uc.reg_write(UC_X86_REG_EAX, 0)
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + (16 if address == 0x7b3a10 else 4))

    u.hook_add(UC_HOOK_CODE, boundary)
    n.invoke(u, 0x799f80, [window])
    assert callbacks in [[], ['group', 'unit']]
    return bool(callbacks)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    frames = [floats(line) for line in args.frames.read_text().splitlines() if line and not line.startswith('#')]
    rows = ['# camera index; group min3 max3; visible min3 max3; hasVisible exteriorDepth accepted']
    for index in range(0, len(frames), 54):
        frame = frames[index]
        corners = [frame[i:i+3] for i in range(64, 88, 3)]
        center = [sum(c[axis] for c in corners) / 8 for axis in range(3)]
        points = [center, *corners, [value + 10000 for value in center]]
        for point in points:
            bounds = [v - .1 for v in point] + [v + .1 for v in point]
            # Shared faces are admitted; shifting one float side outside rejects.
            adjacent = bounds[:]
            adjacent[0] = bounds[3]
            adjacent[3] += 1
            disjoint = [v + 5000 for v in bounds]
            for visible in [None, bounds, adjacent, disjoint]:
                for exterior in [-1., 0.]:
                    result = capture(frame, bounds, visible, exterior)
                    rows.append(f'{index} ' + encoded(bounds + (visible or [0.] * 6))
                                + f' {int(visible is not None)} {int(exterior)} {int(result)}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original overlap admission cases')


if __name__ == '__main__':
    main()
