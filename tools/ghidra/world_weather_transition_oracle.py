"""Run original weather setter and interpolator with clock/resource boundaries.

Only the monotonic clock, current precipitation resource type, and nonempty
texture string are supplied. Native 7846A0/784850 arithmetic remains untouched.
No particle resource is allocated: its callbacks do not change light timing.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from world_light_sampling_oracle import bits


def capture():
    u = n.emulator()
    owner = n.HEAP
    now, kind = 0, 0

    def hook(u, address, size, context):
        if address == 0x86ae20:
            return_value(u, now)
        elif address == 0x783b60:
            return_value(u, kind)
        elif address == 0x76ee30:
            return_value(u, 1)
        elif address == 0x78c500:
            return_value(u, 0)
        elif address == 0x4d3790:
            return_value(u, 0)
            u.reg_write(UC_X86_REG_EDX, 0)
    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Build 12340 weather setter/interpolator: 13 owner words and published light grade.']
    for initial_kind in [0, 1, 2, 3]:
        for initial_grade in [0., .1, .25, .75]:
            for initial_weight in [.4, 1.]:
                u.mem_write(owner, bytes(0x200))
                initial = [bits(initial_grade)] * 3 + [bits(min(initial_grade, .25))] * 3 + [0, 0, 0xffffffff, 0] + [bits(initial_weight)] * 3
                u.mem_write(owner, struct.pack('<13I', *initial))
                kind, now = initial_kind, 1000
                rows.append(f'reset {kind} {bits(initial_grade):08x} {bits(initial_weight):08x}')
                for new_kind, grade, instant, weight, delay in [(1, .8, 0, .7, 123), (2, .1, 0, .2, 3000), (0, 0., 0, 1., 10), (3, .4, 1, .6, 0), (1, 1., 0, .8, 11000), (0, -.1, 1, 1., 4000), (2, 2., 0, 1., 0)]:
                    u.reg_write(UC_X86_REG_ECX, owner)
                    n.invoke(u, 0x7846a0, [new_kind, bits(grade), 0, 1 - instant, bits(weight)])
                    rows.append(f'set {now} {new_kind} {bits(grade):08x} {instant} {bits(weight):08x} ' + bytes(u.mem_read(owner, 52)).hex())
                    kind = new_kind
                    for elapsed in [0, 1, delay]:
                        now = (now + elapsed) & 0xffffffff
                        u.reg_write(UC_X86_REG_ECX, owner)
                        old, target = n.read_words(u, owner + 4, 1)[0], n.read_words(u, owner, 1)[0]
                        n.invoke(u, 0x784850, [old, target])
                        rows.append(f'tick {now} ' + bytes(u.mem_read(owner, 52)).hex() + ' ' + bytes(u.mem_read(0xd38b4c, 4)).hex())
    # Native full-frame threshold/forced-update branch with no pending resource.
    # Player camera and particle advancement are the only additional boundaries.
    for grade in [0., .000001, .000009, .00001, .000011, .1, .25, 1., .00000001] + [struct.unpack('<f', struct.pack('<I', value))[0] for value in [0x347fffff, 0x34800000, 0x34800001]]:
        for instant in [0, 1]:
            u.mem_write(owner, bytes(0x200))
            u.mem_write(owner + 0x20, struct.pack('<I', 0xffffffff))
            u.mem_write(owner + 0x28, struct.pack('<3f', 1., 1., 1.))
            u.mem_write(0xd38b4c, bytes(4))
            kind, now = 1, 5000
            rows.append('reset 1 00000000 3f800000')
            u.reg_write(UC_X86_REG_ECX, owner)
            n.invoke(u, 0x7846a0, [1, bits(grade), 0, 1 - instant, bits(1.)])
            rows.append(f'set {now} 1 {bits(grade):08x} {instant} 3f800000 ' + bytes(u.mem_read(owner, 52)).hex())
            for elapsed in [0, 1, 10, 100, 1000, 10000]:
                now = 5000 + elapsed
                u.reg_write(UC_X86_REG_ECX, owner)
                n.invoke(u, 0x78d170, [])
                rows.append(f'frame {now} ' + bytes(u.mem_read(owner, 52)).hex() + ' ' + bytes(u.mem_read(0xd38b4c, 4)).hex())
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--exe', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    n.initialize(args.exe)
    Path(args.output).write_text(capture(), encoding='utf-8')
