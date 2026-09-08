"""Execute 6357D0's packed-GUID name response reader against supplied cache writes.

Original 76DC20 and 47B480 decode GUIDs and bounded C strings. Cache result
entry points expose record bytes, retry/removal and unknown-name decisions.
Allocation, free and memory helpers are supplied only for declined forms and
the fixed '?' result. 668CE0 executes the original full-GUID query writer.
"""
import argparse,struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX,UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from unit_aura_oracle import packed


def capture():
    u=n.emulator();packet,payload,declined,record=[n.HEAP+i*0x3000 for i in range(4)]
    out=[]
    def string(ptr,limit):return bytes(u.mem_read(ptr,limit)).split(b'\0',1)[0].hex() or '-'
    def hook(u,a,size,context):
        sp=u.reg_read(UC_X86_REG_ESP)
        if a==0x76e540:return_value(u,declined)
        elif a==0x76e5a0:return_value(u,0)
        elif a==0x76ed20:
            dest,source,limit=n.read_words(u,sp+4,3);u.mem_write(dest,bytes(u.mem_read(source,limit)));return_value(u,dest);u.reg_write(UC_X86_REG_ESP,sp+16)
        elif a==0x40bb80:
            dest,value,count=n.read_words(u,sp+4,3);u.mem_write(dest,bytes([value&255])*count);return_value(u,dest)
        elif a==0x680c60:
            data,lo,hi=n.read_words(u,sp+4,3)
            forms=n.read_words(u,data+0x30,1)[0]
            race,gender,cls=n.read_words(u,data+0x140,3)
            out.append(f'found:{hi:08x}{lo:08x}:{string(data,48)}:{string(data+0x34,256)}:{race}:{gender}:{cls}:'+(','.join(string(forms+i*64,64) for i in range(5)) if forms else '-'))
            return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+16)
        elif a in [0x67a2d0,0x67a360,0x67a4c0]:
            lo,hi=n.read_words(u,sp+4,2)
            out.append(f'{ {0x67a2d0:"missing",0x67a360:"retry",0x67a4c0:"flag"}[a]}:{hi:08x}{lo:08x}')
            return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+12)
        elif a in [0x47b0a0,0x47b100]:
            count=1 if a==0x47b0a0 else 2
            out.append(bytes(u.mem_read(sp+4,4*count)).hex())
            return_value(u,0);u.reg_write(UC_X86_REG_ESP,sp+4+4*count)
        elif a==0x6b0b50:
            dest=n.read_words(u,sp+4,1)[0];n.write_words(u,dest+12,0xffffffff);return_value(u,0)
    u.hook_add(UC_HOOK_CODE,hook)
    rows=['# response body consumed cacheWrites; query guid wire']
    for guid in [7,0x1fd0000012345678,0xf130000012345678]:
        for status in [0,1,2,3,255]:
            for forms in ([False,True] if status==0 else [False]):
                body=packed(guid)+bytes([status])
                if status==0:
                    body+=b'Healer\0Realm\0'+bytes([4,1,11,int(forms)])
                    if forms:body+=b'Form1\0Form2\0Form3\0Form4\0Form5\0'
                u.mem_write(payload,body)
                n.write_words(u,packet,0x9e0e24,payload,0,len(body),len(body),0)
                out.clear();n.invoke(u,0x6357d0,[0,0x51,0,packet])
                consumed=n.read_words(u,packet+20,1)[0]
                rows.append(f'response {body.hex()} {consumed} '+('|'.join(out) or '-'))
        n.write_words(u,0xc5d974,0x50);u.mem_write(0xc5d97c,b'\0')
        n.write_words(u,record+0x170,guid&0xffffffff,guid>>32)
        u.reg_write(UC_X86_REG_ECX,0xc5d938);out.clear();n.invoke(u,0x668ce0,[record])
        rows.append(f'query {guid:016x} '+''.join(out))
    return rows


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('executable');p.add_argument('output');a=p.parse_args()
    n.initialize(a.executable);rows=capture();Path(a.output).write_text('\n'.join(rows)+'\n',encoding='utf-8')
    print(f'Captured {len(rows)-1} name-response/query samples')
