"""Capture 984930's full-camera envelope used before WMO scene insertion."""
import argparse
from pathlib import Path

import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from scene_depth_oracle import encoded


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    points, output = n.HEAP, n.HEAP + 0x1000
    rows = ['# full camera fixture index; unhooked 984930 world envelope min3 max3']
    frames = [floats(line) for line in args.frames.read_text().splitlines() if line and not line.startswith('#')]
    for index, frame in enumerate(frames):
        n.write_floats(u, points, frame[64:88])
        n.invoke(u, 0x984930, [output, points, 8])
        rows.append(f'{index} ' + encoded(n.read_floats(u, output, 6)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(frames)} original full-camera envelopes')


if __name__ == '__main__':
    main()
