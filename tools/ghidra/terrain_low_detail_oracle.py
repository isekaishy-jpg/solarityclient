"""Capture native WDL loading and complete mesh uploads (7CC310/7D5150/7D5240).

The pinned PE executes the original loader and geometry loops. Archive bytes,
allocation and GX buffer lock/unlock are supplied by the harness. Native loading
provides the base corners, bounds and unculled counts consumed by the uploads.
This does not establish visibility or render state.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_FPSW, UC_X86_REG_FPTAG
import wmo_registration_oracle as n
from camera_water_oracle import ret


def load_tile(index, heights, masks):
    """Run original 7CC310 with controlled archive bytes and allocation boundaries."""
    uc = n.emulator()
    owner, tile, file_data = n.HEAP, n.HEAP + 0x6000, n.HEAP + 0x10000
    def chunk(magic, payload):
        return magic + struct.pack('<I', len(payload)) + payload
    offsets = [0] * 4096
    offsets[index[1] * 64 + index[0]] = 12 + 8 + 4096 * 4
    blob = chunk(b'REVM', struct.pack('<I', 18)) + chunk(b'FOAM', struct.pack('<4096I', *offsets))
    blob += chunk(b'ERAM', struct.pack('<545h', *heights)) + chunk(b'OHAM', struct.pack('<16H', *masks))
    def hook(machine, address, size, unused):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x76f070: ret(machine)
        elif address == 0x424f80:
            n.write_words(machine, n.read_words(machine, sp + 8, 1)[0], 1)
            ret(machine, 1, 8)
        elif address == 0x4218c0: ret(machine, len(blob), 8)
        elif address == 0x76e540: ret(machine, file_data, 16)
        elif address == 0x422530:
            machine.mem_write(file_data, blob)
            ret(machine, 1, 24)
        elif address == 0x422910: ret(machine, 1, 4)
        elif address == 0x7c0a90: ret(machine, tile)
    uc.hook_add(UC_HOOK_CODE, hook)
    uc.reg_write(UC_X86_REG_ECX, owner)
    n.invoke(uc, 0x7cc310, [0, 0])
    assert n.read_words(uc, owner + 0x18 + (index[1] * 64 + index[0]) * 4, 1)[0] == tile
    return n.read_floats(uc, tile + 0x2c, 2), n.read_floats(uc, tile + 4, 6), n.read_words(uc, tile + 0x40, 1)[0]


def capture(base, heights, masks):
    """Execute both original upload routines with separate marked/unmarked banks."""
    uc = n.emulator()
    tile, gx, vtable, buffer, output, source, face_masks = [
        n.HEAP + offset for offset in (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x8000, 0x9000)
    ]
    n.write_words(uc, 0xc5df88, gx)
    n.write_words(uc, gx, vtable)
    n.write_words(uc, vtable + 0xd8, n.STOP + 0x100, n.STOP + 0x110)
    n.write_floats(uc, tile + 0x2c, base)
    n.write_words(uc, tile + 0x44, source)
    uc.mem_write(source, struct.pack('<545h', *heights))
    uc.mem_write(face_masks, struct.pack('<16H', *masks))

    def hook(machine, address, size, unused):
        if address == n.STOP + 0x100:
            assert n.read_words(machine, machine.reg_read(UC_X86_REG_ESP) + 4, 1)[0] == buffer
            ret(machine, output, 4)
        elif address == n.STOP + 0x110:
            ret(machine, 0, 8)

    uc.hook_add(UC_HOOK_CODE, hook)
    n.invoke(uc, 0x7d5150, [tile, buffer])
    vertices = bytes(uc.mem_read(output, 545 * 16))
    assert all(struct.unpack_from('<I', vertices, i * 16 + 12)[0] == 0xffffffff for i in range(545))
    banks = []
    for marked in (False, True):
        count = sum((mask.bit_count() if marked else 16 - mask.bit_count()) * 12 for mask in masks)
        uc.mem_write(output, b'\xcd' * (3072 * 2))
        uc.reg_write(UC_X86_REG_FPSW, 0)
        uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
        n.invoke(uc, 0x7d5240, [face_masks, int(marked), buffer])
        banks.append(bytes(uc.mem_read(output, count * 2)))
        assert bytes(uc.mem_read(output + count * 2, (3072 - count) * 2)) == b'\xcd' * ((3072 - count) * 2)
    return vertices, banks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    output = bytearray(b'WDL12340')
    cases = [
        ((0, 0), [0] * 16),
        ((40, 29), [0xffff] * 16),
        ((32, 32), [0x5555, 0xaaaa] * 8),
        ((63, 63), [1 << i for i in range(16)]),
    ]
    output += struct.pack('<I', len(cases))
    for index, (tile_index, masks) in enumerate(cases):
        heights = [((i * 127 + index * 193) % 65536) - 32768 for i in range(545)]
        base, bounds, unculled = load_tile(tile_index, heights, masks)
        vertices, banks = capture(base, heights, masks)
        assert unculled == len(banks[0]) // 2
        output += struct.pack('<2I8f545h16HI', *tile_index, *base, *bounds, *heights, *masks, unculled)
        output += vertices + banks[0] + banks[1]
    args.output.write_bytes(output)
    print(f'captured {len(cases)} complete native WDL uploads')


if __name__ == '__main__':
    main()
