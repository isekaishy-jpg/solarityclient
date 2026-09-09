"""Capture ordered native camera-root group and first exterior-portal events.

7AC060 runs over controlled resident groups and projected portal rectangles.
The 7A8F20 boundary consumes its once-per-root portal bit; its displaced window
arithmetic is independently executed by world_model_exterior_portal_oracle.py.
Initial groups share that cache exactly as the 7AD1F0 loop does.
"""
import argparse
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as n
from world_model_visibility_oracle import capture


def float_words(values):
    return ' '.join(f'{word:08x}' for word in struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# scene groupFlags infoFlags edges; query maximum initialCount initialGroups point3 eventCount; ordered group/portal events']
    count = 0
    pairs = [(v, v) for v in [0, 8, 0x40, 0x100, 0x40000, 0x10000, 0x1]]
    pairs += [(0, 0x10000), (0x10000, 0), (8, 0x40)]
    scenes = []
    for shape, (middle, info_middle) in itertools.product(range(2), pairs):
        flags, info = [0, middle, 0x40, 8], [0, info_middle, 0x40, 8]
        bound = [-.5, -.5, .5, .5] if shape == 0 else [.5005, -.5, .501, .5]
        edges = [(0, 1, [0., 0., 1., 0.], [-.5, -.5, .5, .5]),
                 (1, 2, [0., 0., 1., 0.], bound),
                 (2, 0, [0., 0., 1., 0.], [-.9, -.9, .9, .9]),
                 (0, 3, [1., .5, -.25, .125], [.1, -.25, .9, .75])]
        scenes.append((flags, info, edges))
    scenes.append(([0, 0x40, 8, 0x10000], [0, 0x40, 8, 0x10000], []))
    for flags, info, edges in scenes:
        rows.append('scene ' + ' '.join(map(str, [len(flags), *flags, *info, len(edges)])))
        for a, b, plane, window in edges:
            rows.append(f'edge {a} {b} ' + float_words(plane + window))
        for starts, z, maximum in itertools.product([[0], [0, 1], [1, 0], [0, 0]], [-1., 0., 1.], [0, 1, 10]):
            point = [.25, -.125, z]
            events = capture(flags, edges, starts[0], point, maximum, 1, info, scene_events=True, starts=starts)
            rows.append(f'query {maximum} {len(starts)} ' + ' '.join(map(str, starts)) + ' ' + float_words(point) + f' {len(events)}')
            for event in events:
                if event[0] == 'portal':
                    rows.append(f'portal {event[1]}')
                else:
                    _, group, fog, depth, *window = event
                    rows.append(f'group {group} {fog} {depth} ' + ' '.join(f'{word:08x}' for word in window))
            count += 1
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {count} original camera-root scene queries')


if __name__ == '__main__':
    main()
