"""Capture original 78F570 thresholds and 7BABC0 scenery-shadow distance tests."""
import argparse
import math
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EAX
import wmo_registration_oracle as n
from world_shadow_volume_oracle import invoke


def capture(executable):
    """Probe both sides of every class's native fade-start distance."""
    n.initialize(executable)
    u = n.emulator()
    rows = ['# category detail center3 camera3 admitted; 78F570/7BABC0, build 12340']
    for category in range(5):
        for detail in [.5, .75, 1., 1.0001, 1.5]:
            invoke(u, 0x78f570, [struct.unpack('<I', struct.pack('<f', detail))[0]])
            square = n.read_floats(u, 0xadf3dc+4*category, 1)[0]
            boundary = math.sqrt(square)
            for center in [(0., 0., 0.), (15302., -12840., 1200.)]:
                n.write_floats(u, n.HEAP, center)
                for direction in [(1., 0., 0.), (.6, .8, 0.), (0., 0., -1.)]:
                    for offset in [-1., -.01, -.0001, 0., .0001, .01, 1.]:
                        camera = [value+axis*(boundary+offset) for value, axis in zip(center, direction)]
                        n.write_floats(u, 0xcd8f5c, camera)
                        camera = n.read_floats(u, 0xcd8f5c, 3)
                        invoke(u, 0x7babc0, [n.HEAP, category])
                        admitted = u.reg_read(UC_X86_REG_EAX) == 0
                        rows.append('shadow_distance '+' '.join(map(str, [category, detail, *center, *camera, int(admitted)])))
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    rows = capture(args.executable)
    args.output.write_text('\n'.join(rows)+'\n')
    print(f'Captured {len(rows)-1} original scenery shadow distance queries')
