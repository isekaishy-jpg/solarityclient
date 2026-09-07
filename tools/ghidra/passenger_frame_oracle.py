"""Capture original passenger matrices, faces, interval bounds and sweep caches.

The native matrix/point functions and 75FF90's final face conversion loop run
unchanged. Interval/cache captures stop before geometry collection and replace
only the resident parent-GUID provider. Parent facing is a controlled scalar.
No game process, OS entry point, or live archive stack is launched.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
import movement_interval_bounds_oracle as interval
import movement_sweep_cache_oracle as cache
from movement_path_oracle import invoke
from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_ESP,
    UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)


def hex_floats(values):
    return ' '.join(f'{word:08x}' for word in interval.words(values))


def hex_words(values):
    return ' '.join(f'{word:08x}' for word in values)


def transform_point(uc, matrix, value):
    point, scratch, source = native.HEAP + 0x6000, native.HEAP + 0x6100, native.HEAP + 0x6200
    native.write_floats(uc, point, value)
    native.write_floats(uc, source, matrix)
    invoke(uc, 0x4c2300, [scratch, point, source])
    return native.read_floats(uc, point, 3)


def frames(uc, poses):
    matrices = [list(struct.unpack('<16f', struct.pack('<16I', *[int(word, 16) for word in line.split()[7:]])))
                for line in Path(poses).read_text().splitlines() if line and not line.startswith('#')]
    selected = matrices[:16] + matrices[32:144]
    # Non-unit bases distinguish the native transpose from a general inverse.
    for matrix in matrices[32:40]:
        selected.append([v * 2 if i < 12 else v for i, v in enumerate(matrix)])
    random_source = random.Random(12340)
    lines = ['# Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# matrix16 facing angle point3 normal3 vertices9 | reverse16 worldpoint3 localpoint3 worldangle localangle localnormal3 localvertices9']
    for matrix in selected:
        facing, angle = [random_source.uniform(-8, 8) for _ in range(2)]
        point = [random_source.uniform(-30, 30) for _ in range(3)]
        normal = [random_source.uniform(-2, 2) for _ in range(3)]
        vertices = [matrix[12 + i % 3] + random_source.uniform(-10, 10) for i in range(9)]
        frame_base, faces = native.STACK + 0x17000, native.HEAP + 0x7000
        native.write_floats(uc, frame_base - 0x88, matrix)
        native.write_floats(uc, faces, [*normal, 0, *vertices])
        native.write_words(uc, 0xadba38, 1, faces)
        uc.reg_write(UC_X86_REG_EBP, frame_base)
        uc.reg_write(UC_X86_REG_ESP, frame_base - 0x300)
        uc.reg_write(UC_X86_REG_FPSW, 0)
        uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
        uc.emu_start(0x7605ec, 0x760706, count=100_000)
        reverse = native.read_floats(uc, frame_base - 0xe0, 16)
        converted = native.read_words(uc, faces, 3) + native.read_words(uc, faces + 16, 9)
        world_point = transform_point(uc, matrix, point)
        local_point = transform_point(uc, reverse, point)
        component, parent_angle, result = native.HEAP + 0x7200, native.HEAP + 0x7300, native.HEAP + 0x7400
        native.write_words(uc, component + 8, 2, 0)
        uc.mem_write(0x74b590, b'\xd9\x05' + struct.pack('<I', parent_angle) + b'\xc3')
        uc.mem_write(native.STOP + 16, b'\xd9\x1d' + struct.pack('<I', result))
        angles = []
        for value in [facing, -facing]:
            native.write_floats(uc, parent_angle, [value])
            uc.reg_write(UC_X86_REG_ECX, component)
            invoke(uc, 0x4f42a0, interval.words([angle]))
            uc.emu_start(native.STOP + 16, native.STOP + 22, count=1)
            angles.extend(native.read_words(uc, result, 1))
        lines.append(hex_floats([*matrix, facing, angle, *point, *normal, *vertices]) + ' ' +
                     hex_floats([*reverse, *world_point, *local_point]) + ' ' + hex_words([*angles, *converted]))
    return selected, lines


def capture(executable, poses, output_directory):
    native.initialize(executable)
    uc = native.emulator()
    matrices, lines = frames(uc, poses)
    directory = Path(output_directory)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / 'passenger-frame-native.txt').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    intervals = [lines[0], '# mode player slow duration fall_ms matrix16 inputs12 body6 query6']
    sweeps = [lines[0], '# matrix16 origin3 radius height direction3 distance cached6 miss output6']
    for index, matrix in enumerate(matrices[:32]):
        for mode in range(3):
            for controlled in (0, 1):
                values = [2., -3., 4., .5, 2., 1.25, .6, .8, .2, 1., 5., -7.95]
                result = interval.capture(uc, mode, controlled, index % 2, 16, 71, values, matrix)
                intervals.append(f'{mode} {controlled} {index % 2} 16 71 ' + hex_floats([*matrix, *values]) + ' ' + hex_words(result))
        origin = [2., -3., 4.]
        world = transform_point(uc, matrix, origin)
        for distance in (0., 2**-21, .001, .5, 10.):
            for padding in (0., 2., 40.):
                low = [world[i] - [.5, .5, 0.][i] - padding for i in range(3)]
                high = [world[i] + [.5, .5, 2.][i] + padding for i in range(3)]
                values = [*origin, .5, 2., .6, .8, .2, distance, *low, *high]
                miss, query = cache.capture(uc, values, matrix)
                sweeps.append(hex_floats([*matrix, *values]) + f' {miss} ' + hex_words(query))
    (directory / 'passenger-interval-native.txt').write_text('\n'.join(intervals) + '\n', encoding='utf-8')
    (directory / 'passenger-cache-native.txt').write_text('\n'.join(sweeps) + '\n', encoding='utf-8')
    print(f'captured {len(matrices)} frames, {len(intervals)-2} intervals, {len(sweeps)-2} cache decisions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('poses')
    parser.add_argument('output_directory')
    args = parser.parse_args()
    capture(args.executable, args.poses, args.output_directory)
