"""Capture build-12340 ripple texture matrices from 4C3290 and 7E2D60.

Only the unused camera-position query is skipped: ripple callers pass mode 1,
which projects absolute world positions. All matrix construction, products,
trigonometry and float stores execute original instructions. Each 84-byte
record stores position XYZ, radius, stored yaw, then the 16 matrix floats.
"""

import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as n
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_EBP


def word(value):
    """Pass the exact single-precision bits expected by the native stack ABI."""
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture(executable, output, vertex_output=None):
    """Run complete native matrix functions over bounded world-space inputs."""
    n.initialize(executable)
    u = n.emulator()
    rotation, bounds, projection, depth, reset = [n.HEAP + index * 0x1000 for index in range(5)]
    u.mem_write(reset, b'\xdb\xe3')
    identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]

    def hook(u, address, size, data):
        sp = u.reg_read(UC_X86_REG_ESP)
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 4)

    u.hook_add(UC_HOOK_CODE, hook, begin=0x4f6650, end=0x4f6650)
    rng = random.Random(0x7e2d60)
    cases = [(10., 20., .25, 2., yaw) for yaw in [0., .25, -.25, 1.5707964, 3.1415927, 6.2831855]]
    cases += [(rng.uniform(-16000., 16000.), rng.uniform(-16000., 16000.),
               rng.uniform(-100., 1000.), rng.uniform(.1, 8.), rng.uniform(-7., 7.))
              for _ in range(256)]
    records = bytearray()
    vertices = bytearray()
    point, transformed, record, frame = [n.HEAP + index * 0x1000 for index in range(5, 9)]
    for values in cases:
        values = struct.unpack('<5f', struct.pack('<5f', *values))
        x, y, z, radius, yaw = values
        u.emu_start(reset, reset + 2)
        n.write_floats(u, rotation, identity)
        n.write_floats(u, projection, identity)
        n.write_floats(u, depth, identity)
        n.write_floats(u, bounds, [x-radius, y-radius, z-1., x+radius, y+radius, z+1.])
        n.invoke(u, 0x4c3290, [rotation, word(yaw)])
        n.invoke(u, 0x7e2d60, [projection, depth, bounds, rotation, word(.5), 1])
        records += struct.pack('<5f', *values) + bytes(u.mem_read(projection, 64))
        if vertex_output:
            for opacity in [0., .125, .5, .875, 1., rng.random()]:
                opacity = struct.unpack('<f', struct.pack('<f', opacity))[0]
                # Execute the original multiply, float store and nearest-even
                # FISTP through the completed little-endian RGBA word.
                n.write_floats(u, record + 0x18, [opacity])
                n.write_words(u, frame - 8, record)
                u.reg_write(UC_X86_REG_EBP, frame)
                u.reg_write(UC_X86_REG_ESP, n.STACK + 0x10000)
                u.emu_start(0x79ddad, 0x79dddd)
                color = bytes(u.mem_read(frame - 4, 4))
                for dx, dy in [(0., 0.), (1., 0.), (0., 1.), (-1., -1.), (2., 3.)]:
                    n.write_floats(u, point, [x + radius * dx, y + radius * dy, z + .125])
                    n.invoke(u, 0x4c21b0, [transformed, point, projection])
                    vertices += (bytes(u.mem_read(projection, 64))
                                 + struct.pack('<f', opacity)
                                 + bytes(u.mem_read(point, 12))
                                 + color + bytes(u.mem_read(transformed, 8)))
    Path(output).write_bytes(records)
    print('Captured', len(cases), 'native ripple projection matrices')
    if vertex_output:
        Path(vertex_output).write_bytes(vertices)
        print('Captured', len(vertices) // 92, 'native projected vertices and alpha words')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--vertex-output')
    args = parser.parse_args()
    capture(args.executable, args.output, args.vertex_output)
