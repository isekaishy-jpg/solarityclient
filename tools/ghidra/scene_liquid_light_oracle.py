"""Native scene publication/query through 8A38B0's liquid shader constants.

Executes 8356F0/834C70, 834F60/81E400, and 8A38B0 from the fingerprinted
image. Only the fog-enabled provider is stubbed. No client/OS entry point runs.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from water_light_uniform_oracle import cross

POSITIONS = [(-40., 0., 4.), (-1., 0., 1.), (1., 0., 1.), (0., 1., 1.), (20., 0., 2.), (40., 0., 4.)]

def directional_order():
    u = n.emulator()
    scene, lights = n.HEAP, n.HEAP + 0x1000
    for index in range(4):
        n.write_words(u, lights + index * 0x80, scene, 0, 0)
    rows = ['# enabled mask followed by native directional linked-list source IDs']
    for mask in (3, 3, 2, 3, 7, 5, 13, 15, 10, 15, 0, 15):
        for index in range(4):
            u.reg_write(UC_X86_REG_ECX, lights + index * 0x80)
            n.invoke(u, 0x8356f0, [int(bool(mask & (1 << index)))])
        order = []
        light = n.read_words(u, scene + 0x20, 1)[0]
        while light:
            order.append((light - lights) // 0x80)
            light = n.read_words(u, light + 0x68, 1)[0]
        rows.append(' '.join(map(str, [mask] + order)))
    return '\n'.join(rows) + '\n'

def capture(count, directionals, camera):
    u = n.emulator()
    scene, query, exterior, device, lights, grid = [n.HEAP + x for x in (0, 0x400, 0x800, 0x1000, 0x4000, 0x8000)]
    n.write_words(u, scene + 0x14, 7)
    n.write_words(u, scene + 0x24, grid)
    n.write_floats(u, query + 4, [0., 0., 0., 35.])
    n.write_words(u, exterior + 0x60, 1)
    n.write_floats(u, exterior + 0x24, [0., 0., -1.])
    n.write_floats(u, exterior + 0x30, [.125, .25, .375, .5, .625, .75, .25, .375, .5])
    u.reg_write(UC_X86_REG_ECX, query)
    n.invoke(u, 0x834f60, [exterior])
    for index, position in enumerate(POSITIONS[:count]):
        light = lights + index * 0x80
        n.write_words(u, light, scene, 7, 1)
        n.write_floats(u, light + 0xc, position)
        n.write_floats(u, light + 0x3c, [1. + index, 64. + 16. * index, 255. - 32. * index])
        n.write_floats(u, light + 0x54, [0., .7, .03])
        u.reg_write(UC_X86_REG_ECX, light)
        n.invoke(u, 0x8356f0, [1])
    for index in range(directionals):
        light = lights + 0x800 + index * 0x80
        n.write_words(u, light, scene, 7, 0)
        n.write_floats(u, light + 0x24, [1., 0., 0.] if index == 0 else [0., -1., 0.])
        n.write_floats(u, light + 0x30, [.0625 * (index + 1)] * 3 + [.25 * (index + 1), .5, .75])
        u.reg_write(UC_X86_REG_ECX, light)
        n.invoke(u, 0x8356f0, [1])
    u.reg_write(UC_X86_REG_ECX, scene)
    n.invoke(u, 0x81e400, [query])
    backward = [[1.,0.,0.],[-1.,0.,0.],[0.,1.,0.],[0.,-1.,0.],[0.,0.,1.],[0.,0.,-1.]][camera]
    forward = [-v for v in backward]
    right = cross(forward, [0.,0.,1.] if camera < 4 else [0.,1.,0.])
    up = cross(right, forward)
    view = [v for axis in range(3) for v in (right[axis], up[axis], backward[axis], 0.)] + [0.,0.,0.,1.]
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device + 0x1af8, 0)
    n.write_floats(u, device + 0x1b00, view)
    u.hook_add(UC_HOOK_CODE, lambda uc, address, size, context: return_value(uc, 0) if address == 0x683100 else None)
    n.invoke(u, 0x8a38b0, [query])
    return struct.pack('<4I', count, directionals, camera, min(n.read_words(u, query + 0xa4, 1)[0], 3)) + bytes(u.mem_read(0xd44eb8, 208))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = [capture(count, directional, camera) for count in range(7) for directional in (0, 2) for camera in range(6)]
    args.output.write_bytes(b''.join(rows))
    args.output.with_suffix('.order.txt').write_text(directional_order())
    print(f'{len(rows)} original scene-to-liquid constant captures')
