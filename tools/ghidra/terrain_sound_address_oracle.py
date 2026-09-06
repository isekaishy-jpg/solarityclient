"""Capture build-12340 7A0530 terrain sound addressing and authored layer selection.

Only tile residency and the GroundEffectTexture database lookup are supplied by
hooks. The original x87 projection, holes, packed layer selection, and record
field read execute from the fingerprinted local image.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    uc = n.emulator()
    point, output, tile, chunk, header, rows, layers, record = [n.HEAP + i * 0x2000 for i in range(8)]
    n.write_words(uc, tile + 0xbc, *([chunk] * 256))
    n.write_words(uc, chunk + 0x110, header, rows)
    n.write_words(uc, chunk + 0x12c, layers)
    selectors = [0xe4e4, 0x1b1b, 0xaaaa, 0x5555, 0xffff, 0, 0xb1b1, 0x4e4e]
    uc.mem_write(rows, struct.pack('<8H', *selectors))
    for layer in range(4):
        n.write_words(uc, layers + layer * 16 + 12, 100 + layer)
    observed = []

    def hook(machine, address, size, context):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x79b440:
            observed.extend(n.read_words(machine, sp + 4, 2))
            machine.reg_write(UC_X86_REG_EAX, tile)
            machine.reg_write(UC_X86_REG_ESP, sp + 4)
            machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])
        elif address == 0x7a0602:
            observed.extend([machine.reg_read(UC_X86_REG_EDI), machine.reg_read(UC_X86_REG_ESI)])
        elif address == 0x65c290:
            effect = n.read_words(machine, sp + 4, 1)[0]
            n.write_words(machine, record + 0x28, effect + 1000)
            machine.reg_write(UC_X86_REG_EAX, record)
            machine.reg_write(UC_X86_REG_ESP, sp + 8)
            machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])

    uc.hook_add(UC_HOOK_CODE, hook)
    positions = [(0., 0.), (1., -1.), (1000., 5800.), (10349.5, -6368.4),
                 (17066.666, 17066.666), (-17066.666, -17066.666),
                 (17067., 0.), (0., -17067.)]
    positions += [(f32(17066.666 - (2048 + y + delta) / .24),
                   f32(17066.666 - (3072 + x + delta) / .24))
                  for x, y, delta in itertools.product(range(8), range(8), [0., .001, .5, .999])]
    lines = ['# x y holes | tile-x tile-y grid-x grid-y admitted terrain; all hex words',
             '# packed rows: ' + ' '.join(f'{value:04x}' for value in selectors)]
    for (x, y), holes in itertools.product(positions, [0, 1, 0x8000, 0xaaaa, 0xffff]):
        observed.clear()
        n.write_floats(uc, point, [x, y, 0.])
        n.write_words(uc, output, 0xffffffff)
        uc.mem_write(header + 0x3c, struct.pack('<H', holes))
        n.invoke(uc, 0x7a0530, [point, output])
        admitted = uc.reg_read(UC_X86_REG_EAX) & 255
        values = list(n.read_words(uc, point, 2)) + [holes] + (observed or [0xffffffff] * 4)
        values += [admitted, n.read_words(uc, output, 1)[0]]
        lines.append(' '.join(f'{value:08x}' for value in values))
    args.output.write_text('\n'.join(lines) + '\n')
    print(f'Captured {len(lines) - 2} original terrain sound queries')


if __name__ == '__main__':
    main()
