"""Capture the original perspective scene camera pipeline without hooks.

6BFE60 builds the eye-relative positive-forward view; 6BF370 creates the native
perspective matrix; 6BF6D0, 984240 and 4C1F00 produce corners, planes and combined
projection. Scene-eye additions spill to float as in 795400. All inputs and
outputs are retained, including large world positions and rolled cameras.
"""
import argparse
import itertools
import math
import struct
from pathlib import Path

import wmo_registration_oracle as n
from world_model_portal_projection_oracle import capture_frustum, words


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def capture(u, eye, target, up, fov, aspect, near, far):
    origin, direction, up_ptr, view_ptr, projection_ptr = [n.HEAP + i * 0x1000 for i in range(5)]
    n.write_floats(u, origin, [0., 0., 0.])
    n.write_floats(u, direction, [f32(target[i] - eye[i]) for i in range(3)])
    n.write_floats(u, up_ptr, up)
    n.invoke(u, 0x6bfe60, [origin, direction, up_ptr, view_ptr])
    bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    n.invoke(u, 0x6bf370, [bits(v) for v in [fov, aspect, near, far]] + [projection_ptr])
    view, projection = n.read_floats(u, view_ptr, 16), n.read_floats(u, projection_ptr, 16)
    corners, clips, relative = capture_frustum(u, view, projection, eye)
    return view + projection + relative + corners + [v for plane in clips for v in plane]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    rows = ['# eye3 target3 up3 verticalFov aspect near far; native view16 projection16 relative16 worldCorners24 worldClipPlanes20; hex float stores']
    for eye, yaw, pitch, roll, aspect, near_far in itertools.product(
        [[0., 0., 0.], [500., -200., 70.], [15000., -14000., 2500.]],
        [0., .37, -1.13], [0., -.51, .83], [0., .29], [1., 16/9, 32/9], [[.2, 100.], [.2, 5000.]],
    ):
        forward = [math.cos(yaw) * math.cos(pitch), math.sin(yaw) * math.cos(pitch), math.sin(pitch)]
        side = [math.sin(yaw), -math.cos(yaw), 0.]
        base_up = [-math.cos(yaw) * math.sin(pitch), -math.sin(yaw) * math.sin(pitch), math.cos(pitch)]
        up = [f32(base_up[i] * math.cos(roll) + side[i] * math.sin(roll)) for i in range(3)]
        target = [f32(eye[i] + forward[i]) for i in range(3)]
        fov, aspect, near, far = map(f32, [.9424778, aspect, *near_far])
        inputs = eye + target + up + [fov, aspect, near, far]
        rows.append(words(inputs + capture(u, eye, target, up, fov, aspect, near, far)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original scene camera frames')


if __name__ == '__main__':
    main()
