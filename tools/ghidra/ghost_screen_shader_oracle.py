"""Capture original FFXBox4/FFXGauss4/FFXDeath over controlled world rasters.

The native 7E87B0 producer supplies vertex color and 8C0590 supplies coordinates.
Direct3D 9 executes the unchanged archive pixel shaders with normalized NPOT
textures, linear sampling, and explicit input pixels. Native UI/device/resource
calls are supplied boundaries; this does not run a client entry point.
"""
import argparse
import ctypes as c
import json
import hashlib
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
import liquid_shader_oracle as d


class Producer:
    def __init__(self):
        self.u = n.emulator()
        self.owner, self.scene, self.blur, self.output, self.palette = [n.HEAP + i * 0x1000 for i in range(5)]
        n.write_words(self.u, self.owner + 4, self.scene, self.blur)
        n.write_words(self.u, self.owner + 0x24, self.output)
        self.color = 0

        def hook(u, address, size, context):
            sp = u.reg_read(UC_X86_REG_ESP)
            if address == 0x7ecef0:
                return_value(u, self.palette)
            elif address == 0x8c1890:
                return_value(u, 0)
            elif address == 0x682d20:
                return_value(u, 1)  # CGxDeviceD3d's backend identity.
            elif address == 0x4b6cb0:
                return_value(u, 1)
            elif address == 0x685f50:
                return_value(u, 0)
                u.reg_write(UC_X86_REG_ESP, sp + 12)
            elif address == 0x682400:
                color = n.read_words(u, sp + 24, 1)[0]
                self.color = n.read_words(u, color, 1)[0]
                u.reg_write(UC_X86_REG_EIP, n.STOP)
                u.emu_stop()
        self.u.hook_add(UC_HOOK_CODE, hook)

    def vertex_color(self, glow, width, height):
        for address, w, h in [(self.scene, width, height),
                              (self.blur, width // 4, height // 4),
                              (self.output, width, height)]:
            n.write_words(self.u, address, 1, w, h, w, h)
            n.write_floats(self.u, address + 20, [1 / w, 1 / h])
        n.write_floats(self.u, self.palette + 300, [glow])
        self.u.reg_write(UC_X86_REG_ECX, self.owner)
        n.invoke(self.u, 0x7e87b0, [])
        return self.color


class Chain(d.Renderer):
    def __init__(self, width, height, shaders):
        d.EXTENT = max(width, height)
        super().__init__()
        self.width, self.height = width, height
        self.shaders = {name: self.create(self.device, 106, 'p', d.buffer(code)) for name, code in shaders.items()}
        self.scene = self.texture_target(width, height)
        self.box = self.texture_target(width // 4, height // 4)
        self.temporary = self.texture_target(width // 4, height // 4)
        self.destination = self.create(self.device, 28, 'uuuuuup', width, height, 21, 0, 0, 0, None, output_before_last=True)
        self.cpu = self.create(self.device, 36, 'uuuup', width, height, 21, 2, None, output_before_last=True)
        for sampler in range(4):
            for state, value in [(1, 3), (2, 3), (5, 2), (6, 2), (7, 0), (11, 0)]:
                d.call(self.device, 69, 'uuu', sampler, state, value)

    def texture_target(self, width, height):
        texture = self.create(self.device, 23, 'uuuuuup', width, height, 1, 1, 21, 0, None, output_before_last=True)
        surface = self.create(texture, 18, 'u', 0)
        return texture, surface, width, height

    def upload(self, rgba):
        surface = self.create(self.device, 36, 'uuuup', self.width, self.height, 21, 2, None, output_before_last=True)
        locked = d.LockedRect()
        d.call(surface, 13, 'ppu', c.byref(locked), None, 0)
        for y in range(self.height):
            row = bytearray(rgba[y * self.width * 4:(y + 1) * self.width * 4])
            row[0::4], row[2::4] = row[2::4], row[0::4]
            c.memmove(locked.bits + y * locked.pitch, bytes(row), len(row))
        d.call(surface, 14)
        d.call(self.device, 30, 'pppp', surface, None, self.scene[1], None)

    def draw(self, name, target, sources, offsets, color=0xffffffff):
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, None)
        destination, width, height = target
        d.call(self.device, 37, 'up', 0, destination)
        d.call(self.device, 107, 'p', self.shaders[name])
        d.call(self.device, 92, 'p', None)
        d.call(self.device, 89, 'u', 0x44 | (len(sources) << 8))
        for index, source in enumerate(sources):
            d.call(self.device, 65, 'up', index, source[0])
        vertices = bytearray()
        # 8C0590's D3D coordinates after its orthographic projection.
        for x, y in [(0, 0), (0, height), (width, 0), (width, 0), (0, height), (width, height)]:
            vertices += struct.pack('<4fI', x, y, 0., 1., color)
            for source, (dx, dy) in zip(sources, offsets):
                vertices += struct.pack('<2f', x / width + (.5 + dx) / source[2],
                                        y / height + (.5 + dy) / source[3])
        d.call(self.device, 43, 'upuufu', 0, None, 1, 0, 1., 0)
        d.call(self.device, 41)
        d.call(self.device, 83, 'uupu', 4, 2, d.buffer(bytes(vertices)), 20 + len(sources) * 8)
        d.call(self.device, 42)

    def ghost(self, color):
        self.draw('box', self.box[1:], [self.scene] * 4, [(-1.5, -1.5), (.5, -1.5), (.5, .5), (-1.5, .5)])
        self.draw('gauss', self.temporary[1:], [self.box] * 4, [(x, 0) for x in [-2.5, -.5, .5, 2.5]])
        self.draw('gauss', self.box[1:], [self.temporary] * 4, [(0, y) for y in [-2.5, -.5, .5, 2.5]])
        self.draw('ghost', (self.destination, self.width, self.height), [self.scene, self.box], [(0, 0)] * 2, color)
        d.call(self.device, 32, 'pp', self.destination, self.cpu)
        locked = d.LockedRect()
        d.call(self.cpu, 13, 'ppu', c.byref(locked), None, 0x10)
        rgba = bytearray()
        for y in range(self.height):
            row = bytearray(c.string_at(locked.bits + y * locked.pitch, self.width * 4))
            row[0::4], row[2::4] = row[2::4], row[0::4]
            rgba += row
        d.call(self.cpu, 14)
        return rgba


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('shaders', type=Path)
    parser.add_argument('inputs', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    inputs = sorted(args.inputs.glob('*.rgba'))
    if not inputs:
        parser.error('inputs must contain exported world-WIDTH-HEIGHT.rgba files')
    n.initialize(args.executable)
    producer = Producer()
    shaders = {kind: d.shader_variants(args.shaders / f'SHADERS_PIXEL_PS_2_0_FFX{name}.BLS')[0]
               for kind, name in [('box', 'BOX4'), ('gauss', 'GAUSS4'), ('ghost', 'DEATH')]}
    records = []
    for source in inputs:
        width, height = map(int, source.stem.split('-')[-2:])
        rgba = source.read_bytes()
        assert len(rgba) == width * height * 4
        renderer = Chain(width, height, shaders)
        try:
            renderer.upload(rgba)
            for glow in [0., .2, .5, 1., 1.25, 0.5 / 255., 1.5 / 255.]:
                color = producer.vertex_color(glow, width, height)
                result = renderer.ghost(color)
                records.append(struct.pack('<2IfI', width, height, glow, color) + rgba + result)
        finally:
            renderer.close()
    args.output.write_bytes(struct.pack('<4sI', b'GHST', len(records)) + b''.join(records))
    provenance = {
        'executable_sha256': hashlib.sha256(n.data).hexdigest(),
        'pixel_shader_sha256': {name: hashlib.sha256(code).hexdigest() for name, code in shaders.items()},
        'input_sha256': {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs},
        'fixture_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(),
        'native_frames': len(records),
        'texture_policy': 'normalized NPOT, linear/clamp, no sRGB conversion',
    }
    args.output.with_suffix('.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'native_ghost_frames': len(records)}))
