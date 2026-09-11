"""Capture original environment-shadow refresh, quantization and crop policy.

Runs 875F80's primary volume setup followed by 874890 and 7BAFD0. The
scene collector is intercepted after geometry construction, before any GPU or
client entry point. Uses controlled up vectors and an uncropped camera volume.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_ECX, UC_X86_REG_EAX
import wmo_registration_oracle as n


def capture(quality, origin):
    u = n.emulator()
    scene, anchor, camera = [n.HEAP+x for x in (0x1000, 0x3000, 0x4000)]
    device, geometry, d3d, table = [n.HEAP+x for x in (0x8000, 0xc000, 0xd000, 0xe000)]
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device+0x2918, 1)  # texture render target uses top-down viewport Y
    n.write_words(u, device+0x397c, d3d)
    n.write_words(u, d3d, table)
    n.write_words(u, table+0xbc, n.STOP+32)
    n.write_floats(u, geometry+8, [1024. if quality == 3 else 2048.]*2)
    n.write_floats(u, 0xd43180, [0., 0., -1.])
    n.write_words(u, 0xb1d51c, 0)
    n.write_words(u, 0xd25308, n.HEAP)
    n.write_words(u, n.HEAP+0x30, 0)
    n.write_words(u, 0xd43154, quality)
    n.write_words(u, 0xd43150, 1024 if quality == 3 else 2048)
    n.write_floats(u, 0xd43258, [20.])
    n.write_words(u, 0xd43158, 0x7bac10)
    n.write_words(u, 0xd4315c, 0x7bafd0)
    n.write_words(u, 0xd43160, n.STOP+16)
    n.write_words(u, 0xd43164, n.STOP+16)
    n.write_floats(u, 0xd43278, [1., 0., 0.])
    n.write_floats(u, anchor, origin)
    initializing = True
    pixel_viewport = []

    def collector(machine, address, size, context):
        sp = machine.reg_read(UC_X86_REG_ESP)
        if address == 0x682d70:
            machine.reg_write(UC_X86_REG_EAX, geometry)
            machine.reg_write(UC_X86_REG_EIP, n.read_words(machine, sp, 1)[0])
            machine.reg_write(UC_X86_REG_ESP, sp+4)
        elif address == n.STOP+32:
            pointer = n.read_words(machine, sp+8, 1)[0]
            pixel_viewport[:] = n.read_words(machine, pointer, 4)
            machine.reg_write(UC_X86_REG_EIP, n.STOP)
        elif address == n.STOP+16:
            source = n.read_words(machine, sp+4, 1)[0]
            if initializing:
                machine.mem_write(scene, bytes(machine.mem_read(source, 0xb00)))
            machine.reg_write(UC_X86_REG_EIP, n.STOP)

    u.hook_add(UC_HOOK_CODE, collector)
    n.invoke(u, 0x875f80, [anchor, 1])
    initial_scene = bytes(u.mem_read(scene, 0xb00))
    u.mem_write(camera, bytes(u.mem_read(scene+0x6c, 0xf4)))
    for index in range(3):
        # Exact 875D30 radius and squared refresh-distance tables.
        radius = n.read_floats(u, 0xb1d520+index*4, 1)[0]
        threshold = n.read_floats(u, 0xb1d52c+index*4, 1)[0]
        n.write_floats(u, 0xd43298+index*0x3c, [radius, threshold])
        n.write_floats(u, 0xd432b8+index*0x3c, [1., 0., 0.])
    initializing = False
    rows = []
    # Complete both 3x3 and 5x5 refresh cycles, then test equality and travel.
    offsets = ([(0., 0., 0.)]*28 + [(2., 0., 0.)]*2 + [(2.001, 0., 0.)]*10
               + [(40., -33., 9.)]*28 + [(-2.01, .25, -1.5)]*28)
    for frame, offset in enumerate(offsets):
        center = [a+b for a, b in zip(origin, offset)]
        u.mem_write(scene, initial_scene)
        n.write_words(u, 0xd4316c, (frame+1) % 256)
        n.write_floats(u, anchor, center)
        n.invoke(u, 0x874890, [scene, anchor, camera, 1])
        # Float fields stay raw words; no oracle-side policy or rounding model.
        words = list(n.read_words(u, scene, 6))
        words += n.read_words(u, scene+0x930, 9)
        words += n.read_words(u, scene+0x958, 3)
        words += n.read_words(u, scene+0x964, 12)
        words += n.read_words(u, scene+0x994, 12)
        for index in range(3):
            words += n.read_words(u, 0xd432a0+index*0x3c, 6)
            words += n.read_words(u, 0xd432c4+index*0x3c, 2)
        for index in range(3):
            if not words[3+index]:
                words += [0]*4
                continue
            viewport = list(n.read_words(u, scene+0x994+index*16, 4))
            n.invoke(u, 0x681f60, [*viewport, 0, 0x3f800000])
            u.reg_write(UC_X86_REG_ECX, device)
            n.invoke(u, 0x6a99e0, [])
            assert len(pixel_viewport) == 4
            words += pixel_viewport
        encoded = struct.pack('<'+'I'*len(words), *words).hex()
        rows.append('environment '+' '.join(map(str, [quality, frame, *origin,
                    *n.read_floats(u, anchor, 3)]))+' '+encoded)
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# quality frame initial_xyz center_xyz; words: flags3 updates3 scene_centers9 radii3 crops12 viewports12, each published_xyz pending_xyz counter buffer; pixel_viewports12']
    for quality in [3, 4, 5]:
        for origin in [(32.1251, -16.6254, 3.25), (-618.518, -4251.67, 38.718)]:
            rows += capture(quality, origin)
    args.output.write_text('\n'.join(rows)+'\n')
    print(f'Captured {len(rows)-1} original environment-shadow updates')
