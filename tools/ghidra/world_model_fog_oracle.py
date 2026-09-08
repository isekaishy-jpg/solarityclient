"""Original MFOG max-heap palette selection and final camera fog composition.

Only resident group lookup and the already-selected WMO query are supplied.
7A1150's transforms, selection, heap and bank blends, and 7F16F0's complete fog
policy execute unmodified from the pinned executable.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def record(index, layout):
    position = [0., 0., 0.] if layout == 1 else [[0., 0., 0.], [4., 0., 0.], [-4., 0., 0.], [0., 4., 0.], [0., -4., 0.]][index]
    flags = [0x100, 0x10, 0x110, 0x20, 0x40][index]
    if layout == 2 and index == 2:
        flags |= 1
    values = [flags, *map(bits, position), bits(index * 2.), bits(30.), bits(100. + index * 51.25), bits(.2 + index * .125), 0xff123456 + index * 0x182013,
              bits(15. + index * 25.5), bits(-.2 + index * .1), 0xffabcd98 - index * 0x142319]
    return struct.pack('<12I', *values)


def capture():
    u = n.emulator()
    placed, root, group, fogs, heap, group_ids, output, aux, liquid_row, liquid_ptrs = [n.HEAP + i * 0x2000 for i in range(10)]
    n.write_words(u, placed + 0xf4, root)
    n.write_floats(u, placed + 0xb0, [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.])
    n.write_words(u, root + 0x158, fogs)
    n.write_words(u, 0xcd87a4, placed)
    n.write_words(u, 0xcdb0d8, 1, group_ids)
    n.write_words(u, 0xcfbe90, 1)
    n.write_words(u, 0xcfbe7c, heap)
    n.write_words(u, 0xcfbe88, 32, 1)
    n.write_words(u, 0xad4070, 1, 1)
    n.write_words(u, 0xad4084, liquid_ptrs)
    n.write_words(u, liquid_ptrs, liquid_row)
    final_mode, final_data, final_visible, final_distance, camera_liquid = False, b'', False, 0., 0

    def hook(u, address, size, context):
        if address == 0x7aea80:
            sp = u.reg_read(UC_X86_REG_ESP)
            return_value(u, group)
            u.reg_write(UC_X86_REG_ESP, sp + 12)
        elif address == 0x77fb90 and final_mode:
            pointers = n.read_words(u, u.reg_read(UC_X86_REG_ESP) + 4, 5)
            u.mem_write(pointers[0], final_data)
            n.write_words(u, pointers[1], placed)
            u.mem_write(pointers[2], bytes([int(final_visible)]))
            n.write_words(u, pointers[3], 0xd38bc4)
            n.write_words(u, pointers[4], bits(final_distance))
            return_value(u, 1)
        elif address == 0x780620 and final_mode:
            return_value(u, camera_liquid)
    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Original 7A1150 selection and 7F16F0 final scene fog. Hex words/banks.']
    for layout in range(3):
        data = b''.join(record(i, layout) for i in range(5))
        u.mem_write(fogs, data)
        rows.append(f'volumes {layout} {data.hex()}')
        for ids in [(1, 2, 3, 4), (4, 3, 2, 1), (2, 2, 3, 1), (0, 1, 4, 0)]:
            u.mem_write(group + 0x58, bytes(ids))
            for x, y in itertools.product([-34., -30., -15., -5., 0., 5., 10., 15., 20., 26., 30., 34.], [0., .123]):
                point = [x, y, 0.]
                n.write_floats(u, 0xcd8f5c, point)
                n.write_words(u, aux + 12, 0x7f7fffff)
                n.invoke(u, 0x7a1150, [output, aux, aux + 4, aux + 8, aux + 12])
                result = n.read_words(u, output, 12)
                rows.append(f'palette {layout} {bytes(ids).hex()} ' + ' '.join(f'{bits(v):08x}' for v in point) + ' ' + struct.pack('<7I', result[0], *result[6:]).hex())
    final_mode = True
    for mode, flags, liquid, visible, distance in itertools.product([0, 1], [0, 0x10, 0x100, 0x110], [-1, 0, 0x20, 0x40, 0x60, 0x100, 0x120, 0x140, 0x160], [False, True], [0., .001, 6.25, 12.5, 24.99999, 25., 30.]):
        final_data = struct.pack('<I', flags) + record(0, 0)[4:24] + struct.pack('<ffIffI', 300., .25, 0xff9a5731, 40., -.25, 0xff1a7fdb)
        final_visible, final_distance = visible, distance
        camera_liquid = int(liquid != -1)
        n.write_words(u, liquid_row + 8, max(liquid, 0))
        n.write_words(u, 0xd38b40, bits(777.))
        n.write_words(u, 0xd38acc, mode)
        n.write_words(u, 0xd38c1c, bits(500.), bits(.5), bits(2.5))
        n.write_words(u, 0xd38bf4, 0xff234567)
        n.invoke(u, 0x7f16f0, [])
        result = n.read_words(u, 0xd38ba0, 4)
        rows.append(f'final {mode} {flags:x} {liquid} {int(visible)} {bits(distance):08x} ' + ' '.join(f'{v:08x}' for v in result))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
