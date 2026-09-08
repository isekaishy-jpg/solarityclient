"""Execute native 7EC220 weather blending before 7ED4C0 local overlays."""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value
from world_light_sampling_oracle import tables, bits


def capture(far_clip=None):
    u = n.emulator()
    if far_clip is not None:
        n.write_words(u, 0xd38acc, 1)
        n.write_words(u, 0xd38b40, bits(far_clip))
    parameter, output, weather, local = [n.HEAP + i * 0x1000 for i in range(4)]
    data = {i: tables(i) for i in range(1, 5)}

    def hook(u, address, size, context):
        if address == 0x7eb210:
            sp = u.reg_read(UC_X86_REG_ESP)
            identifier, destination = n.read_words(u, sp + 4, 2)
            color = u.reg_read(UC_X86_REG_ECX) == 0xaf49bc
            owner, channel = divmod(identifier - 1, 18 if color else 6)
            u.mem_write(destination, data[owner + 1][1 if color else 2][channel])
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 12)
    u.hook_add(UC_HOOK_CODE, hook)

    def sample(identifier, time, destination):
        u.mem_write(parameter, data[identifier][0])
        u.reg_write(UC_X86_REG_ECX, destination)
        n.invoke(u, 0x7ee360, [])
        u.reg_write(UC_X86_REG_EAX, parameter)
        u.reg_write(UC_X86_REG_ESI, destination)
        u.reg_write(UC_X86_REG_ECX, time)
        n.invoke(u, 0x7ecd80, [])

    rows = ['# Build 12340 original weather palette and ordered local composition.']
    for time in [0, 1, 720, 721, 1440, 2160, 2879]:
        for weight in [-1., 0., .0001, .5 / 255, 1.5 / 255, 2.5 / 255, .01, .125, .25, .5, .75, .99, 1., 2.]:
            for numerator in [0, 1, 128, 256]:
                sample(1, time, output)
                sample(2, time, weather)
                n.invoke(u, 0x7ec220, [output, weather, bits(weight)])
                if numerator:
                    sample(3, time, local)
                    sample(4, time, weather)
                    n.invoke(u, 0x7ec220, [local, weather, bits(weight)])
                    u.reg_write(UC_X86_REG_EDI, output)
                    n.invoke(u, 0x7ed4c0, [local, bits(numerator / 256.)])
                prefix = 'weather' if far_clip is None else f'fog-weather {bits(far_clip):08x}'
                rows.append(f'{prefix} {time} {bits(weight):08x} {numerator} ' + bytes(u.mem_read(output, 156)).hex())
    return '\n'.join(rows) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--exe', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--far-clip', type=float)
    args = parser.parse_args()
    n.initialize(args.exe)
    Path(args.output).write_text(capture(args.far_clip), encoding='utf-8')
