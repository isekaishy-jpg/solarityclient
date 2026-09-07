"""Capture original 793D20 WMO material and lighting-factory selection.

Only allocation, resident group/type accessors, and final queue submission are
supplied. The original function reads the complete LiquidType/LiquidMaterial
records, performs its interior remap, and calls the original geometry setters.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_EAX

import liquid_material_oracle as material
import wmo_registration_oracle as native


def capture(group_flags, root_flags, liquid_flags, liquid_id):
    """Return the real factory's remapped type, tint, UV mode, and depth column."""
    uc = native.emulator()
    node, link, parent, root, group, tint, owner, factory, draw, bank, types, matbank, mats = [
        native.HEAP + offset for offset in
        (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000,
         0x7000, 0x8000, 0x9000, 0xa000, 0xc000, 0xd000)]
    native.write_words(uc, 0xcdb08c, 0, 0, node)
    native.write_words(uc, node + 4, 1)
    native.write_words(uc, node + 0xc, 2 if root_flags & 0x48 == 0 else 4)
    native.write_words(uc, node + 0x20, link)
    native.write_words(uc, link + 8, parent)
    native.write_words(uc, parent + 0xf4, root)
    native.write_words(uc, root + 0x160, tint)
    native.write_words(uc, tint + 0x1c, 0xa1234567)
    native.write_words(uc, group + 0x30, group_flags)
    native.write_words(uc, group + 0x144, liquid_id)
    native.write_words(uc, 0xcd774c, 0x100)
    native.write_words(uc, 0xcd8610, 1)
    native.write_words(uc, 0xad4070, 22, 1)
    native.write_words(uc, 0xad4084, bank)
    for index in range(1, 23):
        row = types + (index - 1) * 180
        native.write_words(uc, bank + (index - 1) * 4, row)
        native.write_words(uc, row, index, 0, liquid_flags)
        native.write_words(uc, row + 0x38, 2 if index == 21 else 1)
    native.write_words(uc, 0xad4094, 2, 1)
    native.write_words(uc, 0xad40a8, matbank)
    native.write_words(uc, matbank, mats, mats + 12)
    native.write_words(uc, mats, 1, 0, 1, 2, 1, 0)
    uc.reg_write(UC_X86_REG_ECX, group)
    native.invoke(uc, 0x7d7310, [liquid_id])
    resolved_id = uc.reg_read(UC_X86_REG_EAX)
    native.write_words(uc, group + 0x144, resolved_id)
    selected = []

    def boundary(uc, address, size, context):
        stack = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7aea80:
            material.return_value(uc, group)
            uc.reg_write(UC_X86_REG_ESP, stack + 12)
        elif address == 0x431f30:
            material.return_value(uc, resolved_id)
        elif address == 0x65c290:
            material.return_value(uc, types + 16 * 180)
            uc.reg_write(UC_X86_REG_ESP, stack + 8)
        elif address == 0x7d5120:
            interior = native.read_words(uc, stack + 4, 1)[0]
            native.write_words(uc, owner + 8, interior)
            material.return_value(uc, owner)
        elif address == 0x7d49b0:
            material.return_value(uc, factory)
        elif address == 0x8a1b00:
            material.return_value(uc, draw)
        elif address in (0x8a1fa0, 0x8a28f0):
            selected.append(native.read_words(uc, stack + 4, 1)[0])
            material.return_value(uc, 0)
        elif address == 0x4b5040:
            material.return_value(uc, 0)
        elif address in (0x407f80, 0x8a20c0):
            material.return_value(uc, 0)
            uc.reg_write(UC_X86_REG_ESP, stack + 8)

    uc.hook_add(UC_HOOK_CODE, boundary)
    native.invoke(uc, 0x793d20, [])
    assert len(selected) == 2 and selected[0] == selected[1]
    output = [selected[0], *native.read_words(uc, factory + 0x14, 4),
              native.read_words(uc, owner + 8, 1)[0]]
    return struct.pack('<11I', group_flags, root_flags, liquid_flags, liquid_id, *output, resolved_id)


def main():
    """Emit deterministic records for independent archive/runtime comparisons."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    records = [capture(*case) for case in itertools.product(
        (0, 8, 0x40, 0x48), (0, 8, 0x40, 0x48), (0, 0x200), (1, 2, 5, 17, 21, 22, 23))]
    Path(args.output).write_bytes(b''.join(records))
    print(f'captured {len(records)} WMO liquid material factories')


if __name__ == '__main__':
    main()
