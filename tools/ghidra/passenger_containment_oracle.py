"""Native 77FFB0/7AEA10 passenger retention with controlled M2 readiness only."""
import struct
import sys
from pathlib import Path
import wmo_registration_oracle as native
from movement_path_oracle import invoke
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EAX


def words(values):
    return struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values))


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def neighbor(value, direction):
    word = words([value])[0]
    if value == 0:
        return struct.unpack('<f', struct.pack('<I', 1 if direction > 0 else 0x80000001))[0]
    word += direction if value > 0 else -direction
    return struct.unpack('<f', struct.pack('<I', word))[0]


native.initialize(sys.argv[1])
uc = native.emulator()
handle, model, cache, header, point, root, planes_ptr = [native.HEAP + value for value in (0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000)]
native.write_words(uc, handle + 8, 0x40)
native.write_words(uc, handle + 0x34, model)
native.write_words(uc, handle + 0xf4, root)
native.write_words(uc, model + 0x2c, cache)
native.write_words(uc, cache + 8, 1)
native.write_words(uc, cache + 0x150, header)
ready_ptr = native.HEAP + 0x7000
uc.mem_write(0x824f00, b'\xa1' + struct.pack('<I', ready_ptr) + b'\xc2\x08\x00')
lines = ['# flags model_present ready loaded_groups plane_count bounds6 point3 planes4N | contains']
for minimum, maximum in [([-2., -3., -4.], [5., 6., 7.]), ([99999., -100003., -4.], [100005., -99994., 100007.]), ([-.001]*3, [.001]*3), ([0.]*3, [0.]*3)]:
    top = f32(f32(maximum[2]) + struct.unpack('<f', struct.pack('<I', 0x3fd1f908))[0] + struct.unpack('<f', struct.pack('<I', 0x3c638e39))[0])
    points = [[f32((lo + hi) / 2) for lo, hi in zip(minimum, maximum)]]
    for axis in range(3):
        for boundary in [minimum[axis], maximum[axis]] + ([top] if axis == 2 else []):
            for offset in [-1, 0, 1]:
                value = f32(boundary) if offset == 0 else neighbor(f32(boundary), offset)
                current = points[0].copy()
                current[axis] = value
                points.append(current)
    native.write_floats(uc, header + 0xbc, [*minimum, *maximum])
    plane_sets = [[], [[1., 0., 0., -maximum[0]], [-1., 0., 0., minimum[0]], [0., 1., 0., -maximum[1]], [0., -1., 0., minimum[1]], [0., 0., 1., -maximum[2]], [0., 0., -1., minimum[2]]], [[.6, -.8, .2, 0.]]]
    for flags, present, ready, loaded, planes in [(0x40, 1, 1, 1, []), (0x40, 0, 1, 1, []), (0x40, 1, 0, 1, []), (0, 0, 0, 0, [])] + [(8, 0, 0, loaded, planes) for loaded in [0, 1] for planes in plane_sets]:
        native.write_words(uc, handle + 8, flags)
        native.write_words(uc, handle + 0x34, model if present else 0)
        native.write_words(uc, ready_ptr, ready)
        native.write_words(uc, root + 0x1e0, loaded)
        native.write_words(uc, root + 0x198, len(planes))
        native.write_words(uc, root + 0x15c, planes_ptr)
        flattened = [value for plane in planes for value in plane]
        if planes:
            native.write_floats(uc, planes_ptr, flattened)
        for position in points:
            native.write_floats(uc, point, position)
            invoke(uc, 0x77ffb0, [handle, point])
            expected = uc.reg_read(UC_X86_REG_EAX) & 255
            lines.append(f'{flags} {present} {ready} {loaded} {len(planes)} ' + ' '.join(f'{word:08x}' for word in words([*minimum, *maximum, *position, *flattened])) + f' {expected}')
Path(sys.argv[2]).write_text('\n'.join(lines) + '\n', encoding='utf-8')
print(f'captured {len(lines)-1} original retention decisions')
