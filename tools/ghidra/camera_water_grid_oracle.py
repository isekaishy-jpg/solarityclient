"""Execute original MLIQ cell-list kernels for grid-local segment endpoints."""
import argparse
import itertools
import random
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX
from unicorn import UC_HOOK_CODE
import wmo_registration_oracle as n
from camera_water_oracle import bits, ret


def terrain_grid(start,end):
    uc=n.emulator()
    a,b,bank=[n.HEAP+i*0x1000 for i in range(3)]
    n.write_floats(uc,a,start+[0.])
    n.write_floats(uc,b,end+[0.])
    n.write_words(uc,0xcf4930,bank)
    def hook(uc,address,size,data):
        if address == 0x7a3570: ret(uc,0)
    uc.hook_add(UC_HOOK_CODE,hook)
    n.invoke(uc,0x7a39f0,[a,b,0,0x20000,0])
    return n.read_words(uc,bank,n.read_words(uc,0xce04c8,1)[0])


def grid(start,end):
    uc = n.emulator()
    group, a,b,count,bank = [n.HEAP+i*0x1000 for i in range(5)]
    n.write_words(uc,group+0x11c,16,16)
    n.write_floats(uc,a,start+[0.])
    n.write_floats(uc,b,end+[0.])
    dx,dy = [abs(n.read_floats(uc,b,2)[i]-n.read_floats(uc,a,2)[i]) for i in range(2)]
    address = 0x7c9110 if dy < 2**-22 else 0x7c91a0 if dx < 2**-22 else 0x7c9230 if dx>dy else 0x7c9370
    uc.reg_write(UC_X86_REG_ECX,group)
    n.invoke(uc,address,[a,b,count,bank,2048])
    return list(uc.mem_read(bank,n.read_words(uc,count,1)[0]))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--terrain',action='store_true')
    args=p.parse_args()
    n.initialize(args.executable)
    cases=list(itertools.product([[0.,0.],[1.,1.],[.5,2.5],[1.000001,1.999999],[3.2,1.7],[15.976,15.976]], repeat=2))
    rng=random.Random(12340)
    cases += [([rng.uniform(0,15.976) for _ in range(2)],[rng.uniform(0,15.976) for _ in range(2)]) for _ in range(400)]
    if args.terrain:
        cases += [([base+v for v in start],[base+v for v in end]) for base in [-15000.,-5000.,5000.,15000.] for start,end in cases[:50]]
    rows=['# start2 end2 | cell XY byte pairs; original MLIQ grid kernels, dimensions 16x16']
    for start,end in cases:
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in values) for values in [[*map(bits,start+end)],(terrain_grid if args.terrain else grid)(start,end)]))
    args.output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'wrote {len(cases)} cell walks')


if __name__=='__main__': main()
