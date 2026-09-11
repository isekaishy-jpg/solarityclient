"""Capture original 793270 registered-model group acceptance and 7C1730 fog.

Supplies registration membership and retained group windows, including direct
moving-root snapshots. Original sphere clipping, first acceptance and fog writes
run natively. Only final scene-list insertion and the palette provider are stubs;
the model pointer is null to omit activation. Spatial registration, outdoor
depth-list admission, owner lifecycle and GPU rendering require separate checks.
"""
import argparse
import itertools
from pathlib import Path
import struct

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from world_scene_bounds_oracle import floats


def words(values):
    return ' '.join(f'{struct.unpack("<I", struct.pack("<f", value))[0]:08x}' for value in values)


def capture(executable, frames):
    n.initialize(executable)
    frame = floats(next(line for line in frames.read_text().splitlines() if not line.startswith('#')))
    left, right, full = [0., 0., 1., .5], [0., .5, 1., 1.], [0., 0., 1., 1.]
    # Final group records: (identity, allowed, bank, windows). Direct records
    # snapshot an earlier clip count and fog bit from that same group.
    scenes = [
        ([(0, 1, 0, [left]), (1, 1, 1, [right]), (2, 1, 0, [full])], []),
        ([(2, 1, 1, [full]), (1, 1, 0, [right]), (0, 1, 1, [left])], []),
        ([(0, 1, 1, [left, right]), (1, 1, 0, [full]), (2, 0, 1, [full])], [(0, 1, 0)]),
        ([(0, 0, 1, [left, right]), (1, 1, 1, [right]), (2, 1, 0, [full])], [(0, 1, 0), (1, 1, 1)]),
        ([(0, 1, 0, []), (1, 0, 1, [full]), (2, 1, 1, [left, right])], []),
    ]
    rows = ['# Original 793270 and 7C1730; supplied registrations/group clips; scene insertion and palette provider stubs.',
            '# case previous interior referencesMask reverseReferences center3 radius -> retainedBank queryRGB3; f32 values are hex words.',
            'colors ff204060 ffc08020']
    for scene_id, (groups, direct) in enumerate(scenes):
        rows.append(f'scene {scene_id}')
        for identity, allowed, bank, windows in groups:
            rows.append(f'group {identity} {allowed} {bank} {len(windows)} ' + words([v for w in windows for v in w]))
        for identity, count, bank in direct:
            rows.append(f'direct {identity} {count} {bank}')
        for previous, interior, mask, reverse, sphere in itertools.product(
                range(2), range(2), range(8), range(2),
                ([10., -4., 0., .25], [10., 4., 0., .25], [10., 0., 0., 2.], [120., 0., 0., .25])):
            u = n.emulator()
            owner, descriptor, reference, clip, window, palette, query = [n.HEAP + i * 0x1000 for i in range(7)]
            n.write_words(u, descriptor, 4, 0, reference)
            n.write_words(u, reference + 4, owner, 1)
            n.write_words(u, owner + 0xc, previous * 0x8000)
            n.write_words(u, owner + 0x20, 2)
            u.mem_write(owner + 0x25, b'\x01')
            n.write_words(u, owner + 0x7c, interior)
            n.write_floats(u, owner + 0x30, [1000.])
            n.write_floats(u, owner + 0x38, sphere)
            n.write_floats(u, 0xcdb108, frame[64:88])
            n.write_words(u, 0xcd8798, 0)
            for at, color in ((0x8c, 0xff204060), (0xa0, 0xffc08020)):
                n.write_words(u, palette + at, color)
                n.write_floats(u, palette + at + 4, [10., 20., 1.])

            def hook(machine, address, size, unused):
                if address == 0x6ded60:
                    sp = machine.reg_read(UC_X86_REG_ESP)
                    return_value(machine, 0)
                    machine.reg_write(UC_X86_REG_ESP, sp + 8)
                elif address == 0x7ecef0:
                    return_value(machine, palette)
            u.hook_add(UC_HOOK_CODE, hook)

            def visit(identity, force, bank, windows):
                if not mask & (1 << identity):
                    return
                for index, values in enumerate(windows):
                    n.write_floats(u, window, values)
                    n.invoke(u, 0x790e20, [0xcdb108, window])
                    u.mem_write(clip + index * 0x100, bytes(u.mem_read(0xcdb168, 0xfc)))
                    n.write_words(u, clip + index * 0x100 + 0xf8,
                                  clip + (index + 1) * 0x100 if index + 1 < len(windows) else 1)
                n.invoke(u, 0x793270, [descriptor, clip if windows else 0, force, bank])
            for identity, count, bank in direct:
                group = next(group for group in groups if group[0] == identity)
                visit(identity, 1, bank, group[3][:count])
            for identity, allowed, bank, windows in groups:
                if allowed:
                    visit(identity, 0, bank, windows)
            selected = int(bool(n.read_words(u, owner + 0xc, 1)[0] & 0x8000))
            u.reg_write(UC_X86_REG_ECX, owner)
            n.invoke(u, 0x7c1730, [query])
            rows.append(f'case {previous} {interior} {mask} {reverse} ' + words(sphere) +
                        f' {selected} ' + ' '.join(f'{v:08x}' for v in n.read_words(u, query + 0xb8, 3)))
    return '\n'.join(line.rstrip() for line in rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    result = capture(args.executable, args.frames)
    args.output.write_text(result, encoding='ascii')
    print(f'{sum(line.startswith("case ") for line in result.splitlines())} native registered model fog queries')
