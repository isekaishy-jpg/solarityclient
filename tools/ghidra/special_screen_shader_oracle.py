"""Execute native special-screen seed geometry and original archive shaders.

The producer supplies exact noise bytes, seed quads, polar mesh and constants.
Direct3D 9 runs the fixed-function seed (material diffuse mapped to equivalent
pretransformed diffuse) and unchanged propagation/composition pixel shaders.
Both native same-texture feedback and a separate destination are measured.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
from pathlib import Path

import ghost_screen_shader_oracle as g
import liquid_shader_oracle as d
import special_screen_oracle as native
import wmo_registration_oracle as n


class Chain(g.Chain):
    def __init__(self, width, height, shaders, pixels):
        super().__init__(width, height, shaders)
        self.history = self.texture_target(256, 128)
        self.next_history = self.texture_target(256, 128)
        self.history_cpu = self.create(self.device, 36, 'uuuup', 256, 128, 21, 2, None, output_before_last=True)
        self.noise = self.create(self.device, 23, 'uuuuuup', 256, 256, 1, 0, 21, 1, None, output_before_last=True)
        locked = d.LockedRect()
        d.call(self.noise, 19, 'uppu', 0, c.byref(locked), None, 0)
        for y in range(256):
            c.memmove(locked.bits + y * locked.pitch, pixels[y * 1024:(y + 1) * 1024], 1024)
        d.call(self.noise, 20, 'u', 0)

    def prepare(self, target, shader, sources, fvf):
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, None)
        d.call(self.device, 37, 'up', 0, target)
        d.call(self.device, 107, 'p', shader)
        d.call(self.device, 92, 'p', None)
        d.call(self.device, 89, 'u', fvf)
        for slot, source in enumerate(sources):
            d.call(self.device, 65, 'up', slot, source)

    def triangles(self, vertices, stride):
        d.call(self.device, 41)
        d.call(self.device, 83, 'uupu', 4, len(vertices) // (stride * 3), d.buffer(vertices), stride)
        d.call(self.device, 42)

    def seed(self, quads, color, clear):
        # 6A43D0 uses material diffuse with white ambient when no diffuse exists.
        # XYZRHW diffuse is the equivalent fixed-function pixel input.
        for index, quad in enumerate(quads):
            textured = index == 0
            self.prepare(self.history[1], None, [self.noise] if textured else [], 0x144 if textured else 0x44)
            for state, value in [(1, 4 if textured else 2), (2, 2 if textured else 0), (3, 0),
                                 (4, 4 if textured else 2), (5, 2 if textured else 0), (6, 0)]:
                d.call(self.device, 67, 'uuu', 0, state, value)
            d.call(self.device, 67, 'uuu', 1, 1, 1)
            d.call(self.device, 67, 'uuu', 1, 4, 1)
            if clear and index == 0:
                d.call(self.device, 43, 'upuufu', 0, None, 1, 0, 1., 0)
            stride = 20 if textured else 12
            vertices = bytearray()
            for vertex in [0, 1, 2, 0, 2, 3]:
                x, y, z = struct.unpack_from('<3f', quad, vertex * stride)
                vertices += struct.pack('<4fI', x, 128 - y, z, 1., color)
                if textured:
                    vertices += quad[vertex * stride + 12:vertex * stride + 20]
            self.triangles(bytes(vertices), 28 if textured else 20)

    def propagate(self, decay, feedback):
        target = self.history if feedback else self.next_history
        self.prepare(target[1], self.shaders['propagate'], [self.history[0]] * 4, 0x444)
        d.call(self.device, 109, 'upu', 0, d.floats([decay] * 4), 1)
        vertices = bytearray()
        for x, y in [(0, 0), (0, 128), (256, 0), (256, 0), (0, 128), (256, 128)]:
            vertices += struct.pack('<4fI', x, y, 0., 1., 0xffffffff)
            for dx, dy in [(-1, 1), (0, 1), (1, 1), (0, 2)]:
                vertices += struct.pack('<2f', x / 256 + (.5 + dx) / 256,
                                        y / 128 + (.5 + dy) / 128)
        self.triangles(bytes(vertices), 52)
        if not feedback:
            self.history, self.next_history = self.next_history, self.history

    def compose(self, mesh, indices, constants):
        self.prepare(self.destination, self.shaders['combine'], [self.history[0], self.scene[0]], 0x244)
        d.call(self.device, 109, 'upu', 0, d.buffer(struct.pack('<8I', *constants)), 2)
        vertices = bytearray()
        for index in struct.unpack('<6144H', indices):
            x, y, z, color, u, v, s, t = struct.unpack_from('<3fI4f', mesh, index * 32)
            vertices += struct.pack('<4fI4f', x, self.height - y, z, 1., color, u, v, s, t)
        self.triangles(bytes(vertices), 36)
        return self.read_pixels(self.destination, self.cpu, self.width, self.height)

    def read_pixels(self, source, destination, width, height):
        for slot in range(4):
            d.call(self.device, 65, 'up', slot, None)
        d.call(self.device, 32, 'pp', source, destination)
        locked = d.LockedRect()
        d.call(destination, 13, 'ppu', c.byref(locked), None, 0x10)
        rgba = bytearray()
        for y in range(height):
            row = bytearray(c.string_at(locked.bits + y * locked.pitch, width * 4))
            row[0::4], row[2::4] = row[2::4], row[0::4]
            rgba += row
        d.call(destination, 14)
        return bytes(rgba)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('shaders', type=Path)
    parser.add_argument('inputs', type=Path)
    parser.add_argument('native', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    shaders = {kind: d.shader_variants(args.shaders / f'SHADERS_PIXEL_PS_2_0_FFX{name}.BLS')[0]
               for kind, name in [('propagate', 'PROPAGATEFOG'), ('combine', 'FOGCOMBINE')]}
    noise = (args.native / 'special_noise_native.rgba').read_bytes()
    records, comparisons = [], []
    for source in sorted(args.inputs.glob('*.rgba')):
        width, height = map(int, source.stem.split('-')[-2:])
        rgba = source.read_bytes()
        producer = native.Producer()
        mesh, indices = producer.mesh(width, height)
        direct, separate = [Chain(width, height, shaders, noise) for _ in range(2)]
        try:
            direct.upload(rgba)
            separate.upload(rgba)
            for frame in range(271):
                reset = frame in (0, 135, 260)
                if reset:
                    parameters = [(0xffffffff, 6, 40), (0x85130f1c, 1, 60), (0xff000000, 1, 100)][(0, 135, 260).index(frame)]
                    color, decay_word, _, _ = producer.select(parameters)
                row, quads = producer.seed_frame()
                state = producer.advance(1/60)
                decay = struct.unpack('<f', struct.pack('<I', decay_word))[0]
                for renderer, feedback in [(direct, True), (separate, False)]:
                    renderer.seed(quads, color, reset or frame == 270)
                    renderer.propagate(decay, feedback)
                a = direct.read_pixels(direct.history[1], direct.history_cpu, 256, 128)
                b = separate.read_pixels(separate.history[1], separate.history_cpu, 256, 128)
                comparisons.append(max(abs(x - y) for x, y in zip(a, b)))
                if frame in (0, 1, 2, 30, 90, 134, 135, 136, 200, 255, 256, 257, 259, 260, 269, 270):
                    result = separate.compose(mesh, indices, state[1:])
                    records.append(struct.pack('<6I', width, height, frame, color, row, int(reset)) + struct.pack('<9I', decay_word, *state[1:]) + rgba + b + result)
        finally:
            direct.close()
            separate.close()
    args.output.write_bytes(struct.pack('<4sI', b'SPGP', len(records)) + b''.join(records))
    metadata = {'executable_sha256': hashlib.sha256(n.data).hexdigest(),
                'shader_sha256': {name: hashlib.sha256(code).hexdigest() for name, code in shaders.items()},
                'fixture_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(),
                'frames': len(records), 'history_comparisons': len(comparisons),
                'feedback_max_difference': max(comparisons),
                'feedback_differing_frames': sum(value != 0 for value in comparisons)}
    args.output.with_suffix('.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()
