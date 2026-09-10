"""Capture native WMO group fog selection through its actual virtual callback.

The fingerprinted image runs constructor 7B3DE0, virtual callback 7B3F30,
and query writer 834990. Only the current DayNight provider is substituted.
Both palette banks are explicit inputs with their shared final range/exponent;
this does not recapture portal traversal or the preceding 7F16F0 composition.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX

import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    """Retain unrelated flag bits and capture both colors over repeated queries."""
    u = n.emulator()
    group, palette, sun, query = [n.HEAP + x for x in (0, 0x1000, 0x2000, 0x3000)]
    u.reg_write(UC_X86_REG_ECX, group)
    n.invoke(u, 0x7b3de0, [])
    vtable = n.read_words(u, group, 1)[0]
    callback = n.read_words(u, vtable + 4, 1)[0]
    assert callback == 0x7b3f30
    n.write_words(u, 0xce04a8, sun)
    n.write_floats(u, 0xcd7668, [100.])

    def provider(uc, address, size, user):
        if address == 0x7ecef0:
            return_value(uc, palette)

    u.hook_add(UC_HOOK_CODE, provider)
    rows = [
        '# Native 7B3DE0 -> vtable+4 7B3F30 -> 834990; only DayNight provider supplied.',
        '# flags ordinaryBGRA indoorBGRA start end exponent; query start end reciprocal exponent RGB (all hex words).',
    ]
    colors = [(0xff204060, 0xffc08020), (0xfff04020, 0xff1060b0),
              (0xff000000, 0xffffffff), (0x00112233, 0x778899aa)]
    flags = [0, 0x8000, 1, 0x8001, 0xffff, 0x7fff, 0xffffffff, 0xffff7fff]
    for (ordinary, indoor), flag, parameters in itertools.product(
            colors, flags, [(0., 1., 1.), (1., 5., 2.)]):
        n.write_words(u, group + 0xc, flag)
        for offset, color in [(0x8c, ordinary), (0xa0, indoor)]:
            n.write_words(u, palette + offset, color)
            n.write_floats(u, palette + offset + 4, parameters)
        u.reg_write(UC_X86_REG_ECX, group)
        n.invoke(u, callback, [query])
        inputs = [flag, ordinary, indoor, *struct.unpack('<3I', struct.pack('<3f', *parameters))]
        rows.append(' '.join(f'{word:08x}' for word in inputs + list(n.read_words(u, query + 0xa8, 7))))
    return '\n'.join(rows) + '\n'


def main():
    """Write portable fixture words without storing proprietary image bytes."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    data = capture()
    args.output.write_text(data, encoding='utf-8')
    print(f'Captured {len(data.splitlines()) - 2} native WMO group fog queries')


if __name__ == '__main__':
    main()
