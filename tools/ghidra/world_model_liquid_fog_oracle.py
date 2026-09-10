"""Capture native WMO liquid fog independently of the interior lighting mode.

Runs factory 7D5120, its virtual setter 7D4F10, callback 7D4F40 and writer
834990 in the fingerprinted build-12340 image. Substitutes the allocator,
DayNight provider and unrelated scene-light append 81E400. The native private
interior light is initialized normally except for process-exit registration.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    u = n.emulator()
    provider, palette, sun, query = [n.HEAP + x for x in (0, 0x1000, 0x2000, 0x3000)]
    n.write_words(u, 0xce04a8, sun)
    n.write_floats(u, 0xcd7668, [100.])

    def boundary(uc, address, size, user):
        if address == 0x95d110:
            return_value(uc, provider)
            uc.reg_write(UC_X86_REG_ESP, uc.reg_read(UC_X86_REG_ESP) + 12)
        elif address == 0x7ecef0:
            return_value(uc, palette)
        elif address in (0x81e400, 0x40c8fa):
            return_value(uc, 0)

    u.hook_add(UC_HOOK_CODE, boundary)
    rows = [
        '# Native 7D5120 -> virtual setter 7D4F10 -> callback 7D4F40 -> 834990.',
        '# indoorFog interiorLight ordinaryBGRA indoorBGRA start end exponent; query start end reciprocal exponent RGB (hex words).',
    ]
    colors = [(0xff204060, 0xffc08020), (0xfff04020, 0xff1060b0),
              (0xff000000, 0xffffffff), (0x00112233, 0x778899aa)]
    for (ordinary, indoor), interior, parameters in itertools.product(
            colors, (0, 1), [(0., 1., 1.), (1., 5., 2.)]):
        n.invoke(u, 0x7d5120, [interior])
        vtable = n.read_words(u, provider, 1)[0]
        setter, callback = n.read_words(u, vtable + 8, 2)
        assert (callback, setter) == (0x7d4f40, 0x7d4f10)
        assert n.read_words(u, provider + 4, 2) == (0, interior)
        for offset, color in [(0x8c, ordinary), (0xa0, indoor)]:
            n.write_words(u, palette + offset, color)
            n.write_floats(u, palette + offset + 4, parameters)
        # Keep the same provider across repeated calls, including a bank reversal.
        for bank in (0, 1, 1, 0):
            u.reg_write(UC_X86_REG_ECX, provider)
            n.invoke(u, setter, [bank])
            u.reg_write(UC_X86_REG_ECX, provider)
            n.invoke(u, callback, [query])
            inputs = [bank, interior, ordinary, indoor,
                      *struct.unpack('<3I', struct.pack('<3f', *parameters))]
            rows.append(' '.join(f'{word:08x}' for word in inputs + list(n.read_words(u, query + 0xa8, 7))))
    return '\n'.join(rows) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    data = capture()
    args.output.write_text(data, encoding='utf-8')
    print(f'Captured {len(data.splitlines()) - 2} native WMO liquid fog queries')


if __name__ == '__main__':
    main()
