"""Check the original camera-to-fog handoff independently of horizon scale.

Runs 4F8410 through its camera far-plane store, then 7ECD80 and 7F16F0.
The sampled exterior palette is supplied at 7EBFF0; WMO and liquid queries
return absent. The sample is the installed Durotar palette at map 1,
(1300, -4530, 50), midnight: end 888.8889, start ratio 0.5.
This bounds the exterior camera/fog distance coupling, not every scene fade.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESI, UC_X86_REG_EIP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from world_fog_policy_oracle import bits


def capture(scale):
    """Execute original registration callback and exterior fog arithmetic."""
    u = n.emulator()
    camera, parsed, world_frame, owner = [n.HEAP + offset for offset in (0, 0x200, 0x1000, 0xa000)]
    n.write_floats(u, parsed, [scale])
    u.mem_write(0x76fb80, b'\xd9\x05' + struct.pack('<I', parsed) + b'\xc2\x04\x00')
    n.invoke(u, 0x78d7c0, [0, 0, parsed])
    n.write_words(u, world_frame + 0x7e20, camera)
    n.write_floats(u, camera + 0x3c, [777.])

    def hook(machine, address, size, unused):
        if address == 0x4f842d:
            machine.reg_write(UC_X86_REG_EIP, n.STOP)
        elif address == 0x7ebff0:
            n.write_words(machine, owner + 0x48, bits(888.8889), bits(.5), bits(1.))
            return_value(machine, 0)
        elif address in (0x77fb90, 0x780620):
            return_value(machine, 0)

    u.hook_add(UC_HOOK_CODE, hook)
    u.reg_write(UC_X86_REG_ECX, world_frame)
    n.invoke(u, 0x4f8410, [0])
    handed_far = n.read_floats(u, 0xd38b40, 1)[0]
    n.write_words(u, 0xd38acc, 0)
    u.reg_write(UC_X86_REG_ESI, owner)
    n.invoke(u, 0x7ecd80, [])
    n.write_words(u, 0xd38c1c, *n.read_words(u, owner + 0x48, 3))
    n.write_words(u, 0xd38bf4, 0xff342a4a)
    n.invoke(u, 0x7f16f0, [])
    fog = n.read_floats(u, 0xd38b90, 3)
    assert handed_far == 777. and fog == [388.5, 777., 1.], (handed_far, fog)
    return [n.read_floats(u, 0xadeecc, 1)[0], handed_far] + fog


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    for scale in (3., 4., 6.):
        print('horizonScale cameraFogFar fogStart fogEnd exponent:', *capture(scale))


if __name__ == '__main__':
    main()
