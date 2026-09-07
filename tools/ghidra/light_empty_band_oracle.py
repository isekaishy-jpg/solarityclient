"""Verify build-12340 zero-key color/scalar sampling without replacing native code.

Original 7EB070 and 7EAEF0 execute against a zero-key row with nonzero padding.
The scalar result is stored from x87 by a six-byte FSTP adapter. No game or
operating-system entry point runs; the imported loader verifies the PE hash.
"""

import argparse
import struct

import wmo_registration_oracle as n
from unicorn.x86_const import UC_X86_REG_EDI


def verify(executable):
    """Check every channel and four times with deliberately nonzero padded values."""
    n.initialize(executable)
    u = n.emulator()
    row, result, store_float = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x2000
    u.mem_write(store_float, b'\xd9\x1d' + struct.pack('<I', result))
    cases = 0
    for time in [0, 719, 1440, 2879]:
        for kind, count in [('color', 18), ('float', 6)]:
            for channel in range(count):
                padding = 0x00112233 if kind == 'color' else 0x418a0000
                n.write_words(u, row, 213 * count - count + channel + 1, 0,
                              *([0] * 16), *([padding] * 16))
                n.write_words(u, result, 0xdeadbeef)
                u.reg_write(UC_X86_REG_EDI, row)
                if kind == 'color':
                    n.invoke(u, 0x7eb070, [result, time])
                    expected = 0xff000000
                else:
                    n.invoke(u, 0x7eaef0, [time, channel])
                    u.emu_start(store_float, store_float + 6)
                    expected = 0
                actual = n.read_words(u, result, 1)[0]
                assert actual == expected, (kind, channel, time, hex(actual))
                cases += 1
    print('Verified', cases, 'native empty-band samples: opaque black / scalar zero')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    verify(parser.parse_args().executable)
