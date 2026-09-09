"""Capture complete 792AD0 WMO exterior-group scene insertion without hooks.

Reuses the exact camera/bounds cases from scene_depth_oracle.py and executes
795400's camera-plane slice. 7AE7B0 performs the real MOGI lookup; 6DED60 writes
initialized native list links. Root flag 400 selects the transformed overlap
list rather than a depth bucket, independently of distance.
"""
import argparse
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from scene_depth_oracle import encoded


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('m2_fixture', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    cases = [floats(line.rsplit(' ', 1)[0]) for line in args.m2_fixture.read_text().splitlines()
             if not line.startswith('#')]
    rows = ['# 7954A6..795644 and complete 792AD0 unhooked; rootFlags groupFlags eye3 target3 min3 max3; destination -2 overlap, -1 absent, 0..63 depth']
    for index, values in enumerate(cases):
        pairs = [(0, 8)]
        if index % 13 == 0:
            pairs += [(root_flags, flags) for root_flags in [0, 0x400]
                      for flags in [0, 8, 0x40, 0x10000, 0x10008, 0x40000]]
        for root_flags, flags in pairs:
            u = n.emulator()
            eye, target, root, model, info, group = [n.HEAP + i * 0x1000 for i in range(6)]
            n.write_floats(u, eye, values[:3])
            n.write_floats(u, target, values[3:6])
            frame = n.STACK + 0x18000
            n.write_words(u, frame + 8, eye, target)
            u.reg_write(UC_X86_REG_EBP, frame)
            u.reg_write(UC_X86_REG_ESP, frame - 0x100)
            u.emu_start(0x7954a6, 0x795644, timeout=1_000_000, count=100_000)
            n.write_floats(u, 0xadf454, [.5])
            n.write_words(u, root + 0xc, root_flags)
            n.write_words(u, root + 0xf4, model)
            n.write_words(u, model + 0x130, info)
            n.write_words(u, model + 0x1e0, 1)
            n.write_words(u, info, flags)
            n.write_floats(u, group + 0x24, values[6:12])
            n.write_words(u, group + 0x50, 0)
            for head in [0xcdaf48] + [0xcd9054 + bucket * 0x6c for bucket in range(64)]:
                # The exact link offset is irrelevant to admission; use two
                # unused group words while retaining native list operations.
                n.write_words(u, head, 0, head + 4, (head + 4) | 1)
            n.invoke(u, 0x792ad0, [root, group])
            link = n.read_words(u, group, 1)[0] & ~1
            destination = -2 if link == 0xcdaf4c else ((link - 0xcd9058) // 0x6c if link else -1)
            assert -2 <= destination < 64, (index, root_flags, flags, hex(link))
            rows.append(f'{root_flags} {flags} ' + encoded(values) + f' {destination}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original WMO scene depth insertions')


if __name__ == '__main__':
    main()
