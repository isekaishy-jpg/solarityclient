"""Capture native 979560/9793B0/979330 particle flipbook sampling without hooks."""
import argparse
from pathlib import Path
import struct
from unicorn.x86_const import UC_X86_REG_EAX
import wmo_registration_oracle as n


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# name timestamps values age-f32-bits selected-cell; native 979560']
    tracks = [
        ('single', [0], [7]),
        ('two', [0, 32767], [0, 16]),
        ('descending', [0, 32767], [16, 0]),
        ('three', [0, 8192, 32767], [0, 8, 24]),
        ('brazier', [0, 16384, 16384, 32767], [0, 16, 17, 33]),
        ('uneven', [0, 4096, 24576, 32767], [1, 9, 3, 20]),
    ]
    ages = sorted(set([i / 128 for i in range(129)] + [16383/32767,16384/32767,16385/32767]))
    for name, timestamps, values in tracks:
        u = n.emulator()
        track, times, keys = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x2000
        n.write_words(u, track, len(timestamps), times, len(values), keys)
        u.mem_write(times, struct.pack('<' + 'H' * len(timestamps), *timestamps))
        u.mem_write(keys, struct.pack('<' + 'H' * len(values), *values))
        for age in ages:
            bits = struct.unpack('<I', struct.pack('<f', age))[0]
            n.invoke(u, 0x979560, [track, bits])
            cell = u.reg_read(UC_X86_REG_EAX)
            rows.append(f'{name} {",".join(map(str,timestamps))} {",".join(map(str,values))} {bits:08x} {cell}')
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows) - 1} native particle flipbook samples')
