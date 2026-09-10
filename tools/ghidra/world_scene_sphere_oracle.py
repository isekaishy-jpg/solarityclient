"""Capture original 983FB0 sphere admission at native camera clipping faces.

984240 constructs the native planes from captured camera corners. Sphere
classification executes unhooked, including equality at each clipping face.
"""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX
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
    corners_ptr, frustum_ptr, sphere_ptr = [n.HEAP + i * 0x1000 for i in range(3)]
    rows = ['# camera fixture index; center3 radius hex floats; original 983FB0 result']
    frames = [floats(line) for line in args.frames.read_text().splitlines() if not line.startswith('#')]
    for index in range(0, len(frames), 27):
        corners = [frames[index][i:i+3] for i in range(64, 88, 3)]
        n.write_floats(u, corners_ptr, frames[index][64:88])
        u.reg_write(UC_X86_REG_ECX, frustum_ptr)
        n.invoke(u, 0x984240, [corners_ptr])
        planes = n.read_floats(u, frustum_ptr, 24)
        for face, vertices in enumerate([[1, 5, 6, 2], [0, 3, 7, 4], [0, 4, 5, 1],
                                          [3, 2, 6, 7], [4, 7, 6, 5], [0, 1, 2, 3]]):
            plane = planes[face*4:face*4+4]
            midpoint = [sum(corners[v][axis] for v in vertices) / 4 for axis in range(3)]
            distance = sum(midpoint[axis] * plane[axis] for axis in range(3)) + plane[3]
            length_squared = sum(value*value for value in plane[:3])
            for radius in [0., .1, 5.]:
                for shift in [-.001, 0., .001]:
                    center = [midpoint[axis] + plane[axis] * (-radius + shift - distance) / length_squared
                              for axis in range(3)]
                    n.write_floats(u, sphere_ptr, center)
                    u.reg_write(UC_X86_REG_ECX, frustum_ptr)
                    n.invoke(u, 0x983fb0, [sphere_ptr, struct.unpack('<I', struct.pack('<f', radius))[0]])
                    result = u.reg_read(UC_X86_REG_EAX)
                    assert result in [0, 3]
                    rows.append(f'{index} ' + words(center + [radius]) + f' {result}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original scene sphere classifications')


if __name__ == '__main__':
    main()
