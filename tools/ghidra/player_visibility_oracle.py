"""Execute original 6E0840/6DE980 player visibility and 714C40 retirement gate.

Runs ordinary Unit_C 730F30 and CObject 743300 beneath the player override.
Only the local GUID accessor is substituted. Async component rebuild, hidden
root models, vehicle seats and alternate effect owners are absent in this bank.
Requires the pinned, locally owned build-12340 executable.
"""
import argparse
import itertools
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EDX, UC_X86_REG_ECX
import wmo_registration_oracle as n
from camera_water_oracle import ret


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    unit, fields, player, bank, row, visible, hidden, vtable = [n.HEAP + i * 0x2000 for i in range(8)]
    n.write_words(u, unit, vtable)
    n.write_words(u, vtable + 0x90, 0x6e0840)
    n.write_words(u, unit + 8, fields)
    n.write_words(u, fields, 7, 0, 0x19)
    n.write_words(u, unit + 0x1008, player)
    n.write_words(u, 0xad4170, 40)
    n.write_words(u, 0xad416c, 41)
    n.write_words(u, 0xad4180, bank)
    n.write_words(u, bank, row, 0)
    local = True

    def hook(u, address, size, context):
        if address == 0x4d3790:
            u.reg_write(UC_X86_REG_EDX, 0)
            ret(u, 7 if local else 8)

    u.hook_add(UC_HOOK_CODE, hook)
    output = ['# player-flags map-id map-kind local camera-visible scene-flags culled hidden retire-allowed']
    for flags, map_id, kind, local, camera, scene in itertools.product(
            [0, 0x80000, 0x400000, 0x480000, 0xffffffff, 0xffb7ffff],
            [39, 40, 41, 42], [0, 3, 4], [False, True], [False, True], [0, 1, 4, 5]):
        n.write_words(u, player + 8, flags)
        n.write_words(u, 0xbd088c, map_id)
        n.write_words(u, row + 8, kind)
        n.write_words(u, 0xc9d540, int(camera))
        n.write_words(u, visible, 0)
        n.write_words(u, hidden, 0)
        u.reg_write(UC_X86_REG_ECX, unit)
        n.invoke(u, 0x6e0840, [scene, visible, hidden])
        outputs = [n.read_words(u, visible, 1)[0], n.read_words(u, hidden, 1)[0]]
        u.reg_write(UC_X86_REG_ECX, unit)
        n.invoke(u, 0x714c40, [])
        outputs.append(u.reg_read(UC_X86_REG_EAX) & 255)
        values = [flags, map_id, kind, int(local), int(camera), scene, *outputs]
        output.append(' '.join(f'{value:08x}' for value in values))
    args.output.write_text('\n'.join(output) + '\n')
    print(f'Captured {len(output) - 1} native visibility cases')


if __name__ == '__main__':
    main()
