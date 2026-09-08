"""Capture original 7A9090 portal projection with real transform and clip code.

Only the optional occlusion provider reports disabled. Near-portal polygon
inclusion, the 12-vertex input cap, five-plane clipping, transforms, perspective
division and screen-window reduction execute the fingerprinted client code.
"""
import argparse
import itertools
import math
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_EAX
import wmo_registration_oracle as n

IDENTITY = [1.,0.,0.,0., 0.,1.,0.,0., 0.,0.,1.,0., 0.,0.,0.,1.]


def transform(matrix, point):
    return [sum(matrix[c * 4 + r] * point[c] for c in range(3)) + matrix[12 + r]
            for r in range(3)]


def capture(u, vertices, plane, local_camera, camera, root_transform, projection, clips):
    root, portal, points, output = [n.HEAP + i * 0x1000 for i in range(4)]
    n.write_words(u, root + 0x134, points)
    u.mem_write(portal, struct.pack('<HH4f', 0, len(vertices), *plane))
    n.write_floats(u, points, [v for p in vertices for v in p])
    n.write_floats(u, 0xd1c42c, local_camera)
    n.write_floats(u, 0xcd8f5c, camera)
    n.write_floats(u, 0xadff10, root_transform)
    n.write_floats(u, 0xadfe90, projection)
    n.write_floats(u, 0xcdd108, [v for p in clips for v in p])
    u.mem_write(output, bytes(20))
    u.reg_write(UC_X86_REG_ECX, root)
    n.invoke(u, 0x7a9090, [portal, output])
    return n.read_words(u, output, 5)


def words(values):
    return ' '.join(f'{v:08x}' for v in struct.unpack('<' + 'I' * len(values),
                      struct.pack('<' + 'f' * len(values), *values)))


def capture_frustum(u, view, projection, camera):
    """Execute native corners, scene eye addition, then native plane creation."""
    view_ptr, projection_ptr, corner_ptr, frustum_ptr = [n.HEAP + i * 0x1000 for i in range(4, 8)]
    n.write_floats(u, view_ptr, view)
    n.write_floats(u, projection_ptr, projection)
    n.invoke(u, 0x6bf6d0, [view_ptr, projection_ptr, corner_ptr])
    # 795400 stores each addition to float before 984240 reads the corners.
    corners = n.read_floats(u, corner_ptr, 24)
    n.write_floats(u, corner_ptr, [value + camera[i % 3] for i, value in enumerate(corners)])
    u.reg_write(UC_X86_REG_ECX, frustum_ptr)
    n.invoke(u, 0x984240, [corner_ptr])
    corners = n.read_floats(u, corner_ptr, 24)
    planes = n.read_floats(u, frustum_ptr, 20)
    n.invoke(u, 0x4c1f00, [frustum_ptr + 0x200, view_ptr, projection_ptr])
    relative_projection = n.read_floats(u, frustum_ptr + 0x200, 16)
    return corners, [planes[i:i+4] for i in range(0, 20, 4)], relative_projection


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output', type=Path)
    parser.add_argument('--camera-output', type=Path, required=True)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()

    def occlusion_disabled(uc, address, size, _):
        sp = uc.reg_read(UC_X86_REG_ESP)
        uc.reg_write(UC_X86_REG_EAX, 0)
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + 4)

    u.hook_add(UC_HOOK_CODE, occlusion_disabled, begin=0x7ccdf0, end=0x7ccdf0)
    roots = [IDENTITY, [*IDENTITY[:12], 3.,-2.,1.,1.],
             [0.,2.,0.,0., -2.,0.,0.,0., 0.,0.,2.,0., 3.,-2.,1.,1.]]
    shapes = [[[-.5,-.8,0.],[.5,-.8,0.],[.5,.8,0.],[-.5,.8,0.]],
              [[-4.,-3.,0.],[4.,-3.,0.],[4.,3.,0.],[-4.,3.,0.]],
              [[8.,-1.,0.],[10.,-1.,0.],[10.,1.,0.],[8.,1.,0.]],
              [[-2.,-.5,0.],[1.,-.7,0.],[.3,2.,0.]],
              [[math.cos(i*math.tau/12), math.sin(i*math.tau/12), 0.] for i in range(12)] + [[20.,20.,0.]],
              [[-.00009,-1.,0.],[.00009,-1.,0.],[.00009,1.,0.],[-.00009,1.,0.]]]
    rows = ['# count; vertices; plane; localCamera; worldCamera; rootMatrix; relativeProjection; fiveWorldPlanes; flags; nativeWindow']
    def append(vertices, plane, local, camera, root, projection, clips):
        result = capture(u, vertices, plane, local, camera, root, projection, clips)
        values = [v for p in vertices for v in p] + plane + local + camera + root + projection + [v for p in clips for v in p]
        rows.append(f'{len(vertices)} ' + words(values) + ' ' + ' '.join(f'{v:08x}' for v in result))
    for root, shape, axis, distance, perspective in itertools.product(
        roots, shapes, range(3), [-3., -.01, 0., .01, 3.], [False, True],
    ):
        permute = lambda p: [p[(i + axis) % 3] for i in range(3)]
        vertices = [permute(p) for p in shape]
        normal = permute([0.,0.,1.])
        local = permute([.1, -.2, distance])
        camera = transform(root, local)
        projection = IDENTITY.copy()
        if perspective:
            projection[11], projection[15] = 1., 0.
            clips = [[0.,0.,1.,-camera[2]-.1],
                     [1.,0.,1.,-camera[0]-camera[2]], [-1.,0.,1.,camera[0]-camera[2]],
                     [0.,1.,1.,-camera[1]-camera[2]], [0.,-1.,1.,camera[1]-camera[2]]]
        else:
            clips = [[0.,0.,1.,-camera[2]-.1],
                     [1.,0.,0.,2.-camera[0]],[-1.,0.,0.,2.+camera[0]],
                     [0.,1.,0.,1.5-camera[1]],[0.,-1.,0.,1.5+camera[1]]]
        plane = [*normal, 0.]
        append(vertices, plane, local, camera, root, projection, clips)
    # Isolate the divide clamp and exact float classification boundaries.
    epsilon_bits = struct.unpack('<I', struct.pack('<f', .0001))[0]
    epsilon_neighbors = [struct.unpack('<f', struct.pack('<I', epsilon_bits+i))[0] for i in [-1,0,1]]
    open_planes = [[0.,0.,0.,1.]] * 5
    for w in [-1., 0., *epsilon_neighbors, 1.]:
        projection = IDENTITY.copy()
        projection[15] = w
        append(shapes[0], [0.,0.,1.,0.], [10.,10.,10.], [0.,0.,0.], IDENTITY, projection, open_planes)
    for x, x2 in itertools.product([-v for v in epsilon_neighbors]+[0.]+epsilon_neighbors, [-1.,1.]):
        polygon = [[x,-1.,0.],[x2,0.,0.],[x,1.,0.]]
        append(polygon, [0.,0.,1.,0.], [10.,10.,10.], [0.,0.,0.], IDENTITY, IDENTITY,
               [[1.,0.,0.,0.], *open_planes[:4]])
    # The classifier retains the unspilled distance even when the intersection
    # distance rounds back to exactly epsilon. This changes surviving topology.
    for tiny, sign in itertools.product([-1.e-12, 0., 1.e-12], [-1., 1.]):
        epsilon = epsilon_neighbors[1] * sign
        polygon = [[epsilon,1.,0.],[-1.,-1.,0.],[epsilon,-1.,0.]]
        append(polygon, [0.,0.,1.,0.], [10.,10.,10.], [0.,0.,0.], IDENTITY, IDENTITY,
               [[1.,tiny,0.,0.], *open_planes[:4]])
    # Nontrivial rotations, scales and large translations expose float stores.
    for angle, scale, shape in itertools.product([.37,-1.13], [.73,1.3], shapes[:4]):
        c, s = math.cos(angle)*scale, math.sin(angle)*scale
        root = [c,s,0.,0., -s,c,0.,0., 0.,0.,scale,0., 500.,-200.,70.,1.]
        local = [.12,-.27,-4.]
        camera = transform(root, local)
        projection = [1.3,.03,0.,0., -.07,1.8,0.,0., .1,-.15,1.,1., 0.,0.,0.,0.]
        append(shape, [0.,0.,1.,0.], local, camera, root, projection, open_planes)
    camera_rows = ['# native 6BF6D0 corners after 795400 eye addition; original 984240 first five planes']
    for angle, tilt, camera, near_far, orthographic in itertools.product(
        [0., .37, -1.13], [0., -.51, .83],
        [[0.,0.,0.], [500.,-200.,70.], [15000.,-14000.,2500.]],
        [[.2, 100.], [1., 5000.]], [False, True],
    ):
        c, s = math.cos(angle), math.sin(angle)
        cp, sp = math.cos(tilt), math.sin(tilt)
        cr, sr = math.cos(tilt*.7), math.sin(tilt*.7)
        forward = [s*cp, sp, c*cp]
        right_base, up_base = [-c,0.,s], [-s*sp,cp,-c*sp]
        right = [right_base[i]*cr + up_base[i]*sr for i in range(3)]
        up = [up_base[i]*cr - right_base[i]*sr for i in range(3)]
        view = [v for i in range(3) for v in [right[i], up[i], forward[i], 0.]] + [0.,0.,0.,1.]
        near, far = near_far
        if orthographic:
            projection = [.5,0.,0.,0., 0.,.75,0.,0., 0.,0.,2./(far-near),0.,
                          .25,-.125,-(far+near)/(far-near),1.]
        else:
            projection = [1.3,0.,0.,0., 0.,1.8,0.,0., 0.,0.,(far+near)/(far-near),1.,
                          0.,0.,-2.*far*near/(far-near),0.]
        corners, clips, relative = capture_frustum(u, view, projection, camera)
        camera_rows.append(words(corners + [v for p in clips for v in p]))
        # Real native frusta exercise side and far clipping, and the deliberate
        # absence of a near clipping plane, through original 7A9090 as well.
        for distance in [near * .25, near, far * .5, far, far * 1.01]:
            half = max(distance * .6, 1.)
            vertices = [[camera[i] + forward[i]*distance + right[i]*x + up[i]*y
                         for i in range(3)]
                        for x, y in [(-half,-half), (half,-half), (half,half), (-half,half)]]
            plane = [*forward, -sum(forward[i]*(camera[i]+forward[i]*distance) for i in range(3))]
            append(vertices, plane, camera, camera, IDENTITY, relative, clips)
    args.output.write_text('\n'.join(rows) + '\n')
    args.camera_output.write_text('\n'.join(camera_rows) + '\n')
    print(f'Captured {len(rows)-1} original portal projections')
    print(f'Captured {len(camera_rows)-1} original camera plane sets')


if __name__ == '__main__':
    main()
