"""Execute original corpse packets, position/range owner and Corpse_C lifecycle.

Packet readers, x87 range arithmetic, retained matrix construction, query clocks
and GUID ownership execute stock instructions. Object lookup/poses, clock,
minimap notification, UI dispatch and byte output are supplied boundaries.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    u=n.emulator()
    unit,fields,player,packet,payload,position,corpse,corpsefields,objectfields,vtable=[n.HEAP+i*0x3000 for i in range(10)]
    n.write_words(u,unit,vtable);n.write_words(u,vtable+0x2c,n.STOP+16)
    n.write_words(u,unit+0xd0,fields);n.write_words(u,unit+0x1008,player)
    n.write_words(u,unit+8,fields+0x200);n.write_words(u,fields+0x200,7,0,0x19)
    n.write_words(u,corpse+0xd0,corpsefields);n.write_words(u,corpse+8,objectfields)
    n.write_words(u,objectfields,0x12345678,0xf101)
    present,localmap,transport,valid,now=1,0,0,0,100
    events,wire,markers,answers=[],[],[],[]
    identity=struct.pack('<16f',1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1)
    resident=struct.pack('<16f',0,1,0,0,-1,0,0,0,0,0,1,0,100,200,300,1)
    def hook(u,address,size,context):
        sp=u.reg_read(UC_X86_REG_ESP)
        if address==0x4d3790:u.reg_write(UC_X86_REG_EDX,0);return_value(u,7)
        elif address==0x4038f0:return_value(u,unit if present else 0)
        elif address==0x84df60:return_value(u,1)
        elif address==0x84e0e0:return_value(u,payload)
        elif address==0x60abf0:
            n.write_words(u,n.read_words(u,sp+8,1)[0],7,0);return_value(u,1)
        elif address==0x729740:return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+8)
        elif address in [0x4cee50,0x513c30]:return_value(u,0)
        elif address in [0x53cf10,0x6cefb0,0x6167e0,0x523eb0,0x51f690,0x530840,0x4f88b0,0x6dc5a0,0x4d4b30,0x7e5550]:return_value(u,0)
        elif address==0x71f8f0:return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+8)
        elif address==0x60bf10:
            events.append(f'{n.read_words(u,sp+8,1)[0]:x}');return_value(u,0)
        elif address==0x4d4db0:
            mask=n.read_words(u,sp+12,1)[0]
            return_value(u,(unit if present else 0) if mask==16 else (corpse if transport else 0))
        elif address==0x74b4c0:
            if valid:u.mem_write(n.read_words(u,sp+12,1)[0],resident)
            return_value(u,valid)
        elif address==0x6ceaf0:return_value(u,localmap)
        elif address==n.STOP+16:
            return_value(u,position);u.reg_write(UC_X86_REG_ESP,sp+8)
        elif address==0x88b821:return_value(u,now)
        elif address==0x7f4990:
            markers.append(bytes(u.mem_read(sp+4,8)).hex());return_value(u,0)
        elif address==0x81b530:
            events.append(f'{n.read_words(u,sp+4,1)[0]:x}');return_value(u,0)
        elif address==0x47b0a0:
            wire.append(struct.pack('<I',n.read_words(u,sp+4,1)[0]).hex())
            return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+8)
        elif address==0x6b0b50:
            store=n.read_words(u,sp+4,1)[0];n.write_words(u,store+12,0xffffffff)
            wire.append('/');return_value(u,0)
        elif address==0x84e2a0:
            answers.append(str(int(struct.unpack('<d',u.mem_read(sp+8,8))[0])));return_value(u,0)
        elif address==0x84e280:answers.append('nil');return_value(u,0)
        elif address in [0x744db0,0x744d20,0x79f820,0x705900,0x743760]:return_value(u,0)
    u.hook_add(UC_HOOK_CODE,hook)
    rows=['# Original build 12340 corpse recovery; float vectors use little-endian raw bytes.']
    def reset(cmap=0,dmap=0,latch=0):
        n.write_words(u,0xbd0818,cmap,dmap,latch,0,0x999,0xf101,0,0)
        u.mem_write(0xbd0a58,struct.pack('<3f',10,20,30));u.mem_write(0xac8aa0,identity)
        u.mem_write(position,struct.pack('<3f',10,20,30))
        events.clear();wire.clear();markers.clear()
    def state():
        maps=n.read_words(u,0xbd0818,3);guid=n.read_words(u,0xbd0830,2)
        return ' '.join([f'{maps[0]:08x}',f'{maps[1]:08x}',str(maps[2]),bytes(u.mem_read(0xbd0a58,12)).hex(),f'{guid[1]:08x}{guid[0]:08x}',','.join(events) or '-',''.join(wire) or '-',','.join(markers) or '-'])
    for present in [0,1]:
        for ghost in [0,16]:
            n.write_words(u,player+8,ghost)
            for arena in [0,4]:
                n.write_words(u,0xbea570,arena)
                for latch in [0,1]:
                    reset(latch=latch);n.invoke(u,0x524a30,[])
                    rows.append(f'clear {present} {ghost} {arena} {latch} '+state())
                for body in [b'\0', b'\xff'+struct.pack('<I3fII',0,11,22,33,1,0),b'\1'+struct.pack('<I3fII',0,11,22,33,0,9),b'\1'+struct.pack('<I3fII',0,11,22,33,0,0x80000009)]:
                    reset(latch=1);u.mem_write(payload,body)
                    n.write_words(u,packet,0x9e0e24,payload,0,len(body),len(body),0)
                    n.invoke(u,0x526530,[0,0x216,0,packet])
                    assert n.read_words(u,packet+20,1)[0]==len(body)
                    rows.append(f'packet {present} {ghost} {arena} {body.hex()} '+state())
    for present in [0,1]:
        for cmap,dmap,localmap in [(0,0,0),(0,1,0),(1,1,0),(0xffffffff,0,0),(0x80000000,0,0)]:
            for arena in [0,4]:
                n.write_words(u,0xbea570,arena)
                for latch in [0,1]:
                    for xyz in [(10,20,30),(50,20,30),(50.000004,20,30),(10,20,70),(10,20,70.000008),(34,52,30),(34,52,30.001),(float('nan'),20,30),(float('inf'),20,30)]:
                        reset(cmap,dmap,latch);raw=struct.pack('<3f',*xyz);u.mem_write(position,raw)
                        n.invoke(u,0x51f5c0,[])
                        rows.append(f'range {present} {cmap:08x} {dmap:08x} {localmap} {arena} {latch} {raw.hex()} {n.read_words(u,0xbd0820,1)[0]} '+(','.join(events) or '-')+' '+(','.join(markers) or '-'))
    present=1;localmap=0
    for owner in [0,7,0x100000007,8]:
        for flags in [0,1,2,3,0x100]:
            for mode,address in [('add',0x705fa0),('remove',0x705f30)]:
                reset();n.write_words(u,corpsefields,owner&0xffffffff,owner>>32)
                n.write_words(u,corpsefields+0x6c,flags,0)
                u.reg_write(UC_X86_REG_ECX,corpse);n.invoke(u,address,[])
                guid=n.read_words(u,0xbd0828,2)
                rows.append(f'owner {owner:016x} {flags} {mode} {guid[1]:08x}{guid[0]:08x}')
    for transport in [0,1]:
        for valid in [0,1]:
            for deadline,now in [(0,100),(129,100),(100,100),(99,100),(0xfffffff0,10)]:
                reset();n.write_words(u,0xbd0830,9,0x1fc00000);n.write_words(u,0xbd0824,deadline)
                u.mem_write(0xac8aa0,resident)
                n.invoke(u,0x51f430,[payload])
                rows.append(f'transport {transport} {valid} {deadline:08x} {now} '+bytes(u.mem_read(payload,12)).hex()+f' {n.read_words(u,0xbd0824,1)[0]:08x} '+(''.join(wire) or '-'))
    for orientation in [0,0.5,-1,3.1415927]:
        body=struct.pack('<4f',100,200,300,orientation)
        reset();u.mem_write(payload,body);n.write_words(u,packet,0x9e0e24,payload,0,len(body),len(body),0)
        n.invoke(u,0x526530,[0,0x4b7,0,packet])
        assert n.read_words(u,packet+20,1)[0]==len(body)
        rows.append('matrix '+body.hex()+' '+bytes(u.mem_read(0xac8aa0,64)).hex())
    present=1;localmap=0
    for previous in [0,16]:
        for flags in [0,16]:
            n.write_words(u,player+8,flags)
            for arena in [0,4]:
                n.write_words(u,0xbea570,arena)
                for latch in [0,1]:
                    reset(latch=latch)
                    u.reg_write(UC_X86_REG_ECX,unit);n.invoke(u,0x6e0fd0,[previous])
                    rows.append(f'flags {previous} {flags} {arena} {latch} '+(','.join(events) or '-')+' '+(''.join(wire) or '-'))
    for present in [0,1]:
        for flags in [0,2,4,6,16,0x400,0x800,0xc00,0xffffffff]:
            n.write_words(u,player+8,flags)
            for name,address in [('ShowingHelm',0x51bfd0),('ShowingCloak',0x51c040),('UnitIsAFK',0x60cc30),('UnitIsDND',0x60cd50)]:
                answers.clear();n.invoke(u,address,[0])
                rows.append(f'display {present} {flags:08x} {name} {answers[0]}')
    return rows


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('executable');p.add_argument('output')
    a=p.parse_args();n.initialize(a.executable);rows=capture()
    Path(a.output).write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'Captured {len(rows)-1} corpse recovery samples')

