"""Native 7D77C0 exterior-portal traversal, including four-level cutoff."""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from world_model_fog_oracle import bits


def capture():
    rows = ['# 7D77C0: scene id count MOGP[count] MOGI[count] edge_count edge_pairs; point primary secondary XYZhex distancehex/-']
    chain = [(i, i + 1) for i in range(5)]
    scenarios = [
        ([0] * 6, [0, 0, 0, 0, 0, 8], chain),
        ([0] * 6, [0, 0, 0, 0, 0x40, 8], chain),
        ([0x40, 0, 0, 0, 0, 8], [0, 0, 0, 0, 0, 8], chain),
        ([0] * 6, [0, 0, 0, 0, 0, 8], [(0, 1), (1, 2), (2, 0), (2, 5)]),
        ([0] * 6, [0, 0, 0, 0, 0, 0], chain),
        ([0] * 6, [0, 0, 8, 0, 0, 0x40], [(0, 1), (1, 2), (0, 5)]),
    ]
    for scene, (mogp, mogi, edges) in enumerate(scenarios):
        u = n.emulator()
        root, info, vertices, portals, refs, groups, point, result = [n.HEAP + i * 0x2000 for i in range(8)]
        n.write_words(u, root + 0x130, info, vertices, portals, refs)
        n.write_words(u, root + 0x1f8, *(groups + i * 0x200 for i in range(6)))
        for index, _ in enumerate(edges):
            z = float(index * 3)
            n.write_floats(u, vertices + index * 48, [-2., -2., z, 2., -2., z, 2., 2., z, -2., 2., z])
            u.mem_write(portals + index * 20, struct.pack('<HH4f', index * 4, 4, 0., 0., 1., -z))
        reference = 0
        for group in range(6):
            n.write_words(u, info + group * 32, mogi[group])
            adjacent = [(index, b if a == group else a) for index, (a, b) in enumerate(edges) if group in (a, b)]
            n.write_words(u, groups + group * 0x200 + 0x50, reference, len(adjacent))
            for portal, neighbor in adjacent:
                u.mem_write(refs + reference * 8, struct.pack('<HHhH', portal, neighbor, 1, 0))
                reference += 1
        rows.append('scene ' + ' '.join(map(str, [scene, 6, *mogp, *mogi, len(edges), *sum(([a,b] for a,b in edges), [])])))
        for primary, secondary in [(0, -1), (0, 1), (1, -1), (2, -1), (5, -1)]:
            for x, z in [(0., -20.), (0., -16.), (0., 0.), (0., 7.5), (0., 25.), (0., 37.), (3., 0.), (20., 0.), (30., 0.)]:
                n.write_floats(u, point, [x, 0., z])
                n.write_words(u, result, 0x7f7fffff)
                enabled = False
                for group in (primary, secondary):
                    if group >= 0 and mogp[group] & 0x48 == 0:
                        enabled = True
                        u.reg_write(UC_X86_REG_ECX, root)
                        n.invoke(u, 0x7d77c0, [0, 0x48, groups + group * 0x200, 0, point, result])
                output = f'{n.read_words(u, result, 1)[0]:08x}' if enabled else '-'
                rows.append(f'point {primary} {secondary} {bits(x):08x} 00000000 {bits(z):08x} {output}')
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
