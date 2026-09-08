"""Capture original procedural cloud construction, lighting and row updates.

Hooks supply resident dynamic arrays, the CRT thread-data block and GPU texture
handles/uploads. Original random generation, trigonometry, noise, lighting,
quantization and double-buffer state execute the fingerprinted PE unchanged.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def invoke(u, address, arguments=()):
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, *arguments)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(address, n.STOP, timeout=30_000_000, count=50_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP, hex(u.reg_read(UC_X86_REG_EIP))


def capture():
    u = n.emulator()
    owner, thread, outputs = 0xd38d90, n.HEAP + 0x1000, n.HEAP + 0x2000
    allocation = n.HEAP + 0x4000
    texture_id = 0
    uploads = []
    strides = {0x6c0270: 12, 0x599820: 4, 0x6171b0: 1, 0x7f1220: 4,
               0x57e360: 8, 0x599030: 4, 0x57e400: 2}
    def hook(u, address, size, context):
        nonlocal allocation, texture_id
        sp = u.reg_read(UC_X86_REG_ESP)
        if address in strides:
            array = u.reg_read(UC_X86_REG_ECX)
            count = n.read_words(u, sp + 4, 1)[0]
            n.write_words(u, array, count, count, allocation)
            allocation += (count * strides[address] + 15) & ~15
            assert allocation < n.HEAP + 0x40000
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 8)
        elif address == 0x40df8d:
            return_value(u, thread)
        elif address == 0x681be0:
            n.write_words(u, u.reg_read(UC_X86_REG_ECX), 0)
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 44)
        elif address == 0x4b9200:
            texture_id += 1
            return_value(u, texture_id)
        elif address == 0x4b6cb0:
            return_value(u, n.read_words(u, sp + 4, 1)[0])
        elif address == 0x681f20:
            uploads.append(n.read_words(u, sp + 4, 6))
            return_value(u, 1)
    for address in [*strides, 0x40df8d, 0x681be0, 0x4b9200, 0x4b6cb0, 0x681f20]:
        u.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)
    n.write_words(u, thread + 0x14, 1)
    u.reg_write(UC_X86_REG_ECX, owner)
    invoke(u, 0x7f04b0)
    rows = ['# Build 12340 procedural clouds; native words and buffers are little endian.']
    rows.append('tables 1 ' + bytes(u.mem_read(0xd38688, 1024)).hex() + ' ' + bytes(u.mem_read(0xd38188, 1024)).hex())
    u.reg_write(UC_X86_REG_ECX, owner)
    invoke(u, 0x7f1b10, [0, 0])
    u.reg_write(UC_X86_REG_ECX, owner)
    invoke(u, 0x7f20e0, [bits(1.)])
    index_count, vertex_count = struct.unpack('<2H', u.mem_read(owner + 0x84, 4))
    rows.append(f'mesh {vertex_count} {index_count} ' + ' '.join(bytes(u.mem_read(n.read_words(u, owner + offset, 1)[0], vertex_count * stride)).hex() for offset,stride in [(0x5c,12),(0x68,8),(0x74,4)]) + ' ' + bytes(u.mem_read(n.read_words(u, owner + 0x80, 1)[0], index_count * 2)).hex())
    # Startup uses the default density 0.6 before constructing the grain table.
    u.mem_write(owner + 8, bytes([int((1. - n.read_floats(u, 0x9f5ad4, 1)[0]) * 255)]))
    u.reg_write(UC_X86_REG_ECX, owner)
    invoke(u, 0x7edb50, [bits(.96)])
    rows.append('grain ' + bytes(u.mem_read(0xd38588,256)).hex())
    n.write_words(u, owner + 0x2c, 1)
    n.write_floats(u, 0xd38b04, [.5])
    n.write_floats(u, 0xd38b18, [0.,0.,0.])
    n.write_floats(u, 0xd38e28, [8.,0.,8.])
    n.write_floats(u, 0xd38e48, [-8.,0.,8.])
    n.write_words(u, 0xd38bfc, 0xff446688, 0xff6688aa, 0xff112233)
    pointers = [outputs + i * 16 for i in range(5)]
    n.write_floats(u, pointers[4], [1.])
    u.reg_write(UC_X86_REG_ECX, owner)
    invoke(u, 0x7efae0, pointers)
    rows.append('lighting ' + ' '.join(bytes(u.mem_read(p, 4 if i == 4 else 12)).hex() for i,p in enumerate(pointers)))
    for step, (density, delta, force) in enumerate([(0.6,0.,1)] + [(0.6,.033,0)] * 34 + [(0.,.033,1),(1.,.033,1),(.25,1.,1),(.9,1.,1)]):
        n.write_floats(u, 0xd38c34, [density])
        n.write_floats(u, 0xd38b48, [delta])
        u.mem_write(owner + 10, bytes([force]))
        start = 0 if force else n.read_words(u, owner + 0x14, 1)[0]
        count = 128 if force else n.read_words(u, owner + 0x10, 1)[0]
        invoke(u, 0x7f1010)
        rgba = n.read_words(u, owner + 0x38, 1)[0]
        alpha = n.read_words(u, owner + 0x44, 1)[0]
        previous = n.read_words(u, owner + 0x50, 1)[0]
        rows.append(f'frame {step} {density} {delta} {force} {start} {count} ' + bytes(u.mem_read(owner + 8, 16)).hex() + ' ' + bytes(u.mem_read(owner + 0x88, 8)).hex() + ' ' + bytes(u.mem_read(rgba + start * 128 * 4, count * 128 * 4)).hex() + ' ' + bytes(u.mem_read(alpha + start * 128, count * 128)).hex() + ' ' + bytes(u.mem_read(previous,128 * 4)).hex())
    rows.append('permutation ' + bytes(u.mem_read(0xaf4a70,256)).hex())
    rows.append('octaves ' + bytes(u.mem_read(0xaf4dc4,50)).hex())
    for day in [0., .20138888, .2013889, .5, .9236111, .92361116, .999]:
        for depth in [0., .125, .5, 1.]:
            for eye in [[0.,0.,0.],[12345.125,-6789.25,128.125]]:
                sun = [eye[0]+8.,eye[1]+2.,eye[2]+8.]
                moon = [eye[0]-8.,eye[1]-3.,eye[2]-4.]
                n.write_floats(u,0xd38b04,[day])
                n.write_floats(u,0xd38b88,[depth])
                n.write_floats(u,0xd38b18,eye)
                n.write_floats(u,0xd38e28,sun)
                n.write_floats(u,0xd38e48,moon)
                n.write_floats(u,pointers[4],[1.])
                u.reg_write(UC_X86_REG_ECX,owner)
                invoke(u,0x7efae0,pointers)
                rows.append(f'light_sample {day} {depth} ' + ' '.join(struct.pack('<3f',*v).hex() for v in [eye,sun,moon]) + ' ' + ' '.join(bytes(u.mem_read(p,4 if i == 4 else 12)).hex() for i,p in enumerate(pointers)))
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} native cloud records')
