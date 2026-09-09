"""Capture original outdoor M2 depth insertion without instruction hooks.

Executes 795400's camera-plane instruction range and complete 792E60/790650/
7B5020 with initialized intrusive lists. This establishes depth admission only;
WMO membership and exterior portal traversal remain separate scene decisions.
"""

import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP


def encoded(values):
    """Serialize exact native float stores."""
    words = struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values))
    return ' '.join(f'{word:08x}' for word in words)


def capture(executable, output):
    """Cover all view octants, vertical views and both sides of bucket edges."""
    native.initialize(executable)
    rng = random.Random(79260)
    cases = []
    for direction in [[1., 0., 0.], [-1., 0., 0.], [0., 1., 1.], [0., 0., 1.], [0., 0., -1.], [1.e-6, 0., 1.]]:
        for distance in [-3000., -1., 0., 1., 1000., 2100., 2133.333, 2133.334, 3000.]:
            cases.append(([0., 0., 0.], direction, [distance, distance, -1.], [distance + 1., distance + 1., 2.]))
    for bucket in range(65):
        for shift in [-.0002, 0., .0002]:
            distance = bucket / .03 + shift
            cases.append(([0., 0., 0.], [1., 0., 0.], [distance, -1., -1.], [distance + 2., 1., 1.]))
    for _ in range(240):
        eye = [rng.uniform(-17000., 17000.) for _ in range(3)]
        target = [value + rng.uniform(-30., 30.) for value in eye]
        low = [value + rng.uniform(-3000., 3000.) for value in eye]
        high = [value + rng.uniform(0., 50.) for value in low]
        cases.append((eye, target, low, high))
    lines = ['# build 12340 camera slice 7954A6..795644 and complete 792E60; no hooks', '# eye3 target3 minimum3 maximum3 (hex float words); depth bucket (-1 = unvisited)']
    for eye, target, low, high in cases:
        uc = native.emulator()
        camera, look, model = native.HEAP, native.HEAP + 0x100, native.HEAP + 0x1000
        native.write_floats(uc, camera, eye)
        native.write_floats(uc, look, target)
        frame = native.STACK + 0x18000
        native.write_words(uc, frame + 8, camera, look)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
        uc.emu_start(0x7954a6, 0x795644, timeout=1_000_000, count=100_000)
        native.write_floats(uc, 0xadf454, [.5])
        native.write_floats(uc, model + 0x48, low + high)
        for bucket in range(64):
            head = 0xcd9060 + bucket * 0x6c
            native.write_words(uc, head, 8, head + 4, (head + 4) | 1)
        native.invoke(uc, 0x792e60, [model])
        link = native.read_words(uc, model + 8, 1)[0]
        bucket = (link - 0xcd9064) // 0x6c if link else -1
        assert bucket == -1 or 0 <= bucket < 64
        lines.append(encoded(eye + target + low + high) + f' {bucket}')
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('Captured', len(cases), 'original scene depth insertions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
