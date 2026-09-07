"""Capture original MO transport construction and route sampling.

Runs 007F92F0/007F8660/007F82B0 and original Path.cpp callees. Only
allocation/free/reallocation are intercepted; route and timing math run intact.
The generated fixture contains synthetic DBC rows, not redistributed game data.
"""
import argparse
import struct
import random
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits
from movement_path_oracle import invoke
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


class RouteOracle:
    """Own one bounded native heap and its stock allocator ABI."""

    def __init__(self):
        self.uc = native.emulator()
        self.uc.mem_map(native.HEAP + 0x40000, 0x200000)
        self.owner = native.HEAP
        self.dbc = native.HEAP + 0x1000
        self.output = native.HEAP + 0x8000
        self.cursor = native.HEAP + 0x10000
        self.allocations = {}
        self.uc.hook_add(UC_HOOK_CODE, self.allocate)

    def allocate(self, uc, address, _size, _data):
        """Preserve callee cleanup and the native no-in-place-copy flag."""
        if address not in (0x76e540, 0x76e5e0, 0x76e5a0):
            return
        sp = uc.reg_read(UC_X86_REG_ESP)
        args = native.read_words(uc, sp, 6)
        result = 0
        if address != 0x76e5a0 and not (address == 0x76e5e0 and args[1] and args[5] & 0x10):
            length = args[1] if address == 0x76e540 else args[2]
            result = self.cursor
            self.cursor += (length + 15) & ~15
            assert self.cursor <= native.HEAP + 0x240000, 'bounded native allocator exhausted'
            if address == 0x76e5e0 and args[1]:
                uc.mem_write(result, bytes(uc.mem_read(args[1], min(length, self.allocations[args[1]]))))
            self.allocations[result] = length
        uc.reg_write(UC_X86_REG_EAX, result)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + (20 if address == 0x76e5e0 else 16))
        uc.reg_write(UC_X86_REG_EIP, args[0])

    def call(self, address, *args):
        self.uc.reg_write(UC_X86_REG_ECX, self.owner)
        invoke(self.uc, address, args)

    def construct(self, nodes, speed, acceleration):
        for index, (map_id, xyz, flags, delay, arrival, departure) in enumerate(nodes):
            native.write_words(self.uc, self.dbc + index * 44, index + 1, 42, index, map_id,
                               *(bits(v) for v in xyz), flags, delay, arrival, departure)
        native.write_words(self.uc, 0xad4bd0, len(nodes))
        native.write_words(self.uc, 0xad4be4, self.dbc)
        self.call(0x7f92f0, 42, bits(speed), bits(acceleration), 0)


def case_nodes(count, spacing, stops=(), map_id=0, offset=0., split=False):
    """Include both native endpoint controls and curved traversed points."""
    return [(map_id, (offset + i * spacing, (i % 3) * spacing * .3, (i % 4) * spacing * .1),
             (2 if i in stops else 0) | (1 if split and i == count - 1 else 0),
             (i % 3) + 1 if i in stops else 0, 100 + i, 200 + i)
            for i in range(count)]


def capture(executable, output):
    native.initialize(executable)
    cases = [
        (10., 2., case_nodes(5, 10., [2])),
        (10., 2., case_nodes(5, 10.)),
        (8., 3., case_nodes(10, 8., [2, 4, 7])),
        (3., 2., case_nodes(8, 90., [2, 5])),
        (25., 1.5, case_nodes(31, 20., [1, 3, 16, 28])),
        (12., 2.5, case_nodes(6, 25., [2]) + case_nodes(7, 40., [3], map_id=1, offset=-800.)),
        (7., 4., case_nodes(5, 30., [2], split=True) + case_nodes(5, 35., [2], offset=600.)),
    ]
    lines = ['# Wow.exe SHA256 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8',
             '# floats are hex IEEE754; times/maps/events/flags are decimal']
    samples = 0
    physics_rows = [None,
                    [.5, .5, .1, .25, .05, .1, .05, .7, 25., .1],
                    [3.3, 1.1, 2., 2.5, .88, 1.9, 2., 3.5, 25., .1],
                    [.3, 3.5, .1, 0., .02, 0., .04, .65, 25., .1]]
    for case, ((speed, acceleration, nodes), physics) in enumerate((case, physics) for case in cases for physics in physics_rows):
        oracle = RouteOracle()
        oracle.construct(nodes, speed, acceleration)
        uc, owner = oracle.uc, oracle.owner
        period = native.read_words(uc, owner + 0x2c, 1)[0]
        lines.append(f'case {case} {bits(speed):08x} {bits(acceleration):08x} {period}')
        for map_id, xyz, flags, delay, arrival, departure in nodes:
            lines.append(f'node {map_id} ' + ' '.join(f'{bits(v):08x}' for v in xyz)
                         + f' {flags} {delay} {arrival} {departure}')
        sections = native.read_words(uc, owner + 0x10, 1)[0]
        count = native.read_words(uc, owner + 0xc, 1)[0]
        times = {0, 1, period - 1, period, period + 1, 0xffffffff}
        times.update(range(0, period, max(1, period // 60)))
        for index in range(count):
            section = sections + index * 0x1e4
            map_id, _, length = native.read_words(uc, section, 3)
            start, moving, end, _, stop_count, stops = native.read_words(uc, section + 0x1c8, 6)
            lines.append(f'section {index} {map_id} {length:08x} {start} {moving} {end}')
            for j in range(stop_count):
                time, distance, delay = native.read_words(uc, stops + j * 12, 3)
                lines.append(f'stop {index} {time} {distance:08x} {delay}')
                for edge in (time, time + delay, start, end):
                    times.update((edge + delta) & 0xffffffff for delta in (-1, 0, 1))
        events = native.read_words(uc, owner + 0x20, 1)[0]
        for index in range(native.read_words(uc, owner + 0x1c, 1)[0]):
            time, event = native.read_words(uc, events + index * 8, 2)
            lines.append(f'event {time} {event}')
        if physics is not None:
            physics_pointer = native.HEAP + 0x9000
            native.write_words(uc, physics_pointer, 1, *(bits(v) for v in physics))
            native.write_words(uc, owner, physics_pointer)
            lines.append('physics ' + ' '.join(f'{bits(v):08x}' for v in physics))
        for override in (None, period + 3000, max(1, period - 500)):
            if override is not None:
                oracle.call(0x7f7fc0, override)
                lines.append(f'period {override}')
            for time in sorted(times):
                output_ptr = oracle.output
                uc.mem_write(output_ptr, bytes(40))
                native.write_words(uc, output_ptr, 0xffffffff)
                oracle.call(0x7f82b0, time, 16, output_ptr, output_ptr + 4, output_ptr + 8,
                            output_ptr + 20, output_ptr + 24, output_ptr + 28, output_ptr + 32, 0)
                words = native.read_words(uc, output_ptr, 9)
                lines.append(f'sample {time} {words[0]} {words[1]} {words[6]} '
                             + ' '.join(f'{v:08x}' for v in words[2:6] + words[7:9]))
                samples += 1
        oracle.call(0x7f7fc0, period)
        lines.append(f'period {period}')
        lines.append('clock_reset')
        # A fresh per-object owner has these native clock words zeroed.
        uc.mem_write(owner + 0x34, bytes(12))
        random_source = random.Random(12340 + case)
        for operation in range(100):
            time = random_source.randrange(period * 3) if operation % 5 else (0xffffffff - operation)
            if operation % 9 == 0:
                progress = [0, 1, 32767, 32768, 65534, 65535][(operation // 9) % 6]
                fraction = progress * struct.unpack('<f', struct.pack('<I', 0x37800080))[0]
                oracle.call(0x7f8120, time, bits(fraction))
                lines.append(f'progress {time} {progress}')
            elif operation % 7 == 0:
                oracle.call(0x7f80a0, time)
                lines.append(f'freeze {time}')
            elif operation % 3 == 0:
                requested = (operation // 3) % 2
                oracle.call(0x7f8000, time, requested)
                lines.append(f'motion {time} {requested}')
            for interval in [0, 16, period // 2, period]:
                oracle.call(0x7f7840, time, interval, 0)
                lines.append(f'clock {time} {interval} {uc.reg_read(UC_X86_REG_EAX)}')
    Path(output).write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Captured {len(cases) * len(physics_rows)} routes and {samples} original samples')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
