"""Capture original remote interpolation setup and sampling without math hooks."""
import argparse
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from movement_path_oracle import invoke
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    owner, event, sample = [native.HEAP + i * 0x4000 for i in range(3)]
    rows = ['# original Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# flags duration elapsed interval | current xyz yaw pitch | endpoint xyz yaw pitch | analytic xyz yaw pitch | output xyz yaw pitch blend-flags']
    for address in [0x9ec218, 0x9f1224, 0x9f193c, 0x9e8d34, 0x9f1ff0]:
        print(hex(address), bytes(uc.mem_read(address, 8)).hex())
    for flags in [0, 1, 0x1000, 0x200001]:
        for duration in [50, 333, 1000]:
            for current in [[0., 0., 0., 0., 0.], [10., -5., 3., 6.2, -0.2]]:
                for endpoint in [[1., 2., 3., 0.1, 0.4], [100., 50., -3., 0., 0.]]:
                    for elapsed in [0, duration // 3, duration - 1, duration, duration + 1]:
                        for interval in [1, 16, 250]:
                            uc.mem_write(owner, bytes(0x200))
                            uc.mem_write(event, bytes(0x58))
                            native.write_words(uc, owner + 0x44, flags)
                            native.write_words(uc, owner + 0x140, event)
                            native.write_floats(uc, owner + 0x10, current[:3])
                            native.write_floats(uc, owner + 0x20, current[3:])
                            native.write_words(uc, event + 8, duration, 43)
                            native.write_floats(uc, event + 0x10, endpoint)
                            uc.mem_write(event + 0x50, b'\x01')
                            uc.reg_write(UC_X86_REG_ECX, owner)
                            invoke(uc, 0x6ea6a0, [0])
                            analytic = [current[0] + elapsed * .007, current[1], current[2], current[3] + elapsed * .001, current[4]]
                            analytic = [native_float(value) for value in analytic]
                            native.write_floats(uc, sample, analytic)
                            uc.reg_write(UC_X86_REG_ECX, owner)
                            invoke(uc, 0x6ea7e0, [elapsed, interval, sample, sample + 12, sample + 16])
                            result = native.read_words(uc, sample, 5)
                            result += (native.read_words(uc, owner + 0x48, 1)[0],)
                            columns = [[flags, duration, elapsed, interval], [bits(v) for v in current], [bits(v) for v in endpoint], [bits(v) for v in analytic], result]
                            rows.append(' | '.join(' '.join(f'{value:08x}' for value in column) for column in columns))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} original remote blend samples')


def native_float(value):
    import struct
    return struct.unpack('<f', struct.pack('<f', value))[0]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
