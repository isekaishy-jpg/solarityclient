"""Capture original CM2Scene root/child callback traversal from supplied links.

81C9C0 and 832450 execute from the pinned image. Context housekeeping is a
provider boundary, and 832260 records entry and returns success; the separate
model-bone captures execute that inner scanner. All five models are loaded and
their attachment channels enabled. These records establish depth-first update
order for supplied lists, not the producer of registration/attachment order.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(output):
    u = n.emulator()
    scene = n.HEAP
    models = [n.HEAP + 0x1000 * (i + 1) for i in range(5)]
    indices = {address: index for index, address in enumerate(models)}
    visited = []

    def hook(uc, address, size, context):
        if address not in (0x81c790, 0x81c290, 0x832260):
            return
        if address == 0x832260:
            visited.append(indices[uc.reg_read(UC_X86_REG_ECX)])
            assert n.read_words(uc, scene + 0x1c, 1)[0] & 4
            uc.reg_write(UC_X86_REG_EAX, 1)
        sp = uc.reg_read(UC_X86_REG_ESP)
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + (8 if address == 0x81c290 else 4))

    u.hook_add(UC_HOOK_CODE, hook)
    lines = [
        '# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
        '# parents[5] supplied_list_order[5] | model_update_order[5] (decimal; -1=root)',
    ]
    records = 0
    for parents, order in itertools.product([
        [-1, -1, 0, 0, 2], [-1, 0, -1, 2, 1],
        [-1, -1, -1, 0, 1], [-1, 0, 0, 0, 2],
    ], itertools.permutations(range(5))):
        u.mem_write(scene, bytes(0x6000))
        n.write_words(u, scene + 0xc, 100)
        for index, model in enumerate(models):
            n.write_words(u, model, 2)
            n.write_words(u, model + 0x10, 1)
            n.write_words(u, model + 0x28, scene)
            n.write_words(u, model + 0x4c, model + 0x400)
            n.write_words(u, model + 0x54, 0)
            n.write_words(u, model + 0x408, 1)
            children = [child for child in order if parents[child] == index]
            if children:
                n.write_words(u, model + 0x58, models[children[0]])
                for child, sibling in zip(children, children[1:]):
                    n.write_words(u, models[child] + 0x60, models[sibling])
        roots = [index for index in order if parents[index] == -1]
        n.write_words(u, scene + 0x28, models[roots[0]])
        for root, sibling in zip(roots, roots[1:]):
            n.write_words(u, models[root] + 0x44, models[sibling])
        visited.clear()
        sp = n.STACK + 0x18000
        n.write_words(u, sp, n.STOP, 21)
        u.reg_write(UC_X86_REG_ESP, sp)
        u.reg_write(UC_X86_REG_ECX, scene)
        u.emu_start(0x81c9c0, n.STOP, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == n.STOP
        assert n.read_words(u, scene + 0xc, 2) == (121, 21)
        assert n.read_words(u, scene + 0x1c, 1)[0] & 4 == 0
        assert sorted(visited) == list(range(5)), (parents, order, visited)
        lines.append(' '.join(map(str, [*parents, *order])) + ' | ' + ' '.join(map(str, visited)))
        records += 1
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return dict(records=records, sha256=hashlib.sha256(output.read_bytes()).hexdigest())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
