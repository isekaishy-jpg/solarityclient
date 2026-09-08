"""Capture the native daylight ray through the actual liquid constant writer.

Runs pinned 7EEA90, 834AE0, 834F60 and 8A38B0: day-table interpolation,
normalization, one exterior directional light, and view-space shader constants.
The renderer owns a supplied resident view matrix; the sole hooked provider is
683100's fog-enabled query. No OS or client entry point runs.
"""

import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def cross(a, b):
    return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]


def capture(half_minutes, camera):
    """Six cardinal cameras keep the supplied view bases exactly representable."""
    uc = native.emulator()
    scene, light, device = native.HEAP, native.HEAP + 0x400, native.HEAP + 0x1000
    native.write_floats(uc, 0xd38b04, [half_minutes / 2880.])
    native.invoke(uc, 0x7eea90, [])
    uc.reg_write(UC_X86_REG_ECX, light)
    native.invoke(uc, 0x834ae0, [0xd38c9c])
    native.write_words(uc, light + 0x60, 1)
    native.write_floats(uc, light + 0x30, [v / 255. for v in (64, 96, 128, 128, 160, 192, 192, 224, 255)])
    uc.reg_write(UC_X86_REG_ECX, scene)
    native.invoke(uc, 0x834f60, [light])
    backward = [[1.,0.,0.],[-1.,0.,0.],[0.,1.,0.],[0.,-1.,0.],[0.,0.,1.],[0.,0.,-1.]][camera]
    up = [0.,0.,1.] if camera < 4 else [0.,1.,0.]
    forward = [-v for v in backward]
    right = cross(forward, up)
    up = cross(right, forward)
    view = [v for axis in range(3) for v in (right[axis], up[axis], backward[axis], 0.)] + [0.,0.,-8.,1.]
    native.write_words(uc, 0xc5df88, device)
    native.write_words(uc, device + 0x1af8, 0)
    native.write_floats(uc, device + 0x1b00, view)
    native.write_floats(uc, scene + 0xa8, [20., 100., 0., 1., 32/255., 48/255., 64/255.])
    # The supplied RH camera has negative forward Z, as the Vulkan camera does.
    native.write_floats(uc, 0xd4300c, [-1.])

    def provider(uc, address, size, context):
        if address == 0x683100:
            return_value(uc, 1)

    uc.hook_add(UC_HOOK_CODE, provider)
    native.invoke(uc, 0x8a38b0, [scene])
    return (struct.pack('<II', half_minutes, camera)
            + bytes(uc.mem_read(0xd44ce8, 16))
            + bytes(uc.mem_read(0xd44eb8, 64)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    native.initialize(args.executable)
    times = sorted(set(range(0, 2880, 120)) | {2, 718, 722, 1438, 1442, 2158, 2162, 2878})
    records = [capture(time, camera) for time in times for camera in range(6)]
    Path(args.output).write_bytes(b''.join(records))
    print(f'{len(records)} original daylight-to-liquid shader constant captures')


if __name__ == '__main__':
    main()
