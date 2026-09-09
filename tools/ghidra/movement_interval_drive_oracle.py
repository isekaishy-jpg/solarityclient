"""Capture unhooked 762E00 interval direction, speed and remaining distance.

The input delta has already been stored by the movement-owner wrapper. Mode
admission is supplied at its return boundary; this does not simulate geometry.
"""

import argparse
from pathlib import Path
import random

import wmo_registration_oracle as native
from path_ground_transition_oracle import encoded
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_EDI, UC_X86_REG_ESP


def capture(executable, output):
    """Exercise zero, tiny, large and diagonal inputs in both native mode lanes."""
    native.initialize(executable)
    rng = random.Random(76200)
    cases = []
    for delta in [[0.,0.,0.], [.0000001,0.,0.], [.000001,0.,0.], [1.,0.,0.], [0.,1.,0.], [0.,0.,1.], [3.,4.,5.]]:
        for duration in [1,16,250,1000]:
            for spatial in [0,1]:
                cases.append((delta,duration,duration,spatial))
    for _ in range(300):
        duration = rng.randrange(1,1001)
        cases.append(([rng.uniform(-100.,100.) for _ in range(3)], duration, rng.randrange(1,duration+1), rng.randrange(2)))
    lines = ['# original 762E76..762F05 and 762F30..762F6F; no instruction hooks', '# delta3 hex, duration remaining spatial decimal, speed direction3 distance hex']
    for delta, duration, remaining, spatial in cases:
        uc = native.emulator()
        frame = native.STACK + 0x18000
        native.write_floats(uc, frame + 0x10, delta)
        native.write_words(uc, frame + 8, 0)
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
        uc.reg_write(UC_X86_REG_EAX, spatial)
        uc.reg_write(UC_X86_REG_EBX, 0)
        uc.reg_write(UC_X86_REG_EDI, duration)
        uc.emu_start(0x762e76, 0x762f05, timeout=1_000_000, count=10000)
        speed = native.read_floats(uc, frame - 0xc, 1)
        direction = native.read_floats(uc, frame - 0x40, 3)
        native.write_words(uc, frame + 0x18, duration-remaining)
        uc.emu_start(0x762f30, 0x762f6f, timeout=1_000_000, count=10000)
        distance = native.read_floats(uc, frame - 4, 1)
        lines.append(encoded(delta) + f' {duration} {remaining} {spatial} ' + encoded(speed + direction + distance))
    output.write_text('\n'.join(lines)+'\n', encoding='utf-8')
    print('Captured', len(cases), 'original movement interval drives')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    capture(args.executable, args.output)
