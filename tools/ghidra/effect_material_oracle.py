"""Capture build-12340 common and ribbon material state at draw submission.

Runs unmodified 81FE90..820071 and 980D31..980E25, including the original
GX dirty-state and alpha-reference helpers. Dirty records are preallocated.
The ribbon input is the decoded material's runtime flag/operation record;
texture binding, GPU submission and the model constructor are outside capture.
"""
import argparse
import itertools
from pathlib import Path

import wmo_registration_oracle as n
from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EDI,
    UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_EIP,
)


def state_words(u, state):
    return [n.read_words(u, state + offset, 1)[0] for offset in (0x90, 0x168, 0x138, 0x198)] + list(n.read_words(u, 0xd43064, 1))


def capture(executable):
    n.initialize(executable)
    rows = ['# flags blend pass alphaBits -> common(gxBlend depthWrite depthTest cull alphaRefBits) ribbon(same); build12340 original submission blocks']
    for flags, blend, outer, alpha in itertools.product(
            (0, 4, 8, 16, 28), range(7), range(3), (0., 0.5, 1.)):
        u = n.emulator()
        scene, element, material, gx, state, ribbon, passes = [
            n.HEAP + i for i in (0, 0x1000, 0x2000, 0x4000, 0x8000, 0x9000, 0xa000)]
        n.write_words(u, 0xc5df88, gx)
        n.write_words(u, 0xd43008, 1)
        n.write_words(u, gx + 0xf58, 1)
        n.write_words(u, gx + 0x28f4, state)
        for slot in range(128):
            n.write_words(u, state + slot * 24 + 20, 1)
        n.write_words(u, scene + 0x4c, outer, element)
        n.write_words(u, scene + 0x58, 4, 0)
        n.write_words(u, scene + 0x98, material)
        n.write_words(u, material, flags | (blend << 16))
        n.write_floats(u, element + 12, [alpha])
        u.reg_write(UC_X86_REG_ESP, n.STACK + 0x18000)
        u.reg_write(UC_X86_REG_ECX, scene)
        u.emu_start(0x81fe90, 0x820071, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == 0x820071
        common = state_words(u, state)

        runtime_flags = (~flags & 3) | (~(flags >> 1) & 12) | (~(flags << 2) & 16)
        gx_blend = n.read_words(u, 0xa45570 + blend * 4, 1)[0]
        n.write_words(u, ribbon + 0x11c, passes)
        n.write_words(u, passes, runtime_flags, gx_blend)
        u.reg_write(UC_X86_REG_ESP, n.STACK + 0x18000)
        u.reg_write(UC_X86_REG_EBP, n.STACK + 0x17000)
        u.reg_write(UC_X86_REG_ESI, ribbon)
        u.reg_write(UC_X86_REG_EDI, 0)
        u.emu_start(0x980d31, 0x980e25, count=100_000)
        assert u.reg_read(UC_X86_REG_EIP) == 0x980e25
        after_ribbon = state_words(u, state)
        alpha_bits = n.read_words(u, element + 12, 1)[0]
        values = common + after_ribbon
        rows.append(f'{flags} {blend} {outer} {alpha_bits:08x} ' + ' '.join(f'{v:08x}' for v in values))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    result = capture(args.executable)
    Path(args.output).write_text(result, encoding='ascii')
    print(f'{len(result.splitlines()) - 1} native common/ribbon material cases')
