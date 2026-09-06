"""Capture original 987B50/987770 yaw across ground and airborne modes."""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn.x86_const import UC_X86_REG_ECX


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, result = native.HEAP, native.HEAP + 0x1000
    rows = ['# flags secondary facing turnSpeed elapsed -> facing: original 987B50']
    for mode in [0, 1, 0x1000, 0x1001, 0x400000, 0x800000]:
        for turn in [0, 0x10, 0x20, 0x30]:
            for secondary in [0, 8]:
                for facing in [0., .7, -1.2, 6.28]:
                    for elapsed in [0, 1, 17, 250, 1000, 10001, 0x1000001, 0xffffffff]:
                        flags = mode | turn
                        uc.mem_write(unit, bytes(0x400))
                        native.write_words(uc, unit + 0x44, flags, secondary)
                        native.write_floats(uc, unit + 0x58, [facing, 0.])
                        native.write_floats(uc, unit + 0xac, [3.1415927410125732])
                        native.write_floats(uc, result, [0., 0., 0., facing, 0.])
                        uc.reg_write(UC_X86_REG_ECX, unit)
                        invoke(uc, 0x987b50, [elapsed, result, result + 12, result + 16])
                        after = native.read_words(uc, result + 12, 1)[0]
                        rows.append(f'{flags:x} {secondary:x} {bits(facing):08x} 40490fdb {elapsed} {after:08x}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original yaw samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
