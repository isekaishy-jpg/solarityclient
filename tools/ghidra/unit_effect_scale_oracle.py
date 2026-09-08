"""Capture original positioned/attached CEffect scale decisions without a client.

Runs complete 6F7950, 6F8AE0 and 6F8C50 routines with controlled model bounds,
attachment scale, readiness, and registry providers. Matrix submission and
attachment publication are observed/no-op boundaries, not renderer evidence.
Uses the same fingerprinted image and Unicorn loader as unit_water_effect_oracle.
"""
import argparse
import random
import struct
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as n
from unit_water_effect_oracle import Oracle, bits, f32, returned


class ScaleOracle(Oracle):
    def __init__(self):
        super().__init__()
        self.body, self.effect_model, self.row, self.matrix = [n.HEAP + i for i in (0x4100, 0x4300, 0x4500, 0x4700)]
        self.unit_scale_call, self.yaw_call, self.body_call = [n.HEAP + i for i in (0x4800, 0x4810, 0x4820)]
        self.scalar, self.bounds = n.HEAP + 0x4900, n.HEAP + 0x4a00
        self.scalar_stub = n.HEAP + 0x4b00
        self.uc.mem_write(self.scalar_stub, b'\xd9\x05' + struct.pack('<I', self.scalar) + b'\xc3')
        self.uc.mem_write(self.scalar_stub + 16, b'\xd9\x05' + struct.pack('<I', self.scalar + 4) + b'\xc2\x04\x00')
        self.uc.mem_write(self.yaw_call, b'\xd9\xee\xc3')
        n.write_words(self.uc, self.vtable + 0x34, self.yaw_call)
        n.write_words(self.uc, self.vtable + 0x7c, self.unit_scale_call)
        n.write_words(self.uc, self.vtable + 0xd4, self.body_call)
        n.write_words(self.uc, self.unit + 0xb4, self.body)
        n.write_words(self.uc, self.fields + 8, 8)
        n.write_words(self.uc, self.effect, self.effect_model)
        n.write_words(self.uc, self.effect + 0x10, 1, 0)
        n.write_words(self.uc, self.effect + 0x20, self.row, 17)
        n.write_words(self.uc, self.effect_model + 0x48, self.body, 0, 17)

    def hook(self, uc, address, size, unused):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address in (0x717e50,):
            uc.reg_write(UC_X86_REG_EIP, self.yaw_call)
        elif address == self.unit_scale_call:
            uc.reg_write(UC_X86_REG_EIP, self.scalar_stub)
        elif address == self.body_call:
            returned(uc, self.body)
        elif address == 0x831550:
            uc.reg_write(UC_X86_REG_EIP, self.scalar_stub + 16)
        elif address == 0x4f5e20:
            out = n.read_words(uc, sp + 4, 1)[0]
            uc.mem_write(out, bytes(uc.mem_read(self.bounds, 24)))
            returned(uc, out, 4)
        elif address == 0x6f7720:
            returned(uc, self.origin, 4)
        elif address == 0x4d4db0:
            returned(uc, self.unit)
        elif address == 0x717a20:
            returned(uc, self.model)
        elif address == 0x824f00:
            returned(uc, 1, 8)
        elif address == 0x8273d0:
            returned(uc, 1, 4)
        elif address == 0x4c1bf0:
            self.scale = n.read_words(uc, sp + 4, 1)[0]
            returned(uc, pop=4)
        elif address in (0x4c3380, 0x4d8630):
            returned(uc, pop=4)
        elif address in (0x407f40, 0x743320):
            returned(uc)
        elif address == 0x831330:
            out = n.read_words(uc, sp + 4, 1)[0]
            n.write_floats(uc, out, [0, 0, 0])
            returned(uc, out, 8)
        else:
            super().hook(uc, address, size, unused)

    def resolve(self, bounds, model_world, model_attached, unit_scale, anchor_scale, multiplier, minimum, maximum):
        n.write_floats(self.uc, self.bounds, bounds)
        n.write_floats(self.uc, self.model + 0x5c, [model_world, model_attached])
        n.write_floats(self.uc, self.scalar, [unit_scale, anchor_scale])
        n.write_floats(self.uc, self.row + 0x10, [multiplier, minimum, maximum])
        n.write_words(self.uc, self.effect + 0x48, 0x200)
        self.scale = None
        self.uc.reg_write(UC_X86_REG_ECX, self.effect)
        n.invoke(self.uc, 0x6f8ae0, [self.matrix, self.unit])
        positioned = self.scale
        assert positioned is not None
        n.write_words(self.uc, self.effect + 0x48, 0)
        self.scale = bits(1.)  # Unit scale omits the matrix call.
        self.uc.reg_write(UC_X86_REG_ECX, self.effect)
        n.invoke(self.uc, 0x6f8c50, [])
        return positioned, self.scale


def capture(executable, output):
    n.initialize(executable)
    oracle = ScaleOracle()
    rng = random.Random(0x6f8ae0)
    lines = ['# 6F7950 -> 6F8AE0 and 6F8C50; all fields are float bits.']
    for i in range(384):
        bounds = [f32(v) for v in [-rng.uniform(0.1, 7), -rng.uniform(0.1, 7), -1., rng.uniform(0.1, 7), rng.uniform(0.1, 7), 1.]]
        world, attached, unit, anchor = [f32(rng.uniform(0.01, 5)) for _ in range(4)]
        multiplier = [0., 1., 2., f32(rng.uniform(0.01, 5))][i % 4]
        minimum, maximum = [(0.01, 100.), (0.5, 1.), (-2., -1.), (4., 2.)][(i // 4) % 4]
        if i % 11 == 0:
            anchor = f32(1e-8)
        params = [world, attached, unit, anchor, multiplier, minimum, maximum]
        output_bits = oracle.resolve(bounds, *params)
        lines.append(' '.join(f'{v:08x}' for v in [*map(bits, bounds + params), *output_bits]))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='ascii')
    print(f'Captured {len(lines) - 1} original unit effect scale cases into {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
