"""Execute original Terrain.bls vertex transforms through D3D9 ProcessVertices.

The shader is unchanged. Only its input vertices, view/projection registers,
and resource plumbing are supplied by this harness; no client entry point runs.
The output retains input float bits and native post-viewport depth/reciprocal W.
Cases cover local, Durotar, and map-edge coordinates during camera rotation.
"""
import argparse
import ctypes as c
import hashlib
import itertools
import math
import struct
from pathlib import Path

from liquid_shader_oracle import Renderer, buffer, call, floats, shader_variants


def scalar(value):
    """Round an input to the float ABI consumed by Direct3D."""
    return struct.unpack('<f', struct.pack('<f', value))[0]


def normalized(vector):
    """Construct a finite unit basis for the controlled camera inputs."""
    length = math.sqrt(sum(v*v for v in vector))
    return [scalar(v/length) for v in vector]


def cross(a, b):
    """Compose the explicit right-handed camera basis."""
    return [scalar(a[1]*b[2]-a[2]*b[1]), scalar(a[2]*b[0]-a[0]*b[2]), scalar(a[0]*b[1]-a[1]*b[0])]


def inputs():
    """Provide legal projection inputs without depending on live scene state."""
    for index, (point, angle, distance) in enumerate(itertools.product(
            [(0., 0., 0.), (1340., -4380., 28.), (16000., -16000., 200.)],
            [0., .17, .43, .91, 1.31, 2.16, 3.3, 4.7], [8., 30., 100.])):
        forward = normalized([math.cos(angle), math.sin(angle), -.15])
        right = normalized(cross(forward, [0., 0., 1.]))
        up = cross(right, forward)
        eye = [scalar(p-f*distance) for p, f in zip(point, forward)]
        rows = [right, up, [-v for v in forward]]
        translation = [scalar(-sum(a*b for a, b in zip(row, eye))) for row in rows]
        view = [rows[row][column] if row < 3 else 0. for column in range(3) for row in range(4)]
        view += translation + [1.]
        cot = 1/math.tan(.9424778/2)
        projection = [scalar(cot/(16/9)), 0., 0., 0., 0., scalar(cot), 0., 0.,
                      0., 0., scalar(1000/(.2-1000)), -1., 0., 0., scalar(200/(.2-1000)), 0.]
        yield index, point, projection, view


class TransformProbe:
    """Own the stock shader and software-vertex-processing input/output buffers."""
    def __init__(self, shader):
        self.renderer = Renderer()
        r = self.renderer
        vertex = r.create(r.device, 91, 'p', buffer(shader))
        elements = [(0, 0, 2, 0, 0, 0), (0, 12, 2, 0, 3, 0), (255, 0, 17, 0, 0, 0)]
        declaration = r.create(r.device, 86, 'p', buffer(b''.join(struct.pack('<HHBBBB', *e) for e in elements)))
        self.source = r.create(r.device, 26, 'uuuup', 24, 0, 0, 2, None, output_before_last=True)
        # D3DFVF_XYZRHW | DIFFUSE | SPECULAR | TEX2 has a 40-byte stride.
        self.destination = r.create(r.device, 26, 'uuuup', 40, 0, 0x2c4, 2, None, output_before_last=True)
        call(r.device, 87, 'p', declaration)
        call(r.device, 92, 'p', vertex)
        call(r.device, 100, 'upuu', 0, self.source, 0, 24)

    def transform(self, point, projection, view):
        """Capture the original shader's post-viewport depth and reciprocal W."""
        constants = [0.] * 184
        constants[:16], constants[16:32] = view, projection
        constants[48:52] = [0., 1., 1., 0.]
        pointer = c.c_void_p()
        call(self.source, 11, 'uupu', 0, 24, c.byref(pointer), 0)
        c.memmove(pointer, struct.pack('<6f', *point, 0., 0., 1.), 24)
        call(self.source, 12)
        call(self.renderer.device, 94, 'upu', 0, floats(constants), 46)
        call(self.renderer.device, 85, 'uuuppu', 0, 0, 1, self.destination, None, 0)
        call(self.destination, 11, 'uupu', 0, 16, c.byref(pointer), 0x10)
        result = struct.unpack('<4f', c.string_at(pointer, 16))
        call(self.destination, 12)
        return result[2:]

    def close(self):
        """Release every Direct3D object in reverse ownership order."""
        self.renderer.close()


def main():
    """Regenerate the portable numeric regression fixture from owned stock data."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('terrain_shader', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    digest = hashlib.sha256(args.terrain_shader.read_bytes()).hexdigest()
    if digest != '3bedcdfa3f183363739338cbac8abd698210cc253b10a3330f1c5778e517186c':
        raise ValueError('terrain shader differs from the pinned 12340 input')
    rows = [f'# Terrain.bls VS_2_0 SHA256 {digest}', '# case point_xyz projection_columns view_columns depth reciprocal_w; float words are hexadecimal']
    probe = TransformProbe(shader_variants(args.terrain_shader)[0])
    try:
        for index, point, projection, view in inputs():
            native = probe.transform(point, projection, view)
            words = [f'{struct.unpack("<I", struct.pack("<f", v))[0]:08x}' for v in [*point, *projection, *view, *native]]
            rows.append(f'case-{index:02} ' + ' '.join(words))
    finally:
        probe.close()
    args.output.write_text('\n'.join(rows)+'\n')
    print(f'captured {len(rows)-2} original shader transforms')


if __name__ == '__main__':
    main()
