"""Capture original build-12340 scalable font sizes and string translation.

Size arithmetic executes 0x006C22F0 with an explicitly supplied scalable
FreeType face. Only FreeType's size-activation boundary is replaced; no
rounding, clamping, baseline, or justification arithmetic is substituted.
Translation executes 0x006C6190 without hooks. The executable entry point
and operating-system imports never run.
"""
import argparse
from pathlib import Path
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    font, handle, face, size = [native.HEAP + offset for offset in (0, 0x1000, 0x2000, 0x3000)]
    native.write_words(uc, font + 0x70, handle)
    native.write_words(uc, handle + 0x24, face)
    native.write_words(uc, face + 0x58, size)
    uc.mem_write(face + 0x44, struct.pack('<Hhh', 2048, 1500, -500))

    def activate_size(uc, address, length, data):
        if address == 0x992780:
            stack = uc.reg_read(UC_X86_REG_ESP)
            uc.reg_write(UC_X86_REG_EAX, 0)
            uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, stack, 1)[0])
            uc.reg_write(UC_X86_REG_ESP, stack + 4)

    hook = uc.hook_add(UC_HOOK_CODE, activate_size)
    rows = ['# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
            '# size: requested pixels, raster pixels, ascender pixels (face 1500/-500)']
    native.write_words(uc, 0xc7d2c4, 768, 1365)
    for height in (.25, 1, 2, 12.49, 12.5, 18, 26, 32, 33, 62, 128):
        native.write_floats(uc, font + 0x184, [height / 768])
        uc.reg_write(UC_X86_REG_ECX, font)
        native.invoke(uc, 0x6c22f0, [])
        raster = native.read_words(uc, font + 0x24c, 1)[0]
        ascender = native.read_words(uc, font + 0x17c, 1)[0]
        rows.append(f'size {height} {raster} {ascender}')
    uc.hook_del(hook)
    string = native.HEAP + 0x4000
    rows.append('# origin: owner height, vertical justification (top=0), screen x/y; bottom=697, line=18')
    for height in (13., 18., 40.):
        for justification in range(3):
            native.write_floats(uc, string + 0x1c, [18 / 768, 100 / 1365, 697 / 768, 0.])
            native.write_floats(uc, string + 0x3c, [244 / 1365, height / 768])
            native.write_words(uc, string + 0x50, justification, 0)
            native.write_words(uc, string + 0xb0, 1)
            uc.reg_write(UC_X86_REG_ECX, string)
            native.invoke(uc, 0x6c6190, [])
            x, y = native.read_floats(uc, string + 0x70, 2)
            rows.append(f'origin {height} {justification} {x} {y}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    arguments = parser.parse_args()
    capture(arguments.executable, arguments.output)
