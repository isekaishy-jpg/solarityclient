"""Run original model activation, active-bone scan, queue and callback dispatch.

832AB0, 832260, 830FB0, 82E790, 8321E0 and 831FC0 remain executable code.
Only CRT rand, application callbacks and the already-sampled identity bone
palette are supplied. No callback changes a sequence in this capture.
"""
import argparse
import hashlib
import itertools
import json
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n


def capture(output):
    u = n.emulator()
    (model, scene, resource, data, sequences, timers, bones, keys, table, rows,
     queue, callback, events, channels, times, palette) = [n.HEAP+i*0x1000 for i in range(16)]
    event_callback = callback + 0x100
    n.write_words(u, model, 2)
    n.write_words(u, model+0x10, 0x400001, 0xffff)
    n.write_words(u, model+0x28, scene, resource)
    n.write_words(u, model+0x78, callback)
    n.write_words(u, model+0x94, timers, palette)
    n.write_words(u, resource+0x150, data)
    n.write_words(u, data+0x1c, 4, sequences, 0, 0, 4, bones, 27, keys)
    u.mem_write(keys, b'\xff\xff'*27)
    bone_keys = [-1, 4, 26, 5]
    for i, (key, parent) in enumerate(zip(bone_keys, [-1, 0, 1, -1])):
        n.write_words(u, bones+i*0x58, key & 0xffffffff)
        u.mem_write(bones+i*0x58+8, struct.pack('<H', parent & 0xffff))
        if key >= 0:
            u.mem_write(keys+key*2, struct.pack('<H', i))
        for offset in [0x48, 0x6c, 0x96]:
            u.mem_write(timers+i*0xac+offset, b'\xff\xff')
    for i in range(4):
        u.mem_write(sequences+i*64, struct.pack('<HH', i, 0))
        n.write_words(u, sequences+i*64+4, 200+(i%2)*100, 0, 0x20+(i//2), 32767, 1, 1, 0)
        u.mem_write(sequences+i*64+0x3c, struct.pack('<HH', 0xffff, 0))
        n.write_words(u, table+i*4, rows+i*0x20)
        n.write_words(u, rows+i*0x20+0x10, 0, i, i)
    n.write_words(u, 0xad30d8, 0)
    n.write_words(u, 0xad30d4, 3)
    n.write_words(u, 0xad30e8, table)
    n.write_words(u, data+0x100, 4, events)
    identity = struct.pack('<16f', *[float(i%5 == 0) for i in range(16)])
    u.mem_write(scene+0xc4, identity)
    for i, timestamps in enumerate([[100]*4, [199,299,200,300], [0]*4, [201,301,201,301]]):
        u.mem_write(palette+i*64, identity)
        n.write_words(u, events+i*0x24, i+100, i+200, i)
        u.mem_write(events+i*0x24+0x1a, struct.pack('<H', 0 if i == 3 else 0xffff))
        n.write_words(u, events+i*0x24+0x1c, 4, channels+i*32)
        for j, tick in enumerate(timestamps):
            n.write_words(u, channels+i*32+j*8, 1, times+i*16+j*4)
            n.write_words(u, times+i*16+j*4, tick)
    base = bytes(u.mem_read(n.HEAP, 0x10000))
    captured = []
    random_state = 1

    def hook(uc, address, size, context):
        nonlocal random_state
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x88b867:
            random_state = (random_state*214013+2531011) & 0xffffffff
            uc.reg_write(UC_X86_REG_EAX, (random_state >> 16) & 0x7fff)
        elif address == callback:
            _, key, animation, _, overdue, _, _ = n.read_words(uc, sp+4, 7)
            captured.append((0, key, animation, overdue))
        elif address == event_callback:
            _, key, identifier, _, _, overdue = n.read_words(uc, sp+4, 6)
            captured.append((1, key, identifier-100, overdue))
        elif address != 0x830dc0:
            return
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp+4)

    u.hook_add(UC_HOOK_CODE, hook)

    def call(address, args=()):
        sp = n.STACK+0x18000
        n.write_words(u, sp, n.STOP, *[v & 0xffffffff for v in args])
        u.reg_write(UC_X86_REG_ESP, sp)
        u.reg_write(UC_X86_REG_ECX, model)
        u.emu_start(address, n.STOP, count=1_000_000)
        assert u.reg_read(UC_X86_REG_EIP) == n.STOP, (case, hex(u.reg_read(UC_X86_REG_EIP)), captured[:20])

    cases = []
    for first, second, order, enabled, now in itertools.product(range(4), range(4), range(2), range(2), [100,199,200,300,700]):
        cases.append((first, second, order, enabled, 1, 0x3f800000, 0x3f800000, 0, 0, 0, now))
    rng = random.Random(832260)
    for _ in range(256):
        previous = rng.choice([0, 199, 200, 299, 500, 0xfffffff0])
        cases.append((rng.randrange(4), rng.randrange(4), rng.randrange(2), rng.randrange(2), rng.randrange(1,3),
                      rng.choice([0x3f000000,0x3f800000,0x40000000,0xbf800000]),
                      rng.choice([0x3f000000,0x3f800000,0x40000000,0xbf800000]),
                      rng.choice([-50,0,45,350]), rng.choice([-50,0,45,350]), previous,
                      (previous+rng.choice([0,1,75,350,1700])) & 0xffffffff))
    lines = ['# Wow.exe SHA256 '+hashlib.sha256(n.data).hexdigest(),
             '# seq0 seq1 order events bone speed0 speed1 offset0 offset1 previous now | kind:key:index:overdue ... (hex)']
    for case in cases:
        first, second, order, enabled, bone, speed0, speed1, offset0, offset1, previous, now = case
        u.mem_write(n.HEAP, base)
        n.write_words(u, scene+0x1c, 4)
        n.write_words(u, model+0x1c4, event_callback if enabled else 0)
        n.write_words(u, 0xd411c0, 256, 0, queue, 0)
        random_state = 1
        captured.clear()
        requests = [[-1,first,-1,offset0,speed0,1,1], [bone_keys[bone],second,-1,offset1,speed1,1,1]]
        for request in requests[::1 if order == 0 else -1]:
            call(0x832ab0, request)
        n.write_words(u, scene+0xc, now, (now-previous)&0xffffffff)
        call(0x832260)
        lines.append(' '.join(f'{v&0xffffffff:08x}' for v in case)+' | '+
                     ' '.join(':'.join(f'{v:08x}' for v in row) for row in captured))
    output.write_text('\n'.join(line.rstrip() for line in lines)+'\n', encoding='utf-8')
    return dict(records=len(cases), sha256=hashlib.sha256(output.read_bytes()).hexdigest())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    print(json.dumps(capture(args.output)))
