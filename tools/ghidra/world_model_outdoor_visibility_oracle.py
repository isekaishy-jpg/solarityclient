"""Capture 7AC060's outdoor-root traversal from bounded screen windows.

The original recursion runs with 7AD350's camera-root bank disabled and fog
false. Group lookup and projected portal windows are supplied; graphics-only
occluder work is disabled. Every captured callback retains original window
stores, order, depth, fog and MOGI/group flag distinctions.
"""
import argparse
import itertools
from pathlib import Path

import wmo_registration_oracle as n
from world_model_scene_visibility_oracle import float_words
from world_model_visibility_oracle import capture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# outdoor 7AC060; scene groupFlags infoFlags edges; query start maximum point3 window4 visitCount']
    count = 0
    pairs = [(0, 0), (8, 8), (0x40, 0x40), (0x100, 0x100),
             (0x10000, 0), (0, 0x10000), (0x40000, 0x40000)]
    windows = [[-1., -1., 1., 1.], [-1., -1., 0., 0.], [0., 0., 1., 1.],
               [.1, -.1, .11, .1], [-2., -.7, .4, 2.]]
    for middle, info_middle in pairs:
        flags, info = [8, middle, 0x40, 0x10000], [8, info_middle, 0x40, 0x10000]
        edges = [(0, 1, [0., 0., 1., 0.], [-.5, -.5, .5, .5]),
                 (1, 2, [0., 0., 1., 0.], [.1, -.75, .75, .75]),
                 (2, 0, [0., 0., 1., 0.], [-.9, -.9, .9, .9]),
                 (0, 3, [1., .5, -.25, .125], [.1, -.25, .9, .75])]
        rows.append('scene ' + ' '.join(map(str, [len(flags), *flags, *info, len(edges)])))
        for a, b, plane, window in edges:
            rows.append(f'edge {a} {b} ' + float_words(plane + window))
        for start, z, maximum, window in itertools.product([0, 1], [-1., 0., 1.], [0, 10], windows):
            point = [.25, -.125, z]
            visits = capture(flags, edges, start, point, maximum, 0, info,
                             camera_root=False, initial_window=window)
            rows.append(f'query {start} {maximum} ' + float_words(point + window) + f' {len(visits)}')
            for group, fog, depth, *rectangle in visits:
                rows.append(f'visit {group} {fog} {depth} ' + ' '.join(f'{word:08x}' for word in rectangle))
            count += 1
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {count} original outdoor-root queries')


if __name__ == '__main__':
    main()
