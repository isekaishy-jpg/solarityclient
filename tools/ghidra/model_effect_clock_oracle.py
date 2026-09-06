"""Probe original model effect timestamp construction and elapsed-time arithmetic.

Executes 834810 and 828A00. Hooks replace scene registration, shared-data
reference/load work, and particle dispatch. The original timestamp stores,
unsigned subtraction, and millisecond conversion remain unmodified. This does
not exercise particle simulation, resource loading, or visibility admission.
"""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    model, scene, resource, data = [native.HEAP + 0x1000 * i for i in range(4)]
    dispatched = []
    def dependencies(u, address, _size, _data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x834540:
            owner = u.reg_read(UC_X86_REG_ECX)
            native.write_words(u, owner + 0x28, native.read_words(u, sp + 4, 1)[0])
            pop = 4
        elif address == 0x835970:
            pop = 0
        elif address == 0x8359c0:
            pop = 4
        elif address == 0x8309c0:
            dispatched.append(native.read_words(u, sp + 4, 1)[0])
            pop = 8
        else:
            return
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, resource + 0x150, data)
    native.write_words(uc, data + 0x128, 1)
    rows = ['# creation previous scene ready -> updated deltaSecondsBits (- means no dispatch)']
    for created, samples in [(5000, [(5000, 1), (5016, 1), (6000, 0), (6200, 1)]),
                              (20000, [(20100, 1), (20200, 1)]),
                              (0xfffffff0, [(0x10, 1), (0x20, 1)])]:
        uc.mem_write(model, bytes(0x400))
        native.write_words(uc, scene + 0xc, created)
        uc.reg_write(UC_X86_REG_ECX, model)
        invoke(uc, 0x834810, [scene, resource, 0, 0])
        assert native.read_words(uc, model + 0x8c, 1)[0] == created
        for sample, ready in samples:
            previous = native.read_words(uc, model + 0x8c, 1)[0]
            native.write_words(uc, scene + 0xc, sample)
            native.write_words(uc, model + 0x10, ready)
            dispatched.clear()
            uc.reg_write(UC_X86_REG_ECX, model)
            invoke(uc, 0x828a00, [])
            updated = native.read_words(uc, model + 0x8c, 1)[0]
            bits = f'{dispatched[0]:08x}' if dispatched else '-'
            rows.append(f'{created} {previous} {sample} {ready} {updated} {bits}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} native model effect updates')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
