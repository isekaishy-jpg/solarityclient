"""Run original 875F80/7BAFD0 volume setup and 7BB9D0 unit admission.

GPU submission is intercepted after volume construction. Each loaded dynamic
unit uses a supplied world AABB/radius; the material collector is intercepted
only to record admission. The native six-plane test runs without substitution.
The default fixture explicitly disables shadowCull; --cropped enables the stock
camera-frustum crop using controlled world-space corner boxes at CDD168.
"""
import argparse
import itertools
import math
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n


def capture(executable, cropped=False):
    n.initialize(executable)
    lines = ['# shadowCull=' + str(int(cropped)),
             '# unit center3 raw_ray3 ' + ('camera_min3 camera_max3 ' if cropped else '')
             + 'minimum3 maximum3 radius admitted']
    boxes = [((-8., -12., -4.), (9., 7., 25.)), ((40., 40., 2.), (60., 60., 20.))] if cropped else [None]
    for center, ray, box in itertools.product([(0., 0., 0.), (15302., -12840., 1200.)],
                                         [(0., 0., -1.), (.3, .4, -.8660254), (-.55, .82, -.17)], boxes):
        u = n.emulator()
        scene, anchor, unit, model = [n.HEAP + offset for offset in (0x1000, 0x2000, 0x3000, 0x4000)]
        direction = [ray[0], ray[1], max(ray[2] * 5., -1.2)]
        length = math.sqrt(sum(value * value for value in direction))
        n.write_floats(u, 0xd43180, [value / length for value in direction])
        n.write_words(u, 0xb1d51c, 0)
        n.write_words(u, 0xd25308, n.HEAP)
        n.write_words(u, n.HEAP + 0x30, int(cropped))
        camera_values = []
        if box is not None:
            camera_min = [value + offset for value, offset in zip(center, box[0])]
            camera_max = [value + offset for value, offset in zip(center, box[1])]
            camera_values = [*camera_min, *camera_max]
            corners = itertools.product(*zip(camera_min, camera_max))
            n.write_floats(u, 0xcdd168, [value for corner in corners for value in corner])
        n.write_words(u, 0xd43154, 2)
        n.write_words(u, 0xd43150, 2048)
        n.write_floats(u, 0xd43258, [20.])
        n.write_words(u, 0xd43158, 0x7bac10)
        n.write_words(u, 0xd4315c, 0x7bafd0)
        n.write_words(u, 0xd43160, n.STOP + 16)
        n.write_words(u, 0xd43164, n.STOP + 16)
        n.write_floats(u, 0xd43278, [1., 0., 0.])
        n.write_floats(u, anchor, center)
        admitted = []

        def hook(machine, address, size, data):
            sp = machine.reg_read(UC_X86_REG_ESP)
            if address == n.STOP + 16:
                source = n.read_words(machine, sp + 4, 1)[0]
                machine.mem_write(scene, bytes(machine.mem_read(source, 0xb00)))
                machine.reg_write(UC_X86_REG_EIP, n.STOP)
            elif address == 0x834660:
                admitted.append(True)
                machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])
                machine.reg_write(UC_X86_REG_ESP, sp + 12)

        u.hook_add(UC_HOOK_CODE, hook)
        n.invoke(u, 0x875f80, [anchor, 1])
        n.write_words(u, scene, 1)
        n.write_words(u, 0xaeedd0, unit)
        n.write_words(u, 0xaeedc8, 0)
        n.write_words(u, unit + 8, 0x20)
        n.write_words(u, unit + 0x34, model)
        for offset, radius in itertools.product(itertools.product([-40., -20., 0., 20., 40.], repeat=2), [.249, .25, 2., 10001.]):
            for z in [-10., 0., 10.]:
                position = [center[0] + offset[0], center[1] + offset[1], center[2] + z]
                minimum = [value - 1. for value in position]
                maximum = [value + 1. for value in position]
                n.write_floats(u, unit + 0x44, [radius, *minimum, *maximum])
                admitted.clear()
                n.invoke(u, 0x7bb9d0, [scene, 0, 0])
                lines.append('unit ' + ' '.join(map(str, [*center, *ray, *camera_values, *minimum, *maximum, radius, int(bool(admitted))])))
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--cropped', action='store_true')
    args = parser.parse_args()
    args.output.write_text(capture(args.executable, args.cropped))
