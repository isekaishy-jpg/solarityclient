"""Original 834C70 spatial publication and 81E400/834F60 point selection."""
import argparse
import random
import struct
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
import wmo_registration_oracle as n
from world_model_fog_oracle import bits


def capture():
    rng = random.Random(12340)
    layouts = [
        [(x, y, z) for x, y, z in [(1.,0.,0.),(-1.,0.,0.),(0.,1.,0.),(0.,-1.,0.),(0.,0.,1.),(0.,0.,-1.)]],
        [(rng.choice([-1290., -1280., -30., -20., -10., -.00001, 0., .00001, 10., 20., 30., 1270., 1280.]), rng.choice([-20., -10., 0., 10., 20.]), rng.choice([-4., 0., 4.])) for _ in range(40)],
        [(rng.uniform(-60.,60.), rng.uniform(-60.,60.), rng.uniform(-10.,10.)) for _ in range(40)],
    ]
    rows = ['# Native spatial light registration/query: lights XYZhex; query XYZradiushex count indices[4] distancehex[4]']
    for layout, points in enumerate(layouts):
        u = n.emulator()
        scene, grid, lights, query = n.HEAP, n.HEAP + 0x1000, n.HEAP + 0x6000, n.HEAP + 0xc000
        n.write_words(u, scene + 0x14, 7)
        n.write_words(u, scene + 0x24, grid)
        rows.append('lights ' + str(layout) + ' ' + ' '.join(f'{bits(v):08x}' for point in points for v in point))
        for index, point in enumerate(points):
            light = lights + index * 0x80
            n.write_words(u, light, scene, 7, 1)
            n.write_floats(u, light + 0xc, point)
            u.reg_write(UC_X86_REG_ECX, light)
            n.invoke(u, 0x8356f0, [1])
        for x in [-1280., -30., -20., -10.00001, -10., -.00001, 0., .00001, 9.99999, 10., 20., 30., 1280.]:
            for y in [-20., 0., 20.]:
                for radius in [0., 5., 25., 100.]:
                    u.mem_write(query, bytes(0xd4))
                    n.write_floats(u, query + 4, [x, y, 1.25, radius])
                    u.reg_write(UC_X86_REG_ECX, scene)
                    n.invoke(u, 0x81e400, [query])
                    ptrs = n.read_words(u, query + 0x84, 4)
                    distances = n.read_words(u, query + 0x94, 4)
                    count = n.read_words(u, query + 0xa4, 1)[0]
                    ids = [(ptr - lights) // 0x80 if ptr else -1 for ptr in ptrs]
                    rows.append('query ' + ' '.join(f'{bits(v):08x}' for v in [x,y,1.25,radius]) + f' {count} ' + ' '.join(map(str,ids)) + ' ' + ' '.join(f'{v:08x}' for v in distances))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
