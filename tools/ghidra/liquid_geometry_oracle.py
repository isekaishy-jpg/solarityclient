"""Capture original build-12340 terrain liquid vertices and triangle strips.

Executes 7CDF80, 7CE390, 7CE270, 7CE1F0 and 95DA20 in the pinned PE.
The resident height/depth/UV provider is represented by tiny accessor stubs;
all geometry, transforms, depth lookup selection and bit-mask traversal execute
original instructions. No client entry point or operating-system code runs.
"""
import argparse
import json
import random
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_ECX

import liquid_material_oracle as material
import wmo_registration_oracle as native


def capture(case, tables):
    """Return interleaved native vertices and the unmodified native index strip."""
    uc = native.emulator()
    owner, chunk, provider, vtable, heights, depths, uvs, mask, matrix, output, pointers, indices, draw, bank, row, mat = [
        native.HEAP + offset for offset in
        (0, 0x1000, 0x2000, 0x2100, 0x3000, 0x3200, 0x3300, 0x3500,
         0x3600, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000, 0x9000, 0xa000)]
    fmt, depth_bank, x, y, width, height, gx, gy = case['header']
    native.write_words(uc, owner + 4, 1, fmt)
    native.write_words(uc, owner + 0x34, y, x, y + height, x + width, provider, mask)
    native.write_words(uc, owner + 0x5c, chunk)
    native.write_words(uc, chunk + 0x34, gx, gy)
    native.write_floats(uc, chunk + 0x7c, case['chunk'])
    native.write_words(uc, provider, vtable)
    stubs = [native.HEAP + 0xb000 + i * 32 for i in range(3)]
    native.write_words(uc, vtable, 0, *stubs)
    # mov eax,[esp+4]; fld dword [eax*4+heights]; ret 4
    uc.mem_write(stubs[0], b'\x8b\x44\x24\x04\xd9\x04\x85' + struct.pack('<I', heights) + b'\xc2\x04\x00')
    # mov eax,[esp+4]; movzx eax,byte [eax+depths]; ret 4
    uc.mem_write(stubs[1], b'\x8b\x44\x24\x04\x0f\xb6\x80' + struct.pack('<I', depths) + b'\xc2\x04\x00')
    # mov eax,[esp+4]; lea eax,[eax*4+uvs]; ret 4
    uc.mem_write(stubs[2], b'\x8b\x44\x24\x04\x8d\x04\x85' + struct.pack('<I', uvs) + b'\xc2\x04\x00')
    native.write_floats(uc, heights, case['heights'])
    uc.mem_write(depths, bytes(case['depths']))
    uc.mem_write(uvs, b''.join(struct.pack('<HH', *uv) for uv in case['uvs']))
    uc.mem_write(mask, case['mask'].to_bytes(8, 'little'))
    native.write_words(uc, 0xad4070, 1, 1)
    native.write_words(uc, 0xad4084, bank)
    native.write_words(uc, bank, row)
    native.write_words(uc, row + 0x38, 1)
    native.write_words(uc, row + 0xa4, min(depth_bank, 1))
    native.write_words(uc, 0xad4094, 1, 1)
    native.write_words(uc, 0xad40a8, bank + 4)
    native.write_words(uc, bank + 4, mat)
    native.write_words(uc, mat + 4, 1 if depth_bank == 2 else 0)
    native.write_words(uc, 0xadfbb4, 0xcdf7d0, 0xcdfbd0)
    for address, table in zip((0xcdf7d0, 0xcdfbd0), tables):
        native.write_words(uc, address, *table)
    uc.reg_write(UC_X86_REG_ECX, owner)
    native.invoke(uc, 0x7cdf80, [])
    transform = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., *case['translation'], 1.]
    native.write_floats(uc, matrix, transform)
    native.write_words(uc, pointers, output, output + 12, output + 24, output + 28, output + 36)
    uc.reg_write(UC_X86_REG_ECX, owner)
    native.invoke(uc, 0x7ce390, [matrix, 44, *[pointers + i * 4 for i in range(5)]])
    uc.reg_write(UC_X86_REG_ECX, owner)
    native.invoke(uc, 0x7ce270, [indices, 0, draw])
    count = native.read_words(uc, draw + 8, 1)[0]
    return bytes(uc.mem_read(output, len(case['heights']) * 44)), bytes(uc.mem_read(indices, count * 2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    tables = material.depth_coordinates()
    rng = random.Random(12340)
    records = []
    for index in range(36):
        fmt, bank = index % 4, index % 3
        x, y = (0, 0) if index < 12 else (index % 7, (index * 3) % 7)
        width, height = 8 - x, 8 - y
        count = (width + 1) * (height + 1)
        mask = (1 << (width * height)) - 1
        if index % 3 == 1:
            mask = rng.getrandbits(width * height)
        elif index % 3 == 2:
            mask = 0 if index < 12 else 1 << (width * height - 1)
        chunk = [rng.uniform(-16000., 16000.), rng.uniform(-16000., 16000.), rng.uniform(-100., 200.)]
        translation = [0., 0., 0.] if index < 12 else [-33.333332, -33.333332, .12345]
        heights = [float((i * 7 + index) % 31) / 3. for i in range(count)]
        if fmt == 2:
            heights = [0.] * count
        case = dict(header=[fmt, bank, x, y, width, height, (index * 29) % 1024, (index * 67) % 1024],
                    chunk=chunk, translation=translation, heights=heights,
                    depths=[(i * 41 + index) % 256 for i in range(count)],
                    uvs=[[(i * 173) % 65536, (i * 439) % 65536] for i in range(count)], mask=mask)
        if fmt == 1:
            case['depths'] = [255] * count
        vertices, strip = capture(case, tables)
        records.append(struct.pack('<8I6fQII', *case['header'], *chunk, *translation, mask, count, len(strip) // 2)
                       + struct.pack('<' + 'f' * count, *heights) + bytes(case['depths'])
                       + b''.join(struct.pack('<HH', *uv) for uv in case['uvs']) + vertices + strip)
    Path(args.output).write_bytes(b''.join(records))
    print(json.dumps({'native_geometry_cases': len(records)}))


if __name__ == '__main__':
    main()
