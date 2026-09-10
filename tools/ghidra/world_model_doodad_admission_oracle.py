"""Capture complete 799B70 model admission and final 7C1730 fog queries.

Resident model records and group reference/clip lists are supplied. The model
activation side effect and downstream 791CB0 submission are recorded stubs;
class selection, clipping, per-frame deduplication and fog queries run natively.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from world_scene_bounds_oracle import floats
from world_model_portal_projection_oracle import words


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('frames', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    entity, descriptor, references, clips, window, palette, sun, query = [n.HEAP + i * 0x2000 for i in range(8)]
    frame = floats(next(line for line in args.frames.read_text().splitlines() if not line.startswith('#')))
    n.write_floats(u, 0xcdb108, frame[64:88])
    n.write_words(u, 0xcd8798, 0)
    n.write_words(u, 0xce04a8, sun)
    n.write_floats(u, 0xcd7668, [100.])
    colors = [0xff204060, 0xffc08020]
    for offset, color in zip([0x8c, 0xa0], colors):
        n.write_words(u, palette + offset, color)
        n.write_floats(u, palette + offset + 4, [0., 1., 1.])
    models = [(.5, [10., 0., 0.]), (2., [10., 4., 0.]), (6., [50., -10., 0.]),
              (16., [50., 10., 0.]), (101., [120., 0., 0.])]
    rows = ['# Native 799B70 with activation/submission stubs; unhooked 78FB60, 983FB0 and 7C1730.',
            '# model extent center3 radius (hex f32); visit frame depth detail bank references windows; result fog banks (-1 unaccepted) and original query RGB per model.',
            'colors ' + ' '.join(f'{color:08x}' for color in colors)]
    for index, (extent, center) in enumerate(models):
        address = entity + index * 0x200
        n.write_words(u, address + 0xc, 0x84)
        n.write_words(u, address + 0x20, 2)
        u.mem_write(address + 0x24, bytes([index]))
        n.write_words(u, address + 0x34, address + 0x180)
        n.write_floats(u, address + 0x38, center + [.5])
        rows.append('model ' + words([extent, *center, .5]))
    submitted = []

    def hook(machine, address, size, unused):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x823f10:
            return_value(machine, 0)
            machine.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x791cb0:
            model = n.read_words(machine, sp + 4, 1)[0]
            submitted.append((model - entity) // 0x200)
            return_value(machine, 0)
        elif address == 0x7ecef0:
            return_value(machine, palette)
    u.hook_add(UC_HOOK_CODE, hook)
    left, right, full = [0., 0., 1., .5], [0., .5, 1., 1.], [0., 0., 1., 1.]
    sequence = [(0., []), (100., [left]), (0., [right]), (0., [left, right]),
                (0., [full]), (200., [right]), (0., [right]), (0., [left])]
    for scene_frame in range(1, 5):
        n.write_words(u, 0xcd87b0, scene_frame)
        detail = [1., .7777, .5, 1.5][scene_frame - 1]
        n.invoke(u, 0x78f570, [bits(detail)])
        for ordinal, (depth, windows) in enumerate(sequence):
            bank = (scene_frame + ordinal) % 2
            ids = list(range(len(models))) if ordinal % 2 == 0 else list(reversed(range(len(models))))
            n.write_words(u, descriptor, 4, 0, references)
            for slot, index in enumerate(ids):
                address = references + slot * 16
                following = references + (slot + 1) * 16 if slot + 1 < len(ids) else 1
                n.write_words(u, address + 4, entity + index * 0x200, following)
            for index, values in enumerate(windows):
                n.write_floats(u, window, values)
                n.invoke(u, 0x790e20, [0xcdb108, window])
                u.mem_write(clips + index * 0x100, bytes(u.mem_read(0xcdb168, 0xfc)))
                n.write_words(u, clips + index * 0x100 + 0xf8, clips + (index + 1) * 0x100 if index + 1 < len(windows) else 1)
            n.invoke(u, 0x78fb60, [bits(depth)])
            minimum = u.reg_read(UC_X86_REG_EAX) & 0xffff
            submitted.clear()
            n.invoke(u, 0x799b70, [descriptor, clips if windows else 0, minimum, bank])
            rows.append(f'visit {scene_frame} ' + words([depth, detail]) + f' {bank} {len(ids)} ' +
                        ' '.join(map(str, ids)) + f' {len(windows)} ' + words([v for values in windows for v in values]))
            result = []
            for index in range(len(models)):
                address = entity + index * 0x200
                seen = n.read_words(u, address + 0xb0, 1)[0] == scene_frame and u.mem_read(address + 0x25, 1)[0] == 0
                fog = int(bool(n.read_words(u, address + 0xc, 1)[0] & 0x8000)) if seen else -1
                u.reg_write(UC_X86_REG_ECX, address)
                n.invoke(u, 0x7c1730, [query])
                result.append(str(fog) + ' ' + ' '.join(f'{word:08x}' for word in n.read_words(u, query + 0xb8, 3)))
            rows.append('result ' + ' '.join(result))
            rows.append(f'submitted {len(submitted)} ' + ' '.join(map(str, submitted)))
    args.output.write_text('\n'.join(line.rstrip() for line in rows) + '\n', encoding='utf-8')
    print('Captured 32 native group submissions with 160 model fog queries')


if __name__ == '__main__':
    main()
