"""Capture original M2 local override, pivot and parent matrix composition.

Runs 82FD16..82FEC4 and original quaternion/axis-angle constructors with
pre-sampled authored tracks. No arithmetic callees are hooked. Each three-bone
chain contains noncommuting authored rotations, nonuniform scales, translation,
pivots and spine/head overrides. Track decoding and interpolation are not run.
"""
import argparse
import struct
from pathlib import Path
import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import bits, invoke
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_EBP, UC_X86_REG_ECX,
    UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_EIP,
)


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    model, bone, descriptor, parent, override, palette, counts, axis = [
        native.HEAP + i * 0x1000 for i in range(8)
    ]
    frame = native.STACK + 0x10000
    uc.mem_write(native.STOP + 16, b'\xdb\xe3')  # FNINIT per composition.
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    native.write_words(uc, model + 0x98, palette)
    native.write_words(uc, descriptor + 0x14, 1, counts)
    native.write_floats(uc, axis, [0., 0., 1.])
    inputs = [
        ([41000, 32000, 25000, 62000], [1., -2., .5], [.3, 1., -2.], [1.2, .7, 2.]),
        ([25000, 44000, 33000, 61000], [-.2, .4, 1.7], [-1., .7, .2], [.8, 1.3, .6]),
        ([35000, 24000, 49000, 58000], [.5, .8, -.1], [.1, .2, .9], [1.1, .9, 1.4]),
    ]
    rows = ['# case bone rawQuaternion[4] pivot[3] translation[3] scale[3] angle overrideMatrix[16] composedMatrix[16]; floats are hex bits']
    scale = struct.unpack('<f', struct.pack('<I', 0x38000080))[0]
    for case, angle in enumerate([-1.57079637, -.7, 0., .4, 1.57079637]):
        native.write_floats(uc, parent, identity)
        for index, (raw, pivot, translation, scaling) in enumerate(inputs):
            uc.emu_start(native.STOP + 16, native.STOP + 18, count=1)
            native.write_floats(uc, bone + 0x1c, [value * scale - 1. for value in raw])
            uc.reg_write(UC_X86_REG_ECX, frame - 0xd4)
            invoke(uc, 0x4c1de0, [bone + 0x1c])
            current_angle = angle if index == 1 else -angle * .5 if index == 2 else 0.
            invoke(uc, 0x4c3460, [override, bits(current_angle), axis, 1])
            native.write_floats(uc, bone + 8, translation)
            native.write_floats(uc, bone + 0x34, scaling)
            native.write_floats(uc, descriptor + 0x4c, pivot)
            native.write_words(uc, bone + 0x88, override)
            native.write_words(uc, frame + 12, 0x80)
            native.write_words(uc, frame + 20, descriptor, index)
            for reg, value in [(UC_X86_REG_EAX, descriptor), (UC_X86_REG_EBX, parent),
                               (UC_X86_REG_ESI, model), (UC_X86_REG_EDI, bone),
                               (UC_X86_REG_EBP, frame), (UC_X86_REG_ESP, frame - 0x1000)]:
                uc.reg_write(reg, value)
            uc.emu_start(0x82fd16, 0x82fec4, count=10000)
            assert uc.reg_read(UC_X86_REG_EIP) == 0x82fec4
            matrix = bytes(uc.mem_read(palette + index * 64, 64))
            prefix = ' '.join(map(str, [case, index, *raw]))
            values = [*[bits(v) for v in [*pivot, *translation, *scaling, current_angle]],
                      *native.read_words(uc, override, 16),
                      *native.read_words(uc, palette + index * 64, 16)]
            rows.append(prefix + ' ' + ' '.join(f'{v:08x}' for v in values))
            uc.mem_write(parent, matrix)
    Path(output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original bone compositions')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
