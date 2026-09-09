"""Execute original 834660 caster-material admission against controlled records.

The model is already loaded, its pose/material arrays are resident, and 831990's
update boundary is skipped. Only 823D50 queue insertion is intercepted; every
batch flag, layer, blend, alpha, and visible-geoset decision runs in the PE.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_ECX
import wmo_registration_oracle as native


def capture(executable):
    native.initialize(executable)
    rows = ['# m2_shadow batch_flags shader layer material_flags blend instance_alpha color_alpha texture_alpha visible queue']
    uc = native.emulator()
    model, shared, skin, batch, decoded, materials, visible, colors, weights, lookup = [native.HEAP + x for x in
        (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000, 0x9000)]
    native.write_words(uc, model + 0x10, 1)
    native.write_words(uc, model + 0x2c, shared)
    native.write_words(uc, shared + 0x170, skin)
    native.write_words(uc, shared + 0x150, decoded)
    native.write_words(uc, skin + 0x24, 1, batch)
    native.write_words(uc, model + 0x9c, visible, colors, 0, weights)
    native.write_words(uc, decoded + 0x48, 1)
    native.write_words(uc, decoded + 0x74, materials)
    native.write_words(uc, decoded + 0x94, lookup)
    uc.mem_write(batch + 0xe, struct.pack('<H', 1))
    queues = []

    def hook(machine, address, size, _):
        if address not in (0x831990, 0x823d50):
            return
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x823d50:
            queues.append(machine.reg_read(UC_X86_REG_ECX))
        machine.reg_write(UC_X86_REG_EIP, native.read_words(machine, sp, 1)[0])
        machine.reg_write(UC_X86_REG_ESP, sp + (12 if address == 0x823d50 else 4))

    uc.hook_add(UC_HOOK_CODE, hook)
    cases = list(itertools.product([0, 4], [0, 0x8000], [0, 1], [0, 0x40, 0x80, 0xc0], range(7)))
    for flags, shader, layer, material_flags, blend in cases:
        for instance_alpha, color_alpha, texture_alpha, shown in [(1., 1., 1., 1),
                (0.55, 1., 1., 1), (0.549, 1., 1., 1), (0.9, 0.6, 1., 1),
                (0.9, 1., 0.6, 1), (1., 1., 1., 0)]:
            uc.mem_write(batch, struct.pack('<BBH', flags, 0, shader))
            uc.mem_write(batch + 0xc, struct.pack('<H', layer))
            uc.mem_write(materials, struct.pack('<HH', material_flags, blend))
            native.write_words(uc, visible, shown)
            native.write_floats(uc, model + 0x19c, [instance_alpha])
            native.write_floats(uc, colors + 0x1c, [color_alpha])
            native.write_floats(uc, weights + 8, [texture_alpha])
            queues.clear()
            uc.reg_write(UC_X86_REG_ECX, model)
            native.invoke(uc, 0x834660, [1, 2])
            assert len(queues) <= 1
            rows.append('m2_shadow ' + ' '.join(map(str, [flags, shader, layer, material_flags, blend,
                instance_alpha, color_alpha, texture_alpha, shown, queues[0] if queues else 0])))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(capture(args.executable))
