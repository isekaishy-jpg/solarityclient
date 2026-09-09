"""Capture 7A8F20's displaced exterior portal windows and original depth scan.

The optional occlusion provider reports disabled. Hooks observe the three scene
window consumers; all projection, clipping, side offset and depth arithmetic
execute the fingerprinted PE. Input camera/polygon cases come from the retained
portal projection fixture. No stock process or entry point is launched.
"""
import argparse
import struct
from pathlib import Path

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP
import wmo_registration_oracle as n
from world_model_portal_projection_oracle import words


def capture(u, count, values, side, forward, exterior):
    """Run one fresh portal cache entry through both scene window banks."""
    root, portal, points, cache, reference = [n.HEAP + i * 0x1000 for i in range(5)]
    offset = count * 3
    vertices, plane = values[:offset], values[offset:offset + 4]
    local, camera = values[offset + 4:offset + 7], values[offset + 7:offset + 10]
    transform, projection = values[offset + 10:offset + 26], values[offset + 26:offset + 42]
    clips = values[offset + 42:offset + 62]
    n.write_words(u, root + 0x134, points)
    u.mem_write(portal, struct.pack('<HH4f', 0, count, *plane))
    n.write_floats(u, points, vertices)
    n.write_floats(u, 0xd1c42c, local)
    n.write_floats(u, 0xd1c444, forward)
    n.write_floats(u, 0xcd8f5c, camera)
    n.write_floats(u, 0xadff10, transform)
    n.write_floats(u, 0xadfe90, projection)
    n.write_floats(u, 0xcdd108, clips)
    u.mem_write(cache, bytes(28))
    u.mem_write(reference, struct.pack('<HHhH', 0, 1, side, 0))
    calls = []

    def hook(uc, address, size, data):
        sp = uc.reg_read(UC_X86_REG_ESP)
        if address == 0x7ccdf0:
            uc.reg_write(UC_X86_REG_EAX, 0)
        elif address in [0x795d00, 0x790ab0, 0x790ad0]:
            output = n.read_words(uc, sp + 4, 1)[0]
            calls.append((address, n.read_words(uc, output, 7)))
        else:
            return
        uc.reg_write(UC_X86_REG_EIP, n.read_words(uc, sp, 1)[0])
        uc.reg_write(UC_X86_REG_ESP, sp + 4)

    handle = u.hook_add(UC_HOOK_CODE, hook)
    u.reg_write(UC_X86_REG_ECX, root)
    n.invoke(u, 0x7a8f20, [portal, reference, cache, exterior])
    first_calls = list(calls)
    # The cache suppresses a second encounter, including the opposite side.
    u.reg_write(UC_X86_REG_ECX, root)
    n.invoke(u, 0x7a8f20, [portal, reference, cache, exterior])
    u.hook_del(handle)
    assert calls == first_calls
    flags = n.read_words(u, cache, 1)[0] & 0xffff
    if not calls:
        assert flags & 1
        return [0, flags, 0, 0, 0, 0, 0]
    assert [a for a, _ in calls] == [0x795d00, 0x790ab0] + ([0x790ad0] if exterior else [])
    assert all(v == calls[0][1] for _, v in calls)
    assert calls[0][1][5:] == (0, 0)
    return [1, flags, *calls[0][1][:5]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('input', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    u = n.emulator()
    rows = ['# count side exterior; vertices plane localEye worldEye rootMatrix projection clipPlanes forwardPlane; admitted cacheFlags window4 depth; all floats as hex bits']
    inputs = [line for line in args.input.read_text().splitlines() if line and not line.startswith('#')]
    for line in inputs[::17]:
        fields = line.split()
        count = int(fields[0])
        values = [struct.unpack('<f', struct.pack('<I', int(v, 16)))[0] for v in fields[1:-5]]
        for side in [-1, 0, 1]:
            for forward in [[0., 0., 1., 0.], [.317, -.481, .817, -13.7], [0., 0., 0., -1.]]:
                for exterior in [0, 1]:
                    result = capture(u, count, values, side, forward, exterior)
                    rows.append(f'{count} {side} {exterior} ' + words(values + forward) + ' ' + ' '.join(f'{v:08x}' for v in result))
    args.output.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original exterior portal windows')


if __name__ == '__main__':
    main()
