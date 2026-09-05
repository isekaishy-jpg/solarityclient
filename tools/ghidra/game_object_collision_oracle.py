"""Run original build-12340 GameObject model admission, door setter, and bounds.

The PE is only mapped into Unicorn. No OS or client entry point is executed.
Model readiness, callback installation, base-object presentation, and script
lookup are controlled external inputs. Collision-header copies, transforms,
eligibility virtuals, retained flag writes, door setters, and query masks execute
original instructions. The state setter sees no active model (743390 returns 0),
so this harness does not cover animation timers or transition notification order.
Requires the Python unicorn package and the fingerprinted locally owned Wow.exe.
"""
import hashlib
from pathlib import Path
import struct

from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPCW, UC_X86_REG_ECX, UC_X86_REG_EAX

import argparse
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('executable', type=Path)
parser.add_argument('--bounds-output', type=Path, required=True)
parser.add_argument('--state-output', type=Path, required=True)
arguments = parser.parse_args()
data = arguments.executable.read_bytes()
assert hashlib.sha256(data).hexdigest() == 'aa63a5750d60ef16746c686b3d5e26876d98953eab08b1c026cd0faf78e88cb8'
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
    assert uc.reg_read(UC_X86_REG_EIP) == STOP


def hook_return(uc, address, pop, result):
    def callback(uc, address, size, user):
        sp = uc.reg_read(UC_X86_REG_ESP)
        ret = read_words(uc, sp, 1)[0]
        uc.reg_write(UC_X86_REG_EAX, result)
        uc.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
        uc.reg_write(UC_X86_REG_EIP, ret)
    uc.hook_add(UC_HOOK_CODE, callback, begin=address, end=address)


IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
MATRICES = [
    IDENTITY,
    [0.3, -0.7, 0.2, 0, 0.8, 0.1, -0.4, 0, -0.6, 0.5, 0.9, 0, 7123.45, -9812.6, 24.7, 1],
    [-1.25, 0, 0, 0, 0, 3.75, 0, 0, 0, 0, 0.125, 0, -123.456, 17.333, 0.01, 1],
    [0.99999994, 0.000001, -0.00001, 0, -0.000001, 0.99999994, 0.0002, 0, 0.00001, -0.0002, 1, 0, 17066.666, -17066.666, 1, 1],
]
BOXES = [[-1, -2, -3, 4, 5, 6], [0, 0, 0, 0, 0, 0], [-1, -2, 0, 1, 2, 0], [-0.1, -0.03, 1.1, 0.7, 0.005, 1.15]]


def hex_floats(values):
    return ' '.join(f'{word:08x}' for word in struct.unpack('<' + 'I' * len(values), struct.pack('<' + 'f' * len(values), *values)))


def model_admission(door, state, previous, box, matrix):
    uc = emulator()
    obj, fields, model, shared, header, behavior, vtable = [HEAP + offset for offset in (0, 0x400, 0x800, 0xc00, 0x1000, 0x1400, 0x1800)]
    write_words(uc, obj + 8, fields)
    write_words(uc, obj + 0xb4, model)
    write_words(uc, obj + 0x1a0, behavior)
    write_words(uc, obj + 0x20c, previous)
    write_floats(uc, obj + 0x1a8, matrix)
    write_words(uc, model + 0x2c, shared)
    write_words(uc, shared + 8, 1)
    write_words(uc, shared + 0x150, header)
    write_floats(uc, header + 0xbc, box)
    write_words(uc, behavior, vtable, obj)
    write_words(uc, behavior + 0x10, state)
    write_words(uc, vtable + 4, 0x712550 if door else 0x8a1420)
    write_words(uc, vtable + 0x74, STOP + 0x20)
    for address, pop, result in [
        (0x4f4230, 4, 0), (0x744a50, 12, 0), (0x4d4d00, 0, 0),
        (0x824060, 12, 0), (0x823fe0, 12, 0), (0x824f00, 8, 1),
        (STOP + 0x20, 4, 0), (0x67bd40, 20, 0),
    ]:
        hook_return(uc, address, pop, result)
    uc.reg_write(UC_X86_REG_ECX, obj)
    invoke(uc, 0x712f30, [0, 0, 0])
    return read_words(uc, obj + 0x20c, 1)[0]


if __name__ == '__main__':
    fingerprint = hashlib.sha256(data).hexdigest()
    bounds_lines = [f'# Unmodified build 12340 SHA256 {fingerprint}', '# matrix16 bounds6 output6; floats are raw hexadecimal words. Both 7F9430 and 984860 must agree.']
    for matrix in MATRICES:
        for box in BOXES:
            uc = emulator()
            write_floats(uc, HEAP, matrix)
            write_floats(uc, HEAP + 0x100, box)
            invoke(uc, 0x7f9430, [HEAP, HEAP + 0x100, HEAP + 0x200])
            first = read_words(uc, HEAP + 0x200, 6)
            invoke(uc, 0x984860, [HEAP + 0x200, HEAP + 0x100, HEAP])
            assert first == read_words(uc, HEAP + 0x200, 6)
            bounds_lines.append(hex_floats(matrix + box) + ' ' + ' '.join(f'{word:08x}' for word in first))
    arguments.bounds_output.write_text('\n'.join(bounds_lines) + '\n')
    state_lines = [f'# Unmodified build 12340 SHA256 {fingerprint}', '# L: door internalState oldFlag bounds6 matrix16 result. D: previousState nextState oldFlag result. Q: objectType flags oldFlag result. Bounds/matrices hexadecimal; other values decimal.']
    for door in (0, 1):
        for state in (1, 2, 3, 4):
            for previous in (0, 1):
                for matrix in MATRICES[:2]:
                    for box in BOXES[:3]:
                        result = model_admission(door, state, previous, box, matrix)
                        state_lines.append(f'L {door} {state} {previous} {hex_floats(box + matrix)} {result}')
    for previous_state in range(8):
        for next_state in range(8):
            for previous in (0, 1):
                uc = emulator()
                write_words(uc, HEAP + 4, HEAP + 0x100)
                write_words(uc, HEAP + 0x10, previous_state)
                write_words(uc, HEAP + 0x30c, previous)
                hook_return(uc, 0x743390, 0, 0)
                uc.reg_write(UC_X86_REG_ECX, HEAP)
                invoke(uc, 0x70d8d0, [next_state])
                state_lines.append(f'D {previous_state} {next_state} {previous} {read_words(uc, HEAP + 0x30c, 1)[0]}')
    for kind in (0, 1, 7, 33):
        for flags in (0, 1, 0x8000, 0x8001, 0xffff_ffff):
            for previous in (0, 1):
                uc = emulator()
                write_words(uc, HEAP + 0xd0, HEAP + 0x400)
                uc.mem_write(HEAP + 0x42d, bytes([kind]))
                write_words(uc, HEAP + 0x20c, previous)
                uc.reg_write(UC_X86_REG_ECX, HEAP)
                invoke(uc, 0x70f550, [flags])
                state_lines.append(f'Q {kind} {flags} {previous} {uc.reg_read(UC_X86_REG_EAX)}')
    arguments.state_output.write_text('\n'.join(state_lines) + '\n')
    print(f'Captured {len(bounds_lines) - 2} bounds cases and {len(state_lines) - 2} state cases.')
