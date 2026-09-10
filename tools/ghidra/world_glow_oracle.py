"""Capture native normal-world glow inputs, wave texture and animated coordinates.

Executes 4F8770, 4F7290, 8BFDE0, 8C2920 and 8C2350 in the fingerprinted PE.
Loaded palette/player records, the clock, allocation and GPU device calls are
controlled boundaries. The original byte conversion, cubic texture generator,
matrix construction and vertex/constant publication run unchanged.
"""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


class NormalProducer:
    def __init__(self):
        self.u = n.emulator()
        self.owner, self.palette, self.unit, self.data, standard, alternate, self.first, self.second = [
            n.HEAP + i * 0x3000 for i in range(8)]
        n.write_words(self.u, self.owner, 0xa941c8)
        n.write_words(self.u, self.owner + 0x10, standard)
        n.write_words(self.u, self.owner + 0x24, alternate)
        n.write_words(self.u, standard + 8, self.first)
        n.write_words(self.u, alternate + 8, self.second)
        n.write_words(self.u, self.unit + 0x1008, self.data)
        n.write_words(self.u, 0xb74364, self.owner)
        self.available, self.wet = True, False

        def hook(u, address, size, context):
            if address == 0x7ecef0:
                return_value(u, self.palette)
            elif address == 0x4d3730:
                return_value(u, int(self.available))
            elif address == 0x4d3790:
                u.reg_write(UC_X86_REG_EDX, 0)
                return_value(u, 1)
            elif address == 0x4d4db0:
                return_value(u, self.unit)
            elif address == 0x780620:
                return_value(u, int(self.wet))
        self.u.hook_add(UC_HOOK_CODE, hook)

    def sample(self, glow, actual, fake, wet, available=True, active=True):
        self.available, self.wet = available, wet
        n.write_words(self.u, 0xd45780, self.owner if active else 0)
        n.write_words(self.u, self.owner + 0x2c, 0xdeadbeef)
        for address in (self.first, self.second):
            n.write_words(self.u, address + 0x30, 0x12345678)
        n.write_floats(self.u, self.palette + 300, [glow])
        self.u.mem_write(self.data + 0x1d, bytes([actual]))
        n.write_words(self.u, self.data + 0x2b8, fake)
        n.invoke(self.u, 0x4f8770, [])
        color = n.read_words(self.u, self.first + 0x30, 1)[0]
        assert n.read_words(self.u, self.second + 0x30, 1)[0] == color
        return n.read_words(self.u, self.owner + 0x2c, 1)[0], color


def wave_texture():
    """8C2920's complete V8U8 texture, including original truncation and clamps."""
    u = n.emulator()
    n.write_words(u, 0xd45c68, 9)
    def allocate(machine, address, size, context):
        return_value(machine, n.HEAP)
    u.hook_add(UC_HOOK_CODE, allocate, begin=0x76e540, end=0x76e540)
    sp = n.STACK + 0x18000
    n.write_words(u, sp, n.STOP, 0)
    u.reg_write(UC_X86_REG_ESP, sp)
    u.emu_start(0x8c2920, n.STOP, timeout=60_000_000, count=10_000_000)
    assert u.reg_read(UC_X86_REG_EIP) == n.STOP
    return bytes(u.mem_read(n.HEAP, 128 * 128 * 2))


class WaveProducer:
    def __init__(self):
        self.u = n.emulator()
        self.owner, self.noise, self.scene, self.blur, self.output, device, table, self.buffer, self.vertices = [
            n.HEAP + i * 0x1000 for i in range(9)]
        n.write_words(self.u, self.owner + 4, self.noise, self.scene, self.blur)
        n.write_words(self.u, self.owner + 0x24, self.output)
        n.write_words(self.u, 0xc5df88, device)
        n.write_words(self.u, device, table)
        for offset, address in [(0xd8, n.STOP + 0x100), (0xdc, n.STOP + 0x200),
                                (0x118, n.STOP + 0x300), (0xa8, n.STOP + 0x400)]:
            n.write_words(self.u, table + offset, address)
        self.state = {}
        self.u.hook_add(UC_HOOK_CODE, self.hook)

    def hook(self, u, address, size, context):
        sp = u.reg_read(UC_X86_REG_ESP)
        cleanup = None
        if address in (0x8c1890, 0x8c0ec0):
            cleanup = 0
        elif address in (0x682d20, 0x4b6cb0):
            return_value(u, 1)
            return
        elif address == 0x86ae20:
            return_value(u, self.state['time'])
            return
        elif address == 0x685f50:
            cleanup = 8
        elif address == 0x684850:
            return_value(u, self.buffer)
            u.reg_write(UC_X86_REG_ESP, sp + 16)
            return
        elif address == 0x6844c0:
            cleanup = 12
        elif address == 0x682eb0:
            cleanup = 4
        elif address == n.STOP + 0x100:
            return_value(u, self.vertices)
            u.reg_write(UC_X86_REG_ESP, sp + 8)
            return
        elif address == n.STOP + 0x200:
            cleanup = 8
        elif address == n.STOP + 0x300:
            domain, index, data, count = n.read_words(u, sp + 4, 4)
            assert domain == 4 and count == 1
            self.state['constants'][index] = n.read_floats(u, data, 4)
            cleanup = 16
        elif address == n.STOP + 0x400:
            self.state['vertices'] = [bytes(u.mem_read(self.vertices + i * 40, 40)) for i in range(4)]
            u.reg_write(UC_X86_REG_EIP, n.STOP)
            u.emu_stop()
            return
        if cleanup is not None:
            return_value(u, 0)
            u.reg_write(UC_X86_REG_ESP, sp + 4 + cleanup)

    def sample(self, width, height, time, color):
        for address, w, h in [(self.noise, 128, 128), (self.scene, width, height),
                              (self.blur, width // 4, height // 4), (self.output, width, height)]:
            n.write_words(self.u, address, 1, w, h, w, h)
            n.write_floats(self.u, address + 20, [1 / w, 1 / h])
        n.write_words(self.u, self.owner + 0x30, color)
        self.state = {'time': time, 'constants': {}}
        self.u.reg_write(UC_X86_REG_ECX, self.owner)
        n.invoke(self.u, 0x8c2350, [])
        return self.state


def capture(output):
    output.mkdir(parents=True, exist_ok=True)
    producer = NormalProducer()
    rows = ['# glow_bits actual fake wet available active selector color (hex words)']
    for glow in [-.5, 0., .2, .5, .75, 1., 1.25, 4., .5 / 255., 1.5 / 255.]:
        for actual, fake in [(0, 0), (1, 0), (25, 50), (50, 25), (99, 0), (0, 100),
                             (255, 0), (0, 0xffffffff), (0, 0x7fffffff)]:
            for wet, available, active in [(0, 1, 1), (1, 1, 1), (1, 0, 1), (1, 1, 0)]:
                selector, color = producer.sample(glow, actual, fake, wet, available, active)
                bits = struct.unpack('<I', struct.pack('<f', glow))[0]
                rows.append(' '.join(f'{v:08x}' for v in [bits, actual, fake, wet, available, active, selector, color]))
    producer_file = output / 'world_glow_producer_native.txt'
    producer_file.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    texture_file = output / 'world_glow_wave_native.bin'
    texture_file.write_bytes(wave_texture())
    wave = WaveProducer()
    rows = ['# width height time, then four native XYZ/color/UV0/UV1/UV2 vertices and two vec4 constants (hex words)']
    for width, height in [(32, 32), (64, 64), (65, 61), (80, 48), (1280, 720)]:
        for time in [0, 1, 1402, 2804, 2805, 3173, 3174, 0xffffffff]:
            result = wave.sample(width, height, time, 0x7f545454)
            data = b''.join(result['vertices']) + struct.pack('<8f', *result['constants'][0], *result['constants'][1])
            words = [width, height, time, *struct.unpack('<48I', data)]
            rows.append(' '.join(f'{v:08x}' for v in words))
    coordinates_file = output / 'world_glow_wave_coordinates_native.txt'
    coordinates_file.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    provenance = {
        'executable_sha256': hashlib.sha256(n.data).hexdigest(),
        'producer_cases': 360,
        'wave_coordinate_cases': 40,
        'texture': '128x128 V8U8, native generator 8C2920; no random inputs',
        'fixture_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                           for p in [producer_file, texture_file, coordinates_file]},
    }
    (output / 'world_glow_native.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(provenance))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    capture(args.output)
