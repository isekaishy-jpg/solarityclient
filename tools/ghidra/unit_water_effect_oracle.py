"""Run build-12340 unit breath and water-footstep decisions without launching it.

Uses the fingerprinted PE loader from wmo_registration_oracle. Liquid/area,
unit transforms, model readiness and allocation are controlled external
providers; decisions, float spills, event dispatch and bank slot selection
execute original instructions. The captured factory call is not a rendering
or full CEffect lifetime claim. Requires Unicorn and a locally owned Wow.exe.
"""
import argparse
import itertools
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as n


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def returned(uc, result=0, pop=0):
    sp = uc.reg_read(UC_X86_REG_ESP)
    address = n.read_words(uc, sp, 1)[0]
    uc.reg_write(UC_X86_REG_EAX, result)
    uc.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    uc.reg_write(UC_X86_REG_EIP, address)


class Oracle:
    def __init__(self):
        self.uc = n.emulator()
        self.real_inebriation = False
        self.unit, self.vtable, self.origin, self.fields, self.model = [n.HEAP + i for i in (0, 0x2000, 0x2200, 0x2300, 0x2400)]
        self.move, self.camera, self.cvar, self.effect, self.foot = [n.HEAP + i for i in (0x3000, 0x3400, 0x3500, 0x3600, 0x3800)]
        self.player_fields = n.HEAP + 0x3a00
        self.float_value, self.float_stub = n.HEAP + 0x3b00, n.HEAP + 0x3c00
        load_float = b'\xd9\x05' + struct.pack('<I', self.float_value)
        self.uc.mem_write(self.float_stub, load_float + b'\xc3')
        self.uc.mem_write(self.float_stub + 16, load_float + b'\xc2\x04\x00')
        self.origin_call, self.transport_call = n.HEAP + 0x3d00, n.HEAP + 0x3e00
        n.write_words(self.uc, self.unit, self.vtable)
        n.write_words(self.uc, self.unit + 8, self.fields)
        n.write_words(self.uc, self.vtable + 0x2c, self.origin_call)
        n.write_words(self.uc, self.vtable + 0x40, self.transport_call)
        n.write_words(self.uc, self.unit + 0xd0, self.move, 0, self.move)
        n.write_words(self.uc, self.unit + 0x1008, self.player_fields)
        n.write_words(self.uc, 0xca11a0, self.cvar)
        n.write_words(self.uc, 0xcd774c, 0)  # Footprint drawing, unrelated to spray.
        self.uc.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, uc, address, size, unused):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x77f1e0:
            args = n.read_words(uc, sp + 4, 4)
            n.write_floats(uc, args[2], [self.surface])
            returned(uc, int(self.liquid_present))
        elif address == 0x78f1f0:
            self.cold_calls += 1
            returned(uc, int(self.cold))
        elif address == self.origin_call:
            out = n.read_words(uc, sp + 4, 1)[0]
            uc.mem_write(out, bytes(uc.mem_read(self.origin, 12)))
            returned(uc, out, 4)
        elif address == self.transport_call:
            uc.reg_write(UC_X86_REG_EDX, 0)
            returned(uc, self.transport)
        elif address == 0x4d37c0:
            returned(uc, self.mode)
        elif address in (0x4f7290, 0x987570):
            if address == 0x4f7290 and self.real_inebriation:
                return
            uc.reg_write(UC_X86_REG_EIP, self.float_stub + (16 if address == 0x987570 else 0))
        elif address == 0x76e540:
            returned(uc, self.effect)
        elif address == 0x6f9d70:
            returned(uc, self.effect)
        elif address == 0x6fa390:
            returned(uc)
        elif address == 0x6f9260:
            self.emitted = n.read_words(uc, sp + 4, 1)[0]
            returned(uc, pop=12)
        elif address == 0x4f5960:
            returned(uc, self.camera)
        elif address == 0x77f220:
            args = n.read_words(uc, sp + 4, 3)
            n.write_words(uc, args[1], 0)
            n.write_words(uc, args[2], self.liquid_id)
            returned(uc)
        elif address == 0x6f93a0:
            args = n.read_words(uc, sp + 4, 7)
            assert args[5:] == (0x220, 0x744870)
            self.emitted = args[:4]
            returned(uc, pop=28)

    def breath(self, height, scale, z, surface, present, cold, now):
        self.surface, self.liquid_present, self.cold = surface, present, cold
        self.cold_calls = 0
        n.write_floats(self.uc, self.unit + 0xac, [height])
        n.write_floats(self.uc, self.unit + 0x98, [scale])
        n.write_floats(self.uc, self.origin, [0, 0, z])
        n.write_words(self.uc, self.unit + 0xa30, 0xf0f0f0f0)
        self.uc.reg_write(UC_X86_REG_ECX, self.unit)
        n.invoke(self.uc, 0x71fa90, [now])
        return (*n.read_words(self.uc, self.unit + 0xa30, 1), *n.read_words(self.uc, self.unit + 0x9bc, 1), self.cold_calls)

    def event(self, state, mode, model_flags, player, inebriation):
        self.mode, self.emitted = mode, -1
        n.write_words(self.uc, self.unit + 0xa30, state)
        n.write_words(self.uc, self.unit + 0x970, 0 if model_flags == -1 else self.model)
        n.write_words(self.uc, self.model + 4, model_flags & 0xffffffff)
        n.write_words(self.uc, self.fields + 8, 0x18 if player else 8)
        n.write_floats(self.uc, self.float_value, [inebriation])
        self.uc.reg_write(UC_X86_REG_ECX, self.unit)
        n.invoke(self.uc, 0x732650, [0, 0x48544224, 0, 0, 0])
        return self.emitted

    def spray(self, foot, camera, z, height, surface, liquid_id, flags, model_flags, enabled, speed, walk, gate):
        self.surface, self.liquid_present, self.liquid_id = surface, True, liquid_id
        self.transport, self.emitted = int(gate == 4), None
        n.write_words(self.uc, self.move + 0x114, int(gate == 1))
        n.write_words(self.uc, self.move + 0x110, 0x20000 if gate == 2 else 0)
        n.write_words(self.uc, self.move + 0x44, flags | (0x40000000 if gate == 3 else 0))
        n.write_words(self.uc, self.fields + 8, 0x18)
        n.write_words(self.uc, self.player_fields + 8, 0x10 if gate == 5 else 0)
        n.write_words(self.uc, self.unit + 0x970, self.model)
        n.write_words(self.uc, self.model + 4, model_flags)
        n.write_words(self.uc, self.cvar + 0x30, enabled)
        n.write_floats(self.uc, self.foot, foot)
        n.write_floats(self.uc, self.camera + 8, camera)
        n.write_floats(self.uc, self.origin, [0, 0, z])
        n.write_floats(self.uc, self.unit + 0x854, [height])
        n.write_floats(self.uc, self.unit + 0x818, [walk])
        n.write_floats(self.uc, self.float_value, [speed])
        self.uc.reg_write(UC_X86_REG_ECX, self.unit)
        try:
            n.invoke(self.uc, 0x723a50, [self.foot, 0])
        except Exception as error:
            raise RuntimeError(f'spray at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {foot}, {surface}, gate {gate}') from error
        return self.emitted


def capture(executable, output):
    n.initialize(executable)
    oracle = Oracle()
    lines = ['# Build 12340; float values are eight-digit hexadecimal IEEE-754 words.']
    rng = random.Random(12340)
    for i in range(384):
        height, scale, z = map(f32, (rng.uniform(0.1, 18), rng.uniform(0.2, 3), rng.uniform(-100, 100)))
        surface = f32(z + f32(height * scale) + 5)
        surface_bits = bits(surface)
        surface = struct.unpack('<f', struct.pack('<I', surface_bits + (i % 3 - 1)))[0]
        present, cold, now = i % 7 != 0, i % 2 != 0, (0xfffffff0 + i * 987) & 0xffffffff
        result = oracle.breath(height, scale, z, surface, present, cold, now)
        fields = [*(f'{bits(v):08x}' for v in [height, scale, z, surface]), str(int(present)), str(int(cold)), str(now), *map(str, result)]
        lines.append('B ' + ' '.join(fields))
    for state, mode, flags, player, drunk in itertools.product([0, 0x20, 0x40, 0x60], [0, 1, 2], [-1, 0, 2], [0, 1], [0., f32(0.49999997), 0.5, 1.]):
        result = oracle.event(state, mode, flags, player, drunk)
        lines.append(f'E {state} {mode} {flags} {player} {bits(drunk):08x} {result}')
    oracle.real_inebriation = True
    for state, mode, actual, fake in itertools.product([0, 0x20, 0x40], [0, 1], [0, 49, 50, 51, 100, 255], [0, 49, 50, 51, 99, 100, 101, 0x80000000, 0xffffffff]):
        oracle.uc.mem_write(oracle.player_fields + 0x1d, bytes([actual]))
        n.write_words(oracle.uc, oracle.player_fields + 0x2b8, fake)
        result = oracle.event(state, mode, 0, 1, 0.)
        lines.append(f'I {state} {mode} {actual} {fake} {result}')
    oracle.real_inebriation = False
    for i in range(384):
        foot = [f32(v) for v in [1.25, -7.5, 20.0 + rng.uniform(-0.1, 0.1)]]
        camera = [f32(foot[0] + [24.999998, 25., 25.000002][i % 3]), foot[1], foot[2]]
        z, height = 20., f32(rng.uniform(0.5, 8))
        surface = f32(z + height * [0.49999, 0.5, 0.50001][(i // 3) % 3])
        liquid_id, flags, model_flags, enabled = int(i % 11 != 0), 2 if i % 13 == 0 else 0, int(i % 17 == 0), int(i % 19 != 0)
        speed, walk = [4.9999995, 5., 5.0000005][(i // 9) % 3], 2.5
        gate = i % 6 if i % 5 == 0 else 0
        result = oracle.spray(foot, camera, z, height, surface, liquid_id, flags, model_flags, enabled, speed, walk, gate)
        floats = ' '.join(f'{bits(v):08x}' for v in [*foot, *camera, z, height, surface, speed, walk])
        emission = '-1' if result is None else ' '.join([str(result[0]), *(f'{v:08x}' for v in result[1:])])
        lines.append(f'S {floats} {liquid_id} {flags} {model_flags} {enabled} {gate} {emission}')
    Path(output).write_text('\n'.join(lines) + '\n', encoding='ascii')
    print(f'Captured {len(lines) - 1} original unit water-effect cases into {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
