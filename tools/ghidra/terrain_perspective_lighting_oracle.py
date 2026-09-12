"""Capture native terrain constant production and perspective lighting.

Executes 7CFBE0 with its original matrix and exterior-light accumulation code,
then renders unchanged Terrain VS8/Terrain1 PS0. The default capture intercepts
only device projection-sign and fog-enabled queries. --world-palettes also
supplies decoded WDBC rows to native band lookup before running the ordinary
palette-to-sun upload; see world_palette for its exact boundary. No client
entry point runs.
"""
import argparse
import math
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
import liquid_shader_oracle as gpu
import world_light_sampling_oracle as sampling


def normalize(values):
    length = math.sqrt(sum(x * x for x in values))
    return [x / length for x in values]


def cross(a, b):
    return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]


def world_palette(u, identifier, time):
    """Sample native bands, derive the palette and execute the ordinary sun upload.

    The 7816F0 suffix starts after fog/environment queries with a zero blackout
    fraction. Every instruction from 7817D8 through 7819AA is original, including
    packed color scaling, RGB conversion and 834AE0 direction normalization.
    This deliberately excludes WMO/weather selection and blackout composition.
    """
    parameter, colors, floats = sampling.tables(identifier)
    address = n.HEAP + 0x7000
    u.mem_write(address, parameter)
    def band_provider(u, pc, size, context):
        if pc != 0x7eb210:
            return
        sp = u.reg_read(UC_X86_REG_ESP)
        band, destination = n.read_words(u, sp + 4, 2)
        color = u.reg_read(UC_X86_REG_ECX) == 0xaf49bc
        owner, channel = divmod(band - 1, 18 if color else 6)
        assert owner + 1 == identifier, (identifier, band, color)
        u.mem_write(destination, (colors if color else floats)[channel])
        return_value(u, 1)
        u.reg_write(UC_X86_REG_ESP, sp + 12)
    handle = u.hook_add(UC_HOOK_CODE, band_provider)
    u.reg_write(UC_X86_REG_ECX, 0xd38bd4)
    n.invoke(u, 0x7ee360, [])
    u.reg_write(UC_X86_REG_EAX, address)
    u.reg_write(UC_X86_REG_ESI, 0xd38bd4)
    u.reg_write(UC_X86_REG_ECX, time)
    n.invoke(u, 0x7ecd80, [])
    u.hook_del(handle)
    n.invoke(u, 0x7ee750, [])
    n.write_floats(u, 0xd38b04, [time / 2880.])
    n.invoke(u, 0x7eea90, [])
    bp = n.STACK + 0x17000
    u.reg_write(UC_X86_REG_EBP, bp)
    u.reg_write(UC_X86_REG_ESP, bp - 0x20)
    u.reg_write(UC_X86_REG_ESI, 0xd38b00)
    n.write_floats(u, bp - 4, [0.])
    u.emu_start(0x7817d8, 0x7819ad, timeout=1_000_000, count=100_000)
    assert u.reg_read(UC_X86_REG_EIP) == 0x7819ad


def constants(origin, height, direction, palette=None):
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
    if palette is not None:
        world_palette(u, *palette)
    u.hook_add(UC_HOOK_CODE, lambda uc, address, size, context:
               return_value(uc, int(address == 0x682d20)) if address in (0x682d20,0x683100) else None)
    n.invoke(u, 0x7cfbe0, [translation, rotation])
    return n.read_floats(u, 0xd250a0, 46*4), projection


def capture(directory, world=False):
    gpu.EXTENT = 64
    vertex = gpu.shader_variants(directory/'SHADERS_VERTEX_VS_2_0_TERRAIN.BLS')[8]
    pixel = gpu.shader_variants(directory/'SHADERS_PIXEL_PS_2_0_TERRAIN1.BLS')[0]
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    unit = f32(f32(1600./3.)/128.)
    reciprocal = f32(1./127.)
    rows = ['# perspective: origin(3), normal signed bytes(3), direction(3), view(16), projection(16), native RGBA(64x64)']
    if world:
        rows = ['# world: parameter, half_minutes, then perspective fields; direction is diagnostic, not a test input.']
        for identifier in (1, 2):
            parameter, colors, floats = sampling.tables(identifier)
            rows.append(f'parameter {identifier} {parameter.hex()} {b"".join(colors).hex()} {b"".join(floats).hex()}')
    renderer = gpu.Renderer()
    try:
        for origin in [(0.,0.,0.),(-600.,-4233.33349609375,38.)]:
            cases = [(2., (identifier, time)) for identifier in (1, 2)
                     for time in (0, 240, 720, 1440, 2160, 2879)] if world else [(2., None), (7., None)]
            for height, palette in cases:
                direction = normalize([-.6,0.,.8])
                native, projection = constants(origin, height, direction, palette)
                if palette is not None:
                    # The fixture retains the native view-space ray for diagnosis.
                    direction = native[24*4:24*4+3]
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
                    prefix = f'world {palette[0]} {palette[1]} ' if palette else 'perspective '
                    rows.append(prefix+' '.join(map(str,[*origin,*normal,*direction,*native[:16],*projection]))+' '+frame.hex())
    finally:
        renderer.close()
    return '\n'.join(rows)+'\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('directory',type=Path)
    parser.add_argument('output',type=Path)
    parser.add_argument('--world-palettes', action='store_true')
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture(args.directory, args.world_palettes))
    print(f'Captured {72 if args.world_palettes else 12} original terrain perspective frames with native light constants')
