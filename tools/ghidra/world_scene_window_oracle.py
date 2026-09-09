"""Capture original outdoor/group screen-window frustum cropping.

790AF0 consumes the global camera corners; 790E20 accepts them by pointer.
Both complete routines and 984240 plane construction execute without hooks.
The returned stack-local corner arrays remain readable after each native call.
"""
import argparse
import random
from pathlib import Path

import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from world_model_portal_projection_oracle import words


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    window_ptr = n.HEAP
    n.write_words(u, 0xcd8798, 0)
    frames = [floats(line) for line in args.frames.read_text().splitlines() if not line.startswith('#')]
    windows = [[0., 0., 1., 1.], [.2, .3, .7, .8], [0., 0., .5, .5], [.5, .5, 1., 1.],
               [.4999, .4999, .5001, .5001], [0., .1, 1., .9], [.1, 0., .9, 1.],
               [-.125, -.25, 1.25, 1.5], [-2., -.1, .3, 2.]]
    rng = random.Random(79020)
    for _ in range(9):
        x = sorted([rng.random(), rng.random()])
        y = sorted([rng.random(), rng.random()])
        windows.append([x[0], y[0], x[1], y[1]])
    rows = ['# camera fixture index; normalized window4; native cropped world corners24 and planes24; hex float stores; 790AF0 and 790E20 verified equal']
    for index in range(0, len(frames), 9):
        n.write_floats(u, 0xcdb108, frames[index][64:88])
        for window in windows:
            n.write_floats(u, window_ptr, window)
            results = []
            for address, arguments, local_offset in [(0x790af0, [window_ptr], 0xc4), (0x790e20, [0xcdb108, window_ptr], 0xec)]:
                n.invoke(u, address, arguments)
                corners = n.read_floats(u, n.STACK + 0x18000 - 4 - local_offset, 24)
                planes = n.read_floats(u, 0xcdb168, 24)
                results.append(words(corners + planes))
            assert results[0] == results[1], (index, window)
            rows.append(f'{index} ' + words(window) + ' ' + results[0])
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} paired original cropped frusta')


if __name__ == '__main__':
    main()
