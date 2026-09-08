"""Execute original camera height feedback and recovery, without scene hooks.

6076C2..60775E runs both feedback branches with unchanged distance;
603F9E..604062 runs the height interpolation including native CRT cosine.
"""
import argparse
import itertools
from pathlib import Path
from unicorn.x86_const import (
    UC_X86_REG_ESI, UC_X86_REG_EBX, UC_X86_REG_EDI, UC_X86_REG_EBP,
    UC_X86_REG_ESP, UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)
import wmo_registration_oracle as n
from camera_water_oracle import bits


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    histories = [
        [(1, 0, 1.), (0, 16, 0.), (0, 100, 0.), (0, 500, 0.),
         (0, 1000, 0.), (0, 1500, 0.), (0, 2000, 0.), (0, 2001, 0.)],
        [(1, 0, 1.), (0, 500, 0.), (1, 500, 1.), (0, 1000, 0.),
         (1, 1000, .5), (0, 2000, 0.), (0, 3100, 0.), (0, 3101, 0.)],
        [(1, 0, 1.), (0, 50, 0.), (1, 50, 1.), (1, 50, 1.), (0, 2100, 0.)],
        [(1, 0, 4.95), (0, 500, 0.), (0, 2100, 0.)],
        [(1, 0, 4.8888888), (0, 500, 0.), (0, 2100, 0.)],
        [(1, 0, 1.), (0, -1, 0.), (0, 1000, 0.), (0, 2001, 0.)],
    ]
    rows = ['# initial | (feedback time value)* | (height target active start anchor)*; hex words']
    for initial, base_time, history in itertools.product([2., 5., 15.], [1000, 0xfffffff0], histories):
        uc = n.emulator()
        camera, frame = n.HEAP, n.STACK + 0x18000
        n.write_floats(uc, camera + 0x128, [initial])
        n.write_floats(uc, camera + 0x218, [initial])
        actions, results = [], []
        for feedback, offset, value in history:
            time = (base_time + offset) & 0xffffffff
            uc.reg_write(UC_X86_REG_FPSW, 0)
            uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
            uc.reg_write(UC_X86_REG_EBP, frame)
            uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
            uc.reg_write(UC_X86_REG_ESI, camera)
            uc.reg_write(UC_X86_REG_EBX, time)
            uc.reg_write(UC_X86_REG_EDI, 0)
            if feedback:
                n.write_floats(uc, frame - 0x18, [value])
                uc.emu_start(0x6076c2, 0x60775e, count=10000)
            else:
                uc.mem_write(n.STOP, b'\xd9\xee')
                uc.emu_start(n.STOP, n.STOP + 2)
                uc.emu_start(0x603f9e, 0x604062, count=10000)
            actions += [feedback, time, bits(value)]
            results += [*n.read_words(uc, camera + 0x128, 1),
                        *n.read_words(uc, camera + 0x218, 1),
                        int(bool(n.read_words(uc, camera + 0x98, 1)[0] & 0x20000000)),
                        *n.read_words(uc, camera + 0x210, 1),
                        *n.read_words(uc, camera + 0x21c, 1)]
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in group)
                              for group in [[bits(initial)], actions, results]))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original height recovery histories')


if __name__ == '__main__':
    main()
