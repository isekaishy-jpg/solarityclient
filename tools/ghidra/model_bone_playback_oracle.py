"""Capture native two-bone variation RNG, callback mutation and clearing.

Uses the same three authored sequences as runtime's Playback fixture. Model
activation, scan, queue, variation restart and clear run from the pinned image.
Application callbacks optionally select/clear another timer or consume rand.
The bone palette is supplied at the pose-sampling boundary; this fixture proves
callback order and timer ownership, not native bone-transform calculation.
"""
import argparse
import hashlib
import itertools
import json
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(output):
    u = n.emulator()
    model, scene, resource, data, sequences, timers, bones, keys, table, rows, queue, callback, event, channels, times, palette = [n.HEAP+i*0x1000 for i in range(16)]
    event_callback, return_callback = callback+0x100, callback+0x200
    n.write_words(u, model, 2)
    n.write_words(u, model+0x10, 0x400001, 0xffff)
    n.write_words(u, model+0x28, scene, resource)
    n.write_words(u, model+0x78, callback)
    n.write_words(u, model+0x94, timers, palette)
    n.write_words(u, model+0x1c4, event_callback)
    n.write_words(u, resource+0x150, data)
    n.write_words(u, data+0x1c, 3, sequences, 0, 0, 3, bones, 27, keys)
    u.mem_write(keys, b'\xff\xff'*27)
    for i, (key, parent) in enumerate([(-1,-1),(4,0),(26,1)]):
        n.write_words(u,bones+i*88,key&0xffffffff)
        u.mem_write(bones+i*88+8,struct.pack('<H',parent&0xffff))
        if key>=0: u.mem_write(keys+key*2,struct.pack('<H',i))
        for offset in [0x48,0x6c,0x96]: u.mem_write(timers+i*172+offset,b'\xff\xff')
    for i,(id,variation,duration,flags,next,cycles) in enumerate([(0,0,1000,0x20,1,2),(0,1,1000,0x20,0xffff,2),(7,0,600,0x21,0xffff,1)]):
        u.mem_write(sequences+i*64,struct.pack('<HH',id,variation))
        n.write_words(u,sequences+i*64+4,duration,0,flags,16384,cycles,cycles,400)
        u.mem_write(sequences+i*64+60,struct.pack('<HH',next,0))
    for id in [0,7]:
        n.write_words(u,table+id*4,rows+id*32)
        n.write_words(u,rows+id*32+16,0,id,id)
    n.write_words(u,0xad30d8,0)
    n.write_words(u,0xad30d4,7)
    n.write_words(u,0xad30e8,table)
    n.write_words(u,data+0x100,1,event)
    n.write_words(u,event,0x444e5324,0,0)
    u.mem_write(event+26,b'\xff\xff')
    n.write_words(u,event+28,3,channels)
    for i in range(3):
        n.write_words(u,channels+i*8,1,times+i*4)
        n.write_words(u,times+i*4,100)
    identity=struct.pack('<16f',*[float(i%5==0) for i in range(16)])
    u.mem_write(scene+0xc4,identity)
    for i in range(3): u.mem_write(palette+i*64,identity)
    base=bytes(u.mem_read(n.HEAP,0x10000))
    random_state, rolls, acted = 1,0,False
    captured, returns = [],[]

    def rand():
        nonlocal random_state, rolls
        random_state=(random_state*214013+2531011)&0xffffffff
        rolls+=1
        return (random_state>>16)&0x7fff

    def hook(uc,address,size,context):
        nonlocal acted
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address==0x88b867:
            uc.reg_write(UC_X86_REG_EAX,rand())
        elif address==return_callback:
            saved_sp, saved_return=returns.pop()
            uc.reg_write(UC_X86_REG_ESP,saved_sp+4)
            uc.reg_write(UC_X86_REG_EIP,saved_return)
            return
        elif address in [callback,event_callback]:
            args=n.read_words(uc,sp+4,7)
            is_event=address==event_callback
            interrupted=not is_event and args[3]!=0
            if not interrupted:
                captured.append((int(is_event),args[1],0 if is_event else args[2],args[5] if is_event else args[4]))
            if is_event and action==4: rand()
            mutate = not acted and not interrupted and (
                (action in [1,6] and not is_event and args[1]==0xffffffff) or
                (action==2 and not is_event and args[1]==4) or
                (action in [3,5] and is_event))
            if mutate:
                acted=True
                returns.append((sp,n.read_words(uc,sp,1)[0]))
                request=[4 if action==1 else -1,0 if action==6 else 7,-1,0,0x3f800000,1,1]
                target=0x832ab0
                if action==5: target,request=0x832840,[4,1,1]
                new_sp=sp-0x200
                n.write_words(uc,new_sp,return_callback,*[v&0xffffffff for v in request])
                uc.reg_write(UC_X86_REG_ESP,new_sp)
                uc.reg_write(UC_X86_REG_ECX,model)
                uc.reg_write(UC_X86_REG_EIP,target)
                return
        elif address!=0x830dc0:
            return
        uc.reg_write(UC_X86_REG_EIP,n.read_words(uc,sp,1)[0])
        uc.reg_write(UC_X86_REG_ESP,sp+4)
    u.hook_add(UC_HOOK_CODE,hook)

    def call(address,args=()):
        sp=n.STACK+0x18000
        n.write_words(u,sp,n.STOP,*[v&0xffffffff for v in args])
        u.reg_write(UC_X86_REG_ESP,sp)
        u.reg_write(UC_X86_REG_ECX,model)
        u.emu_start(address,n.STOP,count=2_000_000)
        assert u.reg_read(UC_X86_REG_EIP)==n.STOP,(case,hex(u.reg_read(UC_X86_REG_EIP)))

    lines=['# Wow.exe SHA256 '+hashlib.sha256(n.data).hexdigest(),
           '# body upper order speed0 speed1 offset previous now phase action | callback(kind:key:index:overdue) | rolls | root/upper(sequence finished start end speed cycles blendSequence blendEnd) hex']
    cases=[]
    for body,upper,order,now,action in itertools.product([0,7],[0,7],[0,1],[100,1001,2200,4500],range(7)):
        cases.append((body,upper,order,0x3f800000,0x3f800000,0,0,now,1,action))
    for speed0,speed1,offset,previous,phase,action in itertools.product([0x3f000000,0x40000000],[0x3f800000,0x40000000],[0,350],[0,500],[0,1],[0,4]):
        cases.append((0,0,0,speed0,speed1,offset,previous,3500,phase,action))
    for case in cases:
        body,upper,order,speed0,speed1,offset,previous,now,phase,action=case
        u.mem_write(n.HEAP,base)
        n.write_words(u,0xd411c0,256,0,queue,0)
        n.write_words(u,scene+0x1c,4*phase)
        random_state,rolls,acted=1,0,False
        captured.clear()
        requests=[[-1,body,-1,offset,speed0,1,1],[4,upper,-1,offset,speed1,1,1]]
        for request in requests[::1 if order==0 else -1]: call(0x832ab0,request)
        n.write_words(u,scene+0xc,now,(now-previous)&0xffffffff)
        n.write_words(u,scene+0x1c,4)
        call(0x832260)
        final=[]
        for bone in range(2):
            timer=timers+bone*172
            sequence,finished=struct.unpack('<HB',u.mem_read(timer+0x48,3))
            start,end,speed,_,_,cycles=n.read_words(u,timer+0x4c,6)
            blend=struct.unpack('<H',u.mem_read(timer+0x6c,2))[0]
            blend_end=n.read_words(u,timer+0x9c,1)[0]
            if blend!=0xffff and ((blend_end-now)&0xffffffff)>=0x80000000 or blend_end==now: blend=0xffff
            final.extend([sequence,finished,start,end,speed,cycles,blend,blend_end if blend!=0xffff else 0])
        lines.append(' '.join(f'{v:08x}' for v in case)+' | '+' '.join(':'.join(f'{v:08x}' for v in event) for event in captured)+f' | {rolls:08x} | '+' '.join(f'{v:08x}' for v in final))
    output.write_text('\n'.join(lines)+'\n',encoding='utf-8')
    return dict(records=len(cases),sha256=hashlib.sha256(output.read_bytes()).hexdigest())


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
