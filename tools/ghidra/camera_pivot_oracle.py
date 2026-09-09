"""Execute stock 6020B0 mouse input, 5FFDE0 pivot admission and offset return.

The pinned PE supplies all branching, angle arithmetic and interpolation.
Only unit lookup, wall clock and CVar storage are supplied by this harness.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EBP, UC_X86_REG_ESI,
    UC_X86_REG_EDI, UC_X86_REG_EBX, UC_X86_REG_EIP, UC_X86_REG_FPSW,
    UC_X86_REG_FPTAG,
)
import wmo_registration_oracle as n
from camera_water_oracle import bits, ret


def probe(orbit, offset, flags, movement, enabled, delta, flips=(0, 0), thresholds=(.05, 0.), sample_times=()):
    """Run a complete original mouse event against controlled ordinary-player state."""
    uc = n.emulator()
    camera, unit, fields, movement_data = [n.HEAP + i * 0x1000 for i in range(4)]
    n.write_words(uc, unit + 8, fields)
    n.write_words(uc, fields, 1, 0, 8)
    n.write_words(uc, unit + 0xd8, movement_data)
    n.write_words(uc, movement_data + 0x44, movement)
    n.write_words(uc, camera + 0x98, flags)
    n.write_floats(uc, camera + 0x11c, [.7, orbit])
    n.write_floats(uc, camera + 0x130, [offset])
    n.write_floats(uc, camera + 0x230, [orbit, orbit])
    n.write_floats(uc, camera + 0x248, [offset])
    n.write_floats(uc, camera + 0x260, [.7, .7])
    variables = [(0xc24e50, 180.), (0xc24e54, 90.), (0xc249a4, thresholds[1]),
                 (0xc249a8, thresholds[0]), (0xc24e34, 45.)]
    for i, (address, value) in enumerate(variables):
        pointer = n.HEAP + 0x4000 + i * 0x100
        n.write_words(uc, address, pointer)
        n.write_floats(uc, pointer + 0x2c, [value])
    for i, (address, value) in enumerate([(0xc24e6c, flips[1]), (0xc24e70, flips[0]), (0xc249ac, enabled)]):
        pointer = n.HEAP + 0x5000 + i * 0x100
        n.write_words(uc, address, pointer)
        n.write_words(uc, pointer + 0x30, value)
    n.write_floats(uc, 0xac0cb4, [1., 1.])
    n.write_floats(uc, 0xad1b50, [-1.553343])
    n.write_floats(uc, 0xad1b4c, [1.553343])
    def hook(uc, address, size, unused):
        if address == 0x4d4db0: ret(uc, unit)
        elif address == 0x86ae20: ret(uc, 1000)
    uc.hook_add(UC_HOOK_CODE, hook)
    uc.reg_write(UC_X86_REG_ECX, camera)
    n.invoke(uc, 0x6020b0, [bits(delta[0]), bits(delta[1]), 0])
    result = [*n.read_words(uc, camera + 0x120, 1), *n.read_words(uc, camera + 0x130, 1),
            *n.read_words(uc, camera + 0x98, 1), *n.read_words(uc, camera + 0x240, 6)]
    for time in sample_times:
        uc.reg_write(UC_X86_REG_FPSW, 0)
        uc.reg_write(UC_X86_REG_FPTAG, 0xffff)
        frame = n.STACK + 0x18000
        uc.reg_write(UC_X86_REG_EBP, frame)
        uc.reg_write(UC_X86_REG_ESP, frame - 0x100)
        uc.reg_write(UC_X86_REG_ESI, camera)
        uc.reg_write(UC_X86_REG_EDI, 0)
        uc.reg_write(UC_X86_REG_EBX, time)
        # The preceding roll lane leaves zero on the original x87 stack.
        uc.mem_write(n.STOP, b'\xd9\xee')
        uc.emu_start(n.STOP, n.STOP + 2)
        uc.emu_start(0x604126, 0x6041f5, count=10000)
        assert uc.reg_read(UC_X86_REG_EIP) == 0x6041f5
        result += [time, *n.read_words(uc, camera + 0x130, 1), *n.read_words(uc, camera + 0x98, 1)]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--edges', action='store_true', help='capture inversion, thresholds and timed return')
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# pinned build-12340 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8; complete 6020B0 / 5FFDE0 / 6012D0',
            '# orbit offset camera-flags movement-flags enabled dx dy invert-yaw invert-pitch dx-max dy-min | orbit offset flags offset-start duration goal anchor factor delay [time offset flags]...; hex words; controlled target smooth speed 45 degrees/sec']
    if args.edges:
        for orbit, offset, flips, delta, thresholds in itertools.product(
                [-.6, 0.], [0., -.25], [(0, 0), (1, 0), (0, 1), (1, 1)],
                [(0., -20.), (0., 20.), (0., 200.), (10., -20.)],
                [(.05, 0.), (0., 0.), (.05, .06)]):
            result = probe(orbit, offset, 0x10001, 0, 1, delta, flips, thresholds,
                           (1000, 1050, 1100, 1200, 1500, 3000, 3001))
            inputs = [bits(orbit), bits(offset), 0x10001, 0, 1, *map(bits, delta), *flips, *map(bits, thresholds)]
            rows.append(' | '.join(' '.join(f'{word:08x}' for word in values) for values in [inputs, result]))
        args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
        print(f'captured {len(rows)-2} native pivot edge histories')
        return
    for orbit, offset, flags, movement, enabled, delta in itertools.product(
            [-.1, -.6, -1.4, 0., .1], [0., -.25, -.0005],
            [1, 0x10001, 0x20001, 0x30001, 0x10009, 0x8010001],
            [0, 1, 4, 0x2000000], [0, 1],
            [(0., -20.), (0., 20.), (0., 200.), (0., -1000.), (100., -20.)]):
        result = probe(orbit, offset, flags, movement, enabled, delta)
        inputs = [bits(orbit), bits(offset), flags, movement, enabled, *map(bits, delta), 0, 0, bits(.05), bits(0.)]
        rows.append(' | '.join(' '.join(f'{word:08x}' for word in values) for values in [inputs, result]))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'captured {len(rows)-2} native pivot mouse events')


if __name__ == '__main__': main()
