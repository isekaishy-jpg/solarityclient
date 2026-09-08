"""Capture 7AC060 portal traversal with resident groups and projected windows.

Original code decides group admission, portal sides, rectangle intersection,
recursion, visit order, and indoor fog inheritance. The resident lookup,
already projected portal bounds, and graphics-only clip-stack operations are
supplied at their boundaries. This does not verify polygon projection or the
external-root visibility pass.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EBP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(flags, edges, start, point, maximum, indoor, info_flags=None):
    u = n.emulator()
    info_flags = flags if info_flags is None else info_flags
    root, info, portals, refs, groups, window = [n.HEAP + i * 0x2000 for i in range(6)]
    n.write_words(u, root + 0x130, info, 0, portals, refs)
    n.write_words(u, root + 0x1e0, 1)
    n.write_words(u, 0xd1bee4, maximum)
    n.write_words(u, 0xcfbec0, 1)
    n.write_words(u, 0xd1bed8, n.STOP + 16)
    n.write_words(u, 0xd1c3d0, 3)
    n.write_words(u, 0xd1c424, 1)
    n.write_floats(u, 0xd1c42c, point)
    n.write_floats(u, window, [-1., -1., 1., 1.])
    reference = 0
    for index, value in enumerate(flags):
        n.write_words(u, info + index * 32, info_flags[index])
        n.write_words(u, groups + index * 0x200 + 0x30, value)
        adjacent = [(i, b if a == index else a, 1 if a == index else -1)
                    for i, (a, b, _, _) in enumerate(edges) if index in (a, b)]
        n.write_words(u, groups + index * 0x200 + 0x50, reference, len(adjacent))
        for portal, neighbor, side in adjacent:
            u.mem_write(refs + reference * 8, struct.pack('<HHhH', portal, neighbor, side, 0))
            reference += 1
    for index, (_, _, plane, _) in enumerate(edges):
        u.mem_write(portals + index * 20, struct.pack('<HH4f', 0, 4, *plane))
    visits = []

    def ret(u, count=0, value=0):
        sp = u.reg_read(UC_X86_REG_ESP)
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, sp + 4 + count * 4)

    def hook(u, address, size, _):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x7aea80:
            group = n.read_words(u, sp + 4, 1)[0]
            ret(u, 2, groups + group * 0x200)
        elif address == 0x7a9090:
            portal, output = n.read_words(u, sp + 4, 2)
            index = (portal - portals) // 20
            n.write_floats(u, output + 4, edges[index][3])
            ret(u, 2)
        elif address == n.STOP + 16:
            # Group callback is invoked before this group's portal loop.
            frame = u.reg_read(UC_X86_REG_EBP)
            group, _, clip, depth, _ = n.read_words(u, frame + 8, 5)
            visits.append((group, n.read_words(u, 0xcfbeb8, 1)[0], depth,
                           *n.read_words(u, clip, 4)))
            ret(u)
        elif address in (0x791950, 0x78fb50, 0x790e20):
            ret(u)
        elif address == 0x7a8f20:
            ret(u, 4)
    u.hook_add(UC_HOOK_CODE, hook)
    u.reg_write(UC_X86_REG_ECX, root)
    n.invoke(u, 0x7ac060, [start, 0xffff, window, 0, indoor])
    return visits


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# Native 7AC060 visits with supplied projected portal rectangles.']
    count = 0
    flag_pairs = [(value, value) for value in [0, 8, 0x40, 0x10000]]
    flag_pairs += [(0x40, 0), (0, 0x40), (8, 0), (0, 8), (0x10000, 0), (0, 0x10000)]
    for shape, (middle, info_middle) in itertools.product(range(4), flag_pairs):
        flags = [0, middle, 0, 0]
        info_flags = [0, info_middle, 0, 0]
        bounds = [[-.5, -.5, .5, .5], [.1, -.75, .75, .75],
                  [.5, -.5, .75, .5], [.5005, -.5, .501, .5]][shape]
        edges = [(0, 1, [0., 0., 1., 0.], [-.5, -.5, .5, .5]),
                 (1, 2, [0., 0., 1., 0.], bounds),
                 (2, 0, [0., 0., 1., 0.], [-.9, -.9, .9, .9]),
                 (0, 3, [1., .5, -.25, .125], [.1, -.25, .9, .75])]
        rows.append('scene ' + ' '.join(map(str, [len(flags), *flags, *info_flags, len(edges)])))
        for a, b, plane, rectangle in edges:
            words = struct.unpack('<8I', struct.pack('<8f', *plane, *rectangle))
            rows.append(f'edge {a} {b} ' + ' '.join(f'{word:08x}' for word in words))
        for start, z, maximum, indoor in itertools.product([0, 1], [-1., 0., 1.], [0, 1, 4], [0, 1]):
            point = [.25, -.125, z]
            visits = capture(flags, edges, start, point, maximum, indoor, info_flags)
            words = struct.unpack('<3I', struct.pack('<3f', *point))
            rows.append(f'query {start} {maximum} {indoor} ' + ' '.join(f'{word:08x}' for word in words) + f' {len(visits)}')
            for group, bank, depth, *rectangle in visits:
                rows.append(f'visit {group} {bank} {depth} ' + ' '.join(f'{word:08x}' for word in rectangle))
            count += 1
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'{count} original portal visibility queries')


if __name__ == '__main__':
    main()
