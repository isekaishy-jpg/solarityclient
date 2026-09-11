"""Render exact stock LiquidWater/NoSpec/Magma BLS through Windows Direct3D 9.

Uses an offscreen render target and never presents, creates a window, or runs
the client. Input shaders must be extracted from the locally owned archives.
The fixture retains Vulkan-facing uniforms, vertices, textures, and native RGBA
pixels. Only the projection's half-pixel raster offset changes. Stock's Y
orientation matches the renderer's negative-height Vulkan viewport.
"""
import argparse
import ctypes as c
import hashlib
import json
import struct
from pathlib import Path

U, P, F = c.c_uint32, c.c_void_p, c.c_float
TYPES = {'u': U, 'p': P, 'f': F}
EXTENT = 16
SHADER_HASHES = {
    'vsLiquidWater.bls': '7faefed32b2779574380437de0484593304516804238466eb9073fc3997b38be',
    'psLiquidWater.bls': '032f27256a57de3ebd2dc488ab12fab0e5dcdc4f6d1ae5f74c2b62216f084fba',
    'vsLiquidWaterNoSpec.bls': 'e4b469a102728795e76efd88156cfa5ce349ac2ba8255ce3f1e78174c84b36c4',
    'psLiquidWaterNoSpec.bls': '84739872d7f4a6974f7552bf791e6370f1a8836fb5c5aaee18f0ee5d0c3ab4f5',
    'vsLiquidMagma.bls': 'd5b408bc6e06a0894d24f581ce64c1d2aa5480e605f90e6d4c9eb980e55e017e',
    'psLiquidMagma.bls': '7dc4a7038dd98aae2c49ac8f70bea99f0ba3fb6e7aa5ff1581d5ec1fca7c6c07',
}
IDENTITY = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]


class PresentParameters(c.Structure):
    _fields_ = [(name, U) for name in ('width', 'height', 'format', 'count', 'samples', 'quality', 'swap')]
    _fields_ += [('window', P)]
    _fields_ += [(name, U) for name in ('windowed', 'depth', 'depth_format', 'flags', 'refresh', 'interval')]


class LockedRect(c.Structure):
    _fields_ = [('pitch', c.c_int32), ('bits', P)]


def call(owner, index, signature='', *args):
    """Invoke the ABI declared by Microsoft's shared/d3d9.h COM interfaces."""
    table = c.cast(owner, c.POINTER(c.POINTER(P))).contents
    function = c.WINFUNCTYPE(c.c_int32, P, *[TYPES[kind] for kind in signature])(table[index])
    result = function(owner, *args)
    if result < 0:
        raise RuntimeError(f'Direct3D method {index} failed: {result & 0xffffffff:08x}')
    return result


def buffer(data):
    return c.create_string_buffer(data)


def floats(values):
    return (F * len(values))(*values)


def shader_variants(path):
    data = path.read_bytes()
    assert data[:8] == struct.pack('<II', 0x47585348, 0x10003)
    count = struct.unpack_from('<I', data, 8)[0]
    result, offset = [], 12
    for _ in range(count):
        size = struct.unpack_from('<I', data, offset + 12)[0]
        offset += 16
        result.append(data[offset:offset + size])
        offset += size
    assert offset == len(data)
    return result


class Renderer:
    def __init__(self):
        self.objects = []
        library = c.WinDLL('d3d9')
        library.Direct3DCreate9.argtypes, library.Direct3DCreate9.restype = [U], P
        self.root = library.Direct3DCreate9(32)
        if not self.root:
            raise RuntimeError('Direct3DCreate9 returned null')
        self.objects.append(self.root)
        user = c.WinDLL('user32')
        user.GetDesktopWindow.restype = P
        window = user.GetDesktopWindow()
        parameters = PresentParameters(width=EXTENT, height=EXTENT, format=21, count=1,
                                       swap=1, window=window, windowed=1, interval=0x80000000)
        self.device = self.create(self.root, 16, 'uupup', 0, 1, window, 0x20, c.byref(parameters))
        self.target = self.create(self.device, 28, 'uuuuuup', EXTENT, EXTENT, 21, 0, 0, 0, None, output_before_last=True)
        self.readback = self.create(self.device, 36, 'uuuup', EXTENT, EXTENT, 21, 2, None, output_before_last=True)
        call(self.device, 37, 'up', 0, self.target)
        for state, value in ((7, 0), (15, 0), (22, 1), (27, 0), (137, 0), (194, 0), (35, 0), (140, 0)):
            call(self.device, 57, 'uu', state, value)
        for sampler in range(2):
            for state, value in ((1, 3), (2, 3), (5, 1), (6, 1), (7, 0), (11, 0)):
                call(self.device, 69, 'uuu', sampler, state, value)

    def create(self, owner, method, signature, *args, output_before_last=False):
        output = P()
        if output_before_last:
            call(owner, method, signature[:-1] + 'p' + signature[-1], *args[:-1], c.byref(output), args[-1])
        else:
            call(owner, method, signature + 'p', *args, c.byref(output))
        if not output.value:
            raise RuntimeError(f'Direct3D creation {method} returned null')
        self.objects.append(output)
        return output

    def texture(self, pixels):
        texture = self.create(self.device, 23, 'uuuuuup', 4, 4, 1, 0, 116, 1, None, output_before_last=True)
        locked = LockedRect()
        call(texture, 19, 'uppu', 0, c.byref(locked), None, 0)
        for row in range(4):
            c.memmove(locked.bits + row * locked.pitch, pixels[row * 64:(row + 1) * 64], 64)
        call(texture, 20, 'u', 0)
        return texture

    def render(self, vertex_shader, pixel_shader, constants, vertices, textures, kind, fog_color):
        vertex = self.create(self.device, 91, 'p', buffer(vertex_shader))
        pixel = self.create(self.device, 106, 'p', buffer(pixel_shader))
        elements = [(0, 0, 2, 0, 0, 0), (0, 12, 2, 0, 3, 0), (0, 24, 4, 0, 10, 0),
                    (0, 36, 1, 0, 5, 0), (0, 28, 1, 0, 5, 1), (255, 0, 17, 0, 0, 0)]
        declaration = self.create(self.device, 86, 'p', buffer(b''.join(struct.pack('<HHBBBB', *e) for e in elements)))
        call(self.device, 87, 'p', declaration)
        call(self.device, 92, 'p', vertex)
        call(self.device, 107, 'p', pixel)
        call(self.device, 94, 'upu', 0, floats(constants), 46)
        for slot, color in enumerate(textures if kind != 2 else [textures[1]]):
            call(self.device, 65, 'up', slot, self.texture(color))
        call(self.device, 57, 'uu', 28, int(fog_color is not None))
        if fog_color is not None:
            call(self.device, 57, 'uu', 34, fog_color)
        call(self.device, 43, 'upuufu', 0, None, 1, 0, 1., 0)
        call(self.device, 41)
        if len(vertices) % (3 * 44):
            raise ValueError('Triangle-list vertices must contain complete 44-byte triples')
        call(self.device, 83, 'uupu', 4, len(vertices) // (3 * 44), buffer(vertices), 44)
        call(self.device, 42)
        call(self.device, 32, 'pp', self.target, self.readback)
        locked = LockedRect()
        call(self.readback, 13, 'ppu', c.byref(locked), None, 0x10)
        result = bytearray()
        for row in range(EXTENT):
            pixels = c.string_at(locked.bits + row * locked.pitch, EXTENT * 4)
            for offset in range(0, len(pixels), 4):
                b, g, r, a = pixels[offset:offset + 4]
                result.extend((r, g, b, a))
        call(self.readback, 14)
        return bytes(result)

    def close(self):
        for owner in reversed(self.objects):
            call(owner, 2)


def cases():
    """Supply explicit legal shader inputs, including overbright color registers."""
    for kind in range(3):
        for points in range(1 if kind == 2 else 4):
            for palette in range(4):
                projection, model_view, surface, depth = [IDENTITY.copy() for _ in range(4)]
                if palette == 3:
                    model_view[0], model_view[5], model_view[10], model_view[14] = 1.25, .8, 1.2, .1
                    projection[0], projection[5], projection[10], projection[14] = .8, 1.25, 1. / 1.2, -.1 / 1.2
                    surface[0], surface[1], surface[4], surface[5], surface[12], surface[13] = .7, .2, -.3, .6, .25, .1
                    depth[0], depth[5], depth[12], depth[13] = .6, .8, .05, .1
                matrices = projection + model_view + surface + depth
                fog = [0., 1., 1., 0.] if palette != 2 else [-.5, .8, 1.5, 0.]
                direction = [.3, 0., -.9539392, 1.]
                ambient = [[.1, .2, .3, 1.], [3., -.5, 2., 1.], [.3, .2, .1, 1.], [.2, .15, .1, 1.]][palette]
                diffuse, specular = [.2, .3, .4, 1.], [.15, .25, .35, 6.]
                fog_color = [8 / 255., 16 / 255., 24 / 255., 1.]
                lights = []
                for index in range(3):
                    lights += [1. + index, -2. + index, 4., 1., .15, .25, .05, 1., 1., .7, .03, 0.]
                uniform = struct.pack('<124f4I', *matrices, *fog, *direction, *ambient, *diffuse, *specular, *fog_color, *lights, points, 0, 0, 0)
                constants = [0.] * (46 * 4)
                native_projection = projection.copy()
                native_projection[12], native_projection[13] = -1. / EXTENT, 1. / EXTENT
                for first, values in ((0, native_projection), (4, fog), (5, model_view), (9, surface), (13, depth),
                                      (33, direction), (34, ambient), (35, diffuse), (36, specular), (37, lights)):
                    constants[first * 4:first * 4 + len(values)] = values
                vertices = b''.join(struct.pack('<6fI4f', x, y, .5, 0., 0., 1., 0xc080c0ff, (x + 1.) / 4. + .031, (y + 1.) / 4. + .023, (x + 1.) / 4. + .019, (y + 1.) / 4. + .041)
                                    for x, y in ((-1., -1.), (3., -1.), (-1., 3.)))
                textures = []
                for slot, color in enumerate([[.12, .24, .48, .4], [.02, .04, .08, .25]]):
                    pixels = []
                    for y in range(4):
                        for x in range(4):
                            factor = (1. + x * .25 + y * .15) if palette == 3 else 1.
                            pixels.extend(value * factor for value in color)
                    textures.append(struct.pack('<64f', *pixels))
                yield kind, points, palette, uniform, constants, vertices, textures, 0x081018 if palette == 2 else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('shader_directory')
    parser.add_argument('output')
    args = parser.parse_args()
    shaders, hashes = [], {}
    for name in ('LiquidWater', 'LiquidWaterNoSpec', 'LiquidMagma'):
        pair = []
        for stage in ('vs', 'ps'):
            path = Path(args.shader_directory) / f'{stage}{name}.bls'
            hashes[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
            if hashes[path.name] != SHADER_HASHES[path.name]:
                raise ValueError(f'Shader fingerprint mismatch: {path.name}')
            pair.append(shader_variants(path))
        shaders.append(pair)
    renderer, records = Renderer(), []
    try:
        for kind, points, palette, uniform, constants, vertices, textures, fog in cases():
            image = renderer.render(shaders[kind][0][points], shaders[kind][1][0], constants, vertices, textures, kind, fog)
            records.append(struct.pack('<4I', kind, points, palette, EXTENT) + uniform + vertices
                           + b''.join(textures) + image)
    finally:
        renderer.close()
    Path(args.output).write_bytes(b''.join(records))
    print(json.dumps({'native_shader_frames': len(records), 'shader_sha256': hashes}))


if __name__ == '__main__':
    main()
