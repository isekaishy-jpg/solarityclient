"""Execute the pinned 98B850 analytic rebase with no hooks or replacements.

Inputs use the existing native passenger-frame matrices and reverse matrices.
No spline is attached. The original anchor, yaw, retained-height, full-direction,
and horizontal launch-direction conversions all run unchanged.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_path_oracle import invoke
from unicorn.x86_const import UC_X86_REG_ECX


def floats(words):
    return list(struct.unpack('<' + 'f' * len(words), struct.pack('<' + 'I' * len(words), *words)))


def words(values):
    return list(struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values)))


def trajectory_cases(uc, frames, output):
    """Keep elapsed analytic time through 98B850, then run the original 987B50."""
    owner, matrix_ptr, point_ptr, result = [native.HEAP + offset for offset in (0, 0x1000, 0x1100, 0x1200)]
    speeds = [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, 3.1415927410125732, 3.1415927410125732]
    lines = ['# entry flags secondary elapsed parent16 facing initialyaw | basis3 horizontal2 speed displacement3 yaw']
    for index, frame in enumerate(frames):
        if index >= 32:
            break
        parent, facing, reverse = frame[:16], frame[16], frame[33:49]
        for entry in (0, 1):
            for flags in (1, 2, 4, 5, 0x11, 0x25, 0x16, 0x128):
                for elapsed in (0, 16, 251, 10001):
                    yaw, secondary = .7, 8 if index % 2 else 0
                    uc.mem_write(owner, bytes(0x400))
                    native.write_words(uc, owner + 0x44, flags, secondary)
                    native.write_floats(uc, owner + 0x58, [yaw, 0.])
                    native.write_floats(uc, owner + 0x90, speeds)
                    uc.reg_write(UC_X86_REG_ECX, owner)
                    invoke(uc, 0x9880c0, [0])
                    uc.reg_write(UC_X86_REG_ECX, owner)
                    invoke(uc, 0x987ef0, [0])
                    native.write_floats(uc, matrix_ptr, reverse if entry else parent)
                    native.write_floats(uc, point_ptr, [1., 2., 3.])
                    uc.reg_write(UC_X86_REG_ECX, owner)
                    invoke(uc, 0x98b850, [matrix_ptr, words([-facing if entry else facing])[0], point_ptr])
                    native.write_floats(uc, result, [0., 0., 0., *native.read_floats(uc, owner + 0x58, 1), 0.])
                    uc.reg_write(UC_X86_REG_ECX, owner)
                    invoke(uc, 0x987b50, [elapsed, result, result + 12, result + 16])
                    values = [*words([*parent, facing, yaw]), *native.read_words(uc, owner + 0x64, 5),
                              *native.read_words(uc, owner + 0x8c, 1), *native.read_words(uc, result, 4)]
                    lines.append(f'{entry} {flags:x} {secondary:x} {elapsed} ' + ' '.join(f'{word:08x}' for word in values))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'captured {len(lines)-1} native rebased trajectory samples')


def capture(executable, frames, output):
    native.initialize(executable)
    uc = native.emulator()
    owner, matrix_ptr, point_ptr = native.HEAP, native.HEAP + 0x1000, native.HEAP + 0x1100
    random_source = random.Random(12340)
    lines = ['# Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# entry parent16 facing current3 anchor3 yaw direction3 launch step | anchor3 yaw direction3 horizontal2 launch step']
    for line in Path(frames).read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        frame = floats([int(word, 16) for word in line.split()])
        parent, facing, reverse = frame[:16], frame[16], frame[33:49]
        for entry in (0, 1):
            for direction in ([.6, -.8, .2], [0., 0., 1.], [2**-11, 0., 0.], [2**-12, 2**-12, 0.]):
                point = [random_source.uniform(-30, 30) for _ in range(3)]
                anchor = [random_source.uniform(-30, 30) for _ in range(3)]
                yaw, launch, step = [random_source.uniform(-10, 10) for _ in range(3)]
                native.write_floats(uc, owner + 0x4c, [*anchor, yaw])
                native.write_floats(uc, owner + 0x64, direction)
                native.write_floats(uc, owner + 0x84, [launch, step])
                native.write_words(uc, owner + 0xbc, 0)
                native.write_floats(uc, matrix_ptr, reverse if entry else parent)
                native.write_floats(uc, point_ptr, point)
                uc.reg_write(UC_X86_REG_ECX, owner)
                invoke(uc, 0x98b850, [matrix_ptr, words([-facing if entry else facing])[0], point_ptr])
                result = [*native.read_words(uc, owner + 0x4c, 4),
                          *native.read_words(uc, owner + 0x64, 5),
                          *native.read_words(uc, owner + 0x84, 2)]
                inputs = words([*parent, facing, *point, *anchor, yaw, *direction, launch, step])
                lines.append(str(entry) + ' ' + ' '.join(f'{word:08x}' for word in inputs + result))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'captured {len(lines) - 2} native analytic frame changes')
    frames = [floats([int(word, 16) for word in line.split()])
              for line in Path(frames).read_text().splitlines() if line and not line.startswith('#')]
    trajectory_cases(uc, frames, Path(output).with_name('passenger-trajectory-native.txt'))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.frames, args.output)
