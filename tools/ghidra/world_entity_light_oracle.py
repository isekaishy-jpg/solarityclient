"""Capture native 7A1E90 entity light transitions and 7C1730 callback outputs."""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture(mode, current, target, diffuse, intensity, target_intensity, dt, frames):
    u = n.emulator()
    entity, palette, sun, query = [n.HEAP + x for x in (0, 0x400, 0x800, 0xc00)]
    n.write_words(u, entity + 0x20, 2)
    n.write_words(u, entity + 0xc, 4 if mode == 0 else 2)
    n.write_words(u, entity + 0x7c, (0 if mode == 0 else 1) | (0x1000 if mode == 2 else 0))
    n.write_words(u, entity + 0x84, current, diffuse)
    n.write_floats(u, entity + 0x8c, [intensity])
    n.write_words(u, entity + 0xc0, target)
    n.write_floats(u, entity + 0xc4, [target_intensity])
    n.write_words(u, palette + 0x1ac, 0xff806040)
    n.write_floats(u, palette + 0x19c, [-.6, -.6, -.3])
    n.write_words(u, 0xce04a8, sun)
    n.write_floats(u, sun + 0x7c, [-.5, -.5, -.70710677])
    n.write_floats(u, sun + 0x94, [.25, .5, .75])
    n.write_floats(u, 0xcd7668, [100.])
    n.write_floats(u, 0xcd76a0, [dt])
    u.hook_add(UC_HOOK_CODE, lambda u, address, size, _: return_value(u, palette) if address == 0x7ecef0 else None)
    for _ in range(frames):
        u.reg_write(UC_X86_REG_ECX, entity)
        n.invoke(u, 0x7a1e90, [])
    u.reg_write(UC_X86_REG_ECX, entity)
    n.invoke(u, 0x7c1730, [query])
    # Accumulator ambient, last ray/diffuse are the callback's raw input to SetupSunlight.
    return struct.pack('<4I3fI', mode, current, target, diffuse, intensity, target_intensity, dt, frames) + bytes(u.mem_read(entity + 0x84, 12)) + bytes(u.mem_read(entity + 0xc0, 4)) + b''.join(bytes(u.mem_read(query + offset, 12)) for offset in (0x54, 0x60, 0x78))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable'); p.add_argument('output', type=Path)
    args = p.parse_args(); n.initialize(args.executable)
    rows = []
    for mode in range(3):
        for current, target, diffuse in [(0xff000000, 0xffffffff, 0x80406080), (0xff806040, 0xff103090, 0xff806040), (0xff806040, 0xff806040, 0x00102030), (0x33102030, 0xaa403020, 0x01102030)]:
            for dt in (0., 1./1200., .016, .1):
                for frames in (1, 2, 60):
                    for intensity, target_intensity in ((1., .5), (.25, 1.), (1., 2.5)):
                        rows.append(capture(mode, current, target, diffuse, intensity, target_intensity, dt, frames))
    args.output.write_bytes(b''.join(rows))
    print(f'{len(rows)} native entity transitions and directional callbacks')


if __name__ == '__main__': main()
