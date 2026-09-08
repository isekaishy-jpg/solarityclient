"""Execute original 7C8DD0 liquid-cell rays, including normalization and 9836B0.

The fixture supplies one MLIQ cell with four vertices and an explicit cell list.
No math or intersection function is hooked. Grid selection and placed-root
transforms are separate evidence. Requires the fingerprinted locally owned PE.
"""
import argparse
import itertools
import random
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX
import wmo_registration_oracle as n
from camera_water_oracle import bits


def trace(start, end, maximum, vertices, flag):
    uc = n.emulator()
    group, mesh, bank, segment, fraction, count, cells, flags = [n.HEAP+i*0x1000 for i in range(8)]
    n.write_words(uc, group+0x1c, mesh)
    n.write_words(uc, mesh+0x10, bank)
    n.write_words(uc, group+0x114, 2, 2, 1, 1)
    n.write_words(uc, group+0x138, flags)
    n.write_words(uc, group+0x144, 1)
    n.write_words(uc, flags, flag)
    n.write_floats(uc, bank, sum(vertices, []))
    n.write_floats(uc, segment, start+end)
    n.write_floats(uc, fraction, [maximum])
    n.write_words(uc, count, 2)
    n.write_words(uc, cells, 0)
    uc.reg_write(UC_X86_REG_ECX, group)
    n.invoke(uc, 0x7c8dd0, [segment, fraction, 0x20000, count, cells, 0, 0])
    return [uc.reg_read(UC_X86_REG_EAX)&0xff, *n.read_words(uc, fraction, 1)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    vertices = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[1.,1.,0.]]
    cases = []
    for x, z, end_z, maximum in itertools.product([-.010001,-.01,0.,.5,1.,1.01,1.010001], [0.,1.,-1.], [-1.,0.,1.], [.5,1.]):
        cases.append(([x,.5,z],[x,.5,end_z],maximum,vertices,0))
    rng = random.Random(12340)
    for _ in range(400):
        points = [[rng.uniform(-5.,5.) for _ in range(3)] for _ in range(4)]
        center = [sum(p[i] for p in points)/4 for i in range(3)]
        offset = [rng.uniform(-10.,10.) for _ in range(3)]
        cases.append(([center[i]+offset[i] for i in range(3)], [center[i]-offset[i] for i in range(3)], 1., points, rng.choice([0,0,0,0xf,0x40])))
    rows = ['# start3 end3 maximum vertices12 tileflag | hit fraction; hex words; 7C8DD0 complete cell kernel']
    for start, end, maximum, points, flag in cases:
        inputs = [*map(bits,start+end+[maximum]+sum(points,[])),flag]
        result = trace(start,end,maximum,points,flag)
        rows.append(' | '.join(' '.join(f'{word:08x}' for word in group) for group in [inputs,result]))
    args.output.write_text('\n'.join(rows)+'\n', encoding='utf-8')
    print(f'wrote {len(cases)} native liquid segment cases')


if __name__ == '__main__':
    main()
