"""Capture stock horizon CVar registration, clamp, and 791170 projection.

The original 78D7C0 callback, 77F4A0 setter and 791170/6BF370 projection run
from the fingerprinted PE. Controlled boundaries supply parsed CVar floats
and the camera's virtual FOV getter. The registration call's actual arguments
are captured at 78E638; neither defaults nor clamp results are substituted.
"""
import argparse
import itertools
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from world_model_portal_projection_oracle import words


def capture_registration():
    """Execute the original argument pushes for the horizon CVar registration."""
    u = n.emulator()
    output = []

    def hook(machine, address, size, unused):
        if address == 0x767fc0:
            args = n.read_words(machine, machine.reg_read(UC_X86_REG_ESP) + 4, 9)
            string = lambda ptr: bytes(machine.mem_read(ptr, 128)).split(b'\0')[0].decode()
            output.append((string(args[0]), string(args[3]), args[4]))
            machine.reg_write(UC_X86_REG_EIP, n.STOP)

    u.hook_add(UC_HOOK_CODE, hook)
    n.invoke(u, 0x78e615, [])
    assert output == [('horizonFarclipScale', '4.0', 0x78d7c0)]
    return output[0]


def capture(requested, camera_far, horizontal_fov, aspect):
    """Run the native multiplier callback and horizon projection unchanged."""
    u = n.emulator()
    camera, vtable, parsed, projection, world_frame = [n.HEAP + offset for offset in (0, 0x100, 0x200, 0x300, 0x1000)]
    n.write_floats(u, parsed, [requested])
    # 76FB80 is the decimal parser boundary: retain its float result and ret 4.
    u.mem_write(0x76fb80, b'\xd9\x05' + struct.pack('<I', parsed) + b'\xc2\x04\x00')
    n.invoke(u, 0x78d7c0, [0, 0, parsed])
    scale = n.read_floats(u, 0xadeecc, 1)[0]
    n.write_words(u, 0xb7436c, world_frame)
    n.write_words(u, world_frame + 0x7e20, camera)
    n.write_words(u, camera, vtable)
    n.write_words(u, vtable, n.STOP + 0x100)
    n.write_floats(u, camera + 0x3c, [camera_far, horizontal_fov, aspect])
    u.mem_write(n.STOP + 0x100, b'\xd9\x05' + struct.pack('<I', camera + 0x40) + b'\xc3')
    n.write_floats(u, 0xcd7748, [camera_far])
    arguments = []

    def hook(machine, address, size, unused):
        if address == 0x6bf370:
            arguments.extend(n.read_floats(machine, machine.reg_read(UC_X86_REG_ESP) + 4, 4))

    u.hook_add(UC_HOOK_CODE, hook)
    n.invoke(u, 0x791170, [projection])
    return [requested, camera_far, horizontal_fov, aspect, scale] + arguments + n.read_floats(u, projection, 16)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    name, default, callback = capture_registration()
    rows = [f'# native {name} default {default}, callback {callback:08x}; requested cameraFar horizontalFov aspect effectiveScale; 791170 verticalFov aspect near far; native projection16; hex f32']
    for inputs in itertools.product([-2., 1., 2.999, 3., 3.25, 4., 5.75, 6., 6.001, 20.], [183.33333, 350., 777., 1583.3334], [1.2566371], [1., 16/9]):
        rows.append(words(capture(*inputs)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {name} default {default} and {len(rows)-1} native horizon projections')


if __name__ == '__main__':
    main()
