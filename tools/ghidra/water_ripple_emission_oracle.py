"""Capture original build-12340 Unit_C ripple requests at 0x0071CBA0.

Position, yaw and scale getters expose controlled unit inputs. The shared
random generator returns the input word at every draw, while its original
float adapter and all emission arithmetic execute unchanged. Only the final
0x0077F400 request is captured instead of submitting it to graphics.

Each 104-byte record contains 6I6fI inputs and 3I8f2I outputs. Inputs are
flags, notification, registration flags, local ownership, deadline, now,
height, scale, speed, yaw, origin Z, surface Z and random word. Outputs are
draw count, next deadline, request count, center XYZ, yaw, radius, lifetime,
strength, growth, directional and local ownership. An absent request has
zero payload words. This fixture includes unsigned-clock wrap and admission
failures as well as idle, turning, directional and forced requests.
"""

import argparse
import struct
from pathlib import Path

import wmo_registration_oracle as n
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP


def capture(executable, output):
    n.initialize(executable)
    u = n.emulator()
    unit, movement, registration, vtable, guid, point, scalar = [n.HEAP + i * 0x2000 for i in range(7)]
    getter, yaw_getter, scale_getter = n.HEAP + 0xe000, n.HEAP + 0xe010, n.HEAP + 0xe020
    n.write_words(u, unit, vtable)
    n.write_words(u, unit + 8, guid)
    n.write_words(u, guid, 1, 0)
    n.write_words(u, 0xca1238, 1, 0)
    n.write_words(u, unit + 0xd8, movement)
    n.write_words(u, unit + 0xb8, registration)
    n.write_words(u, registration + 0x7c, 0x120)
    n.write_floats(u, registration + 0x80, [.25])
    n.write_words(u, registration + 0xbc, 1)
    n.write_floats(u, point, [10., 20., 0.])
    n.write_words(u, vtable + 0x2c, getter)
    n.write_words(u, vtable + 0x34, yaw_getter)
    n.write_words(u, vtable + 0x3c, scale_getter)
    n.write_floats(u, scalar, [.25, 1.])
    u.mem_write(yaw_getter, b'\xd9\x05' + struct.pack('<I', scalar) + b'\xc3')
    u.mem_write(scale_getter, b'\xd9\x05' + struct.pack('<I', scalar + 4) + b'\xc3')
    n.write_floats(u, unit + 0x854, [2.])
    n.write_floats(u, movement + 0x8c, [7.])
    n.write_words(u, 0xcd76ac, 1000)
    draws = 0
    spawn = []
    def hook(u, a, size, data):
        nonlocal draws
        sp = u.reg_read(UC_X86_REG_ESP)
        clean = 0
        if a == getter:
            out = n.read_words(u, sp + 4, 1)[0]
            u.mem_write(out, bytes(u.mem_read(point, 12)))
            u.reg_write(UC_X86_REG_EAX, out)
            clean = 4
        elif a == 0x464580:
            u.reg_write(UC_X86_REG_EAX, random_word)
            draws += 1
        elif a == 0x77f400:
            args = n.read_words(u, sp + 4, 8)
            spawn.append((n.read_floats(u, args[0], 3), n.read_floats(u, sp + 8, 5), args[6:]))
        else:
            return
        u.reg_write(UC_X86_REG_ESP, sp + 4 + clean)
        u.reg_write(UC_X86_REG_EIP, n.read_words(u, sp, 1)[0])
    for a in [getter, 0x464580, 0x77f400]: u.hook_add(UC_HOOK_CODE, hook, begin=a, end=a)

    records = bytearray()
    cases = []
    for flags in [0,0x10,0x20,1,2,4,8,5,9,6,10,15]:
     for event in [0,1,201]:
      for height, scale, speed, depth in [(2.,1.,7.,.25), (.2,.1,0.,-.1), (2.,6.,4.72,3.), (2.,20.,20.,3.99), (2.,1.,25.,4.), (2.,1.,.0001,1.), (2.,1.,.0001001,1.)]:
       for random_word in [0,0x12345678,0x7fffff,0xffffffff]:
        cases.append((flags,event,0x120,1,0,1000,height,scale,speed,.25,0.,depth,random_word))
    for deadline, now in [(1001,1000),(1000,1000),(999,1000),(0xfffffff0,5),(5,0xfffffff0)]:
     for flags in [0,1]:
      for event in [0,201]:
       cases.append((flags,event,0x120,0,deadline,now,2.,1.,7.,-.75,0.,.25,0x12345678))
    for regbits in [0,0x20,0x100,0x120,0x320]:
     cases.append((1,0,regbits,0,0,1000,2.,1.,7.,.25,0.,.25,0x12345678))
    for flags,event,regbits,local,deadline,now,height,scale,speed,yaw,z,surface,random_word in cases:
     n.write_words(u,movement+0x44,flags)
     n.write_words(u,unit+0xa58,deadline)
     n.write_words(u,registration+0x7c,regbits)
     n.write_words(u,guid,1 if local else 2,0)
     n.write_words(u,0xcd76ac,now)
     n.write_floats(u,unit+0x854,[height])
     n.write_floats(u,movement+0x8c,[speed])
     n.write_floats(u,scalar,[yaw,scale])
     n.write_floats(u,point,[10.,20.,z])
     n.write_floats(u,registration+0x80,[surface])
     u.reg_write(UC_X86_REG_ECX,unit)
     draws=0; spawn.clear()
     n.invoke(u,0x71cba0,[event])
     inputs=struct.pack('<6I6fI',flags,event,regbits,local,deadline,now,height,scale,speed,yaw,z,surface,random_word)
     outputs=struct.pack('<3I',draws,n.read_words(u,unit+0xa58,1)[0],len(spawn))
     if spawn:
      pos,values,tail=spawn[0]
      outputs+=struct.pack('<8f2I',*pos,*values,*tail)
     else: outputs+=bytes(40)
     records+=inputs+outputs
    Path(output).write_bytes(records)
    print('Captured',len(cases),'native emission cases; record size',len(records)//len(cases))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    capture(args.executable, args.output)
