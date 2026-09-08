"""Capture original CEffect ready/model sound options with resident providers.

Runs 6F9840, 6F7B00, 4C5990 and the existing original attachment/scale chain.
Hooks only model/unit/setting providers, diagnostic scopes and audio submission.
No audio engine or client entry point runs.
"""
import argparse
from pathlib import Path
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP
import wmo_registration_oracle as n
from unit_effect_scale_oracle import ScaleOracle
from unit_water_effect_oracle import returned, bits

class SoundOracle(ScaleOracle):
    def __init__(self):
        super().__init__()
        self.kit=n.HEAP+0x8000
        self.calls=[]
        self.local=True
        self.ready=True
        self.resolve([-1.,-1.,-1.,1.,1.,1.],1.,1.,1.,1.,1.,0.01,100.)
        n.write_words(self.uc,self.kit+0x3c,5736)
        n.write_words(self.uc,self.fields,1,0,8)
        n.write_words(self.uc,0xca1238,1,0)
        n.write_floats(self.uc,self.origin,[10.,20.,30.])

    def hook(self,uc,address,size,unused):
        sp=uc.reg_read(UC_X86_REG_ESP)
        if address in (0x8b7da0,0x5eeb70,0x422130,0x5124d0): returned(uc)
        elif address==0x824f00: returned(uc,int(self.ready),8)
        elif address==0x7450b0: returned(uc,self.unit if self.local else 0)
        elif address==0x4d43c0: returned(uc,int(self.local))
        elif address==0x767440: returned(uc,self.cvar)
        elif address==0x4c6a40:
            args=n.read_words(uc,sp+4,8)
            options=n.read_words(uc,args[3],58) if args[3] else None
            position=n.read_words(uc,args[1],3) if args[1] else None
            self.calls.append((args[0],position,options))
            returned(uc)
        elif address==0x4c5c80: returned(uc)
        else: super().hook(uc,address,size,unused)

def capture(executable,output):
    n.initialize(executable)
    oracle=SoundOracle()
    lines=['# method local centered ready suppress | sound entry | position words or centered | 58 original option words']
    for method in ('kit','model'):
        for local in (False,True):
            for centered in (False,True):
                for ready in (False,True):
                    for suppress in (False,True):
                        oracle.local,oracle.ready=local,ready
                        n.write_words(oracle.uc,oracle.cvar+0x30,int(centered))
                        n.write_words(oracle.uc,oracle.effect+0x1c,oracle.kit)
                        n.write_words(oracle.uc,oracle.effect+0x48,0x400 if suppress else 0)
                        n.write_words(oracle.uc,0xca1238,1 if local else 2,0)
                        oracle.calls=[]
                        oracle.uc.reg_write(UC_X86_REG_ECX,oracle.effect)
                        if method=='kit': n.invoke(oracle.uc,0x6f9840,[])
                        else: n.invoke(oracle.uc,0x6f7b00,[0,0,0x444e5324,5736,oracle.origin,0,oracle.effect])
                        prefix=f'{method} {int(local)} {int(centered)} {int(ready)} {int(suppress)}'
                        if not oracle.calls: lines.append(prefix+'|none');continue
                        assert len(oracle.calls)==1
                        entry,position,options=oracle.calls[0]
                        lines.append(prefix+'|'+str(entry)+'|'+(' '.join(f'{v:08x}' for v in position) if position else 'centered')+'|'+(' '.join(f'{v:08x}' for v in options) if options else 'default'))
    Path(output).write_text('\n'.join(lines)+'\n',encoding='ascii')
    print(f'Captured {len(lines)-1} original effect audio submissions')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('executable');parser.add_argument('output');args=parser.parse_args()
    capture(args.executable,args.output)
