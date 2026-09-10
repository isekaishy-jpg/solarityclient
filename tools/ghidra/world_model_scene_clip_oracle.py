"""Capture original group callback frusta, including inherited initial clips.

7AC060, 791950, 78FB50, 790E20, and plane construction execute original code.
Resident groups and projected portal rectangles use the existing traversal
oracle's controlled boundaries. Initial clips model camera-root and outdoor
entry calls; this is not a capture of the external root-registration pass.
"""
import argparse
import itertools
import struct
from pathlib import Path

import wmo_registration_oracle as n
from world_model_visibility_oracle import capture
from world_model_portal_projection_oracle import words
from world_scene_bounds_oracle import floats


def f32(value):
    """Preserve the original single-precision input store."""
    return struct.unpack('<f', struct.pack('<f', value))[0]


def main():
    """Capture callback order, fog, rectangle and complete current frustum."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    frames = [floats(line) for line in args.frames.read_text().splitlines() if not line.startswith('#')]
    edges = [(0, 1, [0., 0., 1., 0.], [-.5012345, -.3123456, .7182345, .5123456]),
             (1, 2, [0., 0., 1., 0.], [-.7123456, -.2345678, .4234567, .7234567]),
             (2, 0, [0., 0., 1., 0.], [-.9, -.9, .9, .9])]
    rows = ['# camera index; outdoor(0/1); inherited normalized window4 (hex f32); group fog depth (decimal); clip window4 corners24 planes24 (hex f32)']
    windows = [None, [0., 0., 1., 1.], [.1234567, .2345678, .8123456, .9123456]]
    for index, start, window in itertools.product(range(0, len(frames), 54), [0, 1], windows):
        normalized = [0., 0., 1., 1.] if window is None else list(map(f32, window))
        clip = None if window is None else [f32(v * 2. - 1.) for v in normalized]
        visits = capture([0, 0, 0], edges, start, [0., 0., -1.], 4, int(window is None),
                         camera_root=window is None, initial_window=clip,
                         scene_corners=frames[index][64:88], scene_window=window)
        for group, fog, depth, *values in visits:
            rows.append(f'{index} {int(window is not None)} ' + words(normalized) +
                        f' {group} {fog} {depth} ' + ' '.join(f'{v:08x}' for v in values))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original group callback frusta')


if __name__ == '__main__':
    main()
