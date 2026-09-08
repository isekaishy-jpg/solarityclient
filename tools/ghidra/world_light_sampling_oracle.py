"""Capture original cyclic light bands and ordered packed environment overlays.

7EB070/7EAEF0, 7EBFF0/7ECD80, and 7ED4C0 execute the pinned executable.
The sole sampling boundary supplies the requested decoded WDBC band to 7EB210.
No color interpolation or blending is replaced. Fog mode is zero here; camera
far-clip remapping and later rendering policy are separate owners.
"""
import argparse
import struct
from pathlib import Path
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_ESP
import wmo_registration_oracle as n
from liquid_material_oracle import return_value


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def band(identifier, times, values):
    return struct.pack('<34I', identifier, len(times), *(times + [0] * (16 - len(times))), *(values + [0] * (16 - len(values))))


def tables(identifier):
    parameter = struct.pack('<4I5f', identifier, identifier % 2, 11 if identifier == 4 else 10 + identifier, 20 + identifier, identifier * .125, identifier * .1, identifier * .2, identifier * .3, identifier * .4)
    colors = []
    floats = []
    for channel in range(18):
        first = ((identifier * 31 + channel * 7) % 256) << 16 | ((identifier * 51 + channel * 11) % 256) << 8 | ((identifier * 71 + channel * 17) % 256)
        last = first ^ 0xabcdef
        colors.append(band((identifier - 1) * 18 + channel + 1, [0, 1440], [first, last]))
    for channel in range(6):
        first, last = (identifier * 3600., identifier * 7200.) if channel == 0 else (identifier * .125 + channel * .25, identifier * .25 + channel * .5)
        floats.append(band((identifier - 1) * 6 + channel + 1, [0, 1440], [bits(first), bits(last)]))
    return parameter, colors, floats


def capture():
    u = n.emulator()
    row, result, store_float, parameter, output, overlay = [n.HEAP + i * 0x1000 for i in range(6)]
    u.mem_write(store_float, b'\xd9\x1d' + struct.pack('<I', result))
    data = {identifier: tables(identifier) for identifier in range(1, 5)}
    def hook(u, address, size, context):
        if address == 0x7eb210:
            sp = u.reg_read(UC_X86_REG_ESP)
            identifier, destination = n.read_words(u, sp + 4, 2)
            is_color = u.reg_read(UC_X86_REG_ECX) == 0xaf49bc
            count = 18 if is_color else 6
            owner, channel = divmod(identifier - 1, count)
            u.mem_write(destination, data[owner + 1][1 if is_color else 2][channel])
            return_value(u, 1)
            u.reg_write(UC_X86_REG_ESP, sp + 12)
    u.hook_add(UC_HOOK_CODE, hook)
    rows = ['# Original build 12340 light sampling; vectors are raw little-endian words.']
    time_sets = [[0], [0, 1440], [120, 1440, 2760], [i * 180 for i in range(16)]]
    times = [0, 1, 119, 120, 121, 719, 720, 721, 1439, 1440, 1441, 2159, 2160, 2161, 2759, 2760, 2879]
    for keys in time_sets:
        for kind in ['color', 'float']:
            values = [(0x010305 + index * 0x172b3d) & 0xffffff for index in range(len(keys))] if kind == 'color' else [bits((index - 4) * 123.4567) for index in range(len(keys))]
            raw = band(1, keys, values)
            u.mem_write(row, raw)
            for channel in ([0] if kind == 'color' else [0, 1, 5]):
                for time in times:
                    u.reg_write(UC_X86_REG_EDI, row)
                    if kind == 'color': n.invoke(u, 0x7eb070, [result, time])
                    else:
                        n.invoke(u, 0x7eaef0, [time, channel])
                        u.emu_start(store_float, store_float + 6)
                    rows.append(f'band {kind} {channel} {time} {raw.hex()} ' + bytes(u.mem_read(result, 4)).hex())
    def sample(identifier, time, destination):
        u.mem_write(parameter, data[identifier][0])
        u.reg_write(UC_X86_REG_ECX, destination)
        n.invoke(u, 0x7ee360, [])
        u.reg_write(UC_X86_REG_EAX, parameter)
        u.reg_write(UC_X86_REG_ESI, destination)
        u.reg_write(UC_X86_REG_ECX, time)
        n.invoke(u, 0x7ecd80, [])
    for identifier in range(1, 5):
        p, colors, floats = data[identifier]
        rows.append('parameter ' + str(identifier) + ' ' + p.hex() + ' ' + b''.join(colors).hex() + ' ' + b''.join(floats).hex())
        for time in times:
            sample(identifier, time, output)
            rows.append(f'sample {identifier} {time} ' + bytes(u.mem_read(output, 156)).hex())
    for time in [0, 1, 720, 721, 1440, 2160, 2879]:
        for numerator in [1, 2, 63, 64, 127, 128, 129, 192, 254, 255, 256]:
            weight = numerator / 256.
            for sequence in [[2], [2, 3], [2, 3, 4]]:
                sample(1, time, output)
                for identifier in sequence:
                    sample(identifier, time, overlay)
                    u.reg_write(UC_X86_REG_EDI, output)
                    n.invoke(u, 0x7ed4c0, [overlay, bits(weight)])
                rows.append(f'overlay {time} {numerator} '+','.join(map(str, sequence))+' '+bytes(u.mem_read(output, 156)).hex())
    return rows


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable')
    parser.add_argument('output')
    args = parser.parse_args()
    n.initialize(args.executable)
    rows = capture()
    Path(args.output).write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'Captured {len(rows) - 1} original light samples and table inputs')
