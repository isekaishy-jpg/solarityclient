"""Probe original model effect timestamp construction and elapsed-time arithmetic.

Executes 834810 and 828A00. Hooks replace scene registration, shared-data
reference/load work, and particle dispatch. The original timestamp stores,
unsigned subtraction, and millisecond conversion remain unmodified. Also checks
the independent global-sequence origin at model +0x74. This does
not exercise particle simulation, resource loading, or visibility admission.
"""
import argparse
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output, global_output=None):
    native.initialize(executable)
    uc = native.emulator()
    model, scene, resource, data = [native.HEAP + 0x1000 * i for i in range(4)]
    dispatched = []
    global_sample = False
    def dependencies(u, address, _size, _data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if global_sample and address == 0x4c1f00:
            # Global clock writes precede this camera-matrix operation. Stop
            # before geometry/bones; no interpolation or particle update runs.
            u.reg_write(UC_X86_REG_EIP, native.STOP)
            return
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
        assert native.read_words(uc, model + 0x74, 1)[0] == created
        for sample, ready in samples:
            previous = native.read_words(uc, model + 0x8c, 1)[0]
            native.write_words(uc, scene + 0xc, sample)
            native.write_words(uc, model + 0x10, ready)
            dispatched.clear()
            uc.reg_write(UC_X86_REG_ECX, model)
            invoke(uc, 0x828a00, [])
            assert native.read_words(uc, model + 0x74, 1)[0] == created
            updated = native.read_words(uc, model + 0x8c, 1)[0]
            bits = f'{dispatched[0]:08x}' if dispatched else '-'
            rows.append(f'{created} {previous} {sample} {ready} {updated} {bits}')
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} native model effect updates')
    if global_output is not None:
        global_sample = True
        durations, phases = native.HEAP + 0x4000, native.HEAP + 0x4100
        globals = ['# 834810 construction -> 82F0F0 unsigned global clocks: creation scene duration phase']
        for created in [0, 5000, 20_000, 0x1000001, 0xfffffff0]:
            uc.mem_write(model, bytes(0x400))
            native.write_words(uc, scene + 0xc, created)
            uc.reg_write(UC_X86_REG_ECX, model)
            invoke(uc, 0x834810, [scene, resource, 0, 0])
            assert native.read_words(uc, model + 0x74, 1)[0] == created
            native.write_words(uc, model + 0x10, 1)
            native.write_words(uc, model + 0x70, phases)
            native.write_words(uc, scene + 0x14, 1)
            native.write_words(uc, data + 0x10, 4, 1, durations)
            for elapsed in [0, 1, 16, 33, 667, 0x1000001, 0x7fffffff, 0xffffffff]:
                now = (created + elapsed) & 0xffffffff
                native.write_words(uc, scene + 0xc, now)
                for duration in [0, 1, 667, 1500, 3333, 600_000]:
                    native.write_words(uc, durations, duration)
                    native.write_words(uc, phases, 0xdeadbeef)
                    uc.reg_write(UC_X86_REG_ECX, model)
                    invoke(uc, 0x82f0f0, [0, 0, 0, 0, 0])
                    phase = native.read_words(uc, phases, 1)[0]
                    assert phase != 0xdeadbeef
                    globals.append(f'{created} {now} {duration} {phase}')
        Path(global_output).write_text('\n'.join(globals) + '\n', encoding='utf-8')
        print(f'Captured {len(globals)-1} native global-sequence phases')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--global-output')
    args = parser.parse_args()
    capture(args.executable, args.output, args.global_output)
