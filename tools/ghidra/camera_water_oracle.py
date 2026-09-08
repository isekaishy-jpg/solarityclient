"""Capture 6049C0 subject classification and 6061D0 water-interface correction.

Executes the fingerprinted PE without its entry point. Classification uses the
original 77F1E0 registration accessor. Scene trace/volume and subject/forward
accessors supply controlled inputs; 6061D0 ordering and arithmetic run natively.
This fixture does not establish the geometry providers or camera pitch lanes.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def ret(uc, value=0, cleanup=0):
    sp = uc.reg_read(UC_X86_REG_ESP)
    address = n.read_words(uc, sp, 1)[0]
    uc.reg_write(UC_X86_REG_EAX, value)
    uc.reg_write(UC_X86_REG_ESP, sp + 4 + cleanup)
    uc.reg_write(UC_X86_REG_EIP, address)


def classify(position, surface, height, present, previous):
    uc = n.emulator()
    camera, subject, fields, registration = [n.HEAP + i * 0x1000 for i in range(4)]
    n.write_words(uc, camera + 0x98, previous)
    n.write_words(uc, subject + 8, fields)
    n.write_words(uc, fields + 8, 8)
    n.write_words(uc, subject + 0xb8, registration)
    n.write_floats(uc, subject + 0x854, [height])
    n.write_words(uc, registration + 0x7c, 0x20 if present else 0)
    n.write_floats(uc, registration + 0x80, [surface])

    def hook(uc, address, size, data):
        if address == 0x603090:
            _, output, unit = n.read_words(uc, uc.reg_read(UC_X86_REG_ESP), 3)
            assert unit == subject
            n.write_floats(uc, output, [2., 3., position])
            ret(uc, output, 8)

    uc.hook_add(UC_HOOK_CODE, hook)
    uc.reg_write(UC_X86_REG_ECX, camera)
    n.invoke(uc, 0x6049c0, [subject])
    output = n.HEAP + 0x4000
    uc.mem_write(n.STOP, b'\xd9\x1d' + struct.pack('<I', output))
    uc.emu_start(n.STOP, n.STOP + 6)
    return [*n.read_words(uc, output, 1), *n.read_words(uc, camera + 0x98, 1)]


def interface(eye, pivot, forward, distance, hit, obstruction, shortened):
    uc = n.emulator()
    camera, vtable, output, eye_ptr, pivot_ptr = [n.HEAP + i * 0x1000 for i in range(5)]
    forward_accessor = n.STOP + 0x100
    n.write_words(uc, camera, vtable)
    n.write_words(uc, vtable + 4, forward_accessor)
    n.write_floats(uc, camera + 0x120, [.317])
    n.write_floats(uc, 0xad2178, [-91.])
    n.write_floats(uc, eye_ptr, eye)
    n.write_floats(uc, pivot_ptr, pivot)
    calls = []

    def hook(uc, address, size, data):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x77f310:
            _, start, end, contact, fraction, mask, owner = n.read_words(uc, sp, 7)
            assert mask == 0x20000 and owner == 0
            calls.extend([1, *n.read_words(uc, start, 3), *n.read_words(uc, end, 3)])
            if hit is not None:
                n.write_floats(uc, contact, hit)
            ret(uc, int(hit is not None))
        elif address == 0x6059e0:
            _, dist, adjusted, anchor, mask = n.read_words(uc, sp, 5)
            assert mask == 0x100171
            calls.extend([2, *n.read_words(uc, dist, 1), *n.read_words(uc, adjusted, 3), *n.read_words(uc, anchor, 3)])
            if obstruction:
                n.write_floats(uc, dist, [shortened])
            ret(uc, int(obstruction), 16)
        elif address == forward_accessor:
            _, target = n.read_words(uc, sp, 2)
            calls.append(3)
            n.write_floats(uc, target, forward)
            ret(uc, target, 4)

    uc.hook_add(UC_HOOK_CODE, hook)
    uc.reg_write(UC_X86_REG_ECX, camera)
    n.invoke(uc, 0x6061d0, [output, pivot_ptr, bits(distance), eye_ptr])
    return [*n.read_words(uc, output, 3), *n.read_words(uc, 0xad2178, 1)], calls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# C: position surface height present prior | depth flags; I: eye3 pivot3 forward3 distance hit contact3 obstruct shortened | eye3 cached_pitch | ordered calls; hex words']
    for position, height, present, previous in itertools.product([0., 31.75, -1234.5], [0., .5, 2., 4.7], [False, True], [0, 0x100001, 0x200008, 0x300000]):
        threshold = struct.unpack('<f', struct.pack('<f', position + height - .2222222238779068))[0]
        for surface in [position-10., position, threshold, threshold-0.00001, threshold+0.00001, position+height]:
            inputs = [bits(position), bits(surface), bits(height), int(present), previous]
            rows.append('C ' + ' | '.join(' '.join(f'{v:08x}' for v in group) for group in [inputs, classify(position, surface, height, present, previous)]))
    for eye, distance, dz, obstruction in itertools.product([[2.,3.,0.], [12.5,-7.,31.75], [10000.,-9999.,-1234.5]], [0., -1., 0.0001, 7.25], [None, -.2, 0., .2], [False, True]):
        pivot, forward = [1.3, -2.7, 4.1], [.3, -.4, .8660254]
        shortened = min(.73, max(0., distance * .5))
        hit = None if dz is None else [eye[0], eye[1], eye[2] + dz]
        inputs = [*map(bits, eye+pivot+forward+[distance]), int(hit is not None), *map(bits, hit or [0.,0.,0.]), int(obstruction), bits(shortened)]
        result, calls = interface(eye, pivot, forward, distance, hit, obstruction, shortened)
        rows.append('I ' + ' | '.join(' '.join(f'{v:08x}' for v in group) for group in [inputs, result, calls]))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original camera water cases')


if __name__ == '__main__':
    main()
