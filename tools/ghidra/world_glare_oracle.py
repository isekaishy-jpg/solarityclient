"""Capture 7EF6E0 glare fades with supplied cloud/water/visibility boundaries.

The original constructors, day curve, smoothing, angular size and alpha packing
execute in the fingerprinted PE. Texture creation and the three external
attenuation providers are supplied; this does not certify GPU occlusion.
"""
import argparse
import math
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_EBP, UC_X86_REG_ESI
import wmo_registration_oracle as n


def sequence(kind):
    """Run a retained native owner through time, angle and occlusion transitions."""
    u = n.emulator()
    owner = 0xd38ea8 if kind == 0 else 0xd38f58
    boundary = {'cloud': 1., 'water': 1., 'visible': 1.}
    stub, value = n.STOP + 0x100, n.HEAP
    u.mem_write(stub, b'\xdd\x05' + struct.pack('<I', value) + b'\xc3')

    def hook(uc, address, size, data):
        if address == 0x9ad000:
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = n.read_words(uc, sp, 1)[0]
            uc.reg_write(UC_X86_REG_ESP, sp + 8)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        key = {0x7f1020: 'cloud', 0x7f1040: 'cloud', 0x7eda30: 'water', 0x9ac3c0: 'visible'}.get(address)
        if key:
            uc.mem_write(value, struct.pack('<d', boundary[key]))
            uc.reg_write(UC_X86_REG_EIP, stub)

    u.hook_add(UC_HOOK_CODE, hook)
    n.invoke(u, 0x9d07d0 if kind == 0 else 0x9d07f0, [])
    n.invoke(u, 0x7ee150 if kind == 0 else 0x7ee230, [])
    rows = []
    for index in range(240):
        day = .5 if kind == 0 else 0.
        if index >= 180:
            day = (index - 180) / 59.
        elapsed = [0., 1./60., .1, .4][index % 4]
        angle = [0., .2, .6, 1., 2.][(index // 12) % 5]
        ray = [8., 2., 8.]
        ray_length = math.sqrt(sum(v*v for v in ray))
        direction = [v / ray_length for v in ray]
        forward = [direction[0]*math.cos(angle)-direction[1]*math.sin(angle), direction[0]*math.sin(angle)+direction[1]*math.cos(angle), direction[2]*math.cos(angle)]
        cloud = [0., .25, .5, .9, 1.][(index // 24) % 5]
        water_depth = [None, 2., 10.][(index // 80) % 3]
        sky_weight = [0., .4, 1.][(index // 15) % 3]
        visible = [0., .25, 1.][(index // 8) % 3]
        color = [0xffc9aa81, 0x80fffffa, 0x00ffffff][(index // 60) % 3]
        f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
        day, elapsed, cloud, sky_weight, visible = map(f32, [day, elapsed, cloud, sky_weight, visible])
        forward = list(map(f32, forward))
        boundary['cloud'] = 1. - cloud if kind == 0 else 1. - abs((cloud - .5)*2.)
        boundary['water'] = 1. if water_depth is None else 1. - min(1., max(0., water_depth*f32(.1)))
        boundary['visible'] = visible if color >> 24 else 0.
        n.write_floats(u, owner + 12, ray)
        disc_size = [1., 1.75, 2.625][index % 3]
        if kind == 1:
            # 7EECC0 publishes the current moon disc size into both glare endpoints.
            n.write_floats(u, owner + 0x90, [disc_size, disc_size])
        n.write_words(u, owner + 24, color)
        n.write_floats(u, 0xd38b04, [day])
        n.write_floats(u, 0xd38b30, forward)
        n.write_words(u, 0xd38b5c, 1)
        n.write_floats(u, 0xd38b60, [sky_weight])
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x7ef6e0, [struct.unpack('<I', struct.pack('<f',elapsed))[0]])
        values = [day,elapsed,cloud,water_depth if water_depth is not None else -1.,sky_weight,visible,*ray,*forward]
        packed = ' '.join(struct.pack('<f', x).hex() for x in values)
        size, visibility, target = [bytes(u.mem_read(owner+off,4)).hex() for off in [0x24,0x30,0x34]]
        response = bytes(u.mem_read(owner+0xa4,4)).hex()
        rows.append(f'{kind} {index} {color:08x} {packed} {size} {visibility} {target} {n.read_words(u,owner+24,1)[0]:08x} {struct.pack("<f",disc_size).hex()} {response}')
    return rows


def cloud_samples():
    """Run unmodified cloud projection and truncating independent-alpha lookup."""
    u = n.emulator()
    owner, source, out, alpha = [n.HEAP + i for i in [0, 0x100, 0x200, 0x1000]]
    n.write_words(u, owner + 0x1c, 128, 7)
    n.write_words(u, owner + 0x2c, 1)
    n.write_words(u, owner + 0x44, alpha)
    u.mem_write(alpha, bytes((x*13+y*37) & 255 for y in range(128) for x in range(128)))
    u.mem_write(n.STOP, b'\xd9\x1d' + struct.pack('<I',out))
    rows = ['# eye[3] source[3] alpha; float32 little-endian hex; 7EFA30/7EF920']
    for eye in [[0.,0.,0.], [1300.25,-4398.125,26.5]]:
        for i in range(128):
            angle = i*math.tau/128
            position = [eye[0]+12*math.cos(angle), eye[1]+12*math.sin(angle), eye[2]+(i-64)*.25]
            n.write_floats(u, 0xd38b18, eye)
            n.write_floats(u, source, position)
            u.reg_write(UC_X86_REG_ECX, owner)
            n.invoke(u, 0x7efa30, [source,0x3f800000])
            u.emu_start(n.STOP,n.STOP+6)
            rows.append(' '.join(struct.pack('<f',x).hex() for x in eye+position) + ' ' + bytes(u.mem_read(out,4)).hex())
    return rows


def lighting_samples():
    """Execute 7816F0's two packed light multiplications after palette refresh."""
    u = n.emulator()
    frame, environment = n.STACK + 0x1000, n.HEAP + 0x2000
    rows = ['# response color result; 7817D8..7818B6 with preceding 0.35 attenuation supplied']
    for index in range(256):
        response = struct.unpack('<f', struct.pack('<f', index / 255.))[0]
        scale = n.read_floats(u, 0xa3e860, 1)[0]
        n.write_floats(u, frame - 4, [response * scale])
        colors = [0xff000000 | index << 16 | (255-index) << 8 | (index*31 & 255), 0x80ffffff]
        n.write_words(u, environment + 0x1a8, *colors)
        u.reg_write(UC_X86_REG_EBP, frame)
        u.reg_write(UC_X86_REG_ESI, environment)
        u.reg_write(UC_X86_REG_ESP, frame - 0x100)
        u.emu_start(0x7817d8, 0x7818b6)
        for color, result in zip(colors, n.read_words(u, environment+0x1a8, 2)):
            rows.append(f'{struct.pack("<f",response).hex()} {color:08x} {result:08x}')
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = ['# kind step color day dt cloud depth sky visibility ray[3] forward[3] => size retained target color disc_size lighting_response; float32 little-endian hex']
    rows += sequence(0) + sequence(1)
    args.output.write_text('\n'.join(rows)+'\n')
    cloud = cloud_samples()
    args.output.with_name('world_glare_cloud_native.txt').write_text('\n'.join(cloud)+'\n')
    args.output.with_name('world_glare_lighting_native.txt').write_text('\n'.join(lighting_samples())+'\n')
    print(f'Captured {len(rows)-1} native glare updates')
    print(f'Captured {len(cloud)-1} native cloud opacity samples')
