"""Execute the complete native WMO submerged-liquid query (7C8360).

Only the LiquidType database lookup is supplied by the fixture. Native grid
addressing, floor, mask admission, x87 interpolation and comparison all run.
"""
import argparse
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from movement_ground_trajectory_oracle import invoke, bits
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    group, vertices, tiles, point, result, definition = [native.HEAP + n * 0x1000 for n in range(6)]
    native.write_words(uc, group + 0x114, 3, 3, 2, 2)
    native.write_words(uc, group + 0x134, vertices, tiles)

    def database(u, address, _size, _data):
        if address != 0x65c290:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        assert native.read_words(u, sp + 4, 1)[0] in (21, 22)
        u.reg_write(UC_X86_REG_EAX, definition)
        u.reg_write(UC_X86_REG_EIP, native.read_words(u, sp, 1)[0])
        u.reg_write(UC_X86_REG_ESP, sp + 8)

    uc.hook_add(UC_HOOK_CODE, database)
    rng = random.Random(12340)
    rows = ['# 7c8360: flags id cornerXYZ pointXYZ heights9 tiles4 -> admitted id height (hex words)']
    for case in range(512):
        flags = 4 if case & 1 else 0
        liquid_id = 22 if flags else 21
        corner = [0., 0., 0.] if case < 128 else [rng.uniform(-100,100), rng.uniform(-100,100), 0.]
        corner = [struct.unpack('<f', struct.pack('<f', v))[0] for v in corner]
        heights = [0., 1., 7., 2., 8., 3., 4., 5., 6.] if case < 256 else [rng.uniform(-10,10) for _ in range(9)]
        masks = [0x40, 0x4f if case & 2 else 0x40, 0x40, 0x40]
        if case < 128:
            offsets = [-.001, 0., 2.08333325, 4.1666665, 4.166667, 6.25, 8.333333, 8.333334]
            position = [corner[0] + offsets[(case // 2) % 8], corner[1] + offsets[(case // 16) % 8], 2.005]
        else:
            position = [corner[0] + rng.uniform(-.01,8.34), corner[1] + rng.uniform(-.01,8.34), rng.uniform(-10,10)]
        native.write_floats(uc, group + 0x124, corner)
        native.write_words(uc, group + 0x144, liquid_id)
        native.write_words(uc, definition, liquid_id, 0, flags)
        for i, height in enumerate(heights):
            native.write_words(uc, vertices + i * 8, 0, bits(height))
        uc.mem_write(tiles, bytes(masks))
        native.write_floats(uc, point, position)
        native.write_words(uc, result, 0, 0)
        uc.reg_write(UC_X86_REG_ECX, group)
        invoke(uc, 0x7c8360, [point, result, result + 4])
        values = [flags, liquid_id] + [bits(v) for v in corner + position + heights] + masks
        values += [uc.reg_read(UC_X86_REG_EAX) & 255, *native.read_words(uc, result, 2)]
        rows.append(' '.join(f'{value:x}' for value in values))
    Path(output).write_text('\n'.join(rows) + '\n')


def capture_camera(executable, output):
    """Retain complete native root/group camera selection with real BSP probes."""
    native.initialize(executable)
    rows = ['# 7d59b0 camera: profile mogi0 mogp0 mogi1 mogp1 transformed adjacent side; startXYZ endXYZ maximum distance (float hex); admitted group secondary_group']
    rng = random.Random(12340)
    for case in range(256):
        profile = case % 6
        mogi = [rng.choice([0, 8, 0x80, 0x2000]), rng.choice([0,8])]
        mogp = [rng.choice([0, 8, 0x80, 0x410000]), rng.choice([0,8])]
        transformed, adjacent = case & 1, (case >> 1) & 1
        side = rng.choice([-1, 0, 1])
        start = [rng.choice([0., 2., 3., 3.001, -3.]), rng.choice([0., 2., -2.]), rng.choice([4., 1., .1, -.1])]
        end = [start[0], start[1], rng.choice([-4., -1., 4.])]
        maximum, distance = rng.choice([.1, .5, 1., 1.05]), rng.choice([0., .0005, -.0005])
        result = native.floor_probe(start, end, profile=profile, maximum=maximum, cached=True,
            registration=dict(point=start, mogi=mogi, mogp=mogp, transformed=transformed,
                adjacent_floor=adjacent, side=side, distance=distance, camera=True))
        settings = [profile,mogi[0],mogp[0],mogi[1],mogp[1],transformed,adjacent,side]
        rows.append(' '.join(map(str,settings)) + ' ' + ' '.join(f'{bits(v):x}' for v in start+end+[maximum,distance]) + ' ' + ' '.join(map(str,result)))
    Path(output).write_text('\n'.join(rows) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output')
    parser.add_argument('--camera-output')
    args = parser.parse_args()
    capture(args.executable, args.output)
    if args.camera_output:
        capture_camera(args.executable, args.camera_output)
