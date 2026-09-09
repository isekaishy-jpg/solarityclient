"""Capture native mount-scale stores and model selection without instruction hooks."""

import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as n
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_ECX,
    UC_X86_REG_EBP, UC_X86_REG_ESP,
)


def capture(body_scale, object_scale, display_scale, model_scale, mounted):
    """Run the mount-load prefix, reciprocal block, and complete virtual getters."""
    uc = n.emulator()
    for vtable in (0xA326C8, 0xA34D90):
        assert n.read_words(uc, vtable + 0x7C, 1)[0] == 0x71C0E0
        assert n.read_words(uc, vtable + 0xD4, 1)[0] == 0x6E6F80
    unit, display, model, tables, output = [n.HEAP + value for value in (0, 0x2000, 0x2100, 0x3000, 0x4000)]
    n.write_words(uc, unit + 0x9C0, 100)
    n.write_words(uc, 0xAD34C8, 100)
    n.write_words(uc, 0xAD34C4, 100)
    n.write_words(uc, 0xAD34D8, tables)
    n.write_words(uc, tables, display)
    n.write_words(uc, display + 4, 200)
    n.write_floats(uc, display + 0x10, [display_scale])
    n.write_words(uc, 0xAD3510, 200)
    n.write_words(uc, 0xAD350C, 200)
    n.write_words(uc, 0xAD3520, tables + 4)
    n.write_words(uc, tables + 4, model)
    n.write_floats(uc, model + 0x10, [model_scale])
    uc.reg_write(UC_X86_REG_ESP, n.STACK + 0x18000)
    uc.reg_write(UC_X86_REG_ECX, unit)
    # Stop before resource creation; every DBC lookup and +990 store is original.
    uc.emu_start(0x73D5D0, 0x73D666)
    retained = n.read_floats(uc, unit + 0x990, 1)[0]
    uc.reg_write(UC_X86_REG_EBX, unit)
    uc.reg_write(UC_X86_REG_EBP, n.STACK + 0x17000)
    uc.emu_start(0x73D7DA, 0x73D7E9)
    reciprocal = n.read_floats(uc, uc.reg_read(UC_X86_REG_ESP), 1)[0]
    n.write_floats(uc, unit + 0xB3C, [body_scale])
    n.write_floats(uc, unit + 0x98, [object_scale, 1.0])
    n.write_words(uc, unit + 0x98C, model if mounted else 0)
    n.write_words(uc, unit + 0xB4, display)
    uc.reg_write(UC_X86_REG_ECX, unit)
    n.invoke(uc, 0x6E6F80, [])
    assert uc.reg_read(UC_X86_REG_EAX) == (model if mounted else display)
    n.invoke(uc, 0x71C0E0, [])
    uc.mem_write(n.STOP, b'\xd9\x1d' + struct.pack('<I', output))
    uc.emu_start(n.STOP, n.STOP + 6)
    return retained, reciprocal, n.read_floats(uc, output, 1)[0]


def main():
    """Write portable records consumed by the actual player residency regression."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    n.initialize(args.exe)
    records = []
    for display, model, display_id in [(1.6, 3.5, 102), (0.4, 1.25, 100)]:
        for object_scale in [0.75, 1.0, 1.3, 2.0]:
            for mounted in [False, True]:
                retained, reciprocal, scale = capture(0.5, object_scale, display, model, mounted)
                records.append(struct.pack('<I7f', display_id if mounted else 0, object_scale,
                                           display, model, retained, reciprocal,
                                           scale, capture(0.5, object_scale, display, model, False)[2]))
    Path(args.output).write_bytes(b'UMS12340' + struct.pack('<I', len(records)) + b''.join(records))
    print(f'captured {len(records)} native mount/player scale records')


if __name__ == '__main__':
    main()
