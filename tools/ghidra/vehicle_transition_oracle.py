"""Capture build-12340 passenger transition timing, easing, yaw and airborne pose.

World/model lookups and virtual unit getters supply inputs; 74A200, 747D70,
747A30 and 74A7F0 execute their original arithmetic. Model publication is
captured at 82DD80. This excludes state-entry animation and camera callbacks.
"""
import argparse
import hashlib
import json
import random
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from movement_interval_bounds_oracle import words


def capture(output):
    u = n.emulator()
    owner, child, parent, seat, vtable, model, target = [n.HEAP + x for x in
        (0, 0x1000, 0x3000, 0x5000, 0x6000, 0x7000, 0x8000)]
    yaw_stub, position_stub, scale_stub, model_stub = [n.HEAP + x for x in
        (0x9000, 0x9020, 0x9040, 0x9060)]
    u.mem_write(yaw_stub, b'\xd9\x81\x00\x01\x00\x00\xc3')
    u.mem_write(scale_stub, b'\xd9\x81\x04\x01\x00\x00\xc3')
    n.write_words(u, vtable + 0x34, yaw_stub)
    n.write_words(u, vtable + 0x2c, position_stub)
    n.write_words(u, vtable + 0x7c, scale_stub)
    n.write_words(u, vtable + 0xd4, model_stub)
    n.write_words(u, child, vtable)
    n.write_words(u, parent, vtable)
    n.write_floats(u, child + 0x104, [1.])
    current = {}

    def returned(uc, value=0, popped=0):
        sp = uc.reg_read(UC_X86_REG_ESP)
        destination = n.read_words(uc, sp, 1)[0]
        uc.reg_write(UC_X86_REG_EAX, value)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + popped)
        uc.reg_write(UC_X86_REG_EIP, destination)

    def hook(uc, address, size, data):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d4db0:
            returned(uc, parent if current.get('posing') else 0)
        elif address == position_stub:
            destination = n.read_words(uc, sp + 4, 1)[0]
            n.write_floats(uc, destination, current['unit_position'])
            returned(uc, destination, 4)
        elif address == model_stub:
            returned(uc, model)
        elif address == 0x6fed70:
            destination = n.read_words(uc, sp + 4, 1)[0]
            n.write_floats(uc, destination, current['velocity'])
            returned(uc, destination, 4)
        elif address == 0x749e40:
            destination = n.read_words(uc, sp + 12, 1)[0]
            n.write_floats(uc, destination, current['target'])
            returned(uc, destination, 12)
        elif address == 0x716450:
            returned(uc, int(current['phase'] in (2, 5)))
        elif address == 0x716470:
            destination = n.read_words(uc, sp + 4, 1)[0]
            n.write_floats(uc, destination, [0., 0., 1.])
            returned(uc, destination, 4)
        elif address == 0x82dd80:
            position, yaw = n.read_words(uc, sp + 4, 2)
            current['pose'] = [*n.read_words(uc, position, 3), yaw]
            returned(uc, popped=20)

    u.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(747)
    lines = ['# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
             '# phase parent flags start now params7 origin3 target3 unitPosition3 velocity3 yaw previousYaw | end gravity arc adjustedYaw fraction pose3 poseYaw']
    for index in range(512):
        phase = (1, 2, 4, 5)[index % 4]
        has_parent = bool(index & 4)
        flags = ((index >> 3) % 4) * 0x20 if phase == 2 else ((index >> 3) % 4) * 0x80
        params = [rng.uniform(0., 12.), rng.uniform(.25, 20.), rng.uniform(-5., 30.),
                  rng.uniform(0., .3), rng.uniform(.3, 12.), 0., rng.uniform(1., 10.)]
        if index % 7 == 0:
            params[4] = 0.
        if index % 11 == 0:
            params[2], params[5] = 0., .25
        origin = [rng.uniform(-30., 30.) for _ in range(3)]
        destination = [origin[i] + rng.uniform(-8., 8.) for i in range(3)]
        if index % 13 == 0:
            destination = origin.copy()
        unit_position = destination.copy()
        velocity = [rng.uniform(-20., 20.) for _ in range(3)]
        yaw, previous_yaw = rng.uniform(-9., 9.), rng.uniform(-9., 9.)
        start = (0xffffff80 if index & 32 else 1000)
        current.update(phase=phase, velocity=velocity, target=destination,
                       unit_position=unit_position, posing=False)
        u.mem_write(owner, bytes(0x100))
        n.write_words(u, owner + 0xc, child, 0, phase)
        n.write_words(u, owner + 0x34, start)
        n.write_words(u, owner + 0x54, seat)
        n.write_words(u, seat + 4, flags)
        n.write_floats(u, seat + 0x18, params)
        n.write_floats(u, seat + 0x4c, params)
        n.write_floats(u, owner + 0x84, origin)
        n.write_floats(u, owner + 0xb4, [previous_yaw, previous_yaw, previous_yaw])
        n.write_floats(u, child + 0x100, [yaw])
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x74a200, [parent if has_parent else 0, seat, start, target])
        end = n.read_words(u, owner + 0x38, 1)[0]
        timing = n.read_words(u, owner + 0xc4, 2)
        adjusted_yaw = n.read_words(u, owner + 0xc0, 1)[0]
        duration = (end - start) & 0xffffffff
        for elapsed in (-1, 0, duration // 3, duration, duration + 1):
            now = (start + elapsed) & 0xffffffff
            n.write_floats(u, owner + 0xbc, [previous_yaw])
            u.reg_write(UC_X86_REG_ECX, owner)
            n.invoke(u, 0x747d70, [now, seat])
            fraction = n.read_words(u, owner + 0x48, 1)[0]
            if phase in (2, 5):
                u.reg_write(UC_X86_REG_ECX, owner)
                n.invoke(u, 0x747a30, [])
            current['posing'] = True
            u.reg_write(UC_X86_REG_ECX, owner)
            n.invoke(u, 0x74a7f0, [])
            current['posing'] = False
            inputs = [phase, int(has_parent), flags, start, now,
                      *words([*params, *origin, *destination, *unit_position, *velocity, yaw, previous_yaw])]
            result = [end, *timing, adjusted_yaw, fraction, *current['pose']]
            lines.append(' '.join(f'{word:08x}' for word in [*inputs, *result]))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return {'records': len(lines) - 2, 'sha256': hashlib.sha256(output.read_bytes()).hexdigest()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
