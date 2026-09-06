"""Capture original 75F9D0/75F0A0 cache decisions before world collection.

No native instruction or callee is replaced. Execution stops at the collection
boundary on a miss, or at the original return on a hit/tiny sweep. The inputs
have no transport parent. Only float images and decisions are exported.
"""
import argparse
from pathlib import Path
import random
import struct

import wmo_registration_oracle as native
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
    UC_X86_REG_FPCW, UC_X86_REG_FPSW, UC_X86_REG_FPTAG,
)


def words(values):
    return struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values))


def capture(uc, values):
    unit, direction, selected, planes, count, travel = [native.HEAP + i * 0x1000 for i in range(6)]
    uc.mem_write(native.HEAP, bytes(0x6000))
    native.write_floats(uc, unit + 0x10, values[:3])
    native.write_floats(uc, unit + 0xc8, values[3:5])
    native.write_floats(uc, direction, values[5:8])
    native.write_floats(uc, 0xca1660, values[9:15])
    native.write_words(uc, 0xadba34, 0, 0, 0)
    sp = native.STACK + 0x18000
    native.write_words(uc, sp, native.STOP, 0xadba34, direction, words(values[8:9])[0],
                       selected, planes, count, travel, 0)
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.reg_write(UC_X86_REG_ECX, unit)
    uc.reg_write(UC_X86_REG_FPCW, 0x037f)
    uc.reg_write(UC_X86_REG_FPSW, 0)
    uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
    # Stop before query mask resolution/collection. The union callee has run.
    def stop(uc, address, size, data):
        uc.emu_stop()
    hook = uc.hook_add(UC_HOOK_CODE, stop, begin=0x75f2eb, end=0x75f2eb)
    try:
        uc.emu_start(0x75f9d0, native.STOP, count=100_000)
    finally:
        uc.hook_del(hook)
    address = uc.reg_read(UC_X86_REG_EIP)
    assert address in (0x75f2eb, native.STOP), hex(address)
    miss = address == 0x75f2eb
    query = native.read_words(uc, uc.reg_read(UC_X86_REG_EBP) - 0x30, 6) if miss else words(values[9:15])
    assert native.read_words(uc, 0xca1660, 6) == words(values[9:15])
    return int(miss), query


def cases():
    for origin in ([0., 0., 0.], [1000., -5800., 100.], [-10000., 12000., -500.]):
        for axis in range(3):
            for sign in (-1., 1.):
                direction = [0., 0., 0.]
                direction[axis] = sign
                for distance in (0., 2**-21, 2**-20, .001, .02777778, .5, 1., 10.):
                    for pad in (0., .5, 1., 20.):
                        low = [origin[i] - [.5, .5, 0.][i] - pad for i in range(3)]
                        high = [origin[i] + [.5, .5, 2.][i] + pad for i in range(3)]
                        yield [*origin, .5, 2., *direction, distance, *low, *high]
    rng = random.Random(12340)
    for _ in range(1000):
        origin = [rng.uniform(-16000., 16000.) for _ in range(3)]
        radius = rng.uniform(.05, 3.)
        height = radius * rng.uniform(2., 6.)
        direction = [rng.uniform(-1., 1.) for _ in range(3)]
        pad = [rng.uniform(0., 20.) for _ in range(6)]
        low = [origin[i] - [radius, radius, 0.][i] - pad[i] for i in range(3)]
        high = [origin[i] + [radius, radius, height][i] + pad[i+3] for i in range(3)]
        yield [*origin, radius, height, *direction, rng.uniform(0., 40.), *low, *high]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    native.initialize(args.executable)
    uc = native.emulator()
    lines = [
        '# Native 75F9D0 + 75F0A0; x87 037F; original callees; no transport.',
        '# Wow.exe sha256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
        '# miss | origin3 radius height direction3 distance cached6 | query6',
    ]
    for values in cases():
        inputs = words(values)
        values = struct.unpack('<15f', struct.pack('<15I', *inputs))
        miss, query = capture(uc, values)
        lines.append(f'{miss} ' + ' '.join(f'{v:08x}' for v in inputs + query))
    args.output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(lines)-3} original sweep cache decisions')


if __name__ == '__main__':
    main()
