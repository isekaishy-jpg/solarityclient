"""Execute pinned 12340 terrain-detail placement with controlled resident inputs.

Only model residency, instance allocation, and final batch insertion are supplied
by hooks. Scatter, RNG, terrain planes, colors, shadows, and transforms execute
the original 7D3390 instructions. Requires Unicorn and the pinned Wow.exe.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as native


def probe(seed, density, effect_density, holes, stencil, slope, colors, shadow):
    """Capture final placement arguments before texture batching."""
    uc = native.emulator()
    chunk, header, heights, layers, catalog, effect, instances, wrappers, vertex_colors, shadows = [
        native.HEAP + offset for offset in range(0, 0xa000, 0x1000)
    ]
    native.write_words(uc, chunk + 0x34, seed & 0xffff, seed >> 16)
    native.write_words(uc, chunk + 0x110, header, header + 0x40, header + 0x50, heights,
                       vertex_colors if colors else 0, 0, shadows if shadow else 0, layers)
    native.write_words(uc, header + 0xc, 1)
    uc.mem_write(header + 0x3c, struct.pack('<H', holes))
    uc.mem_write(header + 0x50, stencil.to_bytes(8, 'little'))
    native.write_words(uc, layers + 0xc, 1)
    native.write_words(uc, 0xad3af4, 1, 1)
    native.write_words(uc, 0xad3b08, catalog)
    native.write_words(uc, catalog, effect)
    native.write_words(uc, effect, 1, 1, 2, 0, 3, 5, 3, 0, 0, effect_density, 0)
    native.write_words(uc, 0xd1c4fc, wrappers)
    for index, flags in ((1, 0), (2, 1), (3, 2)):
        wrapper = wrappers + 0x100 + index * 0x20
        native.write_words(uc, wrappers + index * 4, wrapper)
        native.write_words(uc, wrapper, wrapper + 0x10)
        native.write_words(uc, wrapper + 0x18, flags)
    native.write_words(uc, 0xcd773c, density)
    native.write_words(uc, 0xd31948, 0x1f)
    for row in range(17):
        for column in range(9 if row % 2 == 0 else 8):
            index = (row // 2) * 17 + (9 if row % 2 else 0) + column
            x = -(row / 2) * (25 / 6)
            y = -(column + (0.5 if row % 2 else 0)) * (25 / 6)
            native.write_floats(uc, heights + index * 4, [slope * x + 0.1 * y])
            uc.mem_write(vertex_colors + index * 4,
                         bytes((40 + index % 70, 55 + index % 60, 65 + index % 50, 255)))
    uc.mem_write(shadows, bytes([0xaa, 0x55] * 256))
    placements = []

    def boundary(uc, address, size, user):
        stack = uc.reg_read(UC_X86_REG_ESP)
        target = native.read_words(uc, stack, 1)[0]
        pop = 0
        if address == 0x7d05f0:
            uc.reg_write(UC_X86_REG_EAX, 1)
        elif address == 0x7b3910:
            uc.reg_write(UC_X86_REG_EAX, instances)
        elif address == 0x7b31e0:
            model, position, angle, scale, normal, face, color = native.read_words(uc, stack + 4, 7)
            placements.append([model, *native.read_floats(uc, position, 3),
                               *struct.unpack('<2f', struct.pack('<2I', angle, scale)),
                               *native.read_floats(uc, normal, 3), face & 0xffff,
                               native.read_words(uc, color, 1)[0]])
            pop = 28
        uc.reg_write(UC_X86_REG_ESP, stack + 4 + pop)
        uc.reg_write(UC_X86_REG_EIP, target)

    for address in (0x7d05f0, 0x7b3910, 0x7b31e0):
        uc.hook_add(UC_HOOK_CODE, boundary, begin=address, end=address)
    uc.reg_write(UC_X86_REG_ECX, chunk)
    stack = native.STACK + 0x18000
    native.write_words(uc, stack, native.STOP)
    uc.reg_write(UC_X86_REG_ESP, stack)
    uc.emu_start(0x7d3390, native.STOP, timeout=5_000_000, count=2_000_000)
    assert uc.reg_read(UC_X86_REG_EIP) == native.STOP
    return placements


def capture(executable, output):
    """Save deterministic fixture cases spanning native admission branches."""
    native.initialize(executable)
    lines = ['# 7D3390 original instructions: seed density effect_density holes stencil slope colors shadow count']
    mesh_lines = ['# Original 7B1B50 vertex expansion; position normal ARGB uv']
    for seed, density, effect_density, holes, stencil, slope, colors, shadow in (
        (0x02000200, 16, 2, 0, 0, 0, 0, 0),
        (0x02030202, 64, 0, 0x1840, 0x1010101010101010, 0.4, 1, 1),
        (0x02030202, 256, 1, 0, 0, 2.4, 1, 0),
        (0x02030202, 256, 1, 0, 0, 1.2, 1, 0),
        (0x020f020e, 32, 4, 0xffff, 0, 0, 0, 0),
        (0x020f020e, 32, 4, 0, 0xffffffffffffffff, 0, 0, 0),
    ):
        placements = probe(seed, density, effect_density, holes, stencil, slope, colors, shadow)
        lines.append('case ' + ' '.join(map(str, (seed, density, effect_density, holes, stencil, slope,
                                                colors, shadow, len(placements)))))
        lines.extend(' '.join(map(str, row)) for row in placements)
        vertices = mesh_probe(placements)
        mesh_lines.append('case ' + str(len(vertices)))
        mesh_lines.extend(' '.join(map(str, vertex)) for vertex in vertices)
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    Path(output).with_name('ground-detail-mesh-native.txt').write_text('\n'.join(mesh_lines) + '\n', encoding='utf-8')


def mesh_probe(placements):
    """Run stock vertex expansion with equivalent resident SKIN/model providers."""
    uc = native.emulator()
    batch, records, wrappers, gpu, device, vtable, backend, output = [native.HEAP + offset for offset in
        (0, 0x1000, 0x6000, 0x7000, 0x7100, 0x7200, 0x8000, 0x10000)]
    native.write_words(uc, batch + 0xc, gpu)
    native.write_words(uc, batch + 0x18, len(placements), records)
    native.write_words(uc, device, vtable)
    native.write_words(uc, vtable + 0xd8, native.STOP + 0x10, native.STOP + 0x20)
    native.write_words(uc, 0xc5df88, device)
    native.write_words(uc, 0xd1c4fc, wrappers)
    native.write_words(uc, 0xd1c4f0, 1)
    for index, flags in ((1, 0), (2, 1), (3, 2)):
        base = native.HEAP + 0x9000 + index * 0x600
        wrapper, row, model, shared, header, skin, lookup, vertices = [base + offset for offset in
            (0, 0x10, 0x20, 0x60, 0x200, 0x300, 0x400, 0x440)]
        native.write_words(uc, wrappers + index * 4, wrapper)
        native.write_words(uc, wrapper, row, model)
        native.write_words(uc, row + 8, flags)
        native.write_words(uc, model + 0x10, 1)
        native.write_words(uc, model + 0x2c, shared)
        native.write_words(uc, shared + 0x150, header)
        native.write_words(uc, shared + 0x170, skin)
        native.write_words(uc, header + 0x40, vertices)
        native.write_words(uc, skin + 4, 4, lookup)
        uc.mem_write(lookup, struct.pack('<4H', 2, 0, 3, 1))
        for vertex, (x, y, z, u, v) in enumerate(((-1, 0, 0, 0, 1), (1, 0, 0, 1, 1), (-1, 0, 2, 0, 0), (1, 0, 2, 1, 0))):
            native.write_floats(uc, vertices + vertex * 48, [x, y, z])
            native.write_floats(uc, vertices + vertex * 48 + 32, [u, v])
    for index, placement in enumerate(placements):
        model, x, y, z, angle, scale, nx, ny, nz, face, color = placement
        address = records + index * 44
        uc.mem_write(address + 2, struct.pack('<H', face))
        native.write_words(uc, address + 4, model)
        native.write_floats(uc, address + 8, [x, y, z, angle, scale, nx, ny, nz])
        native.write_words(uc, address + 40, color)

    def boundary(uc, address, size, user):
        stack = uc.reg_read(UC_X86_REG_ESP)
        target = native.read_words(uc, stack, 1)[0]
        value, pop = {native.STOP + 0x10: (output, 4), native.STOP + 0x20: (0, 8), 0x532af0: (backend, 0)}[address]
        uc.reg_write(UC_X86_REG_EAX, value)
        uc.reg_write(UC_X86_REG_ESP, stack + 4 + pop)
        uc.reg_write(UC_X86_REG_EIP, target)

    for address in (native.STOP + 0x10, native.STOP + 0x20, 0x532af0):
        uc.hook_add(UC_HOOK_CODE, boundary, begin=address, end=address)
    uc.reg_write(UC_X86_REG_ECX, batch)
    stack = native.STACK + 0x18000
    native.write_words(uc, stack, native.STOP)
    uc.reg_write(UC_X86_REG_ESP, stack)
    uc.emu_start(0x7b1b50, native.STOP, timeout=5_000_000, count=2_000_000)
    assert uc.reg_read(UC_X86_REG_EIP) == native.STOP
    return [struct.unpack('<6fI2f', uc.mem_read(output + index * 36, 36)) for index in range(len(placements) * 4)]


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
