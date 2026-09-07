"""Capture build-12340 ripple normalization, initialization, and frame envelopes.

The original 0x79D460 -> 0x79D180 -> 0x79CF40 path initializes each record.
Only allocation, the water-triangle query, and active-list insertion are
stubbed. Frame evaluation runs 0x79D5E0 until its alive/dead decision, before
graphics submission or list removal. No game or operating-system entry runs.

The output begins with a u32 case count. Each case has 9fI emission inputs
(position XYZ, yaw, radius, strength, lifetime, growth, scene time, direction),
12 original record words, 6 bounds words, a u32 frame count, and frame records
of 2f12II (delta, scene time, original scalar record, alive). Each case starts
a fresh ripple and carries state through its sequence until retirement.
"""

import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as n
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_ESP


def word(value):
    """Preserve one native float argument's exact stack bits."""
    return struct.unpack('<I', struct.pack('<f', value))[0]


def rounded(value):
    """Store a native frame clock scalar at single precision."""
    return struct.unpack('<f', struct.pack('<f', value))[0]


def capture(executable, output):
    """Execute the original envelope with controlled scene times and geometry."""
    n.initialize(executable)
    u = n.emulator()
    record, point, reset = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x2000
    u.mem_write(reset, b'\xdb\xe3')  # FNINIT between independent native calls.
    bounds = []
    alive = False

    def hook(u, address, size, data):
        nonlocal alive
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in [0x79d721, 0x79d661]:
            alive = address == 0x79d721
            u.reg_write(UC_X86_REG_EIP, n.STOP)
            return
        if address == 0x4c4b80:
            clean = 4
        elif address == 0x77f340:
            out = n.read_words(u, sp + 4, 1)[0]
            bounds[:] = n.read_words(u, out, 6)
            u.reg_write(UC_X86_REG_EAX, 0)
            clean = 0
        elif address == 0x6ded60:
            clean = 4
        else:
            return
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + clean)

    for address in [0x4c4b80, 0x77f340, 0x6ded60, 0x79d721, 0x79d661]:
        u.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)
    cases = []
    total_frames = 0
    for direction in [0, 1]:
        for radius, strength, lifetime, growth in [
            (.2, .13333334, .64, .3),
            (.33333334, 1., .65, 3.3724444),
            (1.25, .10416667, .473281, 12.23456),
            (.1234567, .033333335, .999999, 0.),
            (1.6666666, .16666667, .6, 4.),
            (.33333334, 0., .65, 1.),
        ]:
            for scene in [0., 12.345, 10000.]:
                for sequence in [
                    [0., .016],
                    [.016, .08, .12, .3, .001, .2, .4],
                    [lifetime * .3999, .0001, .0001, lifetime * .3],
                    [lifetime * .4, 0., .02],
                    [lifetime * .8], [lifetime], [lifetime + .001],
                ]:
                    u.emu_start(reset, reset + 2)
                    u.mem_write(record, bytes(0x50))
                    n.write_floats(u, point, [10., 20., .25])
                    n.write_floats(u, 0xcd76a4, [scene])
                    n.write_words(u, 0xadf7f0, 1)
                    n.write_words(u, 0xcdffd0, 0)
                    n.write_words(u, 0xcdffe8, record)
                    n.write_words(u, 0xcdf7cc, 0)
                    n.invoke(u, 0x79d460, [point, word(.25), word(radius), word(lifetime),
                                         word(strength), word(growth), direction, 1])
                    initial = bytes(u.mem_read(record, 48))
                    header = struct.pack('<9fI', 10., 20., .25, .25, radius, strength,
                                         lifetime, growth, scene, direction)
                    header += initial + struct.pack('<6I', *bounds)
                    frames = []
                    now = rounded(scene)
                    for delta in sequence:
                        delta = rounded(delta)
                        now = rounded(now + delta)
                        n.write_words(u, 0xadfb58, 0x30)
                        n.write_words(u, 0xadfb60, record)
                        n.write_floats(u, 0xcd76a0, [delta, now])
                        u.emu_start(reset, reset + 2)
                        n.invoke(u, 0x79d5e0, [])
                        frames.append(struct.pack('<2f', delta, now)
                                      + bytes(u.mem_read(record, 48))
                                      + struct.pack('<I', int(alive)))
                        if not alive:
                            break
                    cases.append(header + struct.pack('<I', len(frames)) + b''.join(frames))
                    total_frames += len(frames)
    Path(output).write_bytes(struct.pack('<I', len(cases)) + b''.join(cases))
    print('Captured', len(cases), 'native lifetimes and', total_frames, 'frame decisions')


def capture_pool(executable, output):
    """Capture original ring reuse and active-list order without replacing list code.

    Each record contains owner (0 other / 1 local), emitted X coordinate, active
    count and that many retained X coordinates in native traversal order.
    """
    n.initialize(executable)
    u = n.emulator()
    bank, point, reset = n.HEAP, n.HEAP + 0x10000, n.HEAP + 0x11000
    u.mem_write(reset, b'\xdb\xe3')
    n.write_words(u, 0xadfb58, 0x30, 0xadfb5c, 0xadfb5d)
    n.write_words(u, 0xcdffe8, bank)
    n.write_words(u, 0xcdf7cc, 0)
    n.write_words(u, 0xcdf7c8, 32)
    n.write_words(u, 0xadf7f0, 1)
    n.write_words(u, 0xcdffd0, 0)
    n.write_floats(u, 0xcd76a4, [0.])

    def hook(u, address, size, data):
        sp = u.reg_read(UC_X86_REG_ESP)
        if address == 0x77f340:
            u.reg_write(UC_X86_REG_EAX, 0)
            clean = 0
        else:
            clean = 4
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4 + clean)

    for address in [0x4c4b80, 0x77f340]:
        u.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)
    records = []
    for index, local in enumerate([1] * 33 + [0] * 97 + [0, 1] * 70):
        x = float(index + 1)
        n.write_floats(u, point, [x, 20., .25])
        u.emu_start(reset, reset + 2)
        n.invoke(u, 0x79d460, [point, word(.25), word(.33333334), word(.65),
                             word(.16666667), word(1.), 0, local])
        pointer = n.read_words(u, 0xadfb60, 1)[0]
        active = []
        while pointer and pointer & 1 == 0:
            assert len(active) < 128, 'native active list did not terminate'
            active.append(n.read_words(u, pointer, 1)[0])
            pointer = n.read_words(u, pointer + 0x34, 1)[0]
        records.append(struct.pack('<IfI', local, x, len(active))
                       + struct.pack('<' + 'I' * len(active), *active))
    Path(output).write_bytes(struct.pack('<I', len(records)) + b''.join(records))
    print('Captured', len(records), 'native pool insertions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--pool-output')
    args = parser.parse_args()
    capture(args.executable, args.output)
    if args.pool_output:
        capture_pool(args.executable, args.pool_output)
