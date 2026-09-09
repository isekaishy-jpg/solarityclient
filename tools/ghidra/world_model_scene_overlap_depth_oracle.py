"""Capture original 792AD0 -> 792BD0 transformed-group depth conversion.

Uses complete unhooked list insertion and conversion, retaining native order
and the whole-list early return when an entry exceeds the last depth bucket.
"""
import argparse
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from scene_depth_oracle import encoded


def capture(values, additional=()):
    """Insert transformed entries in caller order and inspect every final list."""
    u = n.emulator()
    eye, target, root, model, info = [n.HEAP + i * 0x1000 for i in range(5)]
    n.write_floats(u, eye, values[:3])
    n.write_floats(u, target, values[3:6])
    frame = n.STACK + 0x18000
    n.write_words(u, frame + 8, eye, target)
    u.reg_write(UC_X86_REG_EBP, frame)
    u.reg_write(UC_X86_REG_ESP, frame - 0x100)
    u.emu_start(0x7954a6, 0x795644, timeout=1_000_000, count=100_000)
    n.write_floats(u, 0xadf454, [.5])
    n.write_words(u, root + 0xc, 0x400)
    n.write_words(u, root + 0xf4, model)
    n.write_words(u, model + 0x130, info)
    n.write_words(u, model + 0x1e0, 1)
    n.write_words(u, info, 8)
    heads = [0xcdaf48] + [0xcd9054 + bucket * 0x6c for bucket in range(64)]
    for head in heads:
        n.write_words(u, head, 0, head + 4, (head + 4) | 1)
    bounds = [values[6:12], *additional]
    groups = [n.HEAP + 0x5000 + i * 0x1000 for i in range(len(bounds))]
    for group, box in zip(groups, bounds):
        n.write_floats(u, group + 0x24, box)
        n.write_words(u, group + 0x50, 0)
        n.invoke(u, 0x792ad0, [root, group])
    order = members(u, heads[0], groups)
    n.invoke(u, 0x792bd0, [])
    destinations = [-1] * len(groups)
    for index, head in enumerate(heads):
        for member in members(u, head, groups):
            destinations[member] = -2 if index == 0 else index - 1
    return order, destinations


def members(u, head, groups):
    """Walk the original link-offset-zero list, rejecting unexpected members."""
    result = []
    member = n.read_words(u, head + 8, 1)[0]
    while member and member & 1 == 0:
        assert member in groups and groups.index(member) not in result
        result.append(groups.index(member))
        member = n.read_words(u, member + 4, 1)[0]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('m2_fixture', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    cases = [floats(line.rsplit(' ', 1)[0]) for line in args.m2_fixture.read_text().splitlines()
             if line and not line.startswith('#')]
    rows = ['# complete unhooked 792AD0 then 792BD0; eye3 target3 min3 max3; destination (-2 retains overlap, 0..63 depth)']
    for values in cases:
        order, destinations = capture(values)
        assert order == [0]
        rows.append(encoded(values) + f' {destinations[0]}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(cases)} original transformed-group depth conversions')
    for distances in [[10., 3000., 20.], [3000., 10., 20.], [10., 20., 3000.], [10., 20., 30.]]:
        boxes = [[x, -1., -1., x + 1., 1., 1.] for x in distances]
        print(distances, capture([0., 0., 0., 1., 0., 0.] + boxes[0], boxes[1:]))


if __name__ == '__main__':
    main()
