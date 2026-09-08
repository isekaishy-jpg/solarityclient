"""Capture native constructor and 7F3230's exact celestial color update block.

The block runs unmodified, with the completed palette, no color override, and
weather as inputs. This isolates colors from unrelated world/skybox providers.
"""
import argparse
import struct
from pathlib import Path
import wmo_registration_oracle as n
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_EIP


def capture():
    u = n.emulator()
    n.invoke(u, 0x9d0760, [])
    rows = ['# Original 9D0760 constructor and 7F36EF..7F3809 color update.']
    initial = b''.join(bytes(u.mem_read(a, 4)) for a in [0xd38e34, 0xd38e54, 0xd38e74])
    rows.append('initial ' + initial.hex())
    for color in [0xff000000, 0xffffffff, 0xff123456, 0xff89abcd]:
        for weather in [0., .001, .5/255, 1.5/255, .125, .25, .5, .75, 1., 0., .33, 0.]:
            n.write_words(u, 0xd38bf8, color)
            n.write_floats(u, 0xd38b88, [weather])
            u.reg_write(UC_X86_REG_ESP, n.STACK+0x10000)
            u.reg_write(UC_X86_REG_EBP, n.STACK+0x11000)
            u.emu_start(0x7f36ef, 0x7f3809, timeout=1_000_000, count=10000)
            assert u.reg_read(UC_X86_REG_EIP) == 0x7f3809
            result = b''.join(bytes(u.mem_read(a, 4)) for a in [0xd38e34, 0xd38e54, 0xd38e74])
            rows.append('color ' + struct.pack('<If', color, weather).hex() + ' ' + result.hex())
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Wrote {len(rows)-1} native celestial color rows to {args.output}')
