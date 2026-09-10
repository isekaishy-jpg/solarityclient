"""Capture mixed static/moving WMO depth-list callback order in build 12340.

Original 792AD0 inserts entries, 792BD0 converts the moving list, and 79A160
consumes all 64 bins. Native list links and bounds tests execute unchanged.
Hooks disable optional occlusion and stop at the group-portal / M2 callback
boundaries. This proves the order entering 7B3A10, not its portal recursion.
"""
import argparse
import random
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ECX,
    UC_X86_REG_EIP, UC_X86_REG_ESP,
)

import wmo_registration_oracle as n
from world_model_portal_projection_oracle import words
from world_scene_projection_oracle import capture as camera_capture


def capture(entries):
    """Each entry is (moving, MOGI flags, X translation) with +/-16 bounds."""
    u = n.emulator()
    eye, target = [-30., -1., 1.], [-29., -1., 1.]
    native_camera = camera_capture(u, eye, [1., 0., 0.], [0., 0., 1.], 1., 1.5, .1, 5000.)
    n.write_floats(u, 0xcdb108, native_camera[48:72])
    u.reg_write(UC_X86_REG_ECX, 0xcdb168)
    n.invoke(u, 0x984240, [0xcdb108])
    n.write_words(u, 0xcd8798, 0)
    window = n.HEAP + 0x7000
    n.write_floats(u, window, [0., 0., 1., 1.])
    n.invoke(u, 0x790e20, [0xcdb108, window])
    n.write_floats(u, n.HEAP, eye)
    n.write_floats(u, n.HEAP + 0x1000, target)
    frame = n.STACK + 0x18000
    n.write_words(u, frame + 8, n.HEAP, n.HEAP + 0x1000)
    u.reg_write(UC_X86_REG_EBP, frame)
    u.reg_write(UC_X86_REG_ESP, frame - 0x100)
    u.emu_start(0x7954a6, 0x795644, timeout=1_000_000, count=100_000)
    n.write_floats(u, 0xadf454, [.5])
    for head in [0xcdaf48] + [0xcd9054 + bucket * 0x6c for bucket in range(64)]:
        n.write_words(u, head, 0, head + 4, (head + 4) | 1)
    groups = []
    for index, (moving, flags, x) in enumerate(entries):
        base = n.HEAP + 0x8000 + index * 0x800
        root, group, model, info, owner = [base + offset for offset in (0, 0x200, 0x400, 0x600, 0x700)]
        groups.append(group)
        n.write_words(u, root + 0xc, 0x400 if moving else 0)
        n.write_words(u, root + 0xf4, model)
        n.write_words(u, model + 0x130, info)
        n.write_words(u, model + 0x1e0, 1)
        n.write_words(u, info, flags)
        n.write_words(u, owner + 8, root)
        n.write_words(u, group + 0x20, owner)
        n.write_floats(u, group + 0x24, [x - 16., -16., -16., x + 16., 16., 16.])
        n.write_words(u, group + 0x50, 0)
        n.invoke(u, 0x792ad0, [root, group])
    n.invoke(u, 0x792bd0, [])
    visited = []

    def callbacks(uc, address, size, user):
        if address not in (0x7cce00, 0x78fdc0, 0x7b3a10, 0x7998a0):
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7b3a10:
            group = n.read_words(uc, sp + 4, 1)[0]
            visited.append(groups.index(group))
        uc.reg_write(UC_X86_REG_EAX, 0)
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + (16 if address == 0x7b3a10 else 4))

    hook = u.hook_add(UC_HOOK_CODE, callbacks)
    try:
        for bucket in range(64):
            n.invoke(u, 0x79a160, [0xcd9048 + bucket * 0x6c, window, bucket])
    finally:
        u.hook_del(hook)
    assert len(set(visited)) == len(visited)
    return visited


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    base = [(0, 8, 200.), (1, 8, 100.), (0, 0x10000, 100.), (0, 8, 101.),
            (1, 0x10008, 200.), (0, 8, 50.), (1, 8, 50.), (0, 0, 75.)]
    cases = [base, list(reversed(base))]
    for moving in (0, 1):
        for offset in (0, 2, len(base)):
            case = base.copy()
            case.insert(offset, (moving, 8, 3000.))
            cases.append(case)
    rng = random.Random(0x79a160)
    for _ in range(16):
        cases.append([(rng.randrange(2), rng.choice([0, 8, 0x10000, 0x10008]),
                       rng.choice([0., 25., 50., 51., 100., 101., 200., 800., 1900., 3000.]))
                      for _ in range(32)])
    rows = ['# native 792AD0/792BD0/79A160; fixed camera eye(-30,-1,1) +X, Z up, FOV1 aspect1.5 near.1 far5000; group bounds +/-16',
            '# scene count; entry moving MOGIflags X(hex f32); visits count ordered entry indices']
    for entries in cases:
        result = capture(entries)
        rows.append(f'scene {len(entries)}')
        rows.extend(f'entry {moving} {flags} {words([x])}' for moving, flags, x in entries)
        rows.append(f'visits {len(result)} ' + ' '.join(map(str, result)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(cases)} mixed-root native outdoor callback sequences')


if __name__ == '__main__':
    main()
