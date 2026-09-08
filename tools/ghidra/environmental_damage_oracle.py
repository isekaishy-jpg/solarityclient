"""Capture build-12340 environmental damage receiver, log and UI arguments.

Runs 756800, 755270, 751150, 750400/74D920, 74DCB0, 74F910/74E290, 513380/5132C0
and 71C260 against resident units and CDataStore bytes. Hooks allocation,
name/identity providers, list admission, Lua output and visual submission.
No client or operating-system entry point runs.
"""
import argparse
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_ESP

import wmo_registration_oracle as native
from liquid_material_oracle import return_value


def capture():
    uc = native.emulator()
    unit, fields, log, packet, payload, name, tokens, visual, visual_bank = [native.HEAP+x for x in (0,0x2000,0x4000,0x5000,0x5100,0x5200,0x5300,0x5400,0x5500)]
    native.write_words(uc, unit+8, fields)
    native.write_words(uc, unit+0xd0, fields+0x100)
    uc.mem_write(name,b'WaterTest\0')
    uc.mem_write(tokens,b'player\0')
    native.write_words(uc, tokens+16,tokens)
    native.write_words(uc, 0xad4a4c, 1)
    native.write_words(uc, 0xad4a48, 1)
    native.write_words(uc, 0xad4a5c, visual_bank)
    native.write_words(uc, visual_bank, visual)
    native.write_words(uc, 0xca120c, 1,1,1,1,1,1)
    native.write_words(uc, 0xca1388, 1000, 1_700_000_000)
    log_ready = False
    output, events, visuals, order, log_payload = [], [], [], [], []
    event_name=native.HEAP+0x5600
    uc.mem_write(event_name,b'COMBAT_LOG_EVENT\0COMBAT_LOG_EVENT_UNFILTERED\0')
    native.write_words(uc,0xc25788,event_name,event_name+17)
    subscriber=native.HEAP+0x5700
    native.write_words(uc,subscriber+8,2)
    resident_guid = 1

    def string(pointer):
        return bytes(uc.mem_read(pointer,256)).split(b'\0')[0].decode() if pointer else None

    def ret(value=0, pop=0):
        sp=uc.reg_read(UC_X86_REG_ESP)
        return_value(uc,value)
        uc.reg_write(UC_X86_REG_ESP,sp+4+pop)

    def provider(uc, address, size, context):
        nonlocal log_ready
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address == 0x4d4db0:
            low,high,mask=native.read_words(uc,sp+4,3)
            ret(unit if low+(high<<32)==resident_guid else 0)
        elif address == 0x4d3790:
            uc.reg_write(UC_X86_REG_EDX,0);ret(1)
        elif address == 0x52bd10:
            ret(0)
        elif address == 0x74f2d0:
            uc.mem_write(log,bytes(0x80));ret(log,12)
        elif address == 0x47cf80:
            ret(0,4)
        elif address == 0x74fd40:
            ret(name,12)
        elif address == 0x86e200:
            ret(0,4)
        elif address == 0x74f910:
            log_ready=True;order.append('log')
        elif address == 0x4fb400:
            ret(0)
        elif address == 0x817db0:
            ret(0)
        elif address == 0x74f6c0:
            ret(1)
        elif address == 0x81b510:
            ret(subscriber)
        elif address == 0x81aa00:
            event,lua,count=native.read_words(uc,sp+4,3)
            values=output[-count:][1:]
            assert len(values)==18,(event,count,output)
            events.append([event,values.copy()]);order.append(f'event:{event}')
            if event==0x236:log_payload[:]=values
            ret()
        elif address == 0x84dcc0:
            index=struct.unpack('<i',uc.mem_read(sp+8,4))[0]
            index=index-1 if index>0 else len(output)+index
            value=output.pop();output.insert(index,value);ret()
        elif address == 0x84dbf0:
            index=struct.unpack('<i',uc.mem_read(sp+8,4))[0]
            index=index if index>=0 else len(output)+index+1
            del output[index:];ret()
        elif address == 0x715d60:
            order.append('damage_feedback');ret()
        elif address == 0x60bb70:
            guid_pointer,count_pointer=native.read_words(uc,sp+4,2)
            low,high=native.read_words(uc,guid_pointer,2)
            native.write_words(uc,count_pointer,int(low==1 and high==0))
            ret(tokens+16)
        elif address == 0x745230:
            data=native.read_words(uc,native.read_words(uc,sp+4,1)[0],13)
            visuals.append(list(data));order.append('visual');ret(0,4)
        elif address == 0x81b530:
            event,fmt=native.read_words(uc,sp+4,2)
            fmt=string(fmt)
            args=native.read_words(uc,sp+12,len(fmt)//2)
            values=[]
            for spec,value in zip((fmt[i:i+2] for i in range(0,len(fmt),2)),args):
                values.append(string(value) if spec=='%s' else struct.unpack('<i',struct.pack('<I',value))[0])
            events.append([event,values]);order.append(f'event:{event}');ret()
        elif address in (0x84dab0,):
            ret(1)
        elif address in (0x84e280,0x84e350,0x84e2a0,0x84e2d0):
            if address==0x84e280: value=None
            elif address==0x84e350:value=string(native.read_words(uc,sp+8,1)[0])
            elif address==0x84e2a0:value=struct.unpack('<d',uc.mem_read(sp+8,8))[0]
            else:value=struct.unpack('<i',uc.mem_read(sp+8,4))[0]
            output.append(value);ret()

    uc.hook_add(UC_HOOK_CODE,provider)
    rows=[]
    cases=[(1,kind,100,0,0,1) for kind in range(6)]
    cases += [(1,kind,amount,absorbed,resisted,1) for kind,amount,absorbed,resisted in
              [(1,0,0,0),(1,0,25,0),(1,0,0,30),(1,10,25,30),(3,0xffffffff,0,0),(4,0x80000000,0,0),
               (1,0,25,30),(1,0,0xffffffff,0),(1,0,0,0x80000000),(1,0xffffffff,25,30)]]
    cases += [(2,1,100,0,0,1),(0,1,100,0,0,1),(1,1,100,0,0,0),(1,1,100,0,0,2)]
    for wire_guid,kind,amount,absorbed,resisted,active_guid in cases:
        output.clear();events.clear();visuals.clear();order.clear();log_payload.clear();log_ready=False
        native.write_words(uc,fields,1,0,0x19)
        native.write_words(uc,fields+0x100+0x68,1000)
        native.write_words(uc,unit+0xfb0,500)
        native.write_words(uc,0xca1380,active_guid,0)
        native.write_words(uc,0xca1394,0)
        native.write_words(uc,0xcd76ac,2250)
        body=struct.pack('<QBIII',wire_guid,kind,amount,absorbed,resisted)
        uc.mem_write(payload,body)
        native.write_words(uc,packet,0,payload,0,len(body),len(body),0)
        native.invoke(uc,0x756800,[0,0x1fc,0,packet])
        rows.append(dict(body=body.hex(),active=active_guid,log=log_payload.copy(),events=events.copy(),visuals=visuals.copy(),order=order.copy(),health=native.read_words(uc,unit+0xfb0,1)[0]))
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable');parser.add_argument('output');args=parser.parse_args()
    native.initialize(args.executable)
    rows=capture();Path(args.output).write_text(json.dumps(rows,indent=2)+'\n')
    def lua(value):
        return 'nil' if value is None else format(value,'.14g') if isinstance(value,(int,float)) else value
    def payload(values):return ','.join(map(lua,values))
    lines=['|'.join([row['body'],str(row['active']),str(row['health']),str(len(row['visuals'])),payload(row['log']),
                    ';'.join(str(event)+':'+payload(values) for event,values in row['events'])]) for row in rows]
    Path(args.output).with_suffix('.txt').write_text('\n'.join(lines)+'\n')
    print(f'{len(rows)} native environmental damage captures')


if __name__=='__main__':main()
