"""Capture 9839E0's six-plane scene AABB test against original camera corners.

The source frames come from world_scene_projection_oracle.py. Plane creation
984240 and box classification execute unhooked in the pinned client. Cases
cross all clipping faces, the stored native tolerance, and large coordinates.
"""
import argparse
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX
import wmo_registration_oracle as n
from world_model_portal_projection_oracle import words


def floats(line):
    return [struct.unpack('<f', struct.pack('<I', int(word, 16)))[0]
            for word in line.split()]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    corners_ptr, frustum_ptr, bounds_ptr = [n.HEAP + i * 0x1000 for i in range(3)]
    rows = ['# camera fixture index; world AABB min3 max3 hex float stores; native 9839E0 result (0 or 3)']
    frames = [floats(line) for line in args.frames.read_text().splitlines() if not line.startswith('#')]
    tolerance = n.read_floats(u, 0xaa2e74, 1)[0]
    for index in range(0, len(frames), 27):
        frame = frames[index]
        corners = [frame[i:i+3] for i in range(64, 88, 3)]
        n.write_floats(u, corners_ptr, frame[64:88])
        u.reg_write(UC_X86_REG_ECX, frustum_ptr)
        n.invoke(u, 0x984240, [corners_ptr])
        planes = n.read_floats(u, frustum_ptr, 24)
        points = corners + [frame[:3], frame[3:6]]
        # Move each face midpoint across the precise rejection tolerance.
        for face, vertex_indices in enumerate([[1, 5, 6, 2], [0, 3, 7, 4], [0, 4, 5, 1],
                                                [3, 2, 6, 7], [4, 7, 6, 5], [0, 1, 2, 3]]):
            plane = planes[face*4:face*4+4]
            midpoint = [sum(corners[v][axis] for v in vertex_indices) / 4 for axis in range(3)]
            distance = sum(midpoint[axis] * plane[axis] for axis in range(3)) + plane[3]
            length_squared = sum(value*value for value in plane[:3])
            for offset in [-.001, 0., .001]:
                points.append([midpoint[axis] + plane[axis] * (tolerance + offset - distance) / length_squared
                               for axis in range(3)])
        for point in points:
            for radius in [0., .1]:
                bounds = [value-radius for value in point] + [value+radius for value in point]
                n.write_floats(u, bounds_ptr, bounds)
                u.reg_write(UC_X86_REG_ECX, frustum_ptr)
                n.invoke(u, 0x9839e0, [bounds_ptr])
                result = u.reg_read(UC_X86_REG_EAX)
                assert result in [0, 3]
                rows.append(f'{index} ' + words(bounds) + f' {result}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original scene bounds classifications')


if __name__ == '__main__':
    main()
