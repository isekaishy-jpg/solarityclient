"""Capture original 7C8560 WMO vertex colors, including absent MOCV streams.

Only GPU buffer lock/unlock and backend identity are controlled providers.
Vertex packing and missing-color selection execute the fingerprinted 12340 PE.
No client entry point or operating-system code executes.
"""
import argparse
import itertools
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX

import wmo_registration_oracle as native
from unit_water_effect_oracle import returned


def capture(executable, output):
    """Record both stock vertex formats and both backend channel orders."""
    native.initialize(executable)
    uc = native.emulator()
    group, owner, root, positions, normals, uv, colors, device, table, backend, gpu, packed = [
        native.HEAP + index * 0x1000 for index in range(12)
    ]
    lock, unlock = native.STOP + 0x100, native.STOP + 0x200
    native.write_words(uc, 0xc5df88, device)
    native.write_words(uc, device, table)
    native.write_words(uc, table + 0xd8, lock, unlock)
    native.write_words(uc, group + 0x18c, owner)
    native.write_words(uc, owner + 0x120, root)
    native.write_words(uc, group + 0xe8, positions, normals, uv)
    native.write_words(uc, group + 0x15c, 1)
    native.write_floats(uc, positions, [1., 2., 3.])
    native.write_floats(uc, normals, [0., 0., 1.])
    native.write_floats(uc, uv, [.25, .75])

    def provider(uc, address, size, context):
        """Supply the renderer-owned buffer and the selected backend identity."""
        if address == lock:
            returned(uc, packed, 4)
        elif address == unlock:
            returned(uc, 0, 8)
        elif address == 0x532af0:
            returned(uc, backend)

    uc.hook_add(UC_HOOK_CODE, provider)
    rows = []
    for flags, vertex_format, api, color in itertools.product(
            [0, 2, 5, 8, 10, 15], [4, 13], [0, 1], [None, 0xff102030, 0x40201005]):
        native.write_words(uc, root + 0x3c, flags)
        native.write_words(uc, backend + 0x14, api)
        native.write_words(uc, group + 0x108, 0 if color is None else colors)
        if color is not None:
            native.write_words(uc, colors, color)
        uc.reg_write(UC_X86_REG_ECX, group)
        native.invoke(uc, 0x7c8560, [gpu, vertex_format])
        result = native.read_words(uc, packed + 24, 1)[0]
        authored = 'none' if color is None else f'{color:08x}'
        rows.append(f'{flags:04x} {vertex_format} {api} {authored} {result:08x}')
    Path(output).write_text(
        '# original 7C8560; root_flags vertex_format backend authored_argb packed_color\n'
        + '\n'.join(rows) + '\n', encoding='ascii')
    print(f'captured {len(rows)} original WMO vertex colors')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
