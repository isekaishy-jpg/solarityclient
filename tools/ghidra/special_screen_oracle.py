"""Capture the original special-screen noise, polar mesh and activation ramp.

The pinned image executes 7E8E40/985580/9854D0/9852A0 for all texture bytes,
7E8410/7E8380/7E8600 for the mesh, and 7E9010/7E9670 for parameters/ramp.
Allocation and GX/device boundaries are supplied; no client entry point runs.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from camera_water_oracle import ret


def invoke(u, address, arguments):
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, *arguments)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(address, n.STOP, count=5_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP, hex(u.reg_read(UC_X86_REG_EIP))


def noise():
    u = n.emulator()
    pixels = 0x03100000
    u.mem_map(pixels, 256 * 256 * 4)
    u.hook_add(UC_HOOK_CODE, lambda u, a, s, c: ret(u, pixels),
               begin=0x76e540, end=0x76e540)
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, 0, 256, 256, 0, 0, 0, 0, 0)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x7e8e40, n.STOP, count=500_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP
    result = bytes(u.mem_read(pixels, 256 * 256 * 4))
    assert all(result[i] == 255 for i in range(len(result)) if i % 4 != 3)
    return result


class Producer:
    def __init__(self):
        self.u = n.emulator()
        (self.root, self.seed, self.propagate, self.combine, self.passes, self.input,
         self.history, self.scene, self.output, self.gx, self.vtable, self.buffer,
         self.indices) = [n.HEAP + i * 0x1000 for i in range(13)]
        self.vertices = n.HEAP + 0x10000
        n.write_words(self.u, self.root + 0x10, self.passes)
        n.write_words(self.u, self.passes, self.seed, self.propagate, self.combine)
        n.write_words(self.u, self.combine + 4, self.history, self.scene)
        n.write_words(self.u, self.combine + 0x24, self.output)
        n.write_words(self.u, self.combine + 0x40, self.buffer)
        n.write_words(self.u, self.combine + 0x48, self.indices)
        n.write_words(self.u, 0xc5df88, self.gx)
        n.write_words(self.u, self.gx, self.vtable)
        for offset, target in [(0xd8, n.STOP + 0x100), (0xdc, n.STOP + 0x200),
                               (0x118, n.STOP + 0x300)]:
            n.write_words(self.u, self.vtable + offset, target)
        self.constants = {}
        self.seed_capture = None
        self.u.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, u, address, size, context):
        sp = u.reg_read(UC_X86_REG_ESP)
        if self.seed_capture is not None:
            if address == 0x684850:
                self.seed_stride = n.read_words(u, sp + 8, 1)[0]
                ret(u, self.buffer, 12)
                return
            if address == n.STOP + 0x200:
                self.seed_capture.append(bytes(u.mem_read(self.vertices, 4 * self.seed_stride)))
            if address in (0x681b00, 0x8c0ec0, 0x8c1520, 0x6813b0):
                ret(u, 0)
                return
            if address == 0x682eb0:
                ret(u, 0, 4)
                return
            if address == n.STOP + 0x400:
                ret(u, 0, 8)
                return
        if address == n.STOP + 0x100:
            ret(u, self.vertices, 4)
        elif address == n.STOP + 0x200:
            ret(u, 0, 8)
        elif address == n.STOP + 0x300:
            kind, slot, pointer, count = n.read_words(u, sp + 4, 4)
            assert kind == 4 and count == 1
            self.constants[slot] = n.read_words(u, pointer, 4)
            ret(u, 0, 16)
        elif address == 0x682d20:
            ret(u, 1)
        elif address in (0x8c1890, 0x4b6cb0):
            ret(u, 1)
        elif address == 0x685f50:
            ret(u, 0, 8)
        elif address == 0x681b00:
            u.reg_write(UC_X86_REG_EIP, n.STOP)

    def select(self, parameters):
        n.write_words(self.u, self.input, *parameters)
        self.u.reg_write(UC_X86_REG_ECX, self.root)
        invoke(self.u, 0x7e9010, [3, self.input])
        return (n.read_words(self.u, self.seed + 0x34, 1)[0],
                n.read_words(self.u, self.propagate + 0x5c, 1)[0],
                *n.read_words(self.u, self.combine + 0x54, 2))

    def mesh(self, width, height):
        for address, w, h in [(self.history, 256, 128), (self.scene, width, height),
                              (self.output, width, height)]:
            n.write_words(self.u, address, 1, w, h, w, h)
            n.write_floats(self.u, address + 20, [1 / w, 1 / h])
        self.u.reg_write(UC_X86_REG_ECX, self.combine)
        invoke(self.u, 0x7e8410, [self.output + 12, self.history, self.scene])
        vertices = bytes(self.u.mem_read(self.vertices, 1089 * 32))
        self.u.reg_write(UC_X86_REG_ECX, self.combine)
        invoke(self.u, 0x7e8600, [])
        indices = bytes(self.u.mem_read(self.vertices, 6144 * 2))
        n.write_words(self.u, self.combine + 0x4c, width, height)
        self.u.mem_write(self.buffer + 0x1c, b'\1\1')
        self.u.mem_write(self.indices + 0x1c, b'\1\1')
        return vertices, indices

    def advance(self, delta):
        self.constants.clear()
        n.write_floats(self.u, 0xcd76a0, [delta])
        self.u.reg_write(UC_X86_REG_ECX, self.combine)
        invoke(self.u, 0x7e9670, [])
        assert set(self.constants) == {0, 1}, self.constants
        return (*n.read_words(self.u, self.combine + 0x58, 1),
                *self.constants[0], *self.constants[1])

    def seed_frame(self):
        noise = n.HEAP + 0x20000
        n.write_words(self.u, noise, 1, 256, 256, 256, 256)
        n.write_words(self.u, self.history, 1, 256, 128, 256, 128)
        n.write_words(self.u, self.seed + 4, noise)
        n.write_words(self.u, self.seed + 0x24, self.history)
        n.write_words(self.u, self.vtable + 0xa8, n.STOP + 0x400)
        row = n.read_words(self.u, self.seed + 0x30, 1)[0]
        self.seed_capture = []
        self.u.reg_write(UC_X86_REG_ECX, self.seed)
        invoke(self.u, 0x7e92a0, [])
        captures = self.seed_capture
        self.seed_capture = None
        assert list(map(len, captures)) == [80, 48]
        return row, captures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.mkdir(parents=True, exist_ok=True)
    pixels = noise()
    (args.output / 'special_noise_native.rgba').write_bytes(pixels)
    producer = Producer()
    meshes = bytearray(b'SPMS' + struct.pack('<I', 4))
    for width, height in [(64, 64), (65, 61), (80, 48), (1280, 720)]:
        vertices, indices = producer.mesh(width, height)
        meshes += struct.pack('<2I', width, height) + vertices + indices
    (args.output / 'special_mesh_native.bin').write_bytes(meshes)
    rows = ['# select parameters[3] native-color decay strength countdown; frame delta countdown c0[4] c1[4] (hex words)']
    for parameters in [(0, 1, 100), (2, 5, 50), (7, 255, 300),
                       (0xffffffff, 0xffffffff, 0x80000000), (1, 0, 0)]:
        rows.append('select ' + ' '.join(f'{v:08x}' for v in (*parameters, *producer.select(parameters))))
        for delta in [0., 1/60, .125, .5, 1., 2., 0., 4., 1/1200]:
            word = struct.unpack('<I', struct.pack('<f', delta))[0]
            rows.append('frame ' + ' '.join(f'{v:08x}' for v in (word, *producer.advance(delta))))
    (args.output / 'special_state_native.txt').write_text('\n'.join(rows) + '\n', encoding='utf-8')
    seeds = bytearray(b'SPSD' + struct.pack('<I', 260))
    for _ in range(260):
        row, captures = producer.seed_frame()
        seeds += struct.pack('<I', row) + b''.join(captures)
    (args.output / 'special_seed_native.bin').write_bytes(seeds)
    metadata = {'executable_sha256': hashlib.sha256(n.data).hexdigest(),
                'noise_sha256': hashlib.sha256(pixels).hexdigest(),
                'mesh_sha256': hashlib.sha256(meshes).hexdigest(),
                'seed_sha256': hashlib.sha256(seeds).hexdigest(),
                'functions': ['7E8E40', '985580', '9854D0', '9852A0', '7E8410', '7E8380', '7E8600', '7E9010', '7E92A0', '7E9670']}
    (args.output / 'special_native.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()
