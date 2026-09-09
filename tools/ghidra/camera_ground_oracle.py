"""Capture native primary camera constraints over a stationary ground plane.

605D60, 601D60, 6059E0, and native triangle clipping execute unchanged.
The controlled scene supplies a horizontal plane for center/anchor rays and
one ground triangle for volume collection. Camera orientation and scalar
banks are supplied independently, as in the original camera owner.
"""

import argparse
import itertools
import math
from pathlib import Path

from camera_primary_oracle import f32, primary
from camera_water_oracle import bits
import wmo_registration_oracle as native


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    rows = ['# native 605D60/601D60/6059E0; pinned build-12340 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# subject3 yaw pitch distance height | distance height vertical-fraction eye3 contact-flags; f32 hex words; fixed ground, aspect 16/9, water off']
    cases = itertools.product(
        [[0., 0., 0.], [1340., -4380., 28.]],
        [-1.553343, -1.4, -.6],
        [.199999, .2, .200001, 1.6391548, 1.7502688, 1.7502698, 1.8, 2.95, 3.2],
    )
    for subject, pitch, distance in cases:
        yaw, pitch, distance = map(f32, [.7, pitch, distance])
        cy, sy, cp, sp = map(f32, [math.cos(yaw), math.sin(yaw), math.cos(pitch), math.sin(pitch)])
        forward = [f32(cy*cp), f32(sy*cp), -sp]
        up = [f32(cy*sp), f32(sy*sp), cp]
        result, _ = primary(subject, forward, up, distance, 1.75, 0., 0, 0, 0., -1., -1., -1., -1., ground_plane=True)
        inputs = list(map(bits, subject + [yaw, pitch, distance, 1.75]))
        rows.append(' | '.join(' '.join(f'{word:08x}' for word in group) for group in [inputs, result]))
    args.output.write_text('\n'.join(rows)+'\n', encoding='utf-8')
    print(f'wrote {len(rows)-2} native stationary-ground probes')


if __name__ == '__main__':
    main()
