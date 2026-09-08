"""Execute original offer reader, Lua answers, response writer and recovery timer.

Original 6DBD00, 6DAC10, 6CDE20, 6DBC60 and 6D1D30 retain their decisions.
Object/name-cache lookup, Lua stack, clock, event dispatch and outgoing byte
writes are supplied boundaries. Packet readers execute original instructions.
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
    unit,fields,player,packet,payload,cached=[n.HEAP+i*0x3000 for i in range(6)]
    n.write_words(u,unit,0xa326c8)
    n.write_words(u,unit+0xd0,fields)
    n.write_words(u,unit+0x1008,player)
    present,cache,now,resolved=1,None,1000,1
    events,wire,answers=[],[],[]
    def cstring(ptr):
        raw=bytes(u.mem_read(ptr,256))
        return raw[:raw.index(0)].hex() or '-'
    def hook(u,address,size,context):
        sp=u.reg_read(UC_X86_REG_ESP)
        if address==0x4d3790:
            u.reg_write(UC_X86_REG_EDX,0);return_value(u,7)
        elif address==0x4d4db0:return_value(u,unit if present else 0)
        elif address==0x84e0e0:return_value(u,payload)
        elif address==0x60abf0:
            dest=n.read_words(u,sp+8,1)[0];n.write_words(u,dest,7,0);return_value(u,resolved)
        elif address==0x86ae20:return_value(u,now)
        elif address==0x67d770:
            events.append('cache')
            if cache is not None:u.mem_write(cached,cache+b'\0')
            return_value(u,cached if cache is not None else 0)
            u.reg_write(UC_X86_REG_ESP,sp+28)
        elif address==0x81b530:
            event=n.read_words(u,sp+4,1)[0]
            events.append(f'{event:x}:'+cstring(n.read_words(u,sp+12,1)[0]) if event==0x106 else f'{event:x}')
            return_value(u,0)
        elif address in [0x47b0a0,0x47b100,0x47afe0]:
            count=2 if address==0x47b100 else 1
            values=n.read_words(u,sp+4,count)
            wire.append(struct.pack('<II',*values).hex() if count==2 else (struct.pack('<I',values[0]).hex() if address==0x47b0a0 else f'{values[0]&255:02x}'))
            return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+4+4*count)
        elif address==0x6b0b50:
            store=n.read_words(u,sp+4,1)[0]
            n.write_words(u,store+12,0xffffffff);wire.append('/')
            return_value(u,0)
        elif address==0x84e2a0:
            answers.append(str(int(struct.unpack('<d',u.mem_read(sp+8,8))[0])))
            return_value(u,0)
        elif address==0x84e280:answers.append('nil');return_value(u,0)
    u.hook_add(UC_HOOK_CODE,hook)
    rows=['# offer: present health ghost cache body guid sickness timer events']
    for present in [0,1]:
        for health,ghost in [(100,0),(0,0),(100,16),(0xffffffff,0)]:
            n.write_words(u,fields+0x48,health);n.write_words(u,player+8,ghost)
            for name,cache,sickness,timer in [(b'Healer\0',None,1,1),(b'\0',b'Cached',0,1),(b'\0',None,2,255),(b'A\0tail',None,0,0),(b'',None,0,0)]:
                body=struct.pack('<QI',0x1234567800000009,len(name))+name+bytes([sickness,timer])
                u.mem_write(payload,body+b'\0'*256)
                n.write_words(u,packet,0x9e0e24,payload,0,len(body),len(body),0)
                events.clear();n.invoke(u,0x6dbd00,[0,0x15b,0,packet])
                assert n.read_words(u,packet+20,1)[0]==len(body)
                lo,hi,s,t=n.read_words(u,0xc9eab8,4)
                rows.append(f'offer {present} {health:08x} {ghost} {(cache.hex() if cache else "-")} {body.hex()} {hi:08x}{lo:08x} {s} {t} '+(','.join(events) or '-'))
    for present in [0,1]:
        for guid in [0,0x1234567800000009]:
            for action,address in [('accept',0x51aac0),('decline',0x51aaf0),('reclaim',0x51b800)]:
                n.write_words(u,0xc9eab8,guid&0xffffffff,guid>>32,2,255)
                n.write_words(u,0xbd0828,0xabcdef12,0xf101)
                wire.clear();n.invoke(u,address,[])
                lo,hi,s,t=n.read_words(u,0xc9eab8,4)
                rows.append(f'action {present} {guid:016x} {action} {hi:08x}{lo:08x} {s} {t} '+(''.join(wire) or '-'))
    for present in [0,1]:
        for health,ghost in [(100,0),(0,0),(100,16),(0xffffffff,0)]:
            n.write_words(u,fields+0x48,health);n.write_words(u,player+8,ghost)
            for cache in [None,b'',b'Resolved']:
                n.write_words(u,0xc9eab8,9,0x12345678,2,255)
                events.clear();n.invoke(u,0x6dbc60,[])
                lo,hi,s,t=n.read_words(u,0xc9eab8,4)
                rows.append(f'callback {present} {health:08x} {ghost} {(cache.hex() or "empty") if cache is not None else "-"} {hi:08x}{lo:08x} {s} {t} '+(','.join(events) or '-'))
    for sickness in [0,1,255]:
        for timer in [0,1,255]:
            for kind in [0,3,4]:
                n.write_words(u,0xc9eac0,sickness,timer);n.write_words(u,0xbea570,kind)
                answers.clear();n.invoke(u,0x5159c0,[0]);n.invoke(u,0x515a00,[0])
                rows.append(f'query {sickness} {timer} {kind} '+ ' '.join(answers))
    for now in [0,1000,0xfffffff0]:
        for delay in [0,999,1001,30000,0x80000000,0xffffffff]:
            for in_range,corpse_map,display_map in [(0,0,0),(1,0,0),(1,0,1)]:
                n.write_words(u,0xbd0818,corpse_map,display_map,in_range)
                events.clear();n.invoke(u,0x513a80,[delay])
                deadline=n.read_words(u,0xbd0850,1)[0]
                answers.clear();n.invoke(u,0x516280,[0])
                rows.append(f'delay {now} {delay} {in_range} {corpse_map} {display_map} {deadline} {answers[0]} '+(','.join(events) or '-'))
    for present in [0,1]:
        for resolved in [0,1]:
            for charm in [0,7,1<<32,0x1234567800000009]:
                for summon in [0,8,1<<32,0x123456780000000a]:
                    n.write_words(u,fields,charm&0xffffffff,charm>>32,summon&0xffffffff,summon>>32)
                    answers.clear();n.invoke(u,0x613c90,[0])
                    rows.append(f'controlling {present} {resolved} {charm:016x} {summon:016x} {answers[0]}')
    return rows


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('executable');p.add_argument('output')
    a=p.parse_args();n.initialize(a.executable);rows=capture()
    Path(a.output).write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'Captured {len(rows)-1} offer/action/query/recovery samples')
