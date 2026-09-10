"""Capture 799310's group depth store and 78FB60's minimum doodad class.

Runs the camera plane slice, original 790650, the group callback's complete
depth arithmetic, and unmodified 78F570/78FB60. No instruction hooks.
"""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from world_scene_bounds_oracle import floats
from world_model_portal_projection_oracle import words


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('depth_frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    camera, target, group = [n.HEAP + i * 0x1000 for i in range(3)]
    stack = n.STACK + 0x18000
    rows = ['# depth eye3 target3 min3 max3 storedDepth; class detail depth minimumClass; all float words hex']
    for line in args.depth_frames.read_text().splitlines():
        if line.startswith('#'):
            continue
        u = n.emulator()
        values = floats(' '.join(line.split()[:12]))
        n.write_floats(u, camera, values[:3])
        n.write_floats(u, target, values[3:6])
        n.write_words(u, stack + 8, camera, target)
        u.reg_write(UC_X86_REG_EBP, stack)
        u.reg_write(UC_X86_REG_ESP, stack - 0x100)
        u.emu_start(0x7954a6, 0x795644, timeout=1_000_000, count=100_000)
        n.write_floats(u, group + 0x24, values[6:12])
        u.reg_write(UC_X86_REG_EBP, stack)
        u.reg_write(UC_X86_REG_ESP, stack - 0x100)
        u.reg_write(UC_X86_REG_ESI, group)
        u.emu_start(0x79938f, 0x7993d2, timeout=1_000_000, count=100_000)
        depth = n.read_words(u, group + 0x4c, 1)[0]
        rows.append('depth ' + words(values) + f' {depth:08x}')
    for detail in [.3333, .5, .7777, 1., 1.5]:
        n.invoke(u, 0x78f570, [bits(detail)])
        far = n.read_floats(u, 0xadf3a0, 4)
        depths = [-100., 0., .001, 10000.]
        for boundary in far:
            depths.extend([struct.unpack('<f', struct.pack('<I', bits(boundary) + offset))[0]
                           for offset in [-1, 0, 1]])
        for depth in depths:
            n.invoke(u, 0x78fb60, [bits(depth)])
            result = u.reg_read(UC_X86_REG_EAX) & 0xffff
            assert result <= 4
            rows.append('class ' + words([detail, depth]) + f' {result}')
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original group depth/class cases')


if __name__ == '__main__':
    main()
