"""Capture build-12340 mount request admission at 739113..73917C.

The original instructions compare resident old/new request records and call
735820. Only that submission is intercepted. This covers the mount commit
gate, strict rate tolerance and submitted arguments; upstream behavior routing,
model resolution, timers and vehicle propagation are separate evidence.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    unit, model, frame = native.HEAP, native.HEAP + 0x4000, native.STACK + 0x10000
    submitted = None

    def dependencies(u, address, _size, _data):
        nonlocal submitted
        if address != 0x735820:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        submitted = native.read_words(u, sp + 4, 9)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 40)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    tolerance = native.read_words(uc, 0x9f1968, 1)[0]
    rows = [f'# native mount rate tolerance {tolerance:08x}',
            '# enabled present oldId newId oldRate newRate blend offset -> submitted (rates hex)']
    rates = [(bits(1.), bits(1.)), (0, tolerance - 1), (0, tolerance),
             (0, tolerance + 1), (tolerance, 0), (tolerance + 1, 0),
             (bits(1.), bits(1.0001)), (bits(1.), bits(1.5)),
             (bits(2.), bits(0.)), (bits(-1.), bits(1.))]
    for enabled in [0, 1]:
        for present in [0, 1]:
            for old_id, new_id in [(5, 5), (4, 5), (5, 0xffffffff)]:
                for old_rate, new_rate in rates:
                    for blend, offset in [(0, 0), (1, 333)]:
                        native.write_words(uc, unit + 0x98c, model if present else 0)
                        native.write_words(uc, frame - 0x70, enabled)
                        native.write_words(uc, frame - 0x130, old_id, 2, 777, old_rate)
                        native.write_words(uc, frame - 0x94, new_id, 0xffffffff, offset, new_rate)
                        native.write_words(uc, frame - 0x20, blend)
                        uc.reg_write(UC_X86_REG_EBP, frame)
                        uc.reg_write(UC_X86_REG_EBX, unit)
                        uc.reg_write(UC_X86_REG_ESP, frame - 0x1000)
                        submitted = None
                        uc.emu_start(0x739113, 0x73917c, count=10000)
                        assert uc.reg_read(UC_X86_REG_EIP) == 0x73917c
                        if submitted is not None:
                            assert submitted == (model, 0xffffffff, new_id, 0xffffffff,
                                                 offset, new_rate, blend, 1, 0), submitted
                        rows.append(f'{enabled} {present} {old_id} {new_id} '
                                    f'{old_rate:08x} {new_rate:08x} {blend} {offset} '
                                    f'{int(submitted is not None)}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 2} native mount submissions; tolerance '
          f'{struct.unpack("<f", struct.pack("<I", tolerance))[0]}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
