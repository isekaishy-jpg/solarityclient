"""Capture build 12340's complete celestial ephemeris without replacing its math.

The fixture supplies the native camera, cyclic day and calendar day providers,
and the scale/period values assigned by 7F2790. All table initialization,
interpolation, phase quantization, cubic trigonometry and output run unmodified.
"""
import argparse
import struct
from pathlib import Path
import wmo_registration_oracle as n


def capture():
    u = n.emulator()
    for address, value in [(0xd38e40, 1.), (0xd38e44, 1.),
                           (0xd38e60, n.read_floats(u, 0xa1ea74, 1)[0]),
                           (0xd38e64, 1.), (0xd38e80, 1.),
                           (0xd38e84, n.read_floats(u, 0xa241a0, 1)[0])]:
        n.write_floats(u, address, [value])
    rows = ['# Build 12340 7EECC0; native little-endian input/output float words.']
    phases = [i / 96 for i in range(97)]
    for address in [0xa41a74, 0xa41a7c, 0xa41a80, 0xa41a8c, 0xa41c78,
                    0xa41c74, 0xa41c60, 0xa41bdc, 0xa41c68, 0xa41c64]:
        word = n.read_words(u, address, 1)[0]
        phases.extend(struct.unpack('<f', struct.pack('<I', word + d))[0] for d in [-1, 0, 1])
    for days in [0., 1., 10957., 20000., 22500.]:
        for eye in [(0., 0., 0.), (12345.125, -6789.25, 128.125)]:
            for phase in sorted(set(phases)):
                n.write_floats(u, 0xd38b04, [phase, days])
                n.write_floats(u, 0xd38b18, eye)
                n.invoke(u, 0x7eecc0, [])
                source = bytes(u.mem_read(0xd38b04, 8)) + bytes(u.mem_read(0xd38b18, 12))
                result = b''.join(bytes(u.mem_read(a, 12)) + bytes(u.mem_read(a+20, 4))
                                  for a in [0xd38e28, 0xd38e48, 0xd38e68])
                result += bytes(u.mem_read(0xd38cd8, 4))
                rows.append('sample ' + source.hex() + ' ' + result.hex())
    for address, count in [(0xd391e0, 5), (0xd391c8, 3), (0xd391a0, 5), (0xd39188, 3),
                           (0xd39160, 5), (0xd39148, 3), (0xd39128, 4), (0xd39108, 4)]:
        rows.append(f'table {address:x} ' + bytes(u.mem_read(address, count*8)).hex())
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Wrote {len(rows)-1} native celestial rows to {args.output}')
