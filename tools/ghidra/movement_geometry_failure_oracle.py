"""Execute original ground/fall loops with one controlled provider failure.

Inputs come from the checked original interval captures. All response, step,
trial, and clock code executes original instructions. Only 75F0A0 is replaced
at the chosen probe with its documented false return, leaving the preceding
complete candidates untouched. This isolates response to provider failure;
world residency and collection failure internals are outside this capture.
"""
import argparse
import math
from pathlib import Path
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_FPCW, UC_X86_REG_FPSW, UC_X86_REG_FPTAG


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def scalar(value):
    return struct.unpack('<f', struct.pack('<I', int(value, 16)))[0]


def setup(uc, origin, radius, height, triangles, player, flags):
    unit, owner, fields, kind, faces, identities = [native.HEAP + i * 0x1000 for i in range(6)]
    uc.mem_write(native.HEAP, bytes(0x10000))
    native.write_words(uc, unit + 0x144, owner)
    native.write_words(uc, owner + 0xd0, fields)
    native.write_words(uc, owner + 8, kind)
    native.write_words(uc, kind, 1)
    native.write_words(uc, kind + 8, 16 if player else 0)
    native.write_words(uc, unit + 0x28, kind)
    native.write_words(uc, unit + 0x44, flags)
    native.write_words(uc, unit + 0x60, 123)
    native.write_floats(uc, unit + 0x10, origin)
    native.write_floats(uc, unit + 0xc8, [radius, height])
    native.write_floats(uc, unit + 0x90, [2.5, 7, 4.5, 4.72, 2.5, 7, 4.5, math.pi, math.pi])
    native.write_floats(uc, 0xca1660, [-1e20]*3 + [1e20]*3)
    native.write_words(uc, 0xadba34, len(triangles), len(triangles), faces)
    native.write_words(uc, 0xadba4c, identities)
    for index, vertices in enumerate(triangles):
        a = [vertices[1][i] - vertices[0][i] for i in range(3)]
        b = [vertices[2][i] - vertices[0][i] for i in range(3)]
        normal = [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
        length = math.sqrt(sum(v*v for v in normal))
        normal = [scalar(f'{bits(v/length):08x}') for v in normal]
        offset = -sum(normal[i]*vertices[0][i] for i in range(3))
        native.write_floats(uc, faces + index*0x34, normal + [offset] + [v for p in vertices for v in p])
        native.write_words(uc, identities + index*8, index+1, 0)
    return unit


def run(uc, line, falling, failure):
    tokens = line.split()
    cursor = 1
    def floats(count):
        nonlocal cursor
        result = list(map(scalar, tokens[cursor:cursor+count]))
        cursor += count
        return result
    def integer():
        nonlocal cursor
        value = int(tokens[cursor]); cursor += 1
        return value
    origin = floats(3); radius, height = floats(2)
    if falling:
        delta = floats(3); launch = floats(1)[0]; clock = integer(); duration = integer()
        speed = floats(1)[0]; direction = floats(2); basis = floats(3)
        player, slow, far, live, moving = [integer() for _ in range(5)]
        launch_height = floats(1)[0]
        flags = 0x1000 | (0x2000 if far else 0) | (0x20000000 if slow else 0) | moving
    else:
        duration = integer(); distance = floats(1)[0]; heading = floats(2)
        player = integer(); step_height = floats(1)[0]
        step, blocked, slow = [integer() for _ in range(3)]
        direction = floats(2); basis = floats(3); speed = floats(1)[0]
        step_anchor = floats(1)[0]; clock = integer(); launch_height, launch = floats(2)
        flags = 1 | (0x4000000 if step else 0) | (0x200 if blocked else 0) | (0x20000000 if slow else 0)
    triangles = [[floats(3) for _ in range(3)] for _ in range(integer())]
    original_inputs = tokens[1:cursor]
    unit = setup(uc, origin, radius, height, triangles, player, flags)
    native.write_words(uc, unit + 0x80, clock)
    native.write_floats(uc, unit + 0x70, direction)
    native.write_floats(uc, unit + 0x64, basis)
    native.write_floats(uc, unit + 0x8c, [speed])
    native.write_floats(uc, unit + 0x84, [launch_height])
    native.write_floats(uc, unit + 0xb8, [launch])
    if falling:
        native.write_floats(uc, unit + 0x20, [.7, .2])
        native.write_floats(uc, unit + 0x4c, [12, 13, 14, .1, .2])
        entry, arguments = 0x7612b0, [10000, duration, *map(bits, delta), live]
    else:
        facing = math.atan2(heading[1], heading[0])
        native.write_floats(uc, unit + 0x20, [facing, 0])
        native.write_floats(uc, unit + 0x4c, [12, 13, 14, facing, 0])
        native.write_floats(uc, unit + 0x88, [step_anchor])
        native.write_floats(uc, unit + 0xd0, [step_height])
        native.write_floats(uc, native.HEAP + 0xf000, heading)
        entry, arguments = 0x7620f0, [10000, duration, bits(distance), native.HEAP + 0xf000]
    read = lambda address: native.read_words(uc, address, 1)[0]
    observed = dict(probes=0, contact=-1, delay=0, ceiling=0, trial_failure=False)
    def trace(uc, address, size, data):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x75f0a0:
            observed['probes'] += 1
            if observed['probes'] == failure:
                observed['trial_failure'] = not falling and bool(read(unit + 0x44) & 0x1000)
                uc.reg_write(UC_X86_REG_EAX, 0)
                uc.reg_write(UC_X86_REG_ESP, sp + 16)
                uc.reg_write(UC_X86_REG_EIP, read(sp))
        elif address == 0x6ec7b0:
            observed['contact'] = read(sp + 4) - 1
        elif address == 0x6e9b20:
            observed['delay'] = (observed['delay'] + read(sp + 4)) & 0xffffffff
        else:
            observed['ceiling'] = 1
    hooks = [uc.hook_add(UC_HOOK_CODE, trace, begin=a, end=a) for a in (0x75f0a0, 0x6ec7b0, 0x6e9b20, 0x6e9270)]
    uc.reg_write(UC_X86_REG_ECX, unit)
    uc.reg_write(UC_X86_REG_FPCW, 0x037f)
    uc.reg_write(UC_X86_REG_FPSW, 0)
    uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
    try:
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, *arguments)
        uc.reg_write(UC_X86_REG_ESP, sp)
        uc.emu_start(entry, native.STOP, count=3_000_000)
        assert uc.reg_read(UC_X86_REG_EIP) == native.STOP, (tokens[0], failure, hex(uc.reg_read(UC_X86_REG_EIP)))
    finally:
        for hook in hooks: uc.hook_del(hook)
    if failure and observed['probes'] < failure:
        return None, False
    words = lambda address, count: [f'{v:08x}' for v in native.read_words(uc, address, count)]
    flags = read(unit + 0x44)
    output = [str(uc.reg_read(UC_X86_REG_EAX)), *words(unit + 0x10, 3), str(read(unit + 0x80)), str(int(bool(flags & 0x1000)))]
    if falling:
        output += [str(int(bool(flags & 0x2000))), str(int(read(unit + 0x60) == 0))]
    else:
        output += [str(int(bool(flags & 0x4000000))), *words(unit + 0x88, 1), str(int(read(unit + 0x60) == 0)), str(observed['contact'])]
    output += words(unit + 0x84, 1) + words(unit + 0xb8, 1) + words(unit + 0x70, 2) + words(unit + 0x64, 3) + words(unit + 0x8c, 1)
    if falling:
        output += [str(observed['ceiling']), str(observed['contact'])]
    return ' '.join([f'fault:{tokens[0]}:{failure}', str(failure), str(observed['delay']), *original_inputs, *output]), observed['trial_failure']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--fixtures', type=Path, default=Path('crates/systems/tests/fixtures'))
    args = parser.parse_args()
    native.initialize(args.executable)
    for falling in (False, True):
        kind = 'fall' if falling else 'ground'
        lines = [v for v in (args.fixtures / f'movement-{kind}-advance-native.txt').read_text().splitlines() if v and not v.startswith('#')]
        if falling:
            lines = lines[::37]
        else:
            lines = [v for v in lines if v.split()[0].endswith(('_1_1_0_0_0', '_1_1_1_0_0')) and v.startswith(('wall_', 'corner_', 'slope_', 'step_0.9_', 'wedge_', 'random_1_'))]
        output = ['# Original build 12340 ground/fall response with one false return at 75F0A0.',
                  '# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
                  '# fault:name failure_probe deferred_ms | original interval input/output schema. No collection or live movement claim.']
        uc = native.emulator()
        trials = 0
        for line in lines:
            for failure in range(9):
                result, trial = run(uc, line, falling, failure)
                if result:
                    output.append(result)
                    trials += int(trial)
        (args.fixtures / f'movement-{kind}-geometry-native.txt').write_text('\n'.join(output) + '\n', encoding='utf-8')
        print(kind, len(output)-3, 'cases;', trials, 'failures inside private fall trials')


if __name__ == '__main__':
    main()
