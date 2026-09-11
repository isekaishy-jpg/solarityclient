"""Capture native terrain constant production and perspective lighting.

Executes 7CFBE0 with its original matrix and exterior-light accumulation code,
then renders unchanged Terrain VS8/Terrain1 PS0. Only device projection-sign
and fog-enabled queries are intercepted. No client entry point runs.
"""
import argparse
import math
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
import liquid_shader_oracle as gpu


def normalize(values):
    length = math.sqrt(sum(x * x for x in values))
    return [x / length for x in values]


def cross(a, b):
    return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]


def constants(origin, height, direction):
    u = n.emulator()
    device, world, translation, rotation = [n.HEAP+x for x in (0, 0x3000, 0x4000, 0x4100)]
    eye = [origin[0]-38., origin[1]-16., origin[2]+height]
    backward = normalize([-22., 0., height])
    right = normalize(cross([0., 0., 1.], backward))
    up = cross(backward, right)
    view_rotation = [value for axis in range(3) for value in (right[axis], up[axis], backward[axis], 0.)] + [0.,0.,0.,1.]
    translated = gpu.IDENTITY[:]
    translated[12:15] = [-v for v in eye]
    near, far = .1, 100.
    focal = 1. / math.tan(math.pi / 6.)
    projection = [focal,0.,0.,0., 0.,focal,0.,0., 0.,0.,far/(near-far),-1., 0.,0.,near*far/(near-far),0.]
    native_projection = projection[:]
    # Half-pixel raster correction is proportional to clip W, which is -view Z.
    native_projection[8], native_projection[9] = 1./64., -1./64.
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, 0xce04a8, world)
    n.write_floats(u, translation, translated)
    n.write_floats(u, rotation, view_rotation)
    n.write_floats(u, device+0xfc8, native_projection)
    sun = world+0x58
    n.write_words(u, sun+0x60, 1)
    n.write_floats(u, sun+0x24, [-v for v in direction])
    n.write_floats(u, sun+0x30, [.2,.3,.4, .3,.2,.1, .65,.35,.15])
    u.hook_add(UC_HOOK_CODE, lambda uc, address, size, context:
               return_value(uc, int(address == 0x682d20)) if address in (0x682d20,0x683100) else None)
    n.invoke(u, 0x7cfbe0, [translation, rotation])
    return n.read_floats(u, 0xd250a0, 46*4), projection


def capture(directory):
    gpu.EXTENT = 64
    vertex = gpu.shader_variants(directory/'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')[8]
    pixel = gpu.shader_variants(directory/'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    unit = f32(f32(1600./3.)/128.)
    reciprocal = f32(1./127.)
    rows = ['# perspective: origin(3), normal signed bytes(3), direction(3), view(16), projection(16), native RGBA(64x64)']
    renderer = gpu.Renderer()
    try:
        for origin in [(0.,0.,0.),(-600.,-4233.33349609375,38.)]:
            for height in [2., 7.]:
                direction = normalize([-.6,0.,.8])
                native, projection = constants(origin, height, direction)
                for normal in [(0,0,127),(-76,0,102),(-102,0,102)]:
                    points = []
                    for row in range(17):
                        for column in range(8 if row%2 else 9):
                            points.append((f32(origin[0]-f32(row*.5*unit)),
                                           f32(origin[1]-f32((column+(row%2)*.5)*unit)),origin[2]))
                    indices = []
                    for row in range(8):
                        for column in range(8):
                            tl = row*17+column
                            center,tr,bl,br = tl+9,tl+1,tl+17,tl+18
                            indices += [tl,center,tr,tr,center,br,br,center,bl,bl,center,tl]
                    vertices = b''.join(struct.pack('<6fI4f',*points[i],*[f32(v*reciprocal) for v in normal],0,0.,0.,0.,0.) for i in indices)
                    textures = [struct.pack('<4f',51/255.,102/255.,153/255.,1.)*16,
                                struct.pack('<4f',0.,0.,0.,1.)*16]
                    frame = renderer.render(vertex,pixel,native,vertices,textures,0,None)
                    rows.append('perspective '+' '.join(map(str,[*origin,*normal,*direction,*native[:16],*projection]))+' '+frame.hex())
    finally:
        renderer.close()
    return '\n'.join(rows)+'\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('directory',type=Path)
    parser.add_argument('output',type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(args.directory))
    print('Captured 12 original terrain perspective frames with native light constants')
