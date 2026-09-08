"""Execute 606F90's original water-pitch block 6073B3..607492.

601190 is controlled to record its request; timed interpolation has a separate
native oracle. Free-look immediate assignments execute in the original block.
"""
import argparse
import itertools
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESI,UC_X86_REG_EDI,UC_X86_REG_EBP,UC_X86_REG_ESP
import wmo_registration_oracle as n
from camera_water_oracle import bits,ret


def capture(previous,current,enabled,free,surface,submerged):
    uc=n.emulator()
    camera,cvar,cv_surface,cv_sub=[n.HEAP+i*0x1000 for i in range(4)]
    frame=n.STACK+0x10000
    n.write_words(uc,camera+0x98,[0,0x100000,0x200000][current]|free)
    n.write_floats(uc,camera+0x120,[.317])
    n.write_floats(uc,camera+0x230,[.317])
    n.write_words(uc,0xc249b4,cvar)
    n.write_words(uc,cvar+0x30,enabled)
    n.write_words(uc,0xc24994,cv_surface)
    n.write_words(uc,0xc24990,cv_sub)
    n.write_floats(uc,cv_surface+0x2c,[surface])
    n.write_floats(uc,cv_sub+0x2c,[submerged])
    n.write_words(uc,frame-0x58,int(previous==2))
    n.write_words(uc,frame+12,1000)
    uc.reg_write(UC_X86_REG_ESI,camera)
    uc.reg_write(UC_X86_REG_EDI,int(previous==1))
    uc.reg_write(UC_X86_REG_EBP,frame)
    uc.reg_write(UC_X86_REG_ESP,frame-0x1000)
    calls=[]
    def hook(uc,address,size,data):
        if address==0x601190:
            args=n.read_words(uc,uc.reg_read(UC_X86_REG_ESP)+4,4)
            assert args[1:]==(0,bits(1.),1000)
            calls.append(args[0])
            ret(uc,1,16)
    uc.hook_add(UC_HOOK_CODE,hook)
    uc.emu_start(0x6073b3,0x607492,count=10000)
    return list(n.read_words(uc,camera+0x120,1)),calls


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('executable')
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    n.initialize(args.executable)
    rows=['# prior current water-collision free-look surface-final submerge-final | current-pitch | optional timed-pitch request; hex words']
    for prev,cur,enabled,free,finals in itertools.product(range(3),range(3),range(2),range(2),[(5.,5.),(0.,0.),(-10.,20.),(20.,-10.)]):
        result,calls=capture(prev,cur,enabled,free,*finals)
        inputs=[prev,cur,enabled,free,*map(bits,finals)]
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in values) for values in [inputs,result,calls]))
    args.output.write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'wrote {len(rows)-1} water pitch cases')


if __name__=='__main__': main()
