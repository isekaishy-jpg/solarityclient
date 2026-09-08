"""Capture MCNR decoding from unchanged native world-space vertex upload 7C4620."""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    chunk, output, normals, heights = [n.HEAP + i * 0x2000 for i in range(4)]
    n.write_words(u, chunk + 0x11c, heights, 0, normals)
    values = [(0,0,127), (127,0,0), (0,127,0), (63,-31,95), (-128,64,-32)]
    values += [tuple(((i * m) % 256) - 128 for m in (13,31,71)) for i in range(140)]
    u.mem_write(normals, b''.join(struct.pack('<3b', *value) for value in values))
    u.reg_write(UC_X86_REG_ECX, chunk)
    n.invoke(u, 0x7c4620, [output])
    rows = ['# 7C4620 raw MCNR bytes (world XYZ order), native output normal float32 bits']
    for i, value in enumerate(values):
        words = n.read_words(u, output + i * 28 + 12, 3)
        rows.append(' '.join(map(str, value)) + ' ' + ' '.join(f'{word:08x}' for word in words))
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(values)} native terrain normals')
