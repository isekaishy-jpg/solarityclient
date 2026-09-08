"""Native parameter, WMO fog conversion and exterior final-fog policy.

7EBFF0's already sampled fog triplet is supplied for isolated threshold tests.
7ECD80/7ECD00 and 7ED1B0 run unchanged. 7F16F0 receives an absent WMO query
and a camera-liquid ID; no interpolation or fog arithmetic is substituted.
"""
import argparse
import itertools
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESI
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def value(word):
    return struct.unpack('<f', struct.pack('<I', word))[0]


def capture():
    u = n.emulator()
    owner = n.HEAP
    fog_end, ratio, liquid = 0., 0., 0

    def hook(u, address, size, context):
        if address == 0x7ebff0:
            n.write_words(u, owner + 0x48, bits(fog_end), bits(ratio), bits(1.))
            return_value(u, 0)
        elif address == 0x77fb90:
            return_value(u, 0)
        elif address == 0x780620:
            return_value(u, liquid)

    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Native fog palette/remapping and exterior final fog; hex f32 words.']
    edge = bits(1000/36)
    ends = [-10., 0., 10., value(edge-1), value(edge), value(edge+1), 100., 200., 500., 1000., 2000.]
    clips = [183.33333, value(bits(200.)-1), 200., value(bits(200.)+1), 350., 777., 1583.3334]
    for far, mode, fog_end, ratio in itertools.product(clips, [0, 1], ends, [-1., -.1, 0., .125, .5, .75, 1.]):
        n.write_words(u, 0xd38b40, bits(far))
        n.write_words(u, 0xd38acc, mode)
        u.reg_write(UC_X86_REG_ESI, owner)
        n.invoke(u, 0x7ecd80, [])
        palette = n.read_words(u, owner + 0x48, 3)
        rows.append(f'palette {bits(far):08x} {mode} {bits(fog_end):08x} {bits(ratio):08x} ' + ' '.join(f'{v:08x}' for v in palette))
        for liquid in [0, 1]:
            n.write_words(u, 0xd38c1c, *palette)
            n.write_words(u, 0xd38bf4, 0xff123456)
            n.invoke(u, 0x7f16f0, [])
            result = n.read_words(u, 0xd38b8c, 4)
            rows.append(f'final {bits(far):08x} {mode} {liquid} ' + ' '.join(f'{v:08x}' for v in palette + result))
        n.write_words(u, owner, bits(fog_end), bits(ratio), 0xffabcdef)
        u.reg_write(UC_X86_REG_ECX, owner)
        n.invoke(u, 0x7ed1b0, [])
        rows.append(f'wmo {bits(far):08x} {mode} {bits(fog_end):08x} {bits(ratio):08x} ' + ' '.join(f'{v:08x}' for v in n.read_words(u, 0xd38bb0, 4)))
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    n.initialize(args.executable)
    args.output.write_text(capture())
