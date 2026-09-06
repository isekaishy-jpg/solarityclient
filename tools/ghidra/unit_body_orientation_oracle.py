"""Capture build-12340 body-yaw, spine/head, and procedural-turn transitions.

Runs the complete ordinary orientation block 73DD12..73E3D6, including the
original 719660 damped yaw and 7156C0 wrap helpers. M2 setters capture their
actual arguments; time and unrelated vehicle/spell/model getters are supplied
at boundaries. No replacement orientation or smoothing arithmetic is used.
"""
import argparse
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_EBP, UC_X86_REG_ESI,
    UC_X86_REG_EDX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
)


def capture(executable, output, timeline_output=None):
    native.initialize(executable)
    uc = native.emulator()
    unit, stats, guid, vtable, model, frame_state, virtual_zero, virtual_model = [native.HEAP + i * 0x3000 for i in range(8)]
    frame = native.STACK + 0x10000
    now = 1000
    masks, angles, matrices = {}, {}, {}
    axis_angle = 0
    requested = 0

    def dependencies(u, address, _size, _data):
        nonlocal axis_angle, requested
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x4c3460:
            axis_angle = native.read_words(u, sp + 8, 1)[0]
            return  # Execute the original matrix constructor too.
        if address == 0x8265e0:
            bone, flags, mask = native.read_words(u, sp + 4, 3)
            masks[bone] = (masks.get(bone, 0) & ~mask) | (flags & mask)
            value, pop = 0, 12
        elif address == 0x8272f0:
            bone, matrix = native.read_words(u, sp + 4, 2)
            angles[bone] = axis_angle
            matrices[bone] = native.read_words(u, matrix, 16)
            value, pop = 0, 8
        elif address == 0x86ae20:
            value, pop = now, 0
        elif address in [0x71bd20, virtual_zero]:
            value, pop = 0, 0
        elif address == 0x4d3790:
            u.reg_write(UC_X86_REG_EDX, 0)
            value, pop = 1, 0
        elif address == virtual_model:
            value, pop = model, 0  # The model getter leaves the bone argument.
        elif address == 0x8267e0:
            value, pop = 0, 4
        elif address == 0x73ac30:
            requested = 1
            value, pop = 0, 8
        else:
            return
        u.reg_write(UC_X86_REG_EAX, value)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)

    uc.hook_add(UC_HOOK_CODE, dependencies)
    native.write_words(uc, unit, vtable)
    native.write_words(uc, unit + 8, guid)
    native.write_words(uc, guid, 1, 0)
    native.write_words(uc, unit + 0xd0, stats, 0, unit + 0x788)
    native.write_words(uc, unit + 0xb4, model)
    native.write_words(uc, vtable + 0x138, virtual_zero)
    native.write_words(uc, vtable + 0xd4, virtual_model)
    native.write_words(uc, 0xb7436c, frame_state)
    uc.mem_write(native.STOP + 16, b'\xdb\xe3')  # FNINIT before each snapshot.
    rows = ['# flags capabilities direct controlled mounted alive camera13 elapsed body velocity mix facing dt turn_rate -> body velocity mix spine_enabled spine_angle head_enabled head_angle procedural resolver']
    cases = []
    for flags in [0, 1, 2, 4, 8, 5, 9, 6, 10, 0x10, 0x1000, 0x200001]:
        for body, velocity, mix, facing, elapsed in [(0., 0., 0., 0., 0), (-.4, 1.25, .3, .7, 16), (-3.1, -.7, 1., 3.13, 250), (2.1, 0., 1., 0., 1000)]:
            for dt in [.001, .016, .1]:
                for capabilities in [0, 0x80, 0x100, 0x180]:
                    for controlled in [0, 1]:
                        cases.append((flags, capabilities, 0, controlled, 0, 1, 0, elapsed, body, velocity, mix, facing, dt, 3.1415927410125732))
    for flags in [0, 1, 4, 5, 0x10, 0x2000001]:
        for direct, mounted, alive, camera13 in [(1, 0, 1, 0), (0, 1, 1, 0), (0, 0, 0, 0), (0, 0, 1, 1)]:
            cases.append((flags, 0x180, direct, 1, mounted, alive, camera13, 16, -.4, 1.25, .3, .7, .016, 3.1415927410125732))
    def run(case, retain=False):
        nonlocal masks, angles, matrices, requested
        flags, capabilities, direct, controlled, mounted, alive, camera13, elapsed, body, velocity, mix, facing, dt, turn_rate = case
        uc.emu_start(native.STOP + 16, native.STOP + 18, count=1)
        native.write_words(uc, unit + 0x7cc, flags, 0)
        native.write_words(uc, unit + 0xa38, 0x40 | capabilities | direct)
        native.write_words(uc, stats + 0x48, alive)
        native.write_words(uc, unit + 0x98c, mounted)
        native.write_words(uc, 0xca1238, controlled, 0)
        native.write_words(uc, 0xca11f4, 13 if camera13 else 0)
        native.write_words(uc, unit + 0xabc, now - elapsed)
        native.write_floats(uc, unit + 0xa94, [body, velocity, mix, facing])
        native.write_floats(uc, frame_state + 0xb14, [dt])
        native.write_floats(uc, unit + 0x834, [turn_rate])
        native.write_words(uc, frame - 4, 0)
        if not retain:
            masks, angles, matrices = {}, {}, {}
        requested = 0
        uc.reg_write(UC_X86_REG_ESI, unit)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x1000)
        uc.emu_start(0x73dd12, 0x73e3d6, count=100000)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x73e3d6
        state = native.read_words(uc, unit + 0xa94, 3)
        procedural = native.read_words(uc, unit + 0xa38, 1)[0] & 0x1800
        prefix = ' '.join(str(v) for v in case[:8]) + ' ' + ' '.join(f'{bits(v):08x}' for v in case[8:])
        output_words = [*state, masks.get(4, 0), angles.get(4, 0) if masks.get(4, 0) else 0,
                        masks.get(6, 0), angles.get(6, 0) if masks.get(6, 0) else 0, procedural, requested]
        return prefix + ' ' + ' '.join(f'{v:08x}' for v in output_words)
    for case in cases:
        rows.append(run(case))
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(cases)} original body-orientation transitions')
    if timeline_output:
        timeline = [rows[0]]
        for controlled in [0, 1]:
            for dt in [1/1200, 1/60, .1]:
                timeline.append('# reset')
                state = [0., 0., 0.]
                elapsed = 0
                previous_facing = 0.
                previous_direct = 0
                for step in range(280):
                    phase = step // 20
                    flags = [1, 5, 4, 6, 2, 10, 8, 9, 0, 0x10, 0, 0x1001, 0x200005, 0][phase]
                    facing = .03 * (step - 180) if phase == 9 else .6 if phase >= 10 else 0.
                    direct = int(phase == 9)
                    elapsed = 0 if facing != previous_facing or direct != previous_direct else elapsed + round(dt * 1000)
                    previous_facing, previous_direct = facing, direct
                    now = 100000  # Large enough for all deliberately long catch-up clocks.
                    row = run((flags, 0x180, direct, controlled, 0, 1, 0, elapsed,
                               *state, facing, dt, 3.1415927410125732), retain=step != 0)
                    timeline.append(row)
                    state = native.read_floats(uc, unit + 0xa94, 3)
        Path(timeline_output).write_text('\n'.join(timeline) + '\n', encoding='utf-8')
        print('Captured 1680 retained timeline frames')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--timeline-output')
    args = parser.parse_args()
    capture(args.executable, args.output, args.timeline_output)
