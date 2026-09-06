"""Capture build-12340 distance collision feedback and two-second recovery.

Runs original 6076C2..607714 feedback, 603E7B..603F9E interpolation,
and ordinary wheel request/tick functions. Geometry is supplied as an already
resolved distance; neither scene tracing nor other camera lanes are claimed.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn.x86_const import (
    UC_X86_REG_ECX, UC_X86_REG_ESI, UC_X86_REG_EBX, UC_X86_REG_EBP,
    UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)
import wmo_registration_oracle as n


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    histories = [
        [(3, 0, 1.), (2, 16, 0.), (2, 100, 0.), (2, 500, 0.),
         (2, 1000, 0.), (2, 1500, 0.), (2, 2000, 0.), (2, 2001, 0.)],
        [(3, 0, 1.), (2, 500, 0.), (3, 500, 1.), (2, 1000, 0.),
         (3, 1000, .5), (2, 2000, 0.), (2, 3100, 0.)],
        [(3, 0, 1.), (0, 50, 1.), (2, 100, 0.), (2, 500, 0.), (2, 2100, 0.)],
        [(3, 0, 1.), (1, 50, 2.), (2, 100, 0.), (2, 500, 0.), (2, 2100, 0.)],
        [(3, 0, 1.), (2, 50, 0.), (3, 50, 1.), (3, 50, 1.), (2, 2100, 0.)],
        [(3, 0, 4.95), (2, 500, 0.), (2, 2100, 0.)],
    ]
    rows = ['# initial | (kind time value)* | (distance target active start anchor)*; hex words']
    for initial, base_time, history in itertools.product([5., 15.], [1000, 0xfffffff0], histories):
        uc = n.emulator()
        camera, frame = n.HEAP, n.STACK + 0x18000
        for i, (pointer, value) in enumerate([(0xC24E58, 8.33), (0xC24988, 15.), (0xC2498C, 1.)]):
            address = n.HEAP + 0x1000 + i * 0x100
            n.write_words(uc, pointer, address)
            n.write_floats(uc, address + 0x2c, [value])
        n.write_floats(uc, camera + 0x118, [initial])
        n.write_floats(uc, camera + 0x1e8, [initial])
        actions, results = [], []
        for kind, offset, value in history:
            time = (base_time + offset) & 0xffffffff
            uc.reg_write(UC_X86_REG_FPSW, 0)
            uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
            uc.reg_write(UC_X86_REG_ECX, camera)
            if kind < 2:
                n.invoke(uc, [0x5ff950, 0x5ffa60][kind], [bits(value), time, 0])
            else:
                if kind == 2:
                    n.invoke(uc, 0x6000e0, [time])
                uc.reg_write(UC_X86_REG_EBP, frame)
                uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
                uc.reg_write(UC_X86_REG_ESI, camera)
                uc.reg_write(UC_X86_REG_EBX, time)
                if kind == 3:
                    n.write_floats(uc, frame - 8, [value])
                    start, end = 0x6076c2, 0x607714
                else:
                    uc.mem_write(n.STOP, b'\xd9\xee')
                    uc.emu_start(n.STOP, n.STOP + 2)
                    start, end = 0x603e7b, 0x603f9e
                uc.emu_start(start, end, count=10000)
                assert uc.reg_read(UC_X86_REG_EIP) == end
            actions += [kind, time, bits(value)]
            results += [*n.read_words(uc, camera + 0x118, 1),
                        *n.read_words(uc, camera + 0x1e8, 1),
                        int(bool(n.read_words(uc, camera + 0x98, 1)[0] & 0x4000000)),
                        *n.read_words(uc, camera + 0x1e0, 1),
                        *n.read_words(uc, camera + 0x1ec, 1)]
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in group)
                               for group in [[bits(initial)], actions, results]))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original obstruction/zoom histories')


if __name__ == '__main__':
    main()
