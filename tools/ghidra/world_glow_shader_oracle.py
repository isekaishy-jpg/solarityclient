"""Execute original normal GLOW/GLOWWAVE shaders over controlled world rasters.

4F8770 supplies vertex colors, 8C2920 generates the signed displacement texture,
and 8C2350 supplies animated vertices and shader constants. Direct3D 9 executes
the unchanged BOX4, GAUSS4, GLOW and GLOWWAVE programs from local archives.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
from pathlib import Path

import ghost_screen_shader_oracle as g
import liquid_shader_oracle as d
import world_glow_oracle as native
import wmo_registration_oracle as n


class Chain(g.Chain):
    def __init__(self, width, height, shaders, wave):
        super().__init__(width, height, shaders)
        # D3DFMT_V8U8 is the native generated texture's signed two-channel format.
        self.wave = self.create(self.device, 23, 'uuuuuup', 128, 128, 1, 0, 60, 1, None, output_before_last=True)
        surface = self.create(self.wave, 18, 'u', 0)
        locked = d.LockedRect()
        d.call(surface, 13, 'ppu', c.byref(locked), None, 0)
        for y in range(128):
            c.memmove(locked.bits + y * locked.pitch, wave[y * 256:(y + 1) * 256], 256)
        d.call(surface, 14)

    def compose(self, color, coordinates):
        for state in [1, 2]:
            d.call(self.device, 69, 'uuu', 0, state, 3)
        self.draw('box', self.box[1:], [self.scene] * 4, [(-1.5, -1.5), (.5, -1.5), (.5, .5), (-1.5, .5)])
        self.draw('gauss', self.temporary[1:], [self.box] * 4, [(x, 0) for x in [-2.5, -.5, .5, 2.5]])
        self.draw('gauss', self.box[1:], [self.temporary] * 4, [(0, y) for y in [-2.5, -.5, .5, 2.5]])
        if coordinates is None:
            self.draw('normal', (self.destination, self.width, self.height), [self.scene, self.box], [(0, 0)] * 2, color)
        else:
            self.draw_wave(coordinates)
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

    def draw_wave(self, coordinates):
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, None)
        d.call(self.device, 37, 'up', 0, self.destination)
        d.call(self.device, 107, 'p', self.shaders['wave'])
        d.call(self.device, 92, 'p', None)
        d.call(self.device, 89, 'u', 0x344)
        for index, texture in enumerate([self.wave, self.scene[0], self.box[0]]):
            d.call(self.device, 65, 'up', index, texture)
        # 681BE0's generated texture wraps; scene and blur remain clamped.
        for state in [1, 2]:
            d.call(self.device, 69, 'uuu', 0, state, 1)
        for index, values in coordinates['constants'].items():
            d.call(self.device, 109, 'upu', index, d.floats(values), 1)
        vertices = bytearray()
        for index in [0, 1, 2, 0, 2, 3]:
            native_vertex = coordinates['vertices'][index]
            x, y, z = struct.unpack_from('<3f', native_vertex)
            # Native orthographic bottom-left positions become D3D screen pixels.
            vertices += struct.pack('<4f', x, self.height - y, z, 1.) + native_vertex[12:]
        d.call(self.device, 43, 'upuufu', 0, None, 1, 0, 1., 0)
        d.call(self.device, 41)
        d.call(self.device, 83, 'uupu', 4, 2, d.buffer(bytes(vertices)), 44)
        d.call(self.device, 42)


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
    producer, wave_producer = native.NormalProducer(), native.WaveProducer()
    wave = native.wave_texture()
    shaders = {kind: d.shader_variants(args.shaders / f'SHADERS_PIXEL_PS_2_0_FFX{name}.BLS')[0]
               for kind, name in [('box', 'BOX4'), ('gauss', 'GAUSS4'), ('normal', 'GLOW'), ('wave', 'GLOWWAVE')]}
    settings = [(glow, 0, 0, 0, 0) for glow in [0., .2, .5, 1., 1.25, .5 / 255., 1.5 / 255.]]
    settings += [(.5, actual, fake, 0, 0) for actual, fake in [(1, 0), (25, 50), (99, 0), (0, 100)]]
    settings += [(.5, actual, 0, 1, time) for actual in [0, 99]
                 for time in [0, 1, 1402, 2804, 2805, 3174, 0xffffffff]]
    records = []
    for source in inputs:
        width, height = map(int, source.stem.split('-')[-2:])
        rgba = source.read_bytes()
        assert len(rgba) == width * height * 4
        renderer = Chain(width, height, shaders, wave)
        try:
            renderer.upload(rgba)
            for glow, actual, fake, wet, time in settings:
                selector, color = producer.sample(glow, actual, fake, wet)
                assert selector == wet
                coordinates = wave_producer.sample(width, height, time, color) if selector else None
                result = renderer.compose(color, coordinates)
                records.append(struct.pack('<2If5I', width, height, glow, actual, fake, wet, time, color) + rgba + result)
        finally:
            renderer.close()
    args.output.write_bytes(struct.pack('<4sI', b'WGLW', len(records)) + b''.join(records))
    provenance = {
        'executable_sha256': hashlib.sha256(n.data).hexdigest(),
        'pixel_shader_sha256': {name: hashlib.sha256(code).hexdigest() for name, code in shaders.items()},
        'input_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},
        'wave_sha256': hashlib.sha256(wave).hexdigest(),
        'fixture_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(),
        'native_frames': len(records),
        'texture_policy': 'normalized NPOT; linear/clamp scene and blur; linear/wrap V8U8 wave; no sRGB',
    }
    args.output.with_suffix('.json').write_text(json.dumps(provenance, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'native_world_glow_frames': len(records)}))
