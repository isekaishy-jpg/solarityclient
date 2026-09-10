"""Execute build-12340's 7490F0 vehicle seat placement with resident model inputs.

Only model attachment lookup and unit virtual getters are supplied. Rotation,
scale cancellation, passenger-anchor correction and fallback composition execute
the original instructions. No client entry point or OS calls run.
"""
import argparse
import hashlib
import json
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from movement_interval_bounds_oracle import words


def capture(output):
    u = n.emulator()
    owner, child, parent, vtable, seat, matrix, model_ptr, attachment = [
        n.HEAP + offset for offset in (0, 0x1000, 0x3000, 0x5000, 0x6000, 0x7000, 0x7100, 0x8000)
    ]
    facing_stub, scale_stub, position_stub = n.HEAP + 0x9000, n.HEAP + 0x9020, n.HEAP + 0x9040
    u.mem_write(facing_stub, b'\xd9\x81\x00\x01\x00\x00\xc3')
    u.mem_write(scale_stub, b'\xd9\x81\x04\x01\x00\x00\xc3')
    n.write_words(u, vtable + 0x34, facing_stub)
    n.write_words(u, vtable + 0x7c, scale_stub)
    n.write_words(u, vtable + 0x2c, position_stub)
    n.write_words(u, child, vtable)
    n.write_words(u, parent, vtable)
    n.write_words(u, owner + 0xc, child)
    current = {}

    def returned(uc, value, popped):
        sp = uc.reg_read(UC_X86_REG_ESP)
        target = n.read_words(uc, sp, 1)[0]
        uc.reg_write(UC_X86_REG_EAX, value)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + popped)
        uc.reg_write(UC_X86_REG_EIP, target)

    def hook(uc, address, size, data):
        if address == 0x8273d0:
            returned(uc, int(current['attached']), 4)
        elif address == 0x831410:
            sp = uc.reg_read(UC_X86_REG_ESP)
            destination = n.read_words(uc, sp + 4, 1)[0]
            uc.mem_write(destination, bytes(uc.mem_read(attachment, 64)))
            returned(uc, destination, 8)
        elif address == position_stub:
            sp = uc.reg_read(UC_X86_REG_ESP)
            destination = n.read_words(uc, sp + 4, 1)[0]
            uc.mem_write(destination, bytes(uc.mem_read(parent + 0x108, 12)))
            returned(uc, destination, 4)

    u.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(7490)
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    lines = ['# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
             '# attached anchor passengerYaw rotation3 offset3 anchor3 passengerScale vehicleScale vehiclePosition3 vehicleYaw attachment16 | matrix16']
    for index in range(512):
        current['attached'] = bool(index & 1)
        anchored = bool(index & 2)
        yaw = rng.uniform(-7., 7.)
        rotation = [rng.uniform(-3., 3.) for _ in range(3)]
        offset = [rng.uniform(-5., 5.) for _ in range(3)]
        anchor = [rng.uniform(-1., 1.) for _ in range(3)]
        scales = [rng.uniform(.1, 3.), rng.uniform(.2, 5.)]
        position = [rng.uniform(-500., 500.) for _ in range(3)]
        facing = rng.uniform(-7., 7.)
        attached_matrix = identity.copy()
        attached_matrix[12:15] = position
        n.write_floats(u, attachment, attached_matrix)
        u.reg_write(UC_X86_REG_ECX, attachment)
        n.invoke(u, 0x4c3380, words([facing]))
        u.reg_write(UC_X86_REG_ECX, attachment)
        n.invoke(u, 0x4c3300, words([rng.uniform(-2., 2.)]))
        u.reg_write(UC_X86_REG_ECX, attachment)
        n.invoke(u, 0x4c1bf0, words([scales[1]]))
        if index < 16:
            yaw = -0. if index & 4 else 0.
            rotation = [0., 0., 0.]
            if index & 8:
                n.write_floats(u, attachment, [0.] * 12 + position + [1.])
        attached_matrix = n.read_floats(u, attachment, 16)
        n.write_words(u, owner + 0x10, 0x60 if anchored else 0)
        n.write_floats(u, owner + 0xcc, [-x for x in anchor])
        n.write_floats(u, child + 0x104, [scales[0]])
        n.write_floats(u, parent + 0x100, [facing, scales[1], *position])
        n.write_floats(u, seat + 0xc, offset)
        n.write_floats(u, seat + 0x74, rotation)
        n.write_floats(u, matrix, identity)
        n.write_words(u, model_ptr, n.HEAP + 0xa000)
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x7490f0, [matrix, model_ptr, parent, seat, 20, *words([yaw])])
        if current['attached']:
            assert n.read_words(u, model_ptr, 1)[0] != 0
            u.reg_write(UC_X86_REG_ECX, matrix)
            n.invoke(u, 0x4c2370, [attachment])
        else:
            assert n.read_words(u, model_ptr, 1)[0] == 0
        result = n.read_words(u, matrix, 16)
        values = words([yaw, *rotation, *offset, *anchor, *scales, *position, facing, *attached_matrix])
        lines.append(f'{int(current["attached"])} {int(anchored)} ' +
                     ' '.join(f'{word:08x}' for word in (*values, *result)))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return {'records': len(lines) - 2, 'epsilon': n.read_floats(u, 0x9e8cd0, 1)[0],
            'sha256': hashlib.sha256(output.read_bytes()).hexdigest()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
