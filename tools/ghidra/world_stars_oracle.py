"""Capture untouched 7EE0D0, including its native cyclic lookup and FIST rounding."""
import argparse
import struct
from pathlib import Path
import wmo_registration_oracle as n


def capture():
    u = n.emulator()
    rows = ['# day-f32-bits alpha-u8; original 7EE0D0 without hooks.']
    days = {struct.unpack('<I', struct.pack('<f', tick / 2880))[0] for tick in range(2881)}
    for edge in [0., .125, .1875, .9375, 1.]:
        bits = struct.unpack('<I', struct.pack('<f', edge))[0]
        days.update(range(max(0, bits - 5), bits + 6))
    for ramp_start in [.125, .9375]:
        for byte in range(255):
            day = ramp_start + (byte / 254) * .0625
            bits = struct.unpack('<I', struct.pack('<f', day))[0]
            days.update(range(bits - 1, bits + 2))
    for bits in sorted(days):
        n.write_words(u, 0xd38b04, bits)
        u.reg_write(n.UC_X86_REG_ECX, n.HEAP)
        n.invoke(u, 0x7ee0d0, [])
        rows.append(f'{bits:08x} {u.mem_read(n.HEAP + 11, 1)[0]}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Wrote {len(rows)-1} native stars samples to {args.output}')
