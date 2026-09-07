"""Capture native LightParams/band projection through all three depth callbacks.

Executes pinned 7EBFF0, 8A2BF0 and 8A2AC0. Only band providers are replaced
with constant authored values; the original row indexing and image arithmetic
run unchanged. This never invokes the client entry point or OS providers.
"""

import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture(parameters, colors, floats):
    """Retain original projection and BGRA output for one complete authored sample."""
    uc = native.emulator()
    environment = native.HEAP
    row, scalar = environment + 0x400, environment + 0x500
    pitch, output = environment + 0x600, environment + 0x604
    uc.mem_write(row, parameters)
    # A float-band provider returns ST0; FLD/RET preserves the native x87 ABI.
    uc.mem_write(0x7ebf90, b'\xd9\x05' + struct.pack('<I', scalar) + b'\xc3')

    def provider(uc, address, size, context):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7ebf30:
            destination, time, owner, column = native.read_words(uc, sp + 4, 4)
            assert owner == row
            native.write_words(uc, destination, colors[column])
            return_value(uc, destination)
        elif address == 0x7ebf90:
            time, owner, column = native.read_words(uc, sp + 4, 3)
            assert owner == row
            native.write_floats(uc, scalar, [floats[column]])
        elif address == 0x7ecef0:
            return_value(uc, environment)

    uc.hook_add(UC_HOOK_CODE, provider)
    native.invoke(uc, 0x7ebff0, [720, environment + 0xd4, row])
    images = []
    for kind in range(3):
        native.invoke(uc, 0x8a2ac0 if kind == 2 else 0x8a2bf0,
                      [1, 8, 64, 0, 0, kind, pitch, output])
        assert native.read_words(uc, pitch, 1)[0] == 32
        images.append(bytes(uc.mem_read(native.read_words(uc, output, 1)[0], 2048)))
    return parameters + struct.pack('<18I6f', *colors, *floats) + b''.join(images)


def main():
    """Write distinct alpha/color banks that expose shifted DBC columns and bank swaps."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    cases = []
    for glow, alphas, water in [
        (.75, [.3, .8, .1, .6], [0x112233, 0x88aacc, 0x446699, 0xbb7733]),
        (.2, [.25, 1., .5, .75], [0xff0080, 0x0080ff, 0x00ff00, 0xff0000]),
        (.9, [0., 1., 1., 0.], [0, 0xffffff, 0xffffff, 0]),
    ]:
        parameters = struct.pack('<4I5f', 1, 1, 0, 0x20, glow, *alphas)
        colors = [0x111111 * (i % 10) for i in range(14)] + water
        cases.append(capture(parameters, colors, [3600., .5, .25, .75, 1.25, 1.5]))
    Path(args.output).write_bytes(b''.join(cases))
    print(f'{len(cases)} original environment projections and {len(cases) * 3} depth images')


if __name__ == '__main__':
    main()
