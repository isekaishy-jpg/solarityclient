"""Capture 7998A0's exterior doodad buckets and 7987A0's clipping/class gate.

The original intrusive list implementation, 78F570/78FB60 and 7C1730 run.
Activation, downstream 791CB0 submission and external occlusion return stubs;
the optional software occlusion path is disabled through the scene flags.
"""
import argparse
import itertools
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
    entity, descriptor, reference, window, palette, sun, query = [n.HEAP + i * 0x1000 for i in range(7)]
    camera = floats(next(line for line in args.frames.read_text().splitlines() if not line.startswith('#')))
    n.write_floats(u, 0xcdb108, camera[64:88])
    n.write_floats(u, window, [0., 0., 1., 1.])
    n.write_words(u, 0xcd8798, 0)
    n.invoke(u, 0x790e20, [0xcdb108, window])
    n.write_floats(u, 0xcd8f90, [1., 0., 0., 0.])
    n.write_floats(u, 0xadf454, [.5])
    n.write_words(u, 0xcd774c, 1)
    n.write_words(u, 0xcd87b0, 1)
    n.write_words(u, descriptor, 4, 0, reference)
    n.write_words(u, reference + 4, entity, 1)
    n.write_words(u, entity + 0x34, entity + 0x300)
    n.write_words(u, entity + 0x20, 2)
    n.write_words(u, 0xce04a8, sun)
    n.write_floats(u, 0xcd7668, [100.])
    for offset, color in [(0x8c, 0xff204060), (0xa0, 0xffc08020)]:
        n.write_words(u, palette + offset, color)
        n.write_floats(u, palette + offset + 4, [0., 1., 1.])
    submitted = []

    def hook(machine, address, size, unused):
        if address == 0x823f10:
            sp = machine.reg_read(UC_X86_REG_ESP)
            return_value(machine, 0)
            machine.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x791cb0:
            submitted.append(1)
            return_value(machine, 0)
        elif address == 0x7cce00:
            return_value(machine, 0)
        elif address == 0x7ecef0:
            return_value(machine, palette)
    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# camera fixture 0; original 7998A0 buckets then 7987A0 admission and 7C1730 fog.',
            '# center3 radius extent detail hex floats; minimumBin initialFog queuedBin submitted finalFog queryRGB(hex).']
    cases = [(10., 0., .5), (34., 0., .5), (34., 20., .5), (99., 0., .5),
             (120., 0., .5), (-10., 0., .5), (2133.833, 0., .5), (2133.834, 0., .5)]
    for (x, y, radius), category, minimum, detail, bank in itertools.product(cases, [0, 2, 4], [0, 2, 63], [0.5, 1.], [0, 1]):
        center = [x, y, 0.]
        extent = [.5, 2., 6., 16., 101.][category]
        n.write_floats(u, entity + 0x38, center + [radius])
        n.write_words(u, entity + 0xc, 0x84 | (0x8000 if bank else 0))
        n.write_words(u, entity + 0xa8, 0, 0, 0)
        u.mem_write(entity + 0x24, bytes([category, 1]))
        for index in range(64):
            head = 0xcd906c + index * 0x6c
            n.write_words(u, head, 0xa8, head + 4, (head + 4) | 1)
        n.invoke(u, 0x78f570, [bits(detail)])
        n.invoke(u, 0x7998a0, [descriptor, minimum])
        queued = [index for index in range(64) if n.read_words(u, 0xcd9074 + index * 0x6c, 1)[0] == entity]
        assert len(queued) <= 1
        selected = queued[0] if queued else -1
        submitted.clear()
        if selected >= 0:
            distance = struct.unpack('<f', struct.pack('<f', selected * n.read_floats(u, 0xa3e554, 1)[0]))[0]
            n.invoke(u, 0x78fb60, [bits(distance)])
            minimum_class = u.reg_read(UC_X86_REG_EAX) & 0xffff
            n.invoke(u, 0x7987a0, [0xcd9048 + selected * 0x6c, minimum_class])
        u.reg_write(UC_X86_REG_ECX, entity)
        n.invoke(u, 0x7c1730, [query])
        final_bank = int(bool(n.read_words(u, entity + 0xc, 1)[0] & 0x8000))
        rows.append(words(center + [radius, extent, detail]) + f' {minimum} {bank} {selected} {len(submitted)} {final_bank} ' +
                    ' '.join(f'{word:08x}' for word in n.read_words(u, query + 0xb8, 3)))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-2} original exterior doodad cases')


if __name__ == '__main__':
    main()
