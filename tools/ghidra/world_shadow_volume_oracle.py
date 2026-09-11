"""Capture native cropped environment admission and full-volume containment.

Executes 7BAFD0 with the exact per-map 874890 crop fields, then 7BB9D0's
world-box/plane admission and 983A60 with 7BA960's reversed exclusion planes.
The primary bootstrap and material callback are intercepted before GPU work.
Only the fingerprinted executable's arithmetic and spatial tests supply results.
"""
import argparse
import itertools
import math
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_ECX, UC_X86_REG_EAX
import wmo_registration_oracle as n


def invoke(machine, address, arguments):
    """Bound each leaf call by instructions without a per-call timer thread."""
    sp = n.STACK+0x18000
    n.write_words(machine, sp, n.STOP, *arguments)
    machine.reg_write(UC_X86_REG_ESP, sp)
    machine.emu_start(address, n.STOP, count=100_000)
    assert machine.reg_read(UC_X86_REG_EIP) == n.STOP


def capture(center, ray, map_index, cropped, region):
    """Construct one native volume and probe boxes around its center and edges."""
    u = n.emulator()
    scene, anchor, unit, model, volume, matrix = [n.HEAP+x for x in
                                                (0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000)]
    direction = [ray[0], ray[1], max(ray[2]*5., -1.2)]
    length = math.sqrt(sum(value*value for value in direction))
    n.write_floats(u, 0xd43180, [value/length for value in direction])
    n.write_words(u, 0xb1d51c, 0)
    n.write_words(u, 0xd25308, n.HEAP)
    n.write_words(u, n.HEAP+0x30, int(cropped))
    camera_min = [center[0]-8., center[1]-12., center[2]-4.]
    camera_max = [center[0]+9., center[1]+7., center[2]+25.]
    n.write_floats(u, 0xcdd168, [value for corner in itertools.product(*zip(camera_min, camera_max)) for value in corner])
    n.write_words(u, 0xd43154, 4)
    n.write_words(u, 0xd43150, 2048)
    n.write_floats(u, 0xd43258, [20.])
    n.write_words(u, 0xd43158, 0x7bac10)
    n.write_words(u, 0xd4315c, 0x7bafd0)
    n.write_words(u, 0xd43160, n.STOP+16)
    n.write_words(u, 0xd43164, n.STOP+16)
    n.write_floats(u, 0xd43278, [1., 0., 0.])
    n.write_floats(u, anchor, center)
    admitted = []

    def hook(machine, address, size, context):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == n.STOP+16:
            source = n.read_words(machine, sp+4, 1)[0]
            machine.mem_write(scene, bytes(machine.mem_read(source, 0xb00)))
            machine.reg_write(UC_X86_REG_EIP, n.STOP)
        elif address == 0x834660:
            admitted.append(True)
            machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])
            machine.reg_write(UC_X86_REG_ESP, sp+12)

    u.hook_add(UC_HOOK_CODE, hook)
    invoke(u, 0x875f80, [anchor, 1])
    radius = [40., 160., 640.][map_index]
    crop = [-radius, radius, -radius, radius]
    if region >= 0:
        # Explicit finite regions test 7BAFD0 separately from the refresh oracle.
        bounds = [-radius, -radius/3., radius/3., radius]
        crop = [bounds[region], bounds[region+1], -radius/3., radius/3.]
    n.write_floats(u, scene+0x930, center)
    n.write_floats(u, scene+0x958, [radius])
    n.write_floats(u, scene+0x964, crop)
    n.write_words(u, scene, 0)  # 8753F0 initializes each environment flag to zero.
    invoke(u, 0x7bafd0, [scene+0x6c, matrix, 0xd43278, scene, 0])
    u.reg_write(UC_X86_REG_ECX, scene+0x6c)
    invoke(u, 0x983990, [scene+0x24])
    u.mem_write(volume, bytes(u.mem_read(scene+0x6c, 0xf4)))
    u.reg_write(UC_X86_REG_ECX, volume)
    invoke(u, 0x7ba960, [])
    n.write_words(u, scene, 1)
    n.write_words(u, 0xaeedd0, unit)
    n.write_words(u, 0xaeedc8, 0)
    n.write_words(u, unit+8, 0x20)
    n.write_words(u, unit+0x34, model)
    rows = []
    offsets = list(itertools.product([-radius-2., -radius, -8., 0., 8., radius, radius+2.], repeat=2))
    for x, y in offsets:
        for z, half in [(0., 1.), (0., radius/2.), (1999., 1.), (-2000., 1.)]:
            position = [center[0]+x, center[1]+y, center[2]+z]
            minimum = [value-half for value in position]
            maximum = [value+half for value in position]
            n.write_floats(u, unit+0x44, [1., *minimum, *maximum])
            admitted.clear()
            invoke(u, 0x7bb9d0, [scene, 0, 0])
            u.reg_write(UC_X86_REG_ECX, volume)
            invoke(u, 0x983a60, [unit+0x48])
            contained = u.reg_read(UC_X86_REG_EAX) == 3
            fields = [map_index, int(cropped), *center, *ray, *camera_min, *camera_max,
                      *crop, *minimum, *maximum, int(bool(admitted)), int(contained)]
            rows.append('volume '+' '.join(map(str, fields)))
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# map camera_cull center3 raw_ray3 camera_min3 camera_max3 crop4 minimum3 maximum3 admitted contained']
    for center, ray, map_index, cropped, region in itertools.product(
            [(0., 0., 0.), (15302., -12840., 1200.)],
            [(0., 0., -1.), (.3, .4, -.8660254)], range(3), [False, True], [-1, 0, 1, 2]):
        rows += capture(center, ray, map_index, cropped, region)
    args.output.write_text('\n'.join(rows)+'\n')
    print(f'Captured {len(rows)-1} original shadow volume queries')
