"""Compare the installed Orgrimmar graph with original build-12340 visibility.

First run the systems capture_ogrimmar_scene example. Its output contains local
archive files and exact input/expected float stores; those assets stay local.
Original 7B3A10/7AD350/7AC060, 7A9090, clip-stack operations, 78FB00 and the
resident MOBA selection loops run on all 144 groups and 157 authored portals.
Hooks supply resident lookup, disable optional occlusion/exclusion output, and
observe graphics callbacks. 7A6E00's graphics setup is replaced by its original
local-camera arithmetic plus the original camera projection. Placement matrices
are shared inputs; this does not establish placement construction, camera
registration, terrain occlusion, doodad visibility, shaders, or GPU output.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP,
)

import wmo_registration_oracle as n
from world_model_batch_visibility_oracle import select
from world_model_local_camera_oracle import capture as local_capture
from world_scene_projection_oracle import capture as camera_capture


def chunks(data):
    """Expose the raw little-endian archive chunks without interpreting geometry."""
    result, offset = {}, 0
    while offset + 8 <= len(data):
        key = data[offset:offset + 4][::-1].decode('ascii')
        size = struct.unpack_from('<I', data, offset + 4)[0]
        result[key] = data[offset + 8:offset + 8 + size]
        offset += 8 + size
    assert offset == len(data)
    return result


def main():
    """Replay original exterior entries and compare every accepted draw region."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('capture_directory', type=Path)
    args = parser.parse_args()
    folder = args.capture_directory
    expected = (folder / 'rust-scene.txt').read_text().splitlines()
    inputs = [struct.unpack('<f', struct.pack('<I', int(x, 16)))[0]
              for x in expected[0].split()[1:]]
    assert len(inputs) == 48
    eye, target, forward, up = [inputs[i:i + 3] for i in (0, 3, 6, 9)]
    transform, inverse = inputs[16:32], inputs[32:48]
    n.initialize(args.executable)
    u = n.emulator()
    camera = camera_capture(u, eye, forward, up, *inputs[12:16])
    local = local_capture(u, inverse, eye, target)
    root, instance, info, vertices, portals, refs, groups, entries, window = [
        n.HEAP + x for x in (0x8000, 0x9000, 0xa000, 0xc000, 0x12000,
                            0x16000, 0x20000, 0x34000, 0x3f000)]
    data = chunks((folder / 'ogrimmar.wmo').read_bytes())
    count = len(data['MOGI']) // 32
    # The fixed evidence layout is intentionally limited to this installed model.
    assert count == 144 and len(data['MOPT']) == 157 * 20
    assert len(data['MOPR']) == 314 * 8 and len(data['MOPV']) <= 0x6000
    for key, pointer in [('MOGI', info), ('MOPV', vertices), ('MOPT', portals), ('MOPR', refs)]:
        u.mem_write(pointer, data[key])
    n.write_words(u, root + 0x130, info, vertices, portals, refs)
    n.write_words(u, root + 0x16c, count)
    n.write_words(u, root + 0x1e0, 1)
    n.write_words(u, instance + 0xf4, root)
    n.write_floats(u, instance + 0x70, transform)
    n.write_floats(u, instance + 0xb0, inverse)
    n.write_floats(u, 0xcd8f5c, eye + target)
    n.write_floats(u, 0xd1c42c, local)
    n.write_floats(u, 0xadff10, transform)
    n.write_floats(u, 0xadfe90, camera[32:48])
    n.write_floats(u, 0xcdd108, camera[72:92])
    n.write_floats(u, 0xcdb108, camera[48:72])
    u.reg_write(UC_X86_REG_ECX, 0xcdb168)
    n.invoke(u, 0x984240, [0xcdb108])
    for address, value in [(0xcd8798, 0), (0xd1bee4, 10), (0xd1c3d0, 3),
                           (0xd1c424, 1), (0xcfbec0, 0), (0xcfbebc, 0)]:
        n.write_words(u, address, value)
    n.write_floats(u, window, [0., 0., 1., 1.])
    group_data = []
    for index in range(count):
        group = chunks((folder / f'ogrimmar_{index:03}.wmo').read_bytes())['MOGP']
        group_data.append(group)
        flags = struct.unpack_from('<I', group, 8)[0]
        start, length = struct.unpack_from('<HH', group, 36)
        n.write_words(u, groups + index * 0x200 + 0x30, flags)
        n.write_words(u, groups + index * 0x200 + 0x50, start, length)
        n.write_words(u, entries + index * 0x100 + 0x50, index)
        n.invoke(u, 0x7f9430, [instance + 0x70, info + index * 32 + 4,
                             entries + index * 0x100 + 0x24])
    clips = []
    entry_index = None

    def ret(uc, argument_count=0, value=0):
        """Return only at the documented resident and graphics boundaries."""
        sp = uc.reg_read(UC_X86_REG_ESP)
        uc.reg_write(UC_X86_REG_EAX, value)
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + argument_count * 4)

    def hook(uc, address, size, user):
        """Keep native graph/projection decisions and observe each world clip."""
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7aea80:
            index = n.read_words(uc, sp + 4, 1)[0]
            assert index < count
            ret(uc, 2, groups + index * 0x200)
        elif address == 0x7a6e00:
            ret(uc, 4)
        elif address == 0x799310:
            index = n.read_words(uc, sp + 4, 1)[0]
            level = n.read_words(uc, 0xcd8798, 1)[0]
            clips.append((entry_index, index, bytes(uc.mem_read(0xcdb168 + level * 0xfc, 0xfc))))
            ret(uc)
        elif address in (0x7ccdf0, 0x78fdc0):
            ret(uc)
        elif address in (0x794190, 0x6156c0):
            ret(uc, 1)
        elif address == 0x7a8e90:
            ret(uc, 2)

    handle = u.hook_add(UC_HOOK_CODE, hook)
    for entry_index in range(count):
        if n.read_words(u, info + entry_index * 32, 1)[0] & 0x10008:
            u.reg_write(UC_X86_REG_ECX, instance)
            n.invoke(u, 0x7b3a10, [entries + entry_index * 0x100, window, 1])
    u.hook_del(handle)
    rows = [expected[0]]
    n.write_words(u, 0xcd8798, 0)
    selected_count = 0
    for entry, index, clip in clips:
        # Native 799310/7ABF50 store local clips before resident batch callbacks.
        u.mem_write(0xcdb168, clip)
        n.invoke(u, 0x78fb00, [instance + 0xb0])
        values = n.read_words(u, 0xcdb168 + 0x60, 24) + n.read_words(u, 0xcdb168, 24)
        group = group_data[index]
        batch_data = chunks(group[68:])['MOBA']
        pointer, batch_pointer = n.HEAP, n.HEAP + 0x1000
        n.write_words(u, pointer + 0xf8, batch_pointer)
        n.write_words(u, pointer + 0x16c, len(batch_data) // 24)
        u.mem_write(pointer + 0x60, group[44:46])
        u.mem_write(batch_pointer, batch_data)
        variant = 'colored' if struct.unpack_from('<I', group, 8)[0] & 4 else 'normal'
        selected = select(u, pointer, batch_pointer, 0, variant)
        selected_count += len(selected)
        rows.append(f'visit {entry} {index} ' + ' '.join(f'{v:08x}' for v in values)
                    + f' batches {len(selected)} ' + ' '.join(map(str, selected)))
    (folder / 'native-scene.txt').write_text('\n'.join(rows) + '\n')
    assert rows == expected, 'native callback clips or selected batches differ; compare scene files'
    print(f'Matched groups {[index for _, index, _ in clips]}, '
          f'{len(clips) * 48} float stores and {selected_count} selected batches')


if __name__ == '__main__':
    main()
