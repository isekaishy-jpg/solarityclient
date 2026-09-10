"""Execute unchanged archive NetherBlur vertex/pixel and NetherCombine shaders.

Native constructors, animation, 6x6 mesh generation and fade supply the draw
inputs. Direct3D 9 executes both blur draws and final scene composition.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
from pathlib import Path

import ghost_screen_shader_oracle as g
import liquid_shader_oracle as d
import nether_screen_oracle as native
import wmo_registration_oracle as n


class Chain(g.Chain):
    def __init__(self, width, height, shaders, vertex):
        super().__init__(width, height, shaders)
        self.vertex = self.create(self.device, 91, 'p', d.buffer(vertex))

    def blur(self, target, source, mesh, angle):
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, None)
        d.call(self.device, 37, 'up', 0, target[1])
        d.call(self.device, 107, 'p', self.shaders['blur'])
        d.call(self.device, 92, 'p', self.vertex)
        d.call(self.device, 89, 'u', 0x142)
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, source[0])
        # 7E9B10 publishes these once; both normalized draws retain them.
        inverse_width, inverse_height = struct.unpack('<2f', struct.pack('<2f', 1. / self.width, 1. / self.height))
        constants = [angle, 0., 0., 0., 8. * inverse_height * inverse_width, inverse_height, 0., 0., 1., 0., 0., 0.]
        d.call(self.device, 94, 'upu', 0, d.floats(constants), 3)
        vertices = bytearray()
        for y in range(5):
            for x in range(5):
                start = y * 6 + x
                for index in [start, start + 7, start + 6, start, start + 1, start + 7]:
                    vertices += mesh[index * 24:(index + 1) * 24]
        d.call(self.device, 43, 'upuufu', 0, None, 1, 0, 1., 0)
        d.call(self.device, 41)
        d.call(self.device, 83, 'uupu', 4, 50, d.buffer(bytes(vertices)), 24)
        d.call(self.device, 42)

    def compose(self, mesh, angle, fade):
        self.blur(self.temporary, self.scene, mesh, angle)
        self.blur(self.box, self.temporary, mesh, angle)
        d.call(self.device, 109, 'upu', 0, d.floats([.6, .6, .78, fade]), 1)
        self.draw('combine', (self.destination, self.width, self.height), [self.scene, self.box], [(0, 0)] * 2)
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
    n.initialize(args.executable)
    vertex = d.shader_variants(args.shaders / 'SHADERS_VERTEX_VS_2_0_FFXNETHERBLUR.BLS')[0]
    shaders = {kind: d.shader_variants(args.shaders / f'SHADERS_PIXEL_PS_2_0_FFXNETHER{name}.BLS')[0]
               for kind, name in [('blur', 'BLUR'), ('combine', 'COMBINE')]}
    settings = [(delta, axis, reset) for delta, axis, reset in [
        (0., (1., 0., 0.), True), (.125, (0., 1., 0.), False),
        (.25, (-1., 0., 0.), False), (.4, (0., -1., 0.), False),
        (.75, (.3, -.8, .5), False), (.333, (.3, -.8, -.5), False),
        (1., (.3, .8, .0001), False), (0., (1., 0., 0.), True),
        (1/60, (1., 0., 0.), False), (.5, (1., 0., 0.), False),
    ]]
    records = []
    for source in sorted(args.inputs.glob('*.rgba')):
        width, height = map(int, source.stem.split('-')[-2:])
        rgba = source.read_bytes()
        assert len(rgba) == width * height * 4
        producer = native.Producer()
        renderer = Chain(width, height, shaders, vertex)
        try:
            renderer.upload(rgba)
            for delta, axis, reset in settings:
                state = producer.advance(delta, [*axis, 0., 0., 1., 0., 0., 0., 0., 1., 0.])
                mesh = producer.mesh(width, height)
                fade = producer.fade(delta, reset)
                result = renderer.compose(mesh, state['angle'], fade)
                records.append(struct.pack('<2I4fI', width, height, delta, *axis, reset) + rgba + result)
        finally:
            renderer.close()
    args.output.write_bytes(struct.pack('<4sI', b'NTHR', len(records)) + b''.join(records))
    metadata = {
        'executable_sha256': hashlib.sha256(n.data).hexdigest(),
        'vertex_shader_sha256': hashlib.sha256(vertex).hexdigest(),
        'pixel_shader_sha256': {name: hashlib.sha256(code).hexdigest() for name, code in shaders.items()},
        'fixture_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(),
        'frames': len(records),
        'texture_policy': 'normalized NPOT, linear/clamp, no sRGB; native 6x6 mesh reused across two blur draws',
    }
    args.output.with_suffix('.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(metadata))
