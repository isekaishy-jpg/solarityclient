"""Capture MCSH alpha from unchanged 7B87F0 texture expansion and edge copying."""
import argparse
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n


def capture(packed, format, preserve):
    u = n.emulator()
    texture, output, layers, shadow = [n.HEAP + i * 0x8000 for i in range(4)]
    if packed is not None:
        u.mem_write(shadow, packed)
    u.reg_write(UC_X86_REG_ECX, texture)
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, output, 0, 64, 64, layers, 0, shadow if packed is not None else 0, format, 0x8000 if preserve else 0)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x7b87f0, n.STOP, timeout=1_000_000, count=5_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP, hex(u.reg_read(UC_X86_REG_EIP))
    if format == 3:
        words = struct.unpack('<4096H', u.mem_read(output, 8192))
        return bytes(255 - (word >> 12) * 17 for word in words)
    return bytes(255 - value for value in bytes(u.mem_read(output, 16384))[3::4])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# Native 7B87F0: format preserve_edges packed_MCSH_or_none shadow_opacity_hex']
    patterns = [None, bytes(512), bytes([255])*512, bytes((i*31+17)&255 for i in range(512))]
    for packed in patterns:
        for format, preserve in [(3,False), (3,True), (2,True)]:
            result = capture(packed, format, preserve)
            rows.append(f'{format} {int(preserve)} ' + ('none' if packed is None else packed.hex()) + ' ' + result.hex())
    args.output.write_text('\n'.join(rows) + '\n')
    print(f'Captured {len(rows)-1} complete native shadow maps')
