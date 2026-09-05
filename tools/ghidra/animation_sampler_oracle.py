"""Execute isolated build-12340 interpolation routines from the fingerprinted PE.

Only interval selection is supplied by the harness. All key addressing,
interpolation, quaternion expansion/normalization/blending, and matrix arithmetic
execute original instructions. No client entry point or operating-system API runs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPCW

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('executable', type=Path, help='locally owned, fingerprinted build-12340 Wow.exe')
parser.add_argument('--output', type=Path, help='optional JSON output file')
arguments = parser.parse_args()
data = arguments.executable.read_bytes()
if hashlib.sha256(data).hexdigest() != 'aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8':
    raise ValueError('executable does not match the pinned build-12340 fingerprint')
pe = struct.unpack_from('<I', data, 0x3c)[0]
section_count = struct.unpack_from('<H', data, pe + 6)[0]
optional_size = struct.unpack_from('<H', data, pe + 20)[0]
image_base = struct.unpack_from('<I', data, pe + 24 + 28)[0]
image_size = struct.unpack_from('<I', data, pe + 24 + 56)[0]
headers = pe + 24 + optional_size
STACK, HEAP, STOP = 0x02000000, 0x03000000, 0x04000000


def emulator():
    uc = Uc(UC_ARCH_X86, UC_MODE_32)
    uc.mem_map(image_base, (image_size + 4095) & ~4095)
    for index in range(section_count):
        base = headers + index * 40
        _, rva, size, raw = struct.unpack_from('<4I', data, base + 8)
        uc.mem_write(image_base + rva, data[raw:raw + size])
    uc.mem_map(STACK, 0x20000)
    uc.mem_map(HEAP, 0x10000)
    uc.mem_map(STOP, 4096)
    uc.reg_write(UC_X86_REG_FPCW, 0x037f)
    return uc


def write_words(uc, address, *words):
    uc.mem_write(address, struct.pack('<' + 'I' * len(words), *words))


def write_floats(uc, address, values):
    uc.mem_write(address, struct.pack('<' + 'f' * len(values), *values))


def read_words(uc, address, count):
    return struct.unpack('<' + 'I' * count, uc.mem_read(address, count * 4))


def read_floats(uc, address, count):
    return list(struct.unpack('<' + 'f' * count, uc.mem_read(address, count * 4)))


def invoke(uc, address, arguments):
    sp = STACK + 0x18000
    write_words(uc, sp, STOP, *arguments)
    uc.reg_write(UC_X86_REG_ESP, sp)
    uc.emu_start(address, STOP, timeout=1_000_000, count=100_000)
    if uc.reg_read(UC_X86_REG_EIP) != STOP:
        raise RuntimeError('native routine did not return within the execution bound')


def sample(address, selector, values, count, width, lower=0, upper=1, amount=0.5, blend=0.0):
    uc = emulator()
    state, track, output, default, channels, payload = [HEAP + offset for offset in (0, 0x200, 0x300, 0x400, 0x500, 0x600)]
    write_floats(uc, state + 0xa8, [blend])
    uc.mem_write(track, struct.pack('<Hh', selector, -1))
    write_words(uc, track + 12, 1, channels)
    write_words(uc, channels, count, payload)
    uc.mem_write(payload, values)
    write_floats(uc, default, [0, 0, 0, 1])
    calls = 0

    def interval(uc, address, size, user):
        nonlocal calls
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret, _, _, first_pointer, second_pointer, amount_pointer = read_words(uc, sp, 6)
        write_words(uc, first_pointer, lower)
        write_words(uc, second_pointer, upper)
        write_floats(uc, amount_pointer, [amount if calls == 0 else 1.0])
        calls += 1
        uc.reg_write(UC_X86_REG_ESP, sp + 24)
        uc.reg_write(UC_X86_REG_EIP, ret)

    uc.hook_add(UC_HOOK_CODE, interval, begin=0x8284d0, end=0x8284d0)
    invoke(uc, address, [0, state, track, output, default])
    result = read_floats(uc, output + 8, width)
    if width == 4:
        matrix = HEAP + 0x900
        write_floats(uc, matrix, [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1])
        invoke(uc, 0x4c1c40, [output + 8, matrix])
        return {'value': result, 'matrix': read_floats(uc, matrix, 16)}
    return result


def floats(values):
    return struct.pack('<' + 'f' * len(values), *values)


if __name__ == '__main__':
    result = {'evidence': {
        'executable_sha256': hashlib.sha256(data).hexdigest(),
        'interval_selection': 'controlled lower/upper indices and fraction; no timestamp-search coverage',
        'executed': 'original key addressing, interpolation, expansion, normalization, blending, matrix construction',
    }}
    for selector in range(4):
        result[f'camera_vector_{selector}'] = sample(0x82b460, selector, floats([0, 1, 2, 21, 21, 21, 22, 22, 22, 3, 4, 5, 23, 23, 23, 24, 24, 24]), 2, 3, amount=.25)
        result[f'camera_roll_{selector}'] = sample(0x82b8a0, selector, floats([0, 31, 32, .5, 33, 34]), 2, 1, amount=.25)
        result[f'ordinary_vector_{selector}'] = sample(0x82b0a0, selector, floats([0, 0, 0, 2, 4, 6]), 2, 3, amount=.25)
        result[f'float_rotation_{selector}'] = sample(0x82ad50, selector, floats([.3, -.2, .4, .5, -.2, .4, -.1, -.6]), 2, 4, amount=.25)
    raw = struct.pack('<8H', 32768, 32768, 32768, 65535, 32768, 32768, 65535, 32768)
    result['bone_rotation_midpoint'] = sample(0x828680, 1, raw, 2, 4)
    result['bone_rotation_step'] = sample(0x828680, 0, raw, 2, 4)
    result['bone_rotation_blend'] = sample(0x828680, 1, raw, 2, 4, amount=0, blend=.25)
    raw_y = struct.pack('<8H', 32768, 32768, 32768, 65535, 32768, 65535, 32768, 32768)
    result['bone_rotation_y_midpoint'] = sample(0x828680, 1, raw_y, 2, 4)
    result['camera_full_turn'] = sample(0x82b8a0, 1, floats([6.2831854820251465, 0, 0, 0, 0, 0]), 2, 1)
    if arguments.output:
        arguments.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
