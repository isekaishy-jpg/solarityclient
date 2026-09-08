"""Run the original continuous sky clock and normalized exterior light provider.

The clock's resident realm minute, rate, anchor and running flag are supplied.
Only the monotonic millisecond source is hooked. Native 76CFF0, 7EEA90 and
834AE0 execute unchanged; a return stub stores ST0 at 4F8410's float boundary.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def capture():
    u = n.emulator()
    now = 0
    u.hook_add(UC_HOOK_CODE, lambda u,a,s,c: return_value(u, now), begin=0x86ae20, end=0x86ae20)
    result, stub, clock, light = n.HEAP+0x1000, n.STOP+64, n.HEAP, n.HEAP+0x2000
    u.mem_write(stub, b'\xd9\x1d'+struct.pack('<I',result)+b'\xe9'+struct.pack('<i',n.STOP-(stub+11)))
    rows = ['# Original 76CFF0 clock and 7EEA90 -> 834AE0 exterior ray.']
    for minute in [0, 1, 330, 720, 1260, 1439]:
        for rate in [0., 1./60., .25, 1., 2.]:
            for elapsed in [0,1,16,33,999,1000,30000,60000,86399000,86400000,172800000]:
                now = elapsed
                n.write_floats(u, clock+0x30, [rate])
                n.write_words(u, clock+0x3c, 0, 1)
                n.write_floats(u, clock+0x44, [float(minute)])
                sp = n.STACK+0x18000
                n.write_words(u, sp, stub)
                u.reg_write(UC_X86_REG_ESP, sp)
                u.reg_write(UC_X86_REG_ECX, clock)
                u.emu_start(0x76cff0, n.STOP, timeout=3_000_000, count=2_000_000)
                assert u.reg_read(UC_X86_REG_EIP) == n.STOP
                day = bytes(u.mem_read(result,4))
                u.mem_write(0xd38b04, day)
                n.invoke(u,0x7eea90,[])
                u.reg_write(UC_X86_REG_ECX,light)
                n.invoke(u,0x834ae0,[0xd38c9c])
                rate_word = n.read_words(u,clock+0x30,1)[0]
                rows.append(f'clock {minute} {rate_word:x} {elapsed} '+day.hex()+' '+bytes(u.mem_read(light+0x24,12)).hex())
    return rows


if __name__ == '__main__':
    p=argparse.ArgumentParser()
    p.add_argument('executable',type=Path)
    p.add_argument('output',type=Path)
    args=p.parse_args()
    n.initialize(args.executable)
    rows=capture()
    args.output.write_text('\n'.join(rows)+'\n')
    print(f'Wrote {len(rows)-1} original sky clock and direction samples')
