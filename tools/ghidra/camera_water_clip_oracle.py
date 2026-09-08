"""Execute original MLIQ ray clipping and cell selection (7C9DD0).

Only the final triangle consumer is hooked to capture its admitted cell list.
The original point/box checks, ray/box clipping and cell walkers run unchanged.
"""
import argparse
import itertools
import random
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from camera_water_oracle import bits, ret


def clip(start,end,corner,dimensions,heights):
    uc=n.emulator()
    group,segment=[n.HEAP+i*0x1000 for i in range(2)]
    n.write_words(uc,group+0x11c,*dimensions)
    n.write_floats(uc,group+0x124,corner+[0.])
    n.write_floats(uc,group+0x13c,heights)
    n.write_floats(uc,segment,start+end)
    cells=[]
    def hook(uc,address,size,data):
        if address==0x7c8dd0:
            args=n.read_words(uc,uc.reg_read(UC_X86_REG_ESP),8)
            cells.extend(uc.mem_read(args[5],n.read_words(uc,args[4],1)[0]))
            ret(uc,0,28)
    uc.hook_add(UC_HOOK_CODE,hook)
    uc.reg_write(UC_X86_REG_ECX,group)
    n.invoke(uc,0x7c9dd0,[segment,0,0x20000,0,0])
    return cells


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    n.initialize(args.executable)
    cases=[]
    for x,y,z0,z1 in itertools.product([-.1,0.,.1,4.05,4.0666666,4.1,4.1666665],[0.,2.,4.05],[-1.,0.,1.],[-1.,0.,1.]):
        if z0!=z1: cases.append(([x,y,z0],[x,y,z1],[0.,0.],[1,1],[0.,0.]))
    rng=random.Random(12340)
    for _ in range(400):
        corner=[rng.uniform(-1000.,1000.) for _ in range(2)]
        dimensions=[rng.randrange(1,17) for _ in range(2)]
        heights=sorted([rng.uniform(-10.,10.) for _ in range(2)])
        a=[corner[i]+rng.uniform(-10.,dimensions[i]*4.1666665+10) for i in range(2)]+[rng.uniform(-20.,20.)]
        b=[corner[i]+rng.uniform(-10.,dimensions[i]*4.1666665+10) for i in range(2)]+[rng.uniform(-20.,20.)]
        cases.append((a,b,corner,dimensions,heights))
    rows=['# start3 end3 corner2 dimensions2 height-range2 | MLIQ admitted XY cell bytes; hex words']
    for a,b,corner,dimensions,heights in cases:
        inputs=[*map(bits,a+b+corner),*dimensions,*map(bits,heights)]
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in values) for values in [inputs,clip(a,b,corner,dimensions,heights)]))
    args.output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'wrote {len(cases)} MLIQ clipping cases')


if __name__=='__main__': main()
