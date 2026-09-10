"""Execute 606F90's ordinary subject fade fragment and original 8CA080 cosine.

Starts at 6077D0 with no vehicle subject and ends before subject dispatch at
6079FD. Primary constrained distance, principal height/pitch and near clip are
controlled inputs. Timed camera flags, reduced-range global and vehicles are
outside this fixture. Requires the fingerprinted, locally owned build-12340 PE.
"""
import argparse
import random
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_EIP
import wmo_registration_oracle as n
from camera_water_oracle import bits


def capture(uc, distance, height, pitch, near):
    camera, frame = n.HEAP, n.STACK + 0x18000
    uc.reg_write(UC_X86_REG_EBP, frame)
    uc.reg_write(UC_X86_REG_ESP, frame - 0x200)
    uc.reg_write(UC_X86_REG_ESI, camera)
    uc.reg_write(UC_X86_REG_EDI, 0)
    n.write_words(uc, frame - 0xc, 255)
    n.write_floats(uc, frame - 8, [distance])
    n.write_floats(uc, camera + 0x128, [height])
    n.write_floats(uc, camera + 0x120, [pitch])
    n.write_floats(uc, camera + 0x38, [near])
    uc.mem_write(0xbd19b8, b'\0')
    uc.emu_start(0x6077d0, 0x6079fd, count=1000)
    assert uc.reg_read(UC_X86_REG_EIP) == 0x6079fd
    return n.read_words(uc, frame - 0xc, 1)[0] & 255


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    uc = n.emulator()
    cases = []
    for pitch in [0., -.8, -1.1868238449, -1.2, -1.4, -1.55, -1.57079637]:
        for height in [0., .5, 1.5, 3.]:
            for distance in [0., .2, .20277777, .20277779, .3, .5, 1., 1.1, 1.5, 2., 2.0315, 2.0315003, 4.]:
                cases.append((distance, height, pitch, .2))
    rng = random.Random(606_790)
    for _ in range(1500):
        cases.append((rng.uniform(0, 5), rng.uniform(0, 5), rng.uniform(-1.57079637, 1.57), rng.choice([.2, .1, .5])))
    lines = ['# 606F90 ordinary fade: distance height pitch near (f32 bits), opacity byte']
    for case in cases:
        lines.append(' '.join(f'{word:08x}' for word in [*(bits(v) for v in case), capture(uc, *case)]))
    args.output.write_text('\n'.join(lines) + '\n')
    print(f'Captured {len(cases)} native camera opacity cases')
    for address in [0xa948b0, 0xa1e308, 0xa1e344, 0xa1ea7c, 0x9f1ff4, 0xa1ea78, 0x9ea27c]:
        print(f'{address:08x}: {n.read_words(uc, address, 1)[0]:08x} {n.read_floats(uc, address, 1)[0]}')


if __name__ == '__main__':
    main()
