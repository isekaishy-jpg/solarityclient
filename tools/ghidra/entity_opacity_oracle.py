"""Capture build-12340 entity opacity setters, updates and retired scene envelopes.

Only the fingerprinted PE is mapped into Unicorn. Model/transport getters,
parent/seat lookups and the clock are supplied external inputs; native entry
policy, byte quantization/interpolation, model readiness, opacity composition,
retirement timing and list recycling execute.
The scene destruction boundary is observed without releasing host resources.
No OS or client entry point runs. Requires Unicorn and the locally owned PE.
"""

import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESP

import wmo_registration_oracle as n


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def return_value(u, value, pop=0):
    sp = u.reg_read(UC_X86_REG_ESP)
    target = n.read_words(u, sp, 1)[0]
    u.reg_write(UC_X86_REG_EAX, value)
    u.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
    u.reg_write(UC_X86_REG_EIP, target)


def capture_states(output):
    u = n.emulator()
    obj, model, vtable = n.HEAP, n.HEAP + 0x2000, n.HEAP + 0x3000
    present = [True]
    n.write_words(u, obj, vtable)
    n.write_words(u, vtable + 0xd4, n.STOP + 0x100)
    u.hook_add(UC_HOOK_CODE, lambda u, *_: return_value(u, model if present[0] else 0),
               begin=n.STOP + 0x100, end=n.STOP + 0x100)
    cases = []
    # Setter quantization, equal-current cancellation, and immediate publication.
    targets = [0., 1., .25, .5, .75, -0.1, 1.1]
    targets += [(value + .5) / 255 for value in range(255)]
    for target in targets:
        for duration in (0, 1000):
            cases.append((0, 800, bits(target), duration, 1, 255, 42, 1300, 127, 17, 239))
    for current in (0, 1, 64, 127, 128, 254, 255):
        cases.append((0, 1800, bits(current / 255), 1000, 1, 137, 123, 777, current, 19, 241))
    # Both directions, exact finish, wrapped clocks and missing model ownership.
    for start in (100, 0xfffffe00):
        for a, b in ((0, 255), (255, 0), (17, 203), (211, 72), (127, 128)):
            for elapsed in (0, 1, 7, 16, 499, 500, 999, 1000, 1001, 2000):
                for multiplier in (0, 1, 127, 255):
                    cases.append((1, (start + elapsed) & 0xffffffff, 0, 0, 1,
                                  multiplier, start, 1000, a, a, b))
    cases += [(1, 3000, 0, 0, 0, 219, 17, 1000, 13, 13, 235),
              (1, 3000, 0, 0, 1, 219, 17, 0, 235, 13, 235)]
    rng = random.Random(12340)
    for _ in range(256):
        start, duration = rng.randrange(2**32), rng.randrange(1, 2001)
        a, b, multiplier = [rng.randrange(256) for _ in range(3)]
        now = (start + rng.randrange(duration + 50)) & 0xffffffff
        cases.append((1, now, 0, 0, 1, multiplier, start, duration, a, a, b))
    lines = ['# op now target_f32 duration model_present multiplier before(start duration current from target) after(start duration current from target) opacity_f32; all hex']
    for case in cases:
        op, now, target, duration, available, multiplier, start, old_duration, current, a, b = case
        present[0] = bool(available)
        outputs = []
        for update in (0x743e10, 0x71ac30):
            n.write_words(u, obj + 0xc0, start, old_duration)
            u.mem_write(obj + 0xc8, bytes((current, a, b, multiplier)))
            n.write_words(u, model + 0x178, 0xffffffff)
            n.write_words(u, 0xcd76ac, now)
            u.reg_write(UC_X86_REG_ECX, obj)
            n.invoke(u, 0x744030 if op == 0 else update,
                     [target, duration] if op == 0 else [now])
            state = (*n.read_words(u, obj + 0xc0, 2), *u.mem_read(obj + 0xc8, 3))
            outputs.append((*state, n.read_words(u, model + 0x178, 1)[0]))
        assert outputs[0] == outputs[1], (case, outputs)
        lines.append(' '.join(f'{word:08x}' for word in (*case, *outputs[0])))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='ascii')
    return len(cases)


def capture_retirement(output):
    u = n.emulator()
    entry, scene, model, shared, header, free = [n.HEAP + offset for offset in
                                             (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000)]
    clock, removed = [0], [False]
    u.hook_add(UC_HOOK_CODE, lambda u, *_: return_value(u, clock[0]), begin=0x86ae20, end=0x86ae20)
    def destroy(u, *_):
        removed[0] = True
        return_value(u, 0)
    u.hook_add(UC_HOOK_CODE, destroy, begin=0x7826e0, end=0x7826e0)
    n.write_words(u, scene + 0x34, model)
    n.write_words(u, model + 0x2c, shared)
    n.write_words(u, shared + 0x150, header)
    n.write_words(u, 0xadf1a8, 0x58)
    n.write_words(u, 0xadf1b4, 0x58)
    lines = ['# now start initial_alpha_f32 model_ready removed opacity_f32; all hex']
    for start in (700, 0xfffffe00):
        for opacity in (0., 1 / 255, .01, .25, .5, .75, 1., 1.25):
            for elapsed in (0, 1, 33, 500, 999, 1000, 1001, 1500, 1999, 2000, 2001):
                for ready in (0, 1):
                    clock[0] = (start + elapsed) & 0xffffffff
                    removed[0] = False
                    n.write_words(u, 0xadf1b0, entry)
                    n.write_words(u, 0xadf1b8, free)
                    n.write_words(u, free, 0, 1)
                    n.write_words(u, entry, scene, start, bits(opacity))
                    n.write_words(u, entry + 0x58, 0, 1)
                    n.write_words(u, model + 0x10, ready)
                    n.write_words(u, model + 0x178, 0xffffffff)
                    n.invoke(u, 0x782f20, [])
                    result = n.read_words(u, model + 0x178, 1)[0]
                    row = (clock[0], start, bits(opacity), ready, int(removed[0]), result)
                    lines.append(' '.join(f'{word:08x}' for word in row))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='ascii')
    return len(lines) - 1


def capture_entry_policy(output):
    u = n.emulator()
    obj, fields, parent, seat, vtable = [n.HEAP + x for x in (0, 0x2000, 0x4000, 0x6000, 0x7000)]
    guid, parent_present, seat_present = [0], [False], [False]
    n.write_words(u, obj, vtable)
    n.write_words(u, obj + 0xd0, fields)
    n.write_words(u, vtable + 0x40, n.STOP + 0x100)
    def get_transport(u, *_):
        u.reg_write(UC_X86_REG_EDX, guid[0] >> 32)
        return_value(u, guid[0] & 0xffffffff)
    u.hook_add(UC_HOOK_CODE, get_transport, begin=n.STOP + 0x100, end=n.STOP + 0x100)
    u.hook_add(UC_HOOK_CODE, lambda u, *_: return_value(u, parent if parent_present[0] else 0),
               begin=0x4d4db0, end=0x4d4db0)
    u.hook_add(UC_HOOK_CODE, lambda u, *_: return_value(u, seat if seat_present[0] else 0, 4),
               begin=0x5d3340, end=0x5d3340)
    lines = ['# primary secondary bytes1 transport_low transport_high parent_present parent_duration seat_present seat_flags fade_allowed; all hex']
    for primary, secondary, bytes1 in [(0, 0, 0), (2, 0, 0), (0, 0x20, 0),
                                      (0, 0, 0x20000), (0xfffffffd, 0xffffffdf, 0xfffdffff)]:
        for transport in (0, 1, 0xf050000000000001, 0xf110000000000001,
                          0xf130000000000001, 0x0800000000000000, 0x0080000000000000):
            for available in (0, 1):
                for duration in (0, 1000):
                    for has_seat, flags in ((0, 0), (1, 0), (1, 0x7fffffff), (1, 0x80000000)):
                        guid[0], parent_present[0], seat_present[0] = transport, available, has_seat
                        n.write_words(u, obj + 0x790, transport & 0xffffffff, transport >> 32)
                        n.write_words(u, fields + 0xd4, primary, secondary)
                        n.write_words(u, fields + 0x110, bytes1)
                        n.write_words(u, parent + 0xc4, duration)
                        n.write_words(u, seat + 8, flags)
                        u.reg_write(UC_X86_REG_ECX, obj)
                        n.invoke(u, 0x716650, [])
                        row = (primary, secondary, bytes1, transport & 0xffffffff, transport >> 32,
                               available, duration, has_seat, flags, u.reg_read(UC_X86_REG_EAX))
                        lines.append(' '.join(f'{word:08x}' for word in row))
    Path(output).write_text('\n'.join(lines) + '\n', encoding='ascii')
    return len(lines) - 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output-directory', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output_directory.mkdir(parents=True, exist_ok=True)
    counts = {}
    for filename, capture in [('entity_opacity_native.txt', capture_states),
                              ('entity_retirement_native.txt', capture_retirement),
                              ('entity_entry_policy_native.txt', capture_entry_policy)]:
        path = args.output_directory / filename
        counts[filename] = {'records': capture(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    metadata = {'executable_sha256': hashlib.sha256(n.data).hexdigest(),
                'functions': ['744030', '743E10', '71AC30', '782F20', '823E40', '716650', '74B8B0'], 'captures': counts}
    (args.output_directory / 'entity_opacity_native.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(json.dumps(metadata, indent=2))


if __name__ == '__main__':
    main()
