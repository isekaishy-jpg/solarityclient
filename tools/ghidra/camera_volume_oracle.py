"""Execute 6059E0 camera volumes with controlled scene triangle collection.

The original camera basis, projection, volume construction, triangle-to-cube
transform, six-plane clipping and distance reduction run inside the pinned PE.
Only camera FOV and the 77F330 geometry/residency boundary are supplied. The
collected volumes and masks are captured for independent portable replay.
"""
import argparse
import itertools
import math
import random
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_EBX
import wmo_registration_oracle as n
from camera_water_oracle import bits, ret


def volume(eye, pivot, distance, aspect, mask, triangles, debug=None):
    uc = n.emulator()
    camera, vtable, output, eye_ptr, pivot_ptr, bank = [n.HEAP + i * 0x1000 for i in range(6)]
    fov_accessor = n.STOP + 0x100
    n.write_words(uc, camera, vtable)
    n.write_words(uc, vtable, fov_accessor)
    n.write_floats(uc, camera + 0x38, [.2, 5000.])
    n.write_floats(uc, camera + 0x44, [aspect])
    n.write_floats(uc, n.HEAP + 0x6000, [1.5707964])
    uc.mem_write(fov_accessor, b'\xd9\x05' + struct.pack('<I', n.HEAP + 0x6000) + b'\xc3')
    n.write_floats(uc, output, [distance])
    n.write_floats(uc, eye_ptr, eye)
    n.write_floats(uc, pivot_ptr, pivot)
    # The original callback-destructor registration does not affect a query.
    uc.mem_write(0x40c8fa, b'\xc3')
    calls = []

    def hook(uc, address, size, data):
        if debug is not None:
            sp = uc.reg_read(UC_X86_REG_ESP)
            if address == 0x6bfe60:
                _, origin, target, up, matrix = n.read_words(uc,sp,5)
                debug.append(['look_at',n.read_floats(uc,origin,3),n.read_floats(uc,target,3),n.read_floats(uc,up,3)])
            elif address == 0x4c1f00:
                _, output_ptr, a, b = n.read_words(uc,sp,4)
                debug.append(['multiply',n.read_floats(uc,a,16),n.read_floats(uc,b,16)])
            elif address == 0x4c2270:
                _, output_ptr, vertex, matrix = n.read_words(uc,sp,4)
                debug.append(['point',n.read_floats(uc,vertex,4),n.read_floats(uc,matrix,16)])
        if address == 0x77f330:
            _, body, collection, flags, unused = n.read_words(uc, uc.reg_read(UC_X86_REG_ESP), 5)
            assert unused == 0
            corners = n.read_words(uc, body + 0x60, 24)
            calls.append([flags, *corners])
            selected = [tri for kind, tri in triangles if bool(kind & flags)]
            n.write_words(uc, collection, len(selected), len(selected), bank, 0x100)
            for index, triangle in enumerate(selected):
                n.write_floats(uc, bank + index * 0x34, [0., 0., 1., 0., *sum(triangle, [])])
            ret(uc, int(bool(selected)))

    uc.hook_add(UC_HOOK_CODE, hook)
    uc.reg_write(UC_X86_REG_ECX, camera)
    try:
        n.invoke(uc, 0x6059e0, [output, eye_ptr, pivot_ptr, mask])
    except Exception:
        print('volume failure EIP', hex(uc.reg_read(UC_X86_REG_EIP)))
        raise
    return [uc.reg_read(UC_X86_REG_EAX), *n.read_words(uc, output, 1)], calls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    fixtures = []
    for z in [-.01, 0., .05, .2, .5, 1., 3., 4.8, 5., 5.01]:
        for extent in [.001, .1, 1., 30.]:
            fixtures.append([[z,-extent,-extent], [z,extent,-extent], [z,0.,extent]])
    rows = ['# inputs eye3 pivot3 distance aspect mask triangle-kind triangle9 | hit distance | (mask corners24)*; hex words']
    cases = list(itertools.product(
        [[5.,0.,0.]], [[0.,0.,0.]], [5.], [4/3, 16/9], [0x100171,0x120171], [0x20000,0x100171], fixtures))
    rng = random.Random(12340)
    for index in range(256):
        distance = rng.uniform(.3, 30.)
        pivot = [rng.uniform(-30.,30.) for _ in range(3)]
        direction = [rng.uniform(-1.,1.) for _ in range(3)]
        length = math.sqrt(sum(x*x for x in direction))
        eye = [pivot[i] + direction[i] / length * distance for i in range(3)]
        center = [pivot[i] + (eye[i]-pivot[i]) * rng.uniform(.1,.9) for i in range(3)]
        triangle = [[center[i]+rng.uniform(-5.,5.) for i in range(3)] for _ in range(3)]
        cases.append((eye,pivot,distance,rng.choice([1.,4/3,16/9,32/9]),0x120171,rng.choice([0x20000,0x100171]),triangle))
    for distance in [0., .2, .20099999, .201, .20100002, .3]:
        cases.append(([distance,0.,0.],[0.,0.,0.],distance,16/9,0x120171,0x100171,fixtures[16]))
    for eye, pivot, distance, aspect, mask, kind, triangle in cases:
        inputs = [*map(bits, eye+pivot+[distance,aspect]), mask, kind, *map(bits, sum(triangle, []))]
        result, calls = volume(eye, pivot, distance, aspect, mask, [(kind, triangle)])
        rows.append(' | '.join(' '.join(f'{v:08x}' for v in group) for group in [inputs,result,sum(calls,[])]))
    args.output.write_text('\n'.join(rows)+'\n', encoding='utf-8')
    print(f'Captured {len(rows)-1} original camera volume cases')


if __name__ == '__main__':
    main()
