"""Capture original sound gain/pitch selection, including x87 stores and clamps.

Runs 4C6FEE..4C7062 and the real 4C5D40 / 982310 / 879710 / 878760 callees.
Only the random-word provider is replaced, to provide reproducible inputs.
"""
import argparse
from pathlib import Path
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_EBP, UC_X86_REG_ESI, UC_X86_REG_EDI, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    entry, options, handle, voice = [native.HEAP + offset for offset in (0, 0x1000, 0x2000, 0x3000)]
    native.write_words(uc, handle, voice)
    native.write_words(uc, voice + 0x14, 0)
    uc.mem_write(native.STOP + 32, b'\xdb\xe3')  # fninit, no fixture arithmetic
    words = []
    draws = []

    def random_word(u, address, _size, _data):
        if address != 0x464580:
            return
        value = words[len(draws)]
        draws.append(value)
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, random_word)
    bits = lambda value: struct.unpack('<I', struct.pack('<f', value))[0]
    rows = ['# flags volume-bits multiplier-bits random0 random1 -> gain-bits pitch-bits draws (hex)']
    for flags in (0, 0x400, 0x800, 0xc00):
        for volume in (0., .1, .65, 1., 1.2):
            for multiplier in (.65, 1.):
                for word in (0, 0x7fffff, 0x800000, 0xffffffff, 0x1234567):
                    words[:] = [word, word ^ 0x5ac379]
                    draws.clear()
                    native.write_words(uc, entry + 0x3c, flags)
                    native.write_words(uc, entry + 0x34, bits(volume))
                    native.write_words(uc, options + 8, bits(multiplier))
                    native.write_words(uc, voice + 0x74, bits(1.))
                    uc.reg_write(UC_X86_REG_ESI, entry)
                    uc.reg_write(UC_X86_REG_EBX, options)
                    uc.reg_write(UC_X86_REG_EDI, handle)
                    uc.reg_write(UC_X86_REG_EBP, native.STACK + 0x10000)
                    uc.reg_write(UC_X86_REG_ESP, native.STACK + 0x8000)
                    uc.emu_start(native.STOP + 32, native.STOP + 34, count=1)
                    uc.emu_start(0x4c6fee, 0x4c7062, count=2000)
                    assert uc.reg_read(UC_X86_REG_EIP) == 0x4c7062
                    gain = native.read_words(uc, voice + 0x20, 1)[0]
                    pitch = native.read_words(uc, voice + 0x74, 1)[0]
                    rows.append(' '.join(f'{value:x}' for value in (flags, bits(volume), bits(multiplier), *words, gain, pitch, len(draws))))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original gain/pitch cases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
