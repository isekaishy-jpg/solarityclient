"""Capture original 760B40 swimming collision with both native candidate banks.

760B40, all narrow-phase code, reanchoring, and surface-jump response execute
original instructions. Candidate residency is complete and fixed; optional
75F0A0 failure isolates the provider's false return at a numbered probe.
"""
import argparse
import math
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_geometry_failure_oracle import setup, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def floor(z):
    return [[-100., -100., z], [100., -100., z], [0., 100., z]]


def wall(x):
    return [[x, -100., -100.], [x, -100., 100.], [x, 100., 0.]]


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    records = bytearray()
    scenarios = []
    for delta in [[0., 0., 0.], [5., 0., 0.], [3., 4., 2.], [1., 2., -3.], [0., 0., 5.], [0., 0., -5.]]:
        for walls in [[], [wall(2.)], [floor(-1.)], [wall(2.), floor(-1.)]]:
            for surface in [[], [floor(2.)], [floor(1.5)]]:
                for ascend in [0, 1]:
                    scenarios.append((delta, walls, surface, ascend, 250, 0))
    for failure in [1, 2, 3, 4]:
        scenarios.append(([3., 4., 2.], [wall(2.)], [floor(2.)], 0, 250, failure))
    rng = random.Random(0x760b40)
    for _ in range(200):
        delta = [rng.uniform(-5, 5), rng.uniform(-5, 5), rng.uniform(-5, 5)]
        walls = [wall(rng.uniform(.6, 4)), floor(rng.uniform(-3, 0))]
        scenarios.append((delta, walls, [floor(rng.uniform(1.5, 4))], rng.randrange(2), rng.choice([1, 17, 250, 1000]), 0))
    for index, (delta, triangles, water, ascend, duration, failure) in enumerate(scenarios):
        flags = 0x200001 | (0x400000 if ascend else 0)
        unit = setup(uc, [0., 0., 0.], .5, 2., triangles, True, flags)
        water_faces = native.HEAP + 0x11000
        native.write_words(uc, 0xadba54, len(water), len(water), water_faces)
        for i, vertices in enumerate(water):
            # 75FF90 negates the plane without reversing authored vertices.
            native.write_floats(uc, water_faces + i*52, [0., 0., -1., vertices[0][2]] + [x for p in vertices for x in p])
        delta = [struct.unpack('<f', struct.pack('<f', x))[0] for x in delta]
        distance = struct.unpack('<f', struct.pack('<f', math.sqrt(sum(x*x for x in delta))))[0]
        direction = [struct.unpack('<f', struct.pack('<f', x/distance))[0] if distance else 0. for x in delta]
        seen = {'probes': 0, 'failed': 0}
        def provider(uc, address, size, data):
            seen['probes'] += 1
            if seen['probes'] == failure:
                seen['failed'] = 1
                sp = uc.reg_read(UC_X86_REG_ESP)
                uc.reg_write(UC_X86_REG_EAX, 0)
                uc.reg_write(UC_X86_REG_ESP, sp+16)
                uc.reg_write(UC_X86_REG_EIP, native.read_words(uc, sp, 1)[0])
        hook = uc.hook_add(UC_HOOK_CODE, provider, begin=0x75f0a0, end=0x75f0a0)
        sp = native.STACK + 0x18000
        native.write_words(uc, sp, native.STOP, 10000, duration, bits(distance), *map(bits,direction))
        uc.reg_write(UC_X86_REG_ECX, unit)
        uc.reg_write(UC_X86_REG_ESP, sp)
        try:
            uc.emu_start(0x760b40, native.STOP, count=3000000)
            assert uc.reg_read(UC_X86_REG_EIP) == native.STOP, (index, hex(uc.reg_read(UC_X86_REG_EIP)))
        finally:
            uc.hook_del(hook)
        inputs = [ascend, duration, bits(distance), *map(bits,direction), len(triangles), len(water), failure]
        records.extend(struct.pack('<9I', *inputs))
        for base, count in [(native.HEAP+0x4000,len(triangles)), (water_faces,len(water))]:
            records.extend(bytes(uc.mem_read(base, count*52)))
        after = [uc.reg_read(UC_X86_REG_EAX), *native.read_words(uc, unit+0x10, 3),
            int(native.read_words(uc, unit+0x60, 1)[0]==0), int(bool(native.read_words(uc,unit+0x44,1)[0]&0x1000)), seen['failed']]
        records.extend(struct.pack('<7I', *after))
    Path(output).write_bytes(records)
    print(f'Captured {len(scenarios)} original swimming collision intervals')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
