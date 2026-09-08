"""Capture original underwater particle initialization and movement histories.

The constructor, reset, drift, frame advance and liquid-selection functions
execute original code. Texture acquisition and camera-liquid queries are
controlled; a recorded deterministic word stream replaces shared RNG calls.
No original client or OS entry point runs.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from camera_water_oracle import bits, ret


class Capture:
    def __init__(self, seed):
        self.uc = n.emulator()
        self.pool = n.HEAP
        self.seed = seed
        self.draws = 0
        self.liquid_id = 0
        self.flags = 8
        self.mode = 0
        self.scale = 1.
        self.enabled = True
        for address in [0x464580, 0x79dff0, 0x7a0b00, 0x7f1070, 0x8a2aa0]:
            self.uc.hook_add(UC_HOOK_CODE, self.hook, begin=address, end=address)
        self.row = n.HEAP + 0x20000
        n.write_words(self.uc, 0xad4074, 1)
        n.write_words(self.uc, 0xad4070, 4)
        n.write_words(self.uc, 0xad4084, self.row + 0x1000)
        n.write_words(self.uc, self.row + 0x1000, *[self.row + i * 0x100 for i in range(4)])
        for i in range(4):
            n.write_words(self.uc, self.row + i * 0x100, i + 1)
        n.write_words(self.uc, 0xcd7548, self.pool)
        self.call(0x79e100, [bits(1/36), bits(30.), 0])

    def hook(self, uc, address, size, data):
        if address == 0x464580:
            self.seed = (self.seed * 1664525 + 1013904223) & 0xffffffff
            self.draws += 1
            ret(uc, self.seed)
        elif address == 0x79dff0:
            ret(uc, 0, 4)
        elif address == 0x7a0b00:
            args = n.read_words(uc, uc.reg_read(UC_X86_REG_ESP) + 4, 5)
            n.write_words(uc, args[1], self.liquid_id)
            n.write_floats(uc, args[2], [10.])
            ret(uc, int(self.liquid_id != 0))
        elif address in [0x7f1070, 0x8a2aa0]:
            ret(uc, 0)

    def call(self, function, args):
        self.uc.reg_write(UC_X86_REG_ECX, self.pool)
        stack = n.STACK + 0x18000
        n.write_words(self.uc, stack, n.STOP, *args)
        self.uc.reg_write(UC_X86_REG_ESP, stack)
        self.uc.emu_start(function, n.STOP, count=2_000_000)
        assert self.uc.reg_read(UC_X86_REG_EIP) == n.STOP

    def select(self, liquid, flags, mode, scale, enabled):
        self.liquid_id = liquid
        self.enabled = enabled
        n.write_words(self.uc, 0xcd774c, 0x2000000 if enabled else 0)
        if liquid:
            row = self.row + (liquid - 1) * 0x100
            n.write_words(self.uc, row + 8, flags)
            n.write_words(self.uc, row + 0x30, mode)
            n.write_floats(self.uc, row + 0x2c, [scale])
        self.call(0x790920, [])

    def advance(self, point, dt):
        n.write_floats(self.uc, 0xcd8f5c, point)
        n.write_floats(self.uc, 0xcd76a0, [dt])
        if self.enabled and self.liquid_id:
            self.call(0x79bf40, [])

    def state(self):
        return [self.seed, self.draws, *n.read_words(self.uc, 0xcd8794, 1),
                *n.read_words(self.uc, self.pool + 0xfa00, 16)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--initial', type=Path, required=True)
    parser.add_argument('--histories', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    initial = bytearray()
    for seed in [0, 12340, 0xdeadbeef, 0xffffffff]:
        capture = Capture(seed)
        state = [seed, *capture.state()]
        initial += struct.pack('<' + 'I' * len(state), *state)
        initial += capture.uc.mem_read(capture.pool, 64000)
    args.initial.write_bytes(initial)
    rows = ['# seed mode | action (kind, enabled, liquid, flags, movement, scale, dt, cameraXYZ) | state (rng draws selected-id count previous-camera3 texture enabled base-size cube unused liquid direction3 frequency phase speed) | 16 particles XYZS; hex words']
    for seed, mode in itertools.product([0, 12340, 0xdeadbeef], range(4)):
        capture = Capture(seed)
        # Limit the history bank after validating all 4000 constructor records.
        n.write_words(capture.uc, capture.pool + 0xfa00, 16)
        actions = [
            (1, 1, 1, 8, mode, 2., 0., [0., 0., 0.]),
            (0, 1, 1, 8, mode, 2., .016, [1., -2., 3.]),
            (0, 1, 1, 8, mode, 2., .1, [15., -15., 15.]),
            (0, 1, 1, 8, mode, 2., .1, [80., -80., 80.]),
            (0, 1, 1, 8, mode, 2., 60., [80., -80., 80.]),
            (1, 1, 2, 8, mode, .5, 0., [80., -80., 80.]),
            (0, 1, 2, 8, mode, .5, .016, [81., -78., 77.]),
            (1, 1, 0, 0, 0, 1., 0., [0., 0., 0.]),
            (0, 1, 0, 0, 0, 1., .016, [300., 0., 0.]),
            (1, 1, 3, 0, mode, 3., 0., [300., 0., 0.]),
            (0, 1, 3, 0, mode, 3., .016, [300., 0., 0.]),
            (1, 0, 4, 8, mode, 1., 0., [300., 0., 0.]),
            (1, 1, 4, 8, mode, 1., 0., [300., 0., 0.]),
            (0, 1, 4, 8, mode, 1., .016, [300., 0., 0.]),
            (1, 1, 1, 8, mode, 1., 0., [300., 0., 0.]),
            (0, 1, 1, 8, mode, 1., .016, [300., 0., 0.]),
        ]
        for kind, enabled, liquid, flags, movement, scale, dt, camera in actions:
            if kind:
                capture.select(liquid, flags, movement, scale, bool(enabled))
            else:
                capture.advance(camera, dt)
            action = [kind, enabled, liquid, flags, movement, *map(bits, [scale, dt, *camera])]
            groups = [[seed, mode], action, capture.state(), list(n.read_words(capture.uc, capture.pool, 64))]
            rows.append(' | '.join(' '.join(f'{word:08x}' for word in group) for group in groups))
    args.histories.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured 4 complete constructors and {len(rows)-1} particle history steps')


if __name__ == '__main__':
    main()
