"""Execute native 605D60 primary anchor/ray/retreat and 601D60 eye composition.

Scene rays, volume results, camera basis and object access are controlled.
The native volume implementation has its own complete geometry oracle.
"""
import argparse
import itertools
import random
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EDX, UC_X86_REG_EAX
import wmo_registration_oracle as n
from camera_water_oracle import bits,ret


def f32(value): return struct.unpack('<f',struct.pack('<f',value))[0]


def primary(subject,forward,up,distance,height,mount,water,state,depth,unit_height,vertical,center,volume,ground_plane=False,targets=None):
    uc=n.emulator()
    camera,vtable,point,dist,anchor,offset,unit,fields,cvar,eye,pivot=[n.HEAP+i*0x1000 for i in range(11)]
    forward_fn,up_fn=n.STOP+0x100,n.STOP+0x200
    n.write_words(uc,camera,vtable)
    n.write_words(uc,vtable+4,forward_fn)
    n.write_words(uc,vtable+12,up_fn)
    n.write_floats(uc,camera+0x38,[.2])
    n.write_floats(uc,camera+0x118,[distance])
    n.write_floats(uc,camera+0x128,[height])
    if targets is not None:
        n.write_floats(uc,camera+0x1e8,[targets[0]])
        n.write_floats(uc,camera+0x218,[targets[1]])
    n.write_floats(uc,camera+0x13c,[mount])
    n.write_words(uc,camera+0x98,[0,0x100000,0x200000][state])
    n.write_words(uc,0xc249b4,cvar)
    n.write_words(uc,cvar+0x30,water)
    n.write_floats(uc,point,subject)
    n.write_words(uc,unit+8,fields)
    n.write_words(uc,fields,1,0,8)
    n.write_floats(uc,unit+0x854,[max(unit_height,0.)])
    if ground_plane:
        # Run 6059E0 and its original triangle clipping, supplying only the
        # scene collection and FOV boundaries used by camera_volume_oracle.
        n.write_floats(uc,camera+0x38,[.2,5000.])
        n.write_floats(uc,camera+0x44,[16/9])
        n.write_words(uc,vtable,n.STOP+0x300)
        n.write_floats(uc,n.HEAP+0xb000,[1.5707964])
        uc.mem_write(n.STOP+0x300,b'\xd9\x05'+struct.pack('<I',n.HEAP+0xb000)+b'\xc3')
        uc.mem_write(0x40c8fa,b'\xc3')
    calls=[]
    ray_index=0
    def hook(uc,address,size,data):
        nonlocal ray_index
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address in [forward_fn,up_fn]:
            target=n.read_words(uc,sp+4,1)[0]
            n.write_floats(uc,target,forward if address==forward_fn else up)
            ret(uc,target,4)
        elif address==0x4d4db0: ret(uc,unit if unit_height>=0 else 0)
        elif address==0x4d3790:
            uc.reg_write(UC_X86_REG_EDX,0)
            ret(uc,0)
        elif address==0x77f310:
            _,a,b,contact,fraction,mask,_=n.read_words(uc,sp,7)
            calls.extend([*n.read_words(uc,a,3),*n.read_words(uc,b,3),mask])
            query_height = max(height, targets[1]) if targets is not None else height
            fraction_value = vertical if ray_index==0 and query_height-f32(.2)>2**-20 else center
            if ground_plane:
                start=n.read_floats(uc,a,3)
                end=n.read_floats(uc,b,3)
                fraction_value=(start[2]-subject[2])/(start[2]-end[2]) if start[2]>=subject[2] and end[2]<subject[2] else -1.
            ray_index+=1
            if fraction_value>=0: n.write_floats(uc,fraction,[fraction_value])
            ret(uc,int(fraction_value>=0))
        elif address==0x77f330 and ground_plane:
            _,body,collection,flags,unused=n.read_words(uc,sp,5)
            assert unused == 0
            bank=n.HEAP+0xc000
            selected=bool(flags & 0x100171)
            n.write_words(uc,collection,int(selected),int(selected),bank,0x100)
            if selected:
                x,y,z=subject
                n.write_floats(uc,bank,[0.,0.,1.,0.,x-100,y-100,z,x+100,y-100,z,x,y+100,z])
            ret(uc,int(selected))
        elif address==0x6059e0 and not ground_plane:
            _,output,_,_,_=n.read_words(uc,sp,5)
            d=n.read_floats(uc,output,1)[0]
            span=f32(d-f32(.2))
            if span<f32(.001):
                n.write_floats(uc,output,[0.])
                ret(uc,1,16)
            elif volume>=0:
                n.write_floats(uc,output,[max(0.,d-f32(volume)*span)])
                ret(uc,1,16)
            else: ret(uc,0,16)
    uc.hook_add(UC_HOOK_CODE,hook)
    uc.reg_write(UC_X86_REG_ECX,camera)
    n.invoke(uc,0x605d60,[point,dist,anchor,offset,bits(depth),camera+0x2c0])
    contacts = uc.reg_read(UC_X86_REG_EAX)
    result=[*n.read_words(uc,dist,1),*n.read_words(uc,anchor,1),*n.read_words(uc,camera+0x2c0,1)]
    n.write_floats(uc,pivot,[subject[0],subject[1],f32(f32(subject[2])+n.read_floats(uc,anchor,1)[0])])
    uc.reg_write(UC_X86_REG_ECX,camera)
    n.invoke(uc,0x601d60,[eye,pivot,result[0],offset])
    return result+list(n.read_words(uc,eye,3))+[contacts],calls


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--targets',action='store_true',help='capture independent zoom and height targets')
    args=p.parse_args()
    n.initialize(args.executable)
    cases=[]
    for state,water,mount,v,c in itertools.product(range(3),range(2),[0.,2.],[-1.,0.,.5],[-1.,0.,.5]):
        cases.append(([0.,0.,0.],[1.,0.,0.],[0.,0.,1.],5.,1.75,mount,water,state,1.8,2.,v,c,-1.))
    rng=random.Random(12340)
    for _ in range(400):
        cases.append(([rng.uniform(-1000,1000) for _ in range(3)],[.8660254,0.,-.5],[.5,0.,.8660254],rng.choice([.01,.2,.2005,1.,5.,15.]),rng.choice([.1,.2,.3,.8333333,1.75,4.]),rng.choice([0.,2.]),rng.randrange(2),rng.randrange(3),rng.uniform(-1.,7.),rng.choice([-1.,2.,4.]),rng.choice([-1.,0.,.5,1.]),rng.choice([-1.,0.,.5,1.]),rng.choice([-1.,0.,.4,1.])))
    rows=['# subject3 forward3 up3 distance height mount water state depth unit-height(-1 absent) vertical-ray(-1 absent) center-ray volume-retreat | distance height vertical-fraction eye3 contact-flags | ordered (start3 end3 mask) rays; hex words']
    if args.targets:
        rows[0] = rows[0].replace('volume-retreat |', 'volume-retreat distance-target height-target |')
        rows.append('# pinned build-12340 aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8; independent current and target banks')
    for case in cases:
        subject,forward,up,d,h,m,w,s,dep,u,v,c,vol=case
        inputs=[*map(bits,subject+forward+up+[d,h,m]),w,s,*map(bits,[dep,u,v,c,vol])]
        targets = (d * 1.7 + .5, h * 1.4 + .8) if args.targets else None
        if targets is not None:
            inputs.extend(map(bits, targets))
        result,calls=primary(*case,targets=targets)
        rows.append(' | '.join(' '.join(f'{word:08x}' for word in values) for values in [inputs,result,calls]).rstrip())
    args.output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'wrote {len(cases)} native primary constraints')


if __name__=='__main__': main()
