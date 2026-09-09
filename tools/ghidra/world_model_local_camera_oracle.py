"""Capture 7A6E00's root-local camera points and forward plane without hooks.

The original 4C21B0 transforms and complete plane arithmetic execute from
7A6EB8 to 7A6FBD, including the short-direction branch at 7A70C4. Graphics state
setup precedes this range and does not participate in these outputs.
"""
import argparse
import itertools
import math
import random
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EBP, UC_X86_REG_ESP, UC_X86_REG_ESI
import wmo_registration_oracle as n
from world_model_portal_projection_oracle import IDENTITY, words


def capture(u, inverse, eye, target):
    matrix, camera, look = [n.HEAP + i * 0x1000 for i in range(3)]
    n.write_floats(u, matrix, inverse)
    n.write_floats(u, camera, eye)
    n.write_floats(u, look, target)
    frame = n.STACK + 0x18000
    n.write_words(u, frame + 0xc, matrix, camera, look)
    u.reg_write(UC_X86_REG_EBP, frame)
    u.reg_write(UC_X86_REG_ESP, frame - 0x100)
    u.reg_write(UC_X86_REG_ESI, camera)
    u.emu_start(0x7a6eb8, 0x7a6fbd, timeout=1_000_000, count=100_000)
    return n.read_floats(u, 0xd1c42c, 10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    rows = ['# inverse16 eye3 target3; native localEye3 localTarget3 forwardPlane4; hex float stores']
    rng = random.Random(0x7a6e00)
    matrices = [IDENTITY, [*IDENTITY[:12], 100., -200., 50., 1.]]
    for angle, scale in itertools.product([.37, -1.13], [.73, 2.]):
        c, s = math.cos(angle) * scale, math.sin(angle) * scale
        matrices.append([c, s, 0., 0., -s, c, 0., 0., 0., 0., scale, 0., 500., -200., 70., 1.])
    for matrix, eye, delta in itertools.product(matrices,
        [[0., 0., 0.], [15000., -14000., 2500.]],
        [[0., 0., 0.], [.0099999, 0., 0.], [.01, 0., 0.], [.0100001, 0., 0.], [.317, -.481, .817], [0., 0., 1.]]):
        target = [eye[i] + delta[i] for i in range(3)]
        rows.append(words(matrix + eye + target + capture(u, matrix, eye, target)))
    for _ in range(240):
        matrix = rng.choice(matrices)
        eye = [rng.uniform(-17000., 17000.) for _ in range(3)]
        target = [value + rng.uniform(-3., 3.) for value in eye]
        rows.append(words(matrix + eye + target + capture(u, matrix, eye, target)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original local camera planes')


if __name__ == '__main__':
    main()
