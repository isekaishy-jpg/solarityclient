"""Run native sky clipping, D3D9 scissor, WMO slot replacement and boundary fade.

Only graphics dispatch, resident model lookup, time, and the camera fog query
are supplied. 48ED60, 7F09B0, 6A38D0, 7F31C0 and 7F16F0 execute original code.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EDI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def capture():
    u = n.emulator()
    device, extent, interface, vtable, window, fog = [n.HEAP + i * 0x4000 for i in range(6)]
    sink = n.STOP + 0x100
    n.write_words(u, 0xc5df88, device)
    n.write_words(u, device + 0x397c, interface)
    n.write_words(u, interface, vtable)
    n.write_words(u, vtable + 300, sink)
    n.write_words(u, 0xd38ad4, 1)
    n.write_words(u, 0xd38b64, 0, 0, 0)
    rectangles, defaults, advances = [], [], []
    boundary, query_ready = 0., 1

    def ret(value=0, arguments=0):
        return_value(u, value)
        u.reg_write(UC_X86_REG_ESP, u.reg_read(UC_X86_REG_ESP) + arguments * 4)

    def hook(u, address, size, context):
        stack = u.reg_read(UC_X86_REG_ESP)
        if address == 0x682d70:
            ret(extent)
        elif address == sink:
            rectangles.append(n.read_words(u, n.read_words(u, stack + 8, 1)[0], 4))
            ret(arguments=2)
        elif address == 0x681f60:
            ret()
        elif address in (0x9abd50, 0x9ac660, 0x9acb00, 0x9acd40):
            defaults.append(address)
            ret()
        elif address == 0x86ae20:
            ret(12345)
        elif address == 0x81c9c0:
            advances.append(n.read_words(u, stack + 4, 1)[0])
            ret(arguments=1)
        elif address in (0x76c360, 0x7ecf20, 0x7f08c0):
            ret()
        elif address == 0x7f30c0:
            ret(u.reg_read(UC_X86_REG_EDI))
        elif address == 0x77fb90:
            pointers = n.read_words(u, stack + 4, 5)
            u.mem_write(pointers[0], bytes(48))
            n.write_words(u, pointers[1], fog)
            u.mem_write(pointers[2], bytes([1]))
            n.write_words(u, pointers[3], 0xd38bc4)
            n.write_words(u, pointers[4], bits(boundary))
            ret(query_ready)
        elif address == 0x780620:
            ret()

    u.hook_add(UC_HOOK_CODE, hook)
    windows = [[0., 0., 1., 1.], [.125, .25, .75, .875], [-2., -.25, .4, 2.],
               [0., 1., 1., 2.], [2., 0., 3., 1.], [.5, .5, .5, .9],
               [.50000006, .33333334, .50000012, .6666667],
               [1e-40, -0., .99999994, 1.]]
    viewport_windows = [[0., 0., 1., 1.], [.25, .125, .875, .75]]
    rows = ['# 7F09B0/48ED60 sky intersection and 6A38D0 backbuffer scissor. Float words are hex.']
    for enabled, view, bounds, dimensions in itertools.product(
            [0, 1], viewport_windows, windows, [(1, 1), (64, 64), (1280, 720), (1919, 1079), (16384, 8192)]):
        rectangles.clear(); defaults.clear(); advances.clear()
        n.write_words(u, device + 0x2534, 0)
        n.write_words(u, 0xd38ccc, enabled)
        n.write_words(u, 0xd38ad8, 1000)
        n.write_floats(u, device + 0xf70, [view[1], view[3], view[0], view[2], 0., 1.])
        n.write_floats(u, extent + 8, [dimensions[1], dimensions[0]])
        n.write_floats(u, window, bounds)
        n.invoke(u, 0x7f09b0, [window])
        active = n.read_words(u, device + 0x2534, 1)[0]
        clip = n.read_words(u, device + 0x2538, 4) if active else ()
        if active:
            u.reg_write(UC_X86_REG_ECX, device)
            n.invoke(u, 0x6a38d0, [])
        assert len(rectangles) == active
        assert len(advances) == active
        fields = f'clip {enabled} {dimensions[0]} {dimensions[1]} '
        fields += ' '.join(f'{bits(v):08x}' for v in view + bounds)
        fields += f' {len(defaults)} {advances[0] if active else 0} '
        fields += ' '.join(f'{v:08x}' for v in clip) + ' ' + ' '.join(map(str, rectangles[0])) if active else '-'
        rows.append(fields)
    state = ['# 7F31C0 WMO slot override and 7F16F0 stored DayNight+9C.']
    for weight in [-1., 0., .25, .99, 1.]:
        n.write_words(u, 0xd38b64, 10, 11, 12, bits(.1), bits(.2), bits(.3), 1, 2, 3)
        n.invoke(u, 0x7f31c0, [0, 99, 0, bits(weight)])
        n.invoke(u, 0x7f31c0, [1, 0, 0, 0])
        state.append(f'slots {bits(weight):08x} ' + ' '.join(f'{v:08x}' for v in n.read_words(u, 0xd38b64, 9)))
    distances = [-1., -0., 0., 1e-40, .001, 6.25, 12.5, 24.749999, 24.75, 24.750002, 24.999998, 25., 30., 3.4028234663852886e38]
    for query_ready, boundary in itertools.product([0, 1], distances):
        n.write_words(u, 0xd38b40, bits(777.))
        n.write_words(u, 0xd38acc, 0)
        n.write_words(u, 0xd38c1c, bits(500.), bits(.5), bits(2.5))
        n.write_words(u, 0xd38bf4, 0xff234567)
        n.invoke(u, 0x7f16f0, [])
        state.append(f'fade {query_ready} {bits(boundary):08x} {n.read_words(u, 0xd38b9c, 1)[0]:08x}')
    return '\n'.join(rows) + '\n', '\n'.join(state) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable')
    parser.add_argument('windows', type=Path)
    parser.add_argument('state', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    windows, state = capture()
    args.windows.write_text(windows)
    args.state.write_text(state)
