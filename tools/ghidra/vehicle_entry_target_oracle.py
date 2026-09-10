"""Capture 7493B0 entry targets with resident parent models and getter boundaries.

The original seat correction, bone transform, inverse model-frame correction and
movement-frame replacement execute in the fingerprinted client image.
"""
import argparse
import hashlib
import json
import random
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from movement_interval_bounds_oracle import words


def capture(output):
    u = n.emulator()
    owner, child, parent, vtable, seat, result, attachment, model_frame, movement_frame = [
        n.HEAP + offset for offset in (0, 0x1000, 0x3000, 0x5000, 0x6000, 0x7000, 0x8000, 0xa000, 0xb000)
    ]
    facing_stub, scale_stub, position_stub, model_stub, matrix_stub = [
        n.HEAP + offset for offset in (0x9000, 0x9020, 0x9040, 0x9060, 0x9080)
    ]
    u.mem_write(facing_stub, b'\xd9\x81\x00\x01\x00\x00\xc3')
    u.mem_write(scale_stub, b'\xd9\x81\x04\x01\x00\x00\xc3')
    u.mem_write(model_stub, b'\xb8\x01\x00\x00\x00\xc3')
    for offset, value in ((0x34, facing_stub), (0x7c, scale_stub), (0x2c, position_stub),
                          (0xd4, model_stub), (0xc4, matrix_stub)):
        n.write_words(u, vtable + offset, value)
    n.write_words(u, child, vtable)
    n.write_words(u, parent, vtable)
    n.write_words(u, owner + 0xc, child)
    current = {}
    parent_model, model_link = n.HEAP + 0xd000, n.HEAP + 0xe000
    n.write_words(u, parent_model + 0x28, model_link)

    def returned(uc, value, popped):
        sp = uc.reg_read(UC_X86_REG_ESP)
        target = n.read_words(uc, sp, 1)[0]
        uc.reg_write(UC_X86_REG_EAX, value)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + popped)
        uc.reg_write(UC_X86_REG_EIP, target)

    def hook(uc, address, size, data):
        if address == 0x8273d0:
            returned(uc, int(current['attached']), 4)
        elif address == 0x830dc0:
            returned(uc, 0, 0)
        elif address in (0x831410, 0x6f1d20, 0x717ec0, position_stub, matrix_stub):
            sp = uc.reg_read(UC_X86_REG_ESP)
            destination = n.read_words(uc, sp + 4, 1)[0]
            source, count, popped = {
                0x831410: (attachment, 64, 8),
                0x6f1d20: (model_frame, 64, 4),
                0x717ec0: (movement_frame, 64, 4),
                position_stub: (parent + 0x108, 12, 4),
                matrix_stub: (movement_frame, 64, 4),
            }[address]
            uc.mem_write(destination, bytes(uc.mem_read(source, count)))
            returned(uc, destination, popped)

    u.hook_add(UC_HOOK_CODE, hook)
    rng = random.Random(7493)
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    lines = ['# Wow.exe SHA256 ' + hashlib.sha256(n.data).hexdigest(),
             '# attached anchored parentTransition yaw rotation3 offset3 anchor3 scales2 position3 facing attachment16 model16 movement16 | target3']
    for index in range(512):
        current['attached'] = bool(index & 1)
        anchored, transitioning = bool(index & 2), bool(index & 4)
        yaw, facing = [rng.uniform(-7., 7.) for _ in range(2)]
        rotation = [rng.uniform(-3., 3.) for _ in range(3)]
        offset = [rng.uniform(-5., 5.) for _ in range(3)]
        anchor = [rng.uniform(-1., 1.) for _ in range(3)]
        scales = [rng.uniform(.1, 3.), rng.uniform(.2, 5.)]
        if index < 64:
            scales[1] = [1., 1.0000001, .9999999, .999][index // 16]
        position = [rng.uniform(-500., 500.) for _ in range(3)]
        matrices = []
        for address, angle, pitch, scale in (
                (attachment, facing + rng.uniform(-1., 1.), rng.uniform(-2., 2.), scales[1]),
                (model_frame, facing, rng.uniform(-2., 2.) if index & 8 else 0., scales[1]),
                (movement_frame, facing, 0., 1.)):
            matrix = identity.copy()
            matrix[12:15] = position
            n.write_floats(u, address, matrix)
            for function, value in ((0x4c3380, angle), (0x4c3300, pitch), (0x4c1bf0, scale)):
                u.reg_write(UC_X86_REG_ECX, address)
                n.invoke(u, function, words([value]))
            matrices.extend(n.read_floats(u, address, 16))
        n.write_words(u, owner + 0x10, 0x60 if anchored else 0x20)
        n.write_words(u, owner + 0x14, 2)
        n.write_words(u, parent + 0xf60, n.HEAP + 0xc000)
        n.write_words(u, n.HEAP + 0xc00c, parent)
        n.write_words(u, n.HEAP + 0xc014, 2 if transitioning else 3)
        n.write_floats(u, n.HEAP + 0xc0bc, [facing])
        n.write_floats(u, parent_model + 0x124, position)
        n.write_floats(u, model_link + 0xc4, identity)
        n.write_floats(u, owner + 0xcc, [-x for x in anchor])
        n.write_floats(u, child + 0x104, [scales[0]])
        n.write_floats(u, child + 0xaa0, [yaw])
        n.write_floats(u, parent + 0x100, [facing, scales[1], *position])
        n.write_words(u, seat + 8, 0)
        n.write_floats(u, seat + 0xc, offset)
        n.write_floats(u, seat + 0x74, rotation)
        u.reg_write(UC_X86_REG_ECX, owner)
        try:
            n.invoke(u, 0x7493b0, [parent, parent_model, seat, result])
        except Exception as error:
            raise RuntimeError(f'case {index}, PC {u.reg_read(UC_X86_REG_EIP):08x}') from error
        values = words([yaw, *rotation, *offset, *anchor, *scales, *position, facing, *matrices])
        lines.append(f'{int(current["attached"])} {int(anchored)} {int(transitioning)} ' +
                     ' '.join(f'{word:08x}' for word in (*values, *n.read_words(u, result, 3))))
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return {'records': len(lines) - 2, 'upright_threshold': n.read_floats(u, 0xa32f70, 1)[0],
            'sha256': hashlib.sha256(output.read_bytes()).hexdigest()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
