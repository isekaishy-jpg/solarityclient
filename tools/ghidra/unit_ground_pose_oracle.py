"""Capture original unit surface smoothing and M2 tilt/sequence blending.

Executes pinned 7197D0 up to model dispatch and complete 82DD80 with synthetic
model, primary timer, and sequence storage. No instruction hooks or replacement
arithmetic are used. These fixtures do not establish live scene admission.
"""

import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_ESP


def words(values):
    """Encode exact float storage for native arguments and portable fixtures."""
    return struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values))


def hex_words(values):
    """Serialize stored words without decimal float conversion."""
    return ' '.join(f'{word:08x}' for word in values)


def smoothing(output):
    """Capture valid retained normals, threshold neighbors, and varied intervals."""
    rng = random.Random(7197)
    threshold = struct.unpack('<f', struct.pack('<I', 0x3eb6e48a))[0]
    cases = []
    for z in [-1., 0., threshold - 0.00000003, threshold, threshold + 0.00000003, .8, 1.]:
        for dt in [0., .0001, .001, .016, .1, 1., 10., 1000.]:
            cases.append(([0., 0., 1.], [.3, -.4, z], dt))
    for _ in range(160):
        old = [rng.uniform(-1., 1.), rng.uniform(-1., 1.), rng.uniform(.36, 1.)]
        target = [rng.uniform(-1., 1.), rng.uniform(-1., 1.), rng.uniform(0., 1.)]
        cases.append((old, target, rng.uniform(0., 1.)))
    lines = ['# pinned build 12340 7197D0 through 71986E, no hooks', '# old3 target3 delta_seconds result3, float bits hex']
    for old, target, dt in cases:
        uc = native.emulator()
        unit, movement = native.HEAP, native.HEAP + 0x2000
        native.write_words(uc, unit + 0xd8, movement)
        native.write_floats(uc, unit + 0x9d8, old)
        native.write_floats(uc, movement + 0x38, target)
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, *words([dt]))
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.reg_write(UC_X86_REG_ECX, unit)
        uc.emu_start(0x7197d0, 0x71986e, timeout=1_000_000, count=100_000)
        lines.append(hex_words(words([*old, *target, dt])) + ' ' + hex_words(native.read_words(uc, unit + 0x9d8, 3)))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(cases), 'native smoothing samples')


def pose(output):
    """Execute the actual sequence lookup, timer sampling, tilt, blend, and scale."""
    rng = random.Random(82)
    cases = []
    for model_flags in range(4):
        for sequence_flags in range(0, 16, 2):
            for tick in [99, 100, 101, 250, 350, 600, 1100, 2000]:
                cases.append(([.3, -.4, .8], [4., 5., 6.], .7, 1.5, model_flags, sequence_flags, 1., 0, 100, 1100, tick))
    for _ in range(240):
        normal = [rng.uniform(-1., 1.), rng.uniform(-1., 1.), rng.uniform(.36, 1.)]
        position = [rng.uniform(-17000., 17000.) for _ in range(3)]
        start = rng.choice([0, 100, 0xfffffff0])
        duration = rng.choice([0, 1, 2, 999, 1000, 8000])
        now = (start + rng.randrange(-100, 10000)) & 0xffffffff
        cases.append((normal, position, rng.uniform(0., 6.3), rng.uniform(.1, 4.), rng.randrange(4), rng.choice([0, 2, 4, 8]), rng.choice([0., -.5, .25, .7, 1., 1.3, 2.]), rng.randrange(200), start, (start + duration) & 0xffffffff, now))
    for yaw in [0., -0., 1.5707963705062866, 3.1415927410125732]:
        for flags in range(4):
            cases.append(([0., 0., 1.], [0., 0., 0.], yaw, 1., flags, 0, 1., 0, 0, 1000, 300))
    for flags in [2, 4]:
        for speed in [0., .5, 1., -.5]:
            cases.append(([.3, -.4, .8], [4., 5., 6.], .7, 1.5, 0, flags, speed, 0, 0, 0, 0))
    lines = ['# pinned build 12340 82DD80 including 8266B0/82CED0/824A80, no hooks', '# normal3 position3 yaw scale (hex); model_flags sequence_flags (decimal); speed(hex); offset start end now(decimal); weight and matrix16(hex)']
    for normal, position, yaw, scale, model_flags, sequence_flags, speed, offset, start, end, now in cases:
        uc = native.emulator()
        model = native.HEAP
        point, normal_ptr, shared, header, sequence, lookup, bone, scene = [model + delta for delta in [0x1000, 0x1010, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000]]
        native.write_words(uc, model + 0x10, 1)
        native.write_words(uc, model + 0x28, scene, shared)
        native.write_words(uc, shared + 8, 1)
        native.write_words(uc, shared + 0x150, header)
        native.write_words(uc, header + 0x1c, 1, sequence, 1, lookup, 1)
        native.write_words(uc, model + 0x94, bone)
        native.write_words(uc, sequence, 0, 1000, 0, sequence_flags)
        native.write_words(uc, bone + 0x90, 0, 0)
        native.write_words(uc, bone + 0x48, 0, start, end)
        native.write_floats(uc, bone + 0x54, [speed])
        native.write_words(uc, bone + 0x5c, offset)
        native.write_words(uc, scene + 0xc, now)
        native.write_floats(uc, point, position)
        native.write_floats(uc, normal_ptr, normal)
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, point, *words([yaw, scale]), normal_ptr, model_flags)
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.reg_write(UC_X86_REG_ECX, model)
        uc.emu_start(0x82dd80, 0x82e095, timeout=1_000_000, count=100_000)
        weight = native.read_words(uc, uc.reg_read(UC_X86_REG_EBP) + 0xc, 1)
        uc.emu_start(0x82e095, native.STOP, timeout=1_000_000, count=100_000)
        line = hex_words(words([*normal, *position, yaw, scale]))
        line += f' {model_flags} {sequence_flags} ' + hex_words(words([speed])) + f' {offset} {start} {end} {now} '
        lines.append(line + hex_words([*weight, *native.read_words(uc, model + 0xb4, 16)]))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(cases), 'native sequence/placement samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('smoothing_output', type=Path)
    parser.add_argument('pose_output', type=Path)
    args = parser.parse_args()
    native.initialize(args.executable)
    smoothing(args.smoothing_output)
    pose(args.pose_output)
