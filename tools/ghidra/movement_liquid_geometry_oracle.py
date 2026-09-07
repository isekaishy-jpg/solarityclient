"""Capture original water collision faces from 7CE960 and 7C94B0/782740.

Supplies retained decoded grids, complete fixed-capacity output arrays, and
the exact local query rectangle. No instruction hooks or client entry point.
Native grid selection, tile masks, Z outcodes, winding, matrix application,
normalization, and plane offsets execute in the pinned executable.
"""
import argparse
import math
import random
import struct
from pathlib import Path

import wmo_registration_oracle as native
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def capture(executable, output):
    native.initialize(executable)
    uc = native.emulator()
    owner, chunk, provider, vertices, mask, query, rectangle, output_array, faces, guids, root = [
        native.HEAP + off for off in (0, 0x2000, 0x3000, 0x4000, 0x6000, 0x7000, 0x7100, 0x8000, 0x9000, 0x18000, 0x20000)]
    rng = random.Random(0x7ce960)
    records = bytearray()
    for kind in range(2):
        for case in range(64):
            width, height = [(1, 1), (3, 5), (8, 8), (2, 4)][case % 4]
            xoff, yoff = ((case % 3, case % 4) if width < 8 and kind == 0 else (0, 0))
            corner = [0., 0., 0.] if kind == 0 else [-1.12345, 21.334, 3.5]
            corner = list(map(f32, corner))
            translation = [0., 0., 0.] if kind == 0 else [12.5, -31.25, 14.]
            transform = [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,*translation,1.]
            flags = [0, 0x80, 0x40, 0x90][case % 4]
            count = (width+1)*(height+1)
            heights = [f32(rng.uniform(-2., 5.)) for _ in range(count)]
            tiles = [15 if (i+case) % 5 == 0 else flags for i in range(width*height)]
            zmin, zmax = [(-100.,100.), (-1.,1.), (3.,4.), (5.02,10.)][case % 4]
            if case % 8 == 4:
                heights = [0.] * count
                zmin, zmax = -.019444445, -.019444443
            if kind == 0:
                r0, c0 = (0, 0) if case % 3 else (2, 1)
                r1, c1 = (7, 7) if case % 3 else (5, 4)
                local_box = [-4.1666665*(r1+1)+.3, -4.1666665*(c1+1)+.3, zmin,
                    -4.1666665*r0-.3, -4.1666665*c0-.3, zmax]
                native.write_words(uc, owner + 0x34, yoff, xoff, yoff+height, xoff+width, provider, mask)
                native.write_words(uc, owner + 0x5c, chunk)
                native.write_floats(uc, chunk + 0x7c, corner)
                raw_vertices = [[f32((r+yoff)*f32(-4.1666665)), f32((c+xoff)*f32(-4.1666665)), heights[r*(width+1)+c]] for r in range(height+1) for c in range(width+1)]
                native.write_floats(uc, owner+0x78, [x for p in raw_vertices for x in p])
                exists = sum((int(v != 15) << i) for i,v in enumerate(tiles))
                uc.mem_write(mask, exists.to_bytes(8, 'little'))
                native.write_words(uc, rectangle, r0, c0, r1, c1)
            else:
                if case % 3:
                    local_box = [corner[0]-1, corner[1]-1, zmin, corner[0]+width*4.1666665+1, corner[1]+height*4.1666665+1, zmax]
                else:
                    local_box = [corner[0]+.3, corner[1]+.3, zmin, corner[0]+5., corner[1]+7., zmax]
                native.write_words(uc, owner + 0x114, width+1, height+1, width, height)
                native.write_floats(uc, owner + 0x124, corner)
                native.write_words(uc, owner + 0x138, mask)
                native.write_words(uc, owner + 0x144, 21)
                native.write_words(uc, owner + 0x1c, provider)
                native.write_words(uc, provider + 0x10, vertices)
                raw_vertices = []
                y = corner[1]
                for r in range(height+1):
                    x = corner[0]
                    for c in range(width+1):
                        raw_vertices.append([x,y,heights[r*(width+1)+c]])
                        x = f32(x+f32(4.1666665))
                    y = f32(y+f32(4.1666665))
                native.write_floats(uc, vertices, [x for p in raw_vertices for x in p])
                uc.mem_write(mask, bytes(tiles))
                native.write_floats(uc, root+0x70, transform)
            local_box = list(map(f32, local_box))
            native.write_floats(uc, query, local_box)
            native.write_words(uc, 0xcb7528, 0, 0, 0, 0, 0)
            native.write_words(uc, 0xcf08f8, 1)
            native.write_words(uc, output_array, 512, 0, faces, 512, 512, 0, guids, 512)
            uc.reg_write(UC_X86_REG_ECX, owner)
            try:
                if kind == 0:
                    native.invoke(uc, 0x7ce960, [query+0x100, query, rectangle])
                    native.invoke(uc, 0x782740, [query+0x100, output_array, 0, 0])
                else:
                    native.invoke(uc, 0x7ca110, [query, 0x20000, output_array, root])
            except Exception as error:
                raise RuntimeError((kind, case, hex(uc.reg_read(UC_X86_REG_EIP)))) from error
            nfaces = native.read_words(uc, output_array+4, 1)[0]
            # Store world-space query bounds, exactly inverse-translatable by tests.
            world_box = [f32(v+translation[i%3]) for i,v in enumerate(local_box)]
            # WMO collection reloads the rounded local query after subtracting
            # the retained translation; preserve this boundary in its capture.
            if kind == 1:
                local_box = [f32(v-translation[i%3]) for i,v in enumerate(world_box)]
                native.write_floats(uc, query, local_box)
                native.write_words(uc, 0xcb7528, 0, 0, 0, 0, 0)
                native.write_words(uc, output_array+4, 0)
                uc.reg_write(UC_X86_REG_ECX, owner)
                native.invoke(uc, 0x7ca110, [query, 0x20000, output_array, root])
                nfaces = native.read_words(uc, output_array+4, 1)[0]
            records.extend(struct.pack('<5I25fI',kind,width,height,xoff,yoff,*corner,*world_box,*transform,nfaces))
            records.extend(struct.pack('<'+'f'*count,*heights))
            records.extend(bytes(tiles))
            records.extend(bytes(uc.mem_read(faces,nfaces*52)))
    Path(output).write_bytes(records)
    print('Captured 128 original liquid movement collections')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable'); parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable,args.output)
