"""Probe native model-default selection through 834540 and 832AB0.

Executes original scene binding, fallback, weighted selection and timer creation.
Only old-scene removal is suppressed and CRT rand is provided by its VC2005 LCG.
832AB0 arguments are observed without modifying execution. Synthetic resident
resources do not exercise asynchronous loading or queued gameplay requests.
"""
import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
)


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    model, scene, resource, data, seq, bones, table, row = [
        native.HEAP + i * 0x1000 for i in range(8)
    ]
    state, rolls, requests = 1, 0, []

    def dependencies(u, address, _size, _context):
        nonlocal state, rolls
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x832ab0:
            requests.append(native.read_words(u, sp + 4, 7))
            return
        if address == 0x88b867:
            state = (state * 214013 + 2531011) & 0xffffffff
            u.reg_write(UC_X86_REG_EAX, (state >> 16) & 0x7fff)
            rolls += 1
        elif address != 0x823d90:
            return
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    rows = ['# ids fallback flags bones metadataBase -> arguments; draws; primary timer words']
    cases = [
        ([0, 0, 7], 0, 0, 1, 0),
        ([0, 0, 7], 0, 0, 1, 5),
        ([7, 147], 0, 0, 1, 0),
        ([7], 0, 0, 1, 0),
        ([7], 7, 0x10, 1, 0),
        ([7], 7, 0x20, 1, 0),
        ([7], 7, 0x30, 1, 0),
        ([0], 0, 0, 0, 0),
    ]
    for ids, fallback, flags, bone_count, metadata_base in cases:
        for target in (model, scene, resource, data, seq, bones, table, row):
            uc.mem_write(target, bytes(0x1000))
        native.write_words(uc, model + 0x10, 1, 0xffff)
        native.write_words(uc, model + 0x2c, resource)
        native.write_words(uc, model + 0x94, bones)
        uc.mem_write(bones + 0x48, struct.pack('<H', 0xffff))
        uc.mem_write(bones + 0x6c, struct.pack('<H', 0xffff))
        native.write_words(uc, resource + 0x150, data)
        native.write_words(uc, data + 0x1c, len(ids), seq, 0, 0, bone_count)
        for index, animation in enumerate(ids):
            record = seq + index * 64
            uc.mem_write(record, struct.pack('<HH', animation, index + metadata_base))
            zero_weight = ids[:2] == [0, 0] and index == 0
            native.write_words(
                uc, record + 4, 1000, 0, 0x20, 0 if zero_weight else 32767, 1, 3, 100,
            )
            uc.mem_write(record + 0x3c, struct.pack('<HH', 1 if zero_weight else 0xffff, 0))
        native.write_words(uc, 0xad30d8, 0)
        native.write_words(uc, 0xad30d4, 0)
        native.write_words(uc, 0xad30e8, table)
        native.write_words(uc, table, row)
        native.write_words(uc, row + 0x10, flags, fallback)
        native.write_words(uc, scene + 0xc, 20000)
        state, rolls, requests = 1, 0, []
        uc.reg_write(UC_X86_REG_ECX, model)
        invoke(uc, 0x834540, [scene])
        timer = native.read_words(uc, bones + 0x48, 8)
        rows.append(
            f'{ids} {fallback} {flags} {bone_count} {metadata_base} -> '
            f'{requests}; {rolls}; {timer}'
        )
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} native default model sequences')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
