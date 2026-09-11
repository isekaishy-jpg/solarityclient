"""Capture the original global filter selection and file-texture override.

Runs 4B61C0/4B6230 and the unchanged 4B9760 flag-selection prefix
(4B97A5..4B97E7). Only device-capability lookup is intercepted. The prefix
ends before filename canonicalization, cache lookup, allocation or GPU work.
The six filter/anisotropy requests come from the original 4048F0 tables.
"""

import argparse
import itertools
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EIP, UC_X86_REG_ESP,
)

import wmo_registration_oracle as native


def capture(executable):
    native.initialize(executable)
    uc = native.emulator()
    capability = native.HEAP

    def device_query(uc, address, size, data):
        if address == 0x532AF0:
            sp = uc.reg_read(UC_X86_REG_ESP)
            uc.reg_write(UC_X86_REG_EAX, capability)
            uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, sp, 1)[0])
            uc.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, device_query)
    filters = native.read_words(uc, 0xAB6128, 6)
    anisotropies = native.read_words(uc, 0xAB6140, 6)
    rows = ['# mode trilinear_supported anisotropy_supported maximum explicit_filter requested_flags ; effective_filter anisotropy texture_flags']
    for mode, trilinear, anisotropic, maximum, explicit, flags in itertools.product(
            range(6), range(2), range(2), [1, 2, 8, 16], range(2), [1, 9, 25, 3, 27]):
        native.write_words(uc, capability + 0xE4, trilinear, anisotropic, maximum)
        native.write_words(uc, 0xAC3298, 3, 1)
        native.invoke(uc, 0x4B61C0, [filters[mode]])
        native.invoke(uc, 0x4B6230, [anisotropies[mode]])
        filtering, anisotropy = native.read_words(uc, 0xAC3298, 2)
        frame = native.STACK + 0x10000
        native.write_words(uc, frame + 12, flags, 0, explicit)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x200)
        uc.emu_start(0x4B97A5, 0x4B97E7, count=100)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x4B97E7
        selected = uc.reg_read(UC_X86_REG_EAX)
        rows.append(f'case {mode} {trilinear} {anisotropic} {maximum} {explicit} {flags} ; {filtering} {anisotropy} {selected}')
    assert len(rows) == 961
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.executable))
    print('Captured 960 original global-filter and texture-selection cases')
