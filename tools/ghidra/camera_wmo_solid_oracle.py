"""Capture complete 7CB2F0 solid camera BSP queries with native face filtering."""
import argparse
import itertools
import random
from pathlib import Path
import wmo_registration_oracle as n
from camera_water_oracle import bits


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    n.initialize(args.executable)
    cases=[]
    for flag,cached in itertools.product(range(256),[False,True]):
        cases.append(([0.,0.,4.],[0.,0.,-4.],1.,cached,0,0,flag))
    rng=random.Random(12340)
    for _ in range(256):
        start=[rng.uniform(-3.01,3.01),rng.uniform(-3.01,3.01),4.]
        end=[rng.uniform(-3.01,3.01),rng.uniform(-3.01,3.01),rng.choice([-4.,0.,2.])]
        cases.append((start,end,rng.choice([.25,.5,.75,1.]),rng.choice([False,True]),rng.choice([0,1]),rng.randrange(5),rng.choice([0,2,4,8,32,64])))
    rows=['# start3 end3 maximum cached geometry topology face-flags | hit fraction face; hex words, original 7CB2F0']
    for start,end,maximum,cached,geometry,topology,flag in cases:
        result=n.floor_probe(start,end,maximum=maximum,cached=cached,geometry=geometry,topology=topology,scene_camera=True,face_flags=flag)
        inputs=[*map(bits,start+end+[maximum]),int(cached),geometry,topology,flag]
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in values) for values in [inputs,result]))
    args.output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'wrote {len(cases)} native solid-camera BSP cases')


if __name__=='__main__': main()
